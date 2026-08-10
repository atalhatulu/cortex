use crate::rangecoder::{Encoder, Decoder, PROB_MAX};

/// Updates the probability model based on the given bit.
/// Ensures the probability stays within the valid range.
#[inline(always)]
fn update_prob(prob: &mut u16, bit: u8) {
    let p = *prob as i32;
    let target = ((bit as i32) ^ 1) * (PROB_MAX as i32);
    let mut new_p = p + ((target - p) >> 5);
    if new_p <= 0 { new_p = 1; }
    if new_p >= PROB_MAX as i32 { new_p = PROB_MAX as i32 - 1; }
    *prob = new_p as u16;
}

/// The context mixing model for Move-To-Front (MTF) token probabilities.
/// It uses two orders of context (order1 and order2) to predict the next bit.
pub struct MtfModel {
    order1: Vec<u16>,
    order2: Vec<u16>,
}

impl MtfModel {
    pub fn new() -> Self {
        Self {
            order1: vec![PROB_MAX / 2; 257 * 512],
            order2: vec![PROB_MAX / 2; 4096 * 512],
        }
    }
}

impl Default for MtfModel {
    fn default() -> Self {
        Self::new()
    }
}

impl MtfModel {
    /// Encodes a sequence of MTF tokens into the arithmetic encoder `enc`.
    pub fn encode_tokens(&mut self, enc: &mut Encoder, tokens: &[u16]) {
        let mut prev1 = 0;
        let mut prev2 = 0;
        for &t in tokens {
            let mut ctx = 1;
            let hash = ((prev1 << 5) ^ prev2) & 4095;
            for i in (0..9).rev() {
                let bit = ((t >> i) & 1) as u8;
                let idx1 = (prev1 * 512) + ctx;
                let idx2 = (hash * 512) + ctx;

                let p1 = self.order1[idx1] as u32;
                let p2 = self.order2[idx2] as u32;
                let p_mix = ((p1 + p2) >> 1) as u16;

                enc.encode_bit_fixed(p_mix, bit);

                update_prob(&mut self.order1[idx1], bit);
                update_prob(&mut self.order2[idx2], bit);

                ctx = (ctx << 1) | bit as usize;
            }
            prev2 = prev1;
            prev1 = t as usize;
        }
    }

    /// Decodes a sequence of MTF tokens from the arithmetic decoder `dec`.
    pub fn decode_tokens(&mut self, dec: &mut Decoder, len: usize) -> Result<Vec<u16>, std::io::Error> {
        let mut tokens = Vec::with_capacity(len);
        let mut prev1 = 0;
        let mut prev2 = 0;
        for _ in 0..len {
            let mut ctx = 1;
            let hash = ((prev1 << 5) ^ prev2) & 4095;
            for _ in 0..9 {
                let idx1 = (prev1 * 512) + ctx;
                let idx2 = (hash * 512) + ctx;

                let p1 = self.order1[idx1] as u32;
                let p2 = self.order2[idx2] as u32;
                let p_mix = ((p1 + p2) >> 1) as u16;

                let bit = dec.decode_bit_fixed(p_mix);

                update_prob(&mut self.order1[idx1], bit);
                update_prob(&mut self.order2[idx2], bit);

                ctx = (ctx << 1) | bit as usize;
            }
            let t = (ctx & 511) as u16;
            if t > 256 {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Corrupt file: token out of bounds"));
            }
            tokens.push(t);
            prev2 = prev1;
            prev1 = t as usize;
        }
        Ok(tokens)
    }
}

/// Performs Burrows-Wheeler Transform (BWT), then Move-To-Front (MTF) coding, 
/// and finally Run-Length Encoding (RLE) on the chunk.
/// Returns the primary index of the BWT and the resulting RLE tokens.
pub fn bwt_mtf_rle(chunk: &[u8]) -> (u32, Vec<u16>) {
    let n = chunk.len();
    let sa_obj = divsufsort::sort(chunk);
    let (_, sa) = sa_obj.into_parts();

    let mut bwt = vec![0u8; n];
    let mut pidx = 0u32;
    for i in 0..n {
        let sa_i = sa[i];
        if sa_i == 0 {
            bwt[i] = chunk[n - 1];
            pidx = i as u32;
        } else {
            bwt[i] = chunk[(sa_i - 1) as usize];
        }
    }

    // Pure MTF + RLE
    let mut state: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut rle_tokens = Vec::with_capacity(n / 2);
    let mut zero_run = 0usize;

    for &b in &bwt {
        let mut idx = 0;
        while state[idx] != b {
            idx += 1;
        }

        if idx == 0 {
            zero_run += 1;
        } else {
            if zero_run > 0 {
                zero_run += 1;
                while zero_run > 1 {
                    rle_tokens.push((zero_run & 1) as u16);
                    zero_run >>= 1;
                }
                zero_run = 0;
            }

            rle_tokens.push(idx as u16 + 1);
            let val = state[idx];
            state.copy_within(0..idx, 1);
            state[0] = val;
        }
    }

    if zero_run > 0 {
        zero_run += 1;
        while zero_run > 1 {
            rle_tokens.push((zero_run & 1) as u16);
            zero_run >>= 1;
        }
    }

    (pidx, rle_tokens)
}

/// Reverses the RLE, MTF, and BWT steps to recover the original data chunk.
pub fn decode_rle_mtf_bwt(pidx: usize, rle_tokens: &[u16], original_size: usize) -> Result<Vec<u8>, std::io::Error> {
    let mut bwt = Vec::with_capacity(original_size);
    let mut state: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut zero_run = 0usize;
    let mut zero_power = 1usize;

    for &t in rle_tokens {
        if t <= 1 {
            let add_val = (t as usize + 1).checked_mul(zero_power)
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "RLE overflow"))?;
            zero_run = zero_run.checked_add(add_val)
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "RLE overflow"))?;
            zero_power = zero_power.checked_shl(1)
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "RLE overflow"))?;
            
            if bwt.len() + zero_run > original_size {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "RLE exceeds chunk size"));
            }
        } else {
            if zero_run > 0 {
                let count = zero_run;
                let b = state[0];
                for _ in 0..count {
                    bwt.push(b);
                }
                zero_run = 0;
                zero_power = 1;
            }

            let idx = (t - 1) as usize;
            if idx > 255 {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid MTF token"));
            }
            let val = state[idx];
            state.copy_within(0..idx, 1);
            state[0] = val;
            if bwt.len() + 1 > original_size {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Chunk exceeds original size"));
            }
            bwt.push(val);
        }
    }

    if zero_run > 0 {
        let count = zero_run;
        let b = state[0];
        for _ in 0..count {
            bwt.push(b);
        }
    }

    let n = bwt.len();
    if n != original_size {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Decoded data length mismatch"));
    }
    if pidx >= n {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid pidx"));
    }

    let mut counts = [0usize; 256];
    for &b in &bwt { counts[b as usize] += 1; }

    let mut start = [0usize; 256];
    let mut sum = 0;
    for i in 0..256 {
        start[i] = sum;
        sum += counts[i];
    }

    // `t_arr` holds bucket positions within the chunk. Chunks are ≤ 64 MB, so
    // positions always fit in u32; using u32 instead of usize halves this
    // array's footprint (4 vs 8 bytes per entry) — the single biggest decode
    // memory line item for large blocks.
    let mut t_arr = vec![0u32; n];
    let c_last = bwt[pidx] as usize;
    t_arr[pidx] = start[c_last] as u32;
    start[c_last] += 1;

    for i in 0..n {
        if i == pidx { continue; }
        let b = bwt[i] as usize;
        t_arr[i] = start[b] as u32;
        start[b] += 1;
    }

    let mut chunk_out = vec![0u8; n];
    let mut p = pidx;
    for i in (0..n).rev() {
        chunk_out[i] = bwt[p];
        p = t_arr[p] as usize;
    }

    Ok(chunk_out)
}
