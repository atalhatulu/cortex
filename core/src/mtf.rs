use crate::rangecoder::{Decoder, Encoder, PROB_MAX};
use std::sync::atomic::{AtomicU64, Ordering};

pub static PROF_ENTROPY: AtomicU64 = AtomicU64::new(0);
pub static PROF_MTF_RLE: AtomicU64 = AtomicU64::new(0);
pub static PROF_INV_BWT_TARR: AtomicU64 = AtomicU64::new(0);
pub static PROF_INV_BWT_TRAVERSAL: AtomicU64 = AtomicU64::new(0);

pub const LANES: usize = 8;

#[inline(always)]
fn update_prob(prob: &mut u16, bit: u8, adapt_rate: i32) {
    let p = *prob as i32;
    let target = ((bit as i32) ^ 1) * (PROB_MAX as i32);
    let new_p = p + ((target - p) >> adapt_rate);
    *prob = new_p.clamp(1, PROB_MAX as i32 - 1) as u16;
}

pub struct MtfModel {
    order0: Vec<u16>,
    order1: Vec<u16>,
    order2: Vec<u16>,
}

impl MtfModel {
    pub fn new() -> Self {
        MtfModel {
            order0: vec![PROB_MAX / 2; 512],
            order1: vec![PROB_MAX / 2; 257 * 512],
            order2: vec![PROB_MAX / 2; 8192 * 512],
        }
    }
}

impl Default for MtfModel {
    fn default() -> Self {
        Self::new()
    }
}

impl MtfModel {
    pub fn encode_tokens(&mut self, enc: &mut Encoder, tokens: &[u16]) {
        let mut prev1 = 0;
        let mut prev2 = 0;
        for &token in tokens {
            let mut ctx = 1;
            let hash = ((prev2 * 257) ^ prev1) & 0x1FFF;
            for i in (0..9).rev() {
                let bit = ((token >> i) & 1) as u8;
                let idx1 = (prev1 * 512) + ctx;
                let idx2 = (hash * 512) + ctx;

                let p0 = self.order0[ctx] as u32;
                let p1 = self.order1[idx1] as u32;
                let p2 = self.order2[idx2] as u32;

                // Weights: O0=1, O1=3, O2=4 (Total 8)
                let p_mix = ((p0 + p1 * 3 + p2 * 4) >> 3) as u16;

                enc.encode_bit_fixed(p_mix, bit);

                update_prob(&mut self.order0[ctx], bit, 5);
                update_prob(&mut self.order1[idx1], bit, 5);
                update_prob(&mut self.order2[idx2], bit, 4);

                ctx = (ctx << 1) | bit as usize;
            }
            prev2 = prev1;
            prev1 = token as usize;
        }
    }

    pub fn decode_tokens(
        &mut self,
        dec: &mut Decoder,
        len: usize,
    ) -> Result<Vec<u16>, std::io::Error> {
        let t0 = std::time::Instant::now();
        let mut tokens = Vec::with_capacity(len);
        let mut prev1 = 0;
        let mut prev2 = 0;
        for _ in 0..len {
            let mut ctx = 1;
            let hash = ((prev2 * 257) ^ prev1) & 0x1FFF;
            while ctx < 512 {
                let idx1 = (prev1 * 512) + ctx;
                let idx2 = (hash * 512) + ctx;

                let p0 = self.order0[ctx] as u32;
                let p1 = self.order1[idx1] as u32;
                let p2 = self.order2[idx2] as u32;

                let p_mix = ((p0 + p1 * 3 + p2 * 4) >> 3) as u16;

                let bit = dec.decode_bit_fixed(p_mix);

                update_prob(&mut self.order0[ctx], bit, 5);
                update_prob(&mut self.order1[idx1], bit, 5);
                update_prob(&mut self.order2[idx2], bit, 4);

                ctx = (ctx << 1) | bit as usize;
            }
            let t = (ctx - 512) as u16;
            if t > 256 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Corrupt file: token out of bounds",
                ));
            }
            tokens.push(t);
            prev2 = prev1;
            prev1 = t as usize;
        }
        PROF_ENTROPY.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
        Ok(tokens)
    }
}

pub fn bwt_mtf_rle(chunk: &[u8]) -> ([u32; LANES], Vec<u16>) {
    let n = chunk.len();
    if n == 0 {
        return ([0; LANES], Vec::new());
    }
    let sa_obj = divsufsort::sort(chunk);
    let (_, sa) = sa_obj.into_parts();

    let mut bwt = vec![0u8; n];
    let mut pidx = [0u32; LANES];

    let mut lengths = [0usize; LANES];
    for i in 0..LANES {
        lengths[i] = n / LANES + if n % LANES > i { 1 } else { 0 };
    }

    let mut t_starts = [0usize; LANES];
    t_starts[0] = 0;
    let mut current_t = n;
    for i in 1..LANES {
        current_t -= lengths[i - 1];
        t_starts[i] = current_t;
    }

    for i in 0..n {
        let sa_i = sa[i];
        for lane in 0..LANES {
            if sa_i == t_starts[lane] as i32 {
                pidx[lane] = i as u32;
            }
        }

        if sa_i == 0 {
            bwt[i] = chunk[n - 1];
        } else {
            bwt[i] = chunk[(sa_i - 1) as usize];
        }
    }

    let mut state: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut positions: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut rle_tokens = Vec::with_capacity(n / 2);
    let mut zero_run = 0usize;

    for &b in &bwt {
        let idx = positions[b as usize] as usize;

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
            positions[val as usize] = 0;
            for p in 1..=idx {
                positions[state[p] as usize] = p as u8;
            }
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

pub fn decode_rle_mtf_bwt(
    pidx: [u32; LANES],
    rle_tokens: &[u16],
    original_size: usize,
) -> Result<Vec<u8>, std::io::Error> {
    if original_size == 0 {
        return Ok(Vec::new());
    }

    let t0 = std::time::Instant::now();
    let mut bwt = Vec::with_capacity(original_size);
    let mut state: [u8; 256] = std::array::from_fn(|i| i as u8);

    let mut zero_run = 0usize;
    let mut zero_power = 1usize;

    for &t in rle_tokens {
        if t <= 1 {
            let add_val = (t as usize + 1).checked_mul(zero_power).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "RLE overflow")
            })?;
            zero_run = zero_run.checked_add(add_val).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "RLE overflow")
            })?;
            zero_power = zero_power.checked_shl(1).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "RLE overflow")
            })?;

            if bwt.len() + zero_run > original_size {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "RLE exceeds chunk size",
                ));
            }
        } else {
            if zero_run > 0 {
                let count = zero_run;
                let b = state[0];
                bwt.resize(bwt.len() + count, b);
                zero_run = 0;
                zero_power = 1;
            }

            let idx = (t - 1) as usize;
            if idx > 255 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid MTF token",
                ));
            }
            let val = state[idx];
            state.copy_within(0..idx, 1);
            state[0] = val;
            if bwt.len() + 1 > original_size {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Chunk exceeds original size",
                ));
            }
            bwt.push(val);
        }
    }

    if zero_run > 0 {
        let count = zero_run;
        let b = state[0];
        bwt.resize(bwt.len() + count, b);
    }

    let n = bwt.len();
    if n != original_size {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Decoded data length mismatch",
        ));
    }

    PROF_MTF_RLE.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
    let t1 = std::time::Instant::now();

    if n == 0 {
        return Ok(bwt);
    }

    let mut counts = [0usize; 256];
    for &b in &bwt {
        counts[b as usize] += 1;
    }

    let mut start = [0usize; 256];
    let mut sum = 0;
    for j in 0..256 {
        start[j] = sum;
        sum += counts[j];
    }

    let mut t_arr = vec![0u64; n];

    // BWT without EOF symbol requires `pidx` (which corresponds to sa_i == 0)
    // to be processed first for its character class so the LF mapping aligns correctly.
    let real_pidx = pidx[0] as usize;
    let c_last = bwt[real_pidx] as usize;
    t_arr[real_pidx] = ((c_last as u64) << 32) | (start[c_last] as u64);
    start[c_last] += 1;

    unsafe {
        for j in 0..n {
            if j == real_pidx {
                continue;
            }
            let b = *bwt.get_unchecked(j) as usize;
            *t_arr.get_unchecked_mut(j) = ((b as u64) << 32) | (*start.get_unchecked(b) as u64);
            *start.get_unchecked_mut(b) += 1;
        }
    }

    PROF_INV_BWT_TARR.fetch_add(t1.elapsed().as_nanos() as u64, Ordering::Relaxed);
    let t2 = std::time::Instant::now();

    let mut lengths = [0usize; LANES];
    for i in 0..LANES {
        lengths[i] = n / LANES + if n % LANES > i { 1 } else { 0 };
    }

    let mut chunk_out = vec![0u8; n];
    let mut p = pidx;

    let mut out_starts = [0usize; LANES];
    let mut current_out = n;
    for i in 0..LANES {
        current_out -= lengths[i];
        out_starts[i] = current_out;
    }

    let min_len = lengths[LANES - 1];

    unsafe {
        for lane in 0..LANES {
            let mut rem = lengths[lane] - min_len;
            let offset = out_starts[lane] + min_len;
            while rem > 0 {
                let val = *t_arr.get_unchecked(p[lane] as usize);
                *chunk_out.get_unchecked_mut(offset + rem - 1) = (val >> 32) as u8;
                p[lane] = (val & 0xFFFF_FFFF) as u32;
                rem -= 1;
            }
        }

        // Using unrolled explicit loop variables rather than an array keeps values
        // in registers and vastly accelerates memory-level parallelism.
        let mut p0 = p[0] as usize;
        let mut p1 = p[1] as usize;
        let mut p2 = p[2] as usize;
        let mut p3 = p[3] as usize;
        let mut p4 = p[4] as usize;
        let mut p5 = p[5] as usize;
        let mut p6 = p[6] as usize;
        let mut p7 = p[7] as usize;

        let out0 = out_starts[0];
        let out1 = out_starts[1];
        let out2 = out_starts[2];
        let out3 = out_starts[3];
        let out4 = out_starts[4];
        let out5 = out_starts[5];
        let out6 = out_starts[6];
        let out7 = out_starts[7];

        for j in (0..min_len).rev() {
            let v0 = *t_arr.get_unchecked(p0);
            *chunk_out.get_unchecked_mut(out0 + j) = (v0 >> 32) as u8;
            p0 = (v0 & 0xFFFF_FFFF) as usize;

            let v1 = *t_arr.get_unchecked(p1);
            *chunk_out.get_unchecked_mut(out1 + j) = (v1 >> 32) as u8;
            p1 = (v1 & 0xFFFF_FFFF) as usize;

            let v2 = *t_arr.get_unchecked(p2);
            *chunk_out.get_unchecked_mut(out2 + j) = (v2 >> 32) as u8;
            p2 = (v2 & 0xFFFF_FFFF) as usize;

            let v3 = *t_arr.get_unchecked(p3);
            *chunk_out.get_unchecked_mut(out3 + j) = (v3 >> 32) as u8;
            p3 = (v3 & 0xFFFF_FFFF) as usize;

            let v4 = *t_arr.get_unchecked(p4);
            *chunk_out.get_unchecked_mut(out4 + j) = (v4 >> 32) as u8;
            p4 = (v4 & 0xFFFF_FFFF) as usize;

            let v5 = *t_arr.get_unchecked(p5);
            *chunk_out.get_unchecked_mut(out5 + j) = (v5 >> 32) as u8;
            p5 = (v5 & 0xFFFF_FFFF) as usize;

            let v6 = *t_arr.get_unchecked(p6);
            *chunk_out.get_unchecked_mut(out6 + j) = (v6 >> 32) as u8;
            p6 = (v6 & 0xFFFF_FFFF) as usize;

            let v7 = *t_arr.get_unchecked(p7);
            *chunk_out.get_unchecked_mut(out7 + j) = (v7 >> 32) as u8;
            p7 = (v7 & 0xFFFF_FFFF) as usize;
        }
    }

    PROF_INV_BWT_TRAVERSAL.fetch_add(t2.elapsed().as_nanos() as u64, Ordering::Relaxed);
    Ok(chunk_out)
}
