pub mod crypto;
pub mod filters;
pub mod mtf;
pub mod rangecoder;
pub mod split_io;

pub const MAGIC: &[u8; 4] = b"CTX8";
pub const MAGIC_FAST: &[u8; 4] = b"CTXF";

use rayon::prelude::*;
use std::fs;
use std::time::{Duration, Instant};

fn get_available_memory_mb() -> Option<usize> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
            for line in contents.lines() {
                if line.starts_with("MemAvailable:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<usize>() {
                            return Some(kb / 1024);
                        }
                    }
                }
            }
        }
    }
    None
}

fn batch_size_for(block_size: usize, _multiplier: usize) -> usize {
    let t = num_cpus::get();

    // Each block uses approx: block_size * 6 (SA + BWT)
    let memory_per_block_mb = (block_size * 6) / (1024 * 1024);

    if let Some(avail_mb) = get_available_memory_mb() {
        // Leave 500MB headroom
        let safe_mb = avail_mb.saturating_sub(500);
        let max_blocks = if memory_per_block_mb > 0 {
            safe_mb / memory_per_block_mb
        } else {
            t
        };
        let final_batch = t.min(std::cmp::max(1, max_blocks));
        final_batch
    } else {
        t
    }
}

/// Single source of truth for the level → block-size mapping.
///
/// Higher levels use larger BWT blocks (more context, better ratio, more
/// memory). Keeping this in one place means the CLI and the GUI can never
/// disagree with the engine about what a given level means. Level 0 is
/// folded into level 1; anything above 9 falls back to level-3 behavior.
pub fn block_size_for_level(level: u8) -> usize {
    match level {
        0..=1 => 1024 * 1024,
        2 => 4 * 1024 * 1024,
        3 => 16 * 1024 * 1024,
        4..=5 => 32 * 1024 * 1024,
        6..=9 => 64 * 1024 * 1024,
        _ => 16 * 1024 * 1024,
    }
}

pub struct Stats {
    pub input_size: usize,
    pub output_size: usize,
    pub elapsed: Duration,
    pub chunks: usize,
}

pub fn compress_file(input: &str, output: &str) -> std::io::Result<Stats> {
    compress_file_with_progress(input, output, None, None, 3, 0, false, |_, _| {})
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
    fast: bool,
    mut callback: F,
) -> std::io::Result<Stats>
where
    F: FnMut(usize, usize),
{
    use split_io::SplitWriter;
    use std::io::{Read, Write};

    let mut in_file = fs::File::open(input)?;
    let start = Instant::now();
    let total_len = in_file.metadata()?.len() as usize;
    callback(0, total_len);

    // Block size is decided here from the level via `block_size_for_level`,
    // the single source of truth. The previous `block_size` parameter was
    // silently ignored and let the GUI/CLI disagree with the engine.
    let block_size_actual = block_size_for_level(_level);

    // Batch size derived from block size via the memory budget (see
    // `batch_size_for`): the biggest blocks run few concurrent chunks so
    // worst-case RSS stays bounded instead of growing without limit.
    let batch_size = batch_size_for(block_size_actual, 10);
    let mut out_file = SplitWriter::new(output, _split_size as u64)?;

    // Crypto
    let encrypted = _password.is_some() && !_password.unwrap().is_empty();
    let flags: u8 = if encrypted { 1 } else { 0 };

    let magic = if fast { MAGIC_FAST } else { MAGIC };
    out_file.write_all(magic)?;
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
                if n == 0 {
                    break;
                }
                bytes_read += n;
            }
            if bytes_read == 0 {
                break;
            }
            buf.truncate(bytes_read);
            batch_data.push(buf);
            if bytes_read < block_size_actual {
                break;
            }
        }

        if batch_data.is_empty() {
            break;
        }

        num_chunks += batch_data.len();
        let batch_processed: usize = batch_data.iter().map(|b| b.len()).sum();

        let compressed_batch: Vec<Vec<u8>> = batch_data
            .par_iter()
            .map(|chunk_in| {
                if fast {
                    zstd::encode_all(chunk_in.as_slice(), 3).unwrap()
                } else {
                    let mut chunk = chunk_in.clone();
                    let is_exec = filters::is_executable(&chunk);
                    if is_exec {
                        filters::e8e9_filter(&mut chunk, true);
                    }

                    let (pidx, tokens) = mtf::bwt_mtf_rle(&chunk);
                    let mut num_tokens = tokens.len() as u32;
                    if is_exec {
                        num_tokens |= 1 << 31;
                    }

                    let mut enc = rangecoder::Encoder::new();
                    let mut model = mtf::MtfModel::new();
                    model.encode_tokens(&mut enc, &tokens);
                    let mut out = enc.finish();

                    let mut final_out = Vec::with_capacity(36 + out.len());
                    for i in 0..mtf::LANES {
                        final_out.extend_from_slice(&pidx[i].to_le_bytes());
                    }
                    final_out.extend_from_slice(&num_tokens.to_le_bytes());
                    final_out.append(&mut out);

                    final_out
                }
            })
            .collect();

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
    use split_io::SplitReader;
    use std::io::{Read, Write};

    let mut in_file = SplitReader::new(input)?;
    // Report the real archive size (all split volumes combined) so stats are
    // truthful. Only used for reporting — never for decode correctness.
    let file_len = in_file.total_size()? as usize;

    let mut header = [0u8; 17];
    in_file.read_exact(&mut header)?;

    let is_ctx8 = &header[0..4] == MAGIC;
    let is_fast = &header[0..4] == MAGIC_FAST;

    if !is_ctx8 && !is_fast {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Not a valid Cortex archive",
        ));
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
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Block size cannot be zero",
        ));
    }

    let mut ml_bytes = [0u8; 4];
    in_file.read_exact(&mut ml_bytes)?;
    let meta_len = u32::from_le_bytes(ml_bytes) as usize;

    let mut crypto_info = None;
    if encrypted {
        if _password.is_none() || _password.unwrap().is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Password required",
            ));
        }
        let mut salt = [0u8; crypto::SALT_LEN];
        let mut nonce = [0u8; crypto::NONCE_LEN];
        in_file.read_exact(&mut salt)?;
        in_file.read_exact(&mut nonce)?;

        let key = crypto::derive_key(_password.unwrap(), &salt);
        crypto_info = Some((key, nonce));
    }

    if meta_len > 0 {
        let mut discard = vec![0u8; meta_len];
        in_file.read_exact(&mut discard)?;
    }

    let start = Instant::now();
    callback(0, orig_len);

    let batch_size = batch_size_for(block_size, 8);
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

            let exp_size = std::cmp::min(
                block_size,
                orig_len.saturating_sub(chunks_read_total * block_size),
            );
            batch_orig_sizes.push(exp_size);
            chunks_read_total += 1;
        }

        if batch_comp_chunks.is_empty() {
            break;
        }

        num_comp_chunks += batch_comp_chunks.len();

        let batch_indices: Vec<u64> =
            (global_chunk_idx..global_chunk_idx + batch_comp_chunks.len() as u64).collect();
        global_chunk_idx += batch_comp_chunks.len() as u64;

        let crypto_ref = crypto_info.as_ref(); // Safe reference for Rayon

        let decompressed_batch: Vec<std::io::Result<Vec<u8>>> = batch_comp_chunks
            .into_par_iter()
            .zip(batch_orig_sizes.into_par_iter())
            .zip(batch_indices.into_par_iter())
            .map(|((mut chunk, size), chunk_idx)| {
                if let Some((key, base_nonce)) = crypto_ref {
                    chunk = match crypto::decrypt_chunk(key, base_nonce, chunk_idx, &chunk) {
                        Ok(d) => d,
                        Err(e) => return Err(e),
                    };
                }

                let decompressed = if is_fast {
                    match zstd::decode_all(chunk.as_slice()) {
                        Ok(d) => d,
                        Err(e) => {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("Zstd Decompression error: {:?}", e),
                            ))
                        }
                    }
                } else {
                    if chunk.len() < (mtf::LANES * 4) + 4 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Chunk too small for headers",
                        ));
                    }
                    let mut pidx = [0u32; mtf::LANES];
                    let mut ptr = 0;
                    for i in 0..mtf::LANES {
                        pidx[i] = u32::from_le_bytes(chunk[ptr..ptr + 4].try_into().unwrap());
                        ptr += 4;
                    }
                    let mut num_tokens =
                        u32::from_le_bytes(chunk[ptr..ptr + 4].try_into().unwrap());
                    ptr += 4;

                    let is_exec = (num_tokens & (1 << 31)) != 0;
                    num_tokens &= !(1 << 31);

                    let mut dec = rangecoder::Decoder::new(&chunk[ptr..]);
                    let mut model = mtf::MtfModel::new();

                    let tokens = match model.decode_tokens(&mut dec, num_tokens as usize) {
                        Ok(t) => t,
                        Err(e) => return Err(e),
                    };

                    let mut block = match mtf::decode_rle_mtf_bwt(pidx, &tokens, size) {
                        Ok(d) => d,
                        Err(e) => return Err(e),
                    };

                    if is_exec {
                        filters::e8e9_filter(&mut block, false);
                    }

                    block
                };

                Ok(decompressed)
            })
            .collect();

        for res in decompressed_batch {
            let block = res?;
            out_file.write_all(&block)?;
            processed += block.len();
        }

        callback(processed, orig_len);
    }

    let elapsed = start.elapsed();
    Ok(Stats {
        input_size: file_len,
        output_size: orig_len,
        elapsed,
        chunks: num_comp_chunks,
    })
}

pub fn read_metadata(input: &str) -> std::io::Result<Option<Vec<u8>>> {
    use split_io::SplitReader;
    use std::io::Read;

    let mut f = SplitReader::new(input)?;
    let mut header = [0u8; 17];
    let n = f.read(&mut header)?;
    if n < 17 {
        return Ok(None);
    }

    let is_ctx8 = &header[0..4] == MAGIC;
    let is_fast = &header[0..4] == MAGIC_FAST;

    if !is_ctx8 && !is_fast {
        return Ok(None);
    }

    let flags = header[12];
    let encrypted = (flags & 1) == 1;

    let mut ml_bytes = [0u8; 4];
    f.read_exact(&mut ml_bytes)?;
    let meta_len = u32::from_le_bytes(ml_bytes) as usize;

    if encrypted {
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
