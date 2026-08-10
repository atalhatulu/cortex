//! Binary range coder (arithmetic coding on individual bits).
//!
//! This is the entropy-coding backend: given a bit and a probability estimate
//! for that bit, it emits close to the information-theoretic minimum number
//! of bits. The probability estimation itself lives in `mtf.rs` — this
//! module only knows how to turn probabilities into bits and back.
//!
//! Design follows the well-known LZMA-style carryless range coder pattern:
//! a 32-bit range, a wider `low` accumulator to detect and propagate carries,
//! and a byte cache to delay output until a carry can no longer occur.

pub const PROB_BITS: u32 = 12;
pub const PROB_MAX: u16 = 1 << PROB_BITS; // 4096
pub const TOP: u32 = 1 << 24;
pub const ADAPT_RATE: u16 = 5;

/// Encodes a stream of bits into compressed bytes.
pub struct Encoder {
    low: u64,
    range: u32,
    cache: u8,
    cache_size: u64,
    out: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Self {
        Encoder {
            low: 0,
            range: 0xFFFF_FFFF,
            cache: 0xFF,
            cache_size: 1,
            out: Vec::new(),
        }
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder {
    fn shift_low(&mut self) {
        if (self.low as u32) < 0xFF00_0000 || (self.low >> 32) != 0 {
            let carry = (self.low >> 32) as u8;
            let mut temp = self.cache;
            loop {
                self.out.push(temp.wrapping_add(carry));
                temp = 0xFF;
                self.cache_size -= 1;
                if self.cache_size == 0 {
                    break;
                }
            }
            self.cache = (self.low >> 24) as u8;
        }
        self.cache_size += 1;
        self.low = (self.low << 8) & 0xFFFF_FFFF;
    }

    /// Encode a single bit given a mutable probability state (probability
    /// that the bit is 0, scaled to `PROB_BITS`). The probability is updated
    /// in place (adaptive modeling).
    #[inline]
    pub fn encode_bit(&mut self, prob: &mut u16, bit: u8) {
        let bound = (self.range >> PROB_BITS) * (*prob as u32);
        
        let bit_u64 = bit as u64;
        self.low += (bound as u64) * bit_u64;
        
        let range_if_0 = bound;
        let range_if_1 = self.range - bound;
        self.range = if bit == 0 { range_if_0 } else { range_if_1 };

        let p = *prob as i32;
        let target = ((bit as i32) ^ 1) * (PROB_MAX as i32);
        *prob = (p + ((target - p) >> ADAPT_RATE)) as u16;

        while self.range < TOP {
            self.range <<= 8;
            self.shift_low();
        }
    }

    #[inline]
    pub fn encode_bit_fixed(&mut self, prob: u16, bit: u8) {
        let bound = (self.range >> PROB_BITS) * (prob as u32);
        if bit == 0 {
            self.range = bound;
        } else {
            self.low += bound as u64;
            self.range -= bound;
        }
        while self.range < TOP {
            self.range <<= 8;
            self.shift_low();
        }
    }

    /// Flush remaining state and return the compressed byte stream.
    pub fn finish(mut self) -> Vec<u8> {
        for _ in 0..5 {
            self.shift_low();
        }
        self.out
    }
}

/// Decodes bits from a compressed byte stream, mirroring `Encoder` exactly.
pub struct Decoder<'a> {
    range: u32,
    code: u32,
    input: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        let mut code: u32 = 0;
        // The encoder's first emitted byte is always the initial cache (0xFF
        // with a carry that resolves to 0x00 in practice) — skip it, then
        // read 4 bytes to prime `code`.
        let mut pos = 1;
        for _ in 0..4 {
            code = (code << 8) | Self::byte_at(input, &mut pos);
        }
        Decoder {
            range: 0xFFFF_FFFF,
            code,
            input,
            pos,
        }
    }

    fn byte_at(input: &[u8], pos: &mut usize) -> u32 {
        let b = if *pos < input.len() { input[*pos] } else { 0 };
        *pos += 1;
        b as u32
    }

    #[inline]
    pub fn decode_bit(&mut self, prob: &mut u16) -> u8 {
        let bound = (self.range >> PROB_BITS) * (*prob as u32);
        let bit = (self.code >= bound) as u8;
        
        let bound_mask = (bit as u32).wrapping_neg();
        self.code -= bound & bound_mask;
        self.range = if bit == 0 { bound } else { self.range - bound };

        let p = *prob as i32;
        let target = ((bit as i32) ^ 1) * (PROB_MAX as i32);
        *prob = (p + ((target - p) >> ADAPT_RATE)) as u16;

        while self.range < TOP {
            self.range <<= 8;
            self.code = (self.code << 8) | Self::byte_at(self.input, &mut self.pos);
        }
        bit
    }

    #[inline]
    pub fn decode_bit_fixed(&mut self, prob: u16) -> u8 {
        let bound = (self.range >> PROB_BITS) * (prob as u32);
        let bit;
        if self.code < bound {
            self.range = bound;
            bit = 0;
        } else {
            self.code -= bound;
            self.range -= bound;
            bit = 1;
        }
        while self.range < TOP {
            self.range <<= 8;
            self.code = (self.code << 8) | Self::byte_at(self.input, &mut self.pos);
        }
        bit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_bit_roundtrip() {
        for &bit in &[0u8, 1u8] {
            let mut prob = PROB_MAX / 2;
            let mut enc = Encoder::new();
            enc.encode_bit(&mut prob, bit);
            let out = enc.finish();

            let mut prob2 = PROB_MAX / 2;
            let mut dec = Decoder::new(&out);
            let decoded = dec.decode_bit(&mut prob2);
            assert_eq!(decoded, bit);
        }
    }

    #[test]
    fn many_bits_roundtrip() {
        let bits: Vec<u8> = (0..10_000).map(|i: u32| (i.wrapping_mul(2654435761u32) >> 20) as u8 & 1).collect();
        let mut prob = PROB_MAX / 2;
        let mut enc = Encoder::new();
        for &b in &bits {
            enc.encode_bit(&mut prob, b);
        }
        let out = enc.finish();

        let mut prob2 = PROB_MAX / 2;
        let mut dec = Decoder::new(&out);
        for &b in &bits {
            assert_eq!(dec.decode_bit(&mut prob2), b);
        }
    }
}
