use std::io::{Error, ErrorKind, Result};

const TABLE_BITS: u32 = 11;
const TABLE_SIZE: usize = 1 << TABLE_BITS;

#[derive(Copy, Clone)]
struct DecodeEntry {
    symbol: u16,
    nb_bits: u8,
    new_x: u16,
}

#[derive(Copy, Clone)]
struct SymbolMeta {
    k: u8,
    threshold: u16,
    offset: u16,
}

#[derive(Clone)]
struct TansTable {
    decode_table: [DecodeEntry; TABLE_SIZE],
    encode_state_table: [u16; TABLE_SIZE],
    symbols_meta: [SymbolMeta; 257],
    norm: [u16; 257],
}

fn normalize_counts(hist: &[u32; 257]) -> [u16; 257] {
    let mut total: u64 = 0;
    for &h in hist {
        total += h as u64;
    }

    let mut norm = [0u16; 257];
    if total == 0 {
        norm[0] = TABLE_SIZE as u16;
        return norm;
    }

    let mut current_total = 0;
    for i in 0..257 {
        if hist[i] == 0 {
            continue;
        }
        let mut v = ((hist[i] as u64 * TABLE_SIZE as u64) / total) as u16;
        if v == 0 {
            v = 1;
        }
        norm[i] = v;
        current_total += v as i32;
    }

    while current_total < TABLE_SIZE as i32 {
        let mut max_i = 0;
        let mut max_v = 0;
        for i in 0..257 {
            if hist[i] > 0 && norm[i] > max_v {
                max_v = norm[i];
                max_i = i;
            }
        }
        if max_v == 0 {
            for i in 0..257 {
                if hist[i] > 0 {
                    norm[i] += 1;
                    break;
                }
            }
        } else {
            norm[max_i] += 1;
        }
        current_total += 1;
    }

    while current_total > TABLE_SIZE as i32 {
        let mut max_i = 0;
        let mut max_v = 0;
        for i in 0..257 {
            if hist[i] > 0 && norm[i] > max_v {
                max_v = norm[i];
                max_i = i;
            }
        }
        if norm[max_i] > 1 {
            norm[max_i] -= 1;
        }
        current_total -= 1;
    }

    norm
}

fn build_tables(hist: &[u32; 257]) -> TansTable {
    let norm = normalize_counts(hist);

    let mut symbols_meta = [SymbolMeta {
        k: 0,
        threshold: 0,
        offset: 0,
    }; 257];
    let mut offset = 0;
    for s in 0..257 {
        let f = norm[s];
        if f > 0 {
            let k = (TABLE_BITS - f.ilog2()) as u8;
            let threshold = f << k;
            symbols_meta[s] = SymbolMeta {
                k,
                threshold,
                offset,
            };
            offset += f;
        }
    }

    let mut symbol_table = [0xFFFFu16; TABLE_SIZE];
    let step = (TABLE_SIZE >> 1) + (TABLE_SIZE >> 3) + 3;
    let mut pos = 0;
    for s in 0..257 {
        for _ in 0..norm[s] {
            symbol_table[pos] = s as u16;
            pos = (pos + step) & (TABLE_SIZE - 1);
        }
    }

    let mut decode_table = [DecodeEntry {
        symbol: 0,
        nb_bits: 0,
        new_x: 0,
    }; TABLE_SIZE];
    let mut encode_state_table = [0u16; TABLE_SIZE];
    let mut next_freq = [0u32; 257];
    
    for s in 0..257 {
        next_freq[s] = norm[s] as u32;
    }

    for i in 0..TABLE_SIZE {
        let s = symbol_table[i] as usize;
        let x = next_freq[s];
        next_freq[s] += 1;

        let nb_bits = TABLE_BITS - x.ilog2();
        let new_x = (x << nb_bits) - TABLE_SIZE as u32;

        decode_table[i] = DecodeEntry {
            symbol: s as u16,
            nb_bits: nb_bits as u8,
            new_x: new_x as u16,
        };

        let meta_offset = symbols_meta[s].offset;
        let x_offset = x - norm[s] as u32;
        encode_state_table[(meta_offset as u32 + x_offset) as usize] = (i + TABLE_SIZE) as u16;
    }

    TansTable {
        decode_table,
        encode_state_table,
        symbols_meta,
        norm,
    }
}

struct ForwardByteWriter<'a> {
    buf: &'a mut [u8],
    ptr: usize,
    bit_container: u64,
    bits_in_container: u8,
}

impl<'a> ForwardByteWriter<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self {
            buf,
            ptr: 0,
            bit_container: 0,
            bits_in_container: 0,
        }
    }

    fn push_bits(&mut self, val: u16, nb: u8) {
        self.bit_container |= (val as u64) << self.bits_in_container;
        self.bits_in_container += nb;
        while self.bits_in_container >= 8 {
            self.buf[self.ptr] = (self.bit_container & 0xFF) as u8;
            self.ptr += 1;
            self.bit_container >>= 8;
            self.bits_in_container -= 8;
        }
    }

    fn flush(&mut self) -> usize {
        self.bit_container |= 1 << self.bits_in_container;
        self.bits_in_container += 1;
        while self.bits_in_container > 0 {
            self.buf[self.ptr] = (self.bit_container & 0xFF) as u8;
            self.ptr += 1;
            self.bit_container >>= 8;
            self.bits_in_container = self.bits_in_container.saturating_sub(8);
        }
        self.ptr
    }
}

struct BackwardBitReader<'a> {
    buf: &'a [u8],
    ptr: usize,
    bit_container: u64,
    bits_in_container: u8,
}

impl<'a> BackwardBitReader<'a> {
    fn new(buf: &'a [u8]) -> Result<Self> {
        if buf.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "Empty bitstream"));
        }
        
        let mut ptr = buf.len();
        let mut bit_container = 0u64;
        let mut bits = 0;

        while ptr > 0 && bits <= 56 {
            ptr -= 1;
            bit_container = (bit_container << 8) | (buf[ptr] as u64);
            bits += 8;
        }

        if bit_container == 0 {
            return Err(Error::new(ErrorKind::InvalidData, "No marker bit found"));
        }

        let marker = 63 - bit_container.leading_zeros() as u8;
        bits = marker;

        Ok(Self {
            buf,
            ptr,
            bit_container,
            bits_in_container: bits,
        })
    }

    fn pull_bits(&mut self, nb: u8) -> u16 {
        let val = (self.bit_container >> (self.bits_in_container - nb)) as u16;
        let mask = (1 << nb) - 1;
        let result = val & mask;
        self.bits_in_container -= nb;

        while self.bits_in_container <= 56 && self.ptr > 0 {
            self.ptr -= 1;
            self.bit_container = (self.bit_container << 8) | (self.buf[self.ptr] as u64);
            self.bits_in_container += 8;
        }

        result
    }
}

pub struct Order1Tables {
    tables: Box<[TansTable]>,
}

impl Order1Tables {
    pub fn new() -> Self {
        let dummy = TansTable {
            decode_table: [DecodeEntry { symbol: 0, nb_bits: 0, new_x: 0 }; TABLE_SIZE],
            encode_state_table: [0u16; TABLE_SIZE],
            symbols_meta: [SymbolMeta { k: 0, threshold: 0, offset: 0 }; 257],
            norm: [0u16; 257],
        };
        let tables = vec![dummy; 257].into_boxed_slice();
        Self { tables }
    }

    pub fn build(&mut self, hist: &[u32; 257], hist2d: &[[u32; 257]; 256]) {
        for ctx in 0..256 {
            let sum: u32 = hist2d[ctx].iter().sum();
            if sum > 0 {
                self.tables[ctx] = build_tables(&hist2d[ctx]);
            }
        }
        self.tables[256] = build_tables(hist);
    }
}

thread_local! {
    static TANS_POOL_ENCODE: std::cell::RefCell<Order1Tables> = std::cell::RefCell::new(Order1Tables::new());
    static TANS_POOL_DECODE: std::cell::RefCell<Order1Tables> = std::cell::RefCell::new(Order1Tables::new());
}

pub fn encode(hist: &[u32; 257], hist2d: &[[u32; 257]; 256], tokens: &[u16]) -> Vec<u8> {
    let hist_size = 1028;
    let mut out = vec![0u8; hist_size + tokens.len() * 2 + 16];
    
    for i in 0..257 {
        let bytes = hist[i].to_le_bytes();
        out[i * 4..i * 4 + 4].copy_from_slice(&bytes);
    }
    
    if tokens.is_empty() {
        out.truncate(hist_size);
        return out;
    }
    
    TANS_POOL_ENCODE.with(|pool| {
        let mut order1_tables = pool.borrow_mut();
        order1_tables.build(hist, hist2d);
        let mut state = TABLE_SIZE;
        
        let mut writer = ForwardByteWriter::new(&mut out[hist_size..]);
        
        let mut ctxs = Vec::with_capacity(tokens.len());
        let mut prev = 256;
        for &token in tokens {
            ctxs.push(prev);
            if token <= 255 {
                prev = token as usize;
            }
        }
        
        for i in (0..tokens.len()).rev() {
            let token = tokens[i];
            let ctx = ctxs[i];
            let s = token as usize;
            
            let tables = &order1_tables.tables[ctx];
            let meta = &tables.symbols_meta[s];
            
            let nb_bits = if state < meta.threshold as usize {
                meta.k - 1
            } else {
                meta.k
            };
            let bits = (state & ((1 << nb_bits) - 1)) as u16;
            writer.push_bits(bits, nb_bits);
            
            let x = state >> nb_bits;
            let offset = meta.offset as usize + x - tables.norm[s] as usize;
            state = tables.encode_state_table[offset] as usize;
        }
        
        writer.push_bits(state as u16, (TABLE_BITS + 1) as u8);
        let bytes_written = writer.flush();
        
        out.truncate(hist_size + bytes_written);
    });
    
    out
}

pub fn decode(hist: &[u32; 257], hist2d: &[[u32; 257]; 256], len: usize, bytes: &[u8]) -> Result<Vec<u16>> {
    if len == 0 {
        return Ok(Vec::new());
    }
    
    if bytes.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "Empty payload for non-zero length"));
    }
    
    TANS_POOL_DECODE.with(|pool| {
        let mut order1_tables = pool.borrow_mut();
        order1_tables.build(hist, hist2d);
        
        let mut reader = BackwardBitReader::new(bytes)?;
        
        let mut state = reader.pull_bits((TABLE_BITS + 1) as u8) as usize;
        if state >= 2 * TABLE_SIZE || state < TABLE_SIZE {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid initial state"));
        }
        state -= TABLE_SIZE;
        
        let mut tokens = Vec::with_capacity(len);
        let mut prev = 256;
        for _ in 0..len {
            let tables = &order1_tables.tables[prev];
            let entry = &tables.decode_table[state];
            let bits = reader.pull_bits(entry.nb_bits);
            state = entry.new_x as usize + bits as usize;
            let symbol = entry.symbol;
            tokens.push(symbol);
            
            if symbol <= 255 {
                prev = symbol as usize;
            }
        }
        
        Ok(tokens)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tans_roundtrip() {
        let tokens: Vec<u16> = vec![0, 1, 2, 0, 1, 2, 256, 128, 0, 1, 1, 1, 5, 10];
        let mut hist = [0u32; 257];
        let mut hist2d = Box::new([[0u32; 257]; 256]);
        let mut prev = 256;
        for &t in &tokens {
            hist[t as usize] += 1;
            if prev < 256 {
                hist2d[prev][t as usize] += 1;
            }
            if t <= 255 {
                prev = t as usize;
            }
        }

        let encoded = encode(&hist, &*hist2d, &tokens);
        let decoded = decode(&hist, &*hist2d, tokens.len(), &encoded[1028..]).unwrap();
        assert_eq!(tokens, decoded);
    }

    #[test]
    fn test_tans_same_token() {
        let tokens: Vec<u16> = vec![42; 1000];
        let mut hist = [0u32; 257];
        let mut hist2d = Box::new([[0u32; 257]; 256]);
        let mut prev = 256;
        for &t in &tokens {
            hist[t as usize] += 1;
            if prev < 256 {
                hist2d[prev][t as usize] += 1;
            }
            if t <= 255 {
                prev = t as usize;
            }
        }

        let encoded = encode(&hist, &*hist2d, &tokens);
        let decoded = decode(&hist, &*hist2d, tokens.len(), &encoded[1028..]).unwrap();
        assert_eq!(tokens, decoded);
    }

    #[test]
    fn test_tans_empty() {
        let tokens: Vec<u16> = vec![];
        let hist = [0u32; 257];
        let hist2d = Box::new([[0u32; 257]; 256]);

        let encoded = encode(&hist, &*hist2d, &tokens);
        let decoded = decode(&hist, &*hist2d, tokens.len(), &encoded[1028..]).unwrap();
        assert_eq!(tokens, decoded);
    }
}
