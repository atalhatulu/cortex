pub mod rangecoder;
pub mod mtf;
use rayon::prelude::*;
use std::fs;
use std::io::Write;
use std::time::{Duration, Instant};

const BLOCK_SIZE: usize = 16 * 1024 * 1024; // 16 MB blocks for maximum compression (The 24.99 MB Record)

pub struct Stats {
    pub input_size: usize,
    pub output_size: usize,
    pub elapsed: Duration,
    pub chunks: usize,
}

pub fn compress_file(input: &str, output: &str) -> std::io::Result<Stats> {
    let data = fs::read(input)?;
    let start = Instant::now();

    let chunks: Vec<&[u8]> = data.chunks(BLOCK_SIZE).collect();

    let compressed_blocks: Vec<Vec<u8>> = chunks.par_iter().map(|chunk| {
        let (pidx, tokens) = mtf::bwt_mtf_rle(chunk);
        let num_tokens = tokens.len() as u32;

        let mut enc = rangecoder::Encoder::new();
        let mut model = mtf::MtfModel::new();
        model.encode_tokens(&mut enc, &tokens);
        let mut out = enc.finish();

        let mut final_out = Vec::with_capacity(8 + out.len());
        final_out.extend_from_slice(&pidx.to_le_bytes());
        final_out.extend_from_slice(&num_tokens.to_le_bytes());
        final_out.append(&mut out);

        final_out
    }).collect();

    let elapsed = start.elapsed();
    let mut file = fs::File::create(output)?;

    file.write_all(b"CTX4")?;
    file.write_all(&(data.len() as u64).to_le_bytes())?;
    file.write_all(&[0u8])?; // previously order, kept 0 for format compat
    file.write_all(&(BLOCK_SIZE as u32).to_le_bytes())?;

    let mut total_compressed_size = 17;
    for block in &compressed_blocks {
        file.write_all(&(block.len() as u32).to_le_bytes())?;
        file.write_all(block)?;
        total_compressed_size += 4 + block.len();
    }

    Ok(Stats {
        input_size: data.len(),
       output_size: total_compressed_size,
       elapsed,
       chunks: chunks.len(),
    })
}

pub fn decompress_file(input: &str, output: &str) -> std::io::Result<Stats> {
    let raw = fs::read(input)?;
    if raw.len() < 17 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "File too small"));
    }
    if &raw[0..4] != b"CTX4" {
        if &raw[0..4] == b"CTX2" || &raw[0..4] == b"CTX3" || &raw[0..4] == b"CRTX" {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "File compressed with an older Cortex format. Rebuild the stable engine (pre-CM tag) to decompress."));
        }
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid magic bytes"));
    }

    let mut len_bytes = [0u8; 8];
    len_bytes.copy_from_slice(&raw[4..12]);
    let orig_len = u64::from_le_bytes(len_bytes) as usize;

    let _order = raw[12] as usize;

    let mut bs_bytes = [0u8; 4];
    bs_bytes.copy_from_slice(&raw[13..17]);
    let block_size = u32::from_le_bytes(bs_bytes) as usize;

    let mut comp_chunks = Vec::new();
    let mut offset = 17;
    while offset < raw.len() {
        if offset + 4 > raw.len() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Corrupted block headers"));
        }
        let mut cl_bytes = [0u8; 4];
        cl_bytes.copy_from_slice(&raw[offset..offset+4]);
        let chunk_len = u32::from_le_bytes(cl_bytes) as usize;
        offset += 4;

        if offset + chunk_len > raw.len() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Corrupted block data"));
        }
        comp_chunks.push(&raw[offset..offset+chunk_len]);
        offset += chunk_len;
    }

    let chunk_orig_sizes: Vec<usize> = (0..comp_chunks.len()).map(|i| {
        if i == comp_chunks.len() - 1 {
            let rem = orig_len % block_size;
            if rem == 0 { block_size } else { rem }
        } else {
            block_size
        }
    }).collect();

    let start = Instant::now();
    let decompressed_blocks: Vec<Vec<u8>> = comp_chunks.par_iter().zip(chunk_orig_sizes.par_iter()).map(|(chunk, &size)| {
        let pidx = u32::from_le_bytes(chunk[0..4].try_into().unwrap()) as usize;
        let num_tokens = u32::from_le_bytes(chunk[4..8].try_into().unwrap()) as usize;

        let mut dec = rangecoder::Decoder::new(&chunk[8..]);
        let mut model = mtf::MtfModel::new();
        let tokens = model.decode_tokens(&mut dec, num_tokens)?;

        Ok(mtf::decode_rle_mtf_bwt(pidx, &tokens, size))
    }).collect::<Result<Vec<Vec<u8>>, std::io::Error>>()?;

    let elapsed = start.elapsed();

    let mut out_file = fs::File::create(output)?;
    for block in decompressed_blocks {
        out_file.write_all(&block)?;
    }

    Ok(Stats {
        input_size: raw.len(),
       output_size: orig_len,
       elapsed,
       chunks: comp_chunks.len(),
    })
}
