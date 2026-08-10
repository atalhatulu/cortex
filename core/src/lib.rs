pub mod rangecoder;
pub mod mtf;
pub mod crypto;
pub mod split_io;

use rayon::prelude::*;
use std::fs;
use std::time::{Duration, Instant};

const BLOCK_SIZE: usize = 16 * 1024 * 1024;

pub struct Stats {
    pub input_size: usize,
    pub output_size: usize,
    pub elapsed: Duration,
    pub chunks: usize,
}

pub fn compress_file(input: &str, output: &str) -> std::io::Result<Stats> {
    compress_file_with_progress(input, output, None, None, 3, 0, BLOCK_SIZE, |_, _| {})
}

pub fn decompress_file(input: &str, output: &str) -> std::io::Result<Stats> {
    decompress_file_with_progress(input, output, None, |_, _| {})
}

pub fn compress_file_with_progress<F>(
    input: &str,
    output: &str,
    _metadata: Option<&[u8]>,
    _password: Option<&str>,
    _level: u8,
    _split_size: usize,
    _block_size: usize,
    mut callback: F,
) -> std::io::Result<Stats>
where
    F: FnMut(usize, usize),
{
    use std::io::{Read, Write};
    use split_io::SplitWriter;
    
    let mut in_file = fs::File::open(input)?;
    let start = Instant::now();
    let total_len = in_file.metadata()?.len() as usize;
    callback(0, total_len);

    // Map _level (0..=9) to block_size. Level 3 = 16MB. Level 1 = 1MB. Level 9 = 64MB.
    let block_size_actual = match _level {
        0..=2 => 1 * 1024 * 1024,
        3..=5 => 16 * 1024 * 1024,
        6..=9 => 64 * 1024 * 1024,
        _ => 16 * 1024 * 1024,
    };

    let batch_size = 16;
    let mut out_file = SplitWriter::new(output, _split_size as u64)?;

    // Crypto
    let encrypted = _password.is_some() && !_password.unwrap().is_empty();
    let flags: u8 = if encrypted { 1 } else { 0 };

    out_file.write_all(b"CTX6")?;
    out_file.write_all(&(total_len as u64).to_le_bytes())?;
    out_file.write_all(&[flags])?;
    out_file.write_all(&(block_size_actual as u32).to_le_bytes())?;

    let meta_len = _metadata.map(|m| m.len() as u32).unwrap_or(0);
    out_file.write_all(&meta_len.to_le_bytes())?;
    
    let mut crypto_info = None;
    if encrypted {
        let (salt, nonce) = crypto::generate_salt_and_nonce();
        out_file.write_all(&salt)?;
        out_file.write_all(&nonce)?;
        let key = crypto::derive_key(_password.unwrap(), &salt);
        crypto_info = Some((key, nonce));
    }

    if let Some(meta_bytes) = _metadata {
        out_file.write_all(meta_bytes)?;
    }

    let mut total_compressed_size = 21 + meta_len as usize + if encrypted { 28 } else { 0 };
    let mut num_chunks = 0;
    let mut processed = 0;
    let mut global_chunk_idx: u64 = 0;

    loop {
        let mut batch_data = Vec::new();
        for _ in 0..batch_size {
            let mut buf = vec![0u8; block_size_actual];
            let mut bytes_read = 0;
            while bytes_read < block_size_actual {
                let n = in_file.read(&mut buf[bytes_read..])?;
                if n == 0 { break; }
                bytes_read += n;
            }
            if bytes_read == 0 { break; }
            buf.truncate(bytes_read);
            batch_data.push(buf);
            if bytes_read < block_size_actual { break; }
        }

        if batch_data.is_empty() {
            break;
        }

        num_chunks += batch_data.len();
        let batch_processed: usize = batch_data.iter().map(|b| b.len()).sum();

        let compressed_batch: Vec<Vec<u8>> = batch_data.par_iter().map(|chunk| {
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

        for block in compressed_batch {
            let block_to_write = if let Some((ref key, ref base_nonce)) = crypto_info {
                crypto::encrypt_chunk(key, base_nonce, global_chunk_idx, &block)?
            } else {
                block
            };

            out_file.write_all(&(block_to_write.len() as u32).to_le_bytes())?;
            out_file.write_all(&block_to_write)?;
            total_compressed_size += 4 + block_to_write.len();
            global_chunk_idx += 1;
        }

        processed += batch_processed;
        callback(processed, total_len);
    }

    let elapsed = start.elapsed();
    Ok(Stats {
        input_size: total_len,
        output_size: total_compressed_size,
        elapsed,
        chunks: num_chunks,
    })
}

pub fn decompress_file_with_progress<F>(
    input: &str,
    output: &str,
    _password: Option<&str>,
    mut callback: F,
) -> std::io::Result<Stats>
where
    F: FnMut(usize, usize),
{
    use std::io::{Read, Write, Seek};
    use split_io::SplitReader;
    
    let mut in_file = SplitReader::new(input)?;
    let file_len = 0; // Not critical for output
    
    let mut header = vec![0u8; 17];
    in_file.read_exact(&mut header)?;

    let is_ctx6 = &header[0..4] == b"CTX6";
    let is_ctx5 = &header[0..4] == b"CTX5";
    let is_ctx4 = &header[0..4] == b"CTX4";

    if !is_ctx6 && !is_ctx5 && !is_ctx4 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid or outdated magic bytes"));
    }

    let mut len_bytes = [0u8; 8];
    len_bytes.copy_from_slice(&header[4..12]);
    let orig_len = u64::from_le_bytes(len_bytes) as usize;

    let flags = header[12];
    let encrypted = (flags & 1) == 1;

    let mut bs_bytes = [0u8; 4];
    bs_bytes.copy_from_slice(&header[13..17]);
    let block_size = u32::from_le_bytes(bs_bytes) as usize;
    if block_size == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Block size cannot be zero"));
    }

    let mut meta_len = 0;
    if is_ctx5 || is_ctx6 {
        let mut ml_bytes = [0u8; 4];
        in_file.read_exact(&mut ml_bytes)?;
        meta_len = u32::from_le_bytes(ml_bytes) as usize;
    }

    let mut crypto_info = None;
    if is_ctx6 && encrypted {
        if _password.is_none() || _password.unwrap().is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Password required"));
        }
        let mut salt = [0u8; crypto::SALT_LEN];
        let mut nonce = [0u8; crypto::NONCE_LEN];
        in_file.read_exact(&mut salt)?;
        in_file.read_exact(&mut nonce)?;
        
        let key = crypto::derive_key(_password.unwrap(), &salt);
        crypto_info = Some((key, nonce));
    } else if encrypted {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Encrypted format mismatch"));
    }

    if meta_len > 0 {
        let mut discard = vec![0u8; meta_len];
        in_file.read_exact(&mut discard)?;
    }

    let start = Instant::now();
    callback(0, orig_len);

    let batch_size = 16;
    let mut out_file = fs::File::create(output)?;
    let mut num_comp_chunks = 0;
    let mut processed = 0;
    let mut chunks_read_total = 0;
    let mut global_chunk_idx: u64 = 0;

    loop {
        let mut batch_comp_chunks = Vec::new();
        let mut batch_orig_sizes = Vec::new();

        for _ in 0..batch_size {
            let mut cl_bytes = [0u8; 4];
            if let Err(e) = in_file.read_exact(&mut cl_bytes) {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    break;
                }
                return Err(e);
            }
            let chunk_len = u32::from_le_bytes(cl_bytes) as usize;

            let mut comp_data = vec![0u8; chunk_len];
            in_file.read_exact(&mut comp_data)?;

            batch_comp_chunks.push(comp_data);

            let exp_size = std::cmp::min(block_size, orig_len.saturating_sub(chunks_read_total * block_size));
            batch_orig_sizes.push(exp_size);
            chunks_read_total += 1;
        }

        if batch_comp_chunks.is_empty() {
            break;
        }

        num_comp_chunks += batch_comp_chunks.len();

        let batch_indices: Vec<u64> = (global_chunk_idx .. global_chunk_idx + batch_comp_chunks.len() as u64).collect();
        global_chunk_idx += batch_comp_chunks.len() as u64;
        
        let crypto_ref = crypto_info.as_ref(); // Safe reference for Rayon

        let decompressed_batch: Vec<std::io::Result<Vec<u8>>> = batch_comp_chunks.into_par_iter()
            .zip(batch_orig_sizes.into_par_iter())
            .zip(batch_indices.into_par_iter())
            .map(|((mut chunk, size), chunk_idx)| {
            
            if let Some((key, base_nonce)) = crypto_ref {
                chunk = match crypto::decrypt_chunk(key, base_nonce, chunk_idx, &chunk) {
                    Ok(d) => d,
                    Err(e) => return Err(e),
                };
            }

            if chunk.len() < 8 {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Chunk too small for headers"));
            }
            let pidx = u32::from_le_bytes(chunk[0..4].try_into().unwrap()) as usize;
            let num_tokens = u32::from_le_bytes(chunk[4..8].try_into().unwrap()) as usize;

            let mut dec = rangecoder::Decoder::new(&chunk[8..]);
            let mut model = mtf::MtfModel::new();

            let tokens = match model.decode_tokens(&mut dec, num_tokens) {
                Ok(t) => t,
                Err(e) => return Err(e),
            };

            let decompressed = match mtf::decode_rle_mtf_bwt(pidx, &tokens, size) {
                Ok(d) => d,
                Err(e) => return Err(e),
            };

            Ok(decompressed)
        }).collect();

        for res in decompressed_batch {
            let block = res?;
            out_file.write_all(&block)?;
            processed += block.len();
        }

        callback(processed, orig_len);
    }

    let elapsed = start.elapsed();
    Ok(Stats {
        input_size: file_len as usize,
        output_size: orig_len,
        elapsed,
        chunks: num_comp_chunks,
    })
}

pub fn read_metadata(input: &str) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read;
    use split_io::SplitReader;
    
    let mut f = SplitReader::new(input)?;
    let mut header = [0u8; 17];
    let n = f.read(&mut header)?;
    if n < 17 {
        return Ok(None);
    }
    
    let is_ctx6 = &header[0..4] == b"CTX6";
    let is_ctx5 = &header[0..4] == b"CTX5";
    
    if !is_ctx6 && !is_ctx5 {
        return Ok(None);
    }

    let flags = header[12];
    let encrypted = (flags & 1) == 1;

    let mut ml_bytes = [0u8; 4];
    f.read_exact(&mut ml_bytes)?;
    let meta_len = u32::from_le_bytes(ml_bytes) as usize;

    if is_ctx6 && encrypted {
        let mut discard = [0u8; 28]; // salt + nonce
        f.read_exact(&mut discard)?;
    }

    if meta_len > 0 {
        let mut meta_data = vec![0u8; meta_len];
        f.read_exact(&mut meta_data)?;
        return Ok(Some(meta_data));
    }

    Ok(None)
}
