mod cli;

use clap::Parser;
use cli::{Cli, Commands};
use cortex::{compress_file_with_progress, decompress_file_with_progress};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;

fn setup_threads(threads: usize) {
    if threads > 0 {
        if let Err(e) = rayon::ThreadPoolBuilder::new().num_threads(threads).build_global() {
            eprintln!("Warning: Failed to set thread pool size: {}", e);
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / 1048576.0)
    } else {
        format!("{:.2} GB", bytes as f64 / 1073741824.0)
    }
}

fn check_force(out_file: &str, force: bool) -> std::io::Result<()> {
    if !force && Path::new(out_file).exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("Output file '{}' already exists. Use --force to overwrite.", out_file)
        ));
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compress { input, output, level, password, split, threads, force, quiet, verbose, fast } => {
            setup_threads(threads);
            let out_file = output.unwrap_or_else(|| format!("{}.crx", input));
            check_force(&out_file, force)?;
            
            let pb = if quiet {
                ProgressBar::hidden()
            } else {
                let p = ProgressBar::new(0);
                p.set_style(ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})").unwrap()
                    .progress_chars("#>-"));
                p
            };
            
            let mut init = false;
            let pwd_ref = password.as_deref();

            // `split` comes from the CLI in MB (see cli.rs). The library API
            // `compress_file_with_progress` expects split_size in bytes, so we
            // convert here. saturating_mul guards against absurd MB values.
            let split_bytes = (split as u64).saturating_mul(1024 * 1024) as usize;

            let stats = compress_file_with_progress(
                &input,
                &out_file,
                None, // metadata
                pwd_ref, // password
                level, // level
                split_bytes, // split_size (bytes)
                fast, // fast mode
                |processed, total| {
                    if !quiet {
                        if !init {
                            pb.set_length(total as u64);
                            init = true;
                        }
                        pb.set_position(processed as u64);
                    }
                }
            )?;
            
            if !quiet {
                pb.finish_and_clear();
            }
            
            if !quiet || verbose {
                let ratio = stats.output_size as f64 / stats.input_size.max(1) as f64 * 100.0;
                let speed = stats.input_size as f64 / 1_000_000.0 / stats.elapsed.as_secs_f64().max(1e-9);
                if verbose {
                    println!("--- Cortex Compression Stats ---");
                    println!("Input:           {} bytes", stats.input_size);
                    println!("Output:          {} bytes", stats.output_size);
                    println!("Ratio:           {:.2}%", ratio);
                    println!("Time:            {:.2} s", stats.elapsed.as_secs_f64());
                    println!("Speed:           {:.2} MB/s", speed);
                    println!("Blocks:          {}", stats.chunks);
                } else {
                    println!(
                        "compressed {} -> {} bytes ({:.2}%) in {:.2}s ({:.2} MB/s) using {} blocks",
                             stats.input_size,
                             stats.output_size,
                             ratio,
                             stats.elapsed.as_secs_f64(),
                             speed,
                             stats.chunks
                    );
                }
            }
        }
        Commands::Decompress { input, output, password, threads, force, quiet, verbose } => {
            setup_threads(threads);
            let out_file = output.unwrap_or_else(|| {
                let mut stem = input.to_string();
                let mut changed = false;
                // Normalize a split-volume path (archive.crx.001, archive.crx.002, …)
                // to its base name so restore produces the original filename.
                if stem.len() >= 4
                    && stem.as_bytes()[stem.len() - 4] == b'.'
                    && stem.as_bytes()[stem.len() - 3..].iter().all(u8::is_ascii_digit)
                {
                    stem.truncate(stem.len() - 4);
                    changed = true;
                }
                if stem.ends_with(".crx") {
                    stem.truncate(stem.len() - 4);
                    changed = true;
                }
                if changed && !stem.is_empty() {
                    stem
                } else {
                    format!("{}.out", input)
                }
            });
            check_force(&out_file, force)?;
            
            let pb = if quiet {
                ProgressBar::hidden()
            } else {
                let p = ProgressBar::new(0);
                p.set_style(ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})").unwrap()
                    .progress_chars("#>-"));
                p
            };
            
            let mut init = false;
            let pwd_ref = password.as_deref();

            let stats = decompress_file_with_progress(
                &input, 
                &out_file,
                pwd_ref, // password
                |processed, total| {
                    if !quiet {
                        if !init {
                            pb.set_length(total as u64);
                            init = true;
                        }
                        pb.set_position(processed as u64);
                    }
                }
            )?;
            
            if !quiet {
                pb.finish_and_clear();
            }

            if !quiet || verbose {
                let speed = stats.output_size as f64 / 1_000_000.0 / stats.elapsed.as_secs_f64().max(1e-9);
                if verbose {
                    println!("--- Cortex Decompression Stats ---");
                    println!("Input:           {} bytes", stats.input_size);
                    println!("Output:          {} bytes", stats.output_size);
                    println!("Time:            {:.2} s", stats.elapsed.as_secs_f64());
                    println!("Speed:           {:.2} MB/s", speed);
                    println!("Blocks:          {}", stats.chunks);
                } else {
                    println!(
                        "decompressed {} -> {} bytes in {:.2}s using {} blocks",
                        stats.input_size,
                        stats.output_size,
                        stats.elapsed.as_secs_f64(),
                        stats.chunks
                    );

                    let entropy_s = cortex::mtf::PROF_ENTROPY.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9;
                    let mtf_rle_s = cortex::mtf::PROF_MTF_RLE.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9;
                    let tarr_s = cortex::mtf::PROF_INV_BWT_TARR.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9;
                    let invbwt_s = cortex::mtf::PROF_INV_BWT_TRAVERSAL.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9;
                    let sum = entropy_s + mtf_rle_s + tarr_s + invbwt_s;
                    println!("\n========================================");
                    println!("CORTEX DECODE PROFILE (Sum across threads)");
                    println!("========================================");
                    println!("{:<30} {:.2} s", "Entropy decode & Context", entropy_s);
                    println!("{:<30} {:.2} s", "MTF + RLE", mtf_rle_s);
                    println!("{:<30} {:.2} s", "Inverse BWT (t_arr build)", tarr_s);
                    println!("{:<30} {:.2} s", "Inverse BWT (traversal)", invbwt_s);
                    println!("{:<30} {:.2} s", "Subtotal Thread CPU Time", sum);
                    println!("========================================");
                }
            }
        }
        Commands::Info { input } => {
            use std::io::Read;

            let mut file = cortex::split_io::SplitReader::new(&input)?;

            let mut header = [0u8; 17];
            file.read_exact(&mut header)?;

            let is_ctx6 = &header[0..4] == b"CTX6";
            let is_ctx5 = &header[0..4] == b"CTX5";
            let is_ctx4 = &header[0..4] == b"CTX4";

            if !is_ctx6 && !is_ctx5 && !is_ctx4 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid or outdated magic bytes",
                ));
            }

            let format = if is_ctx6 {
                "CTX6"
            } else if is_ctx5 {
                "CTX5"
            } else {
                "CTX4"
            };

            let mut len_bytes = [0u8; 8];
            len_bytes.copy_from_slice(&header[4..12]);
            let orig_len = u64::from_le_bytes(len_bytes);

            let flags = header[12];
            let encrypted = (flags & 1) == 1;

            let mut bs_bytes = [0u8; 4];
            bs_bytes.copy_from_slice(&header[13..17]);
            let block_size = u32::from_le_bytes(bs_bytes) as u64;

            let mut meta_len: u64 = 0;
            if is_ctx5 || is_ctx6 {
                let mut ml_bytes = [0u8; 4];
                file.read_exact(&mut ml_bytes)?;
                meta_len = u32::from_le_bytes(ml_bytes) as u64;
            }

            if is_ctx6 && encrypted {
                let mut salt_nonce = [0u8; 28]; // salt + nonce
                file.read_exact(&mut salt_nonce)?;
            }

            println!("Format:        {}", format);
            println!("Encrypted:     {}", if encrypted { "yes" } else { "no" });
            println!("Original size: {} bytes ({})", orig_len, format_bytes(orig_len));
            println!("Block size:    {} bytes ({})", block_size, format_bytes(block_size));
            println!("Metadata:      {} bytes", meta_len);
        }
        Commands::Test { input } => {
            // Unique temp directory: system temp + pid + a random-looking nanos suffix.
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0);
            let tmp_dir = std::env::temp_dir()
                .join(format!("cortex-test-{}-{}", std::process::id(), nanos));
            std::fs::create_dir_all(&tmp_dir)?;
            let crx_path = tmp_dir.join("test.crx");
            let out_path = tmp_dir.join("test.out");

            let ok = (|| -> std::io::Result<bool> {
                let crx = crx_path.to_string_lossy();
                let out = out_path.to_string_lossy();
                cortex::compress_file(&input, &crx)?;
                cortex::decompress_file(&crx, &out)?;

                let orig = std::fs::read(&input)?;
                let round = std::fs::read(&out_path)?;

                // Fast path: differing sizes can never roundtrip byte-exact.
                if orig.len() != round.len() {
                    println!(
                        "FAIL: {} differs after roundtrip (size mismatch: {} vs {} bytes)",
                        input,
                        orig.len(),
                        round.len()
                    );
                    return Ok(false);
                }

                match orig.iter().zip(round.iter()).position(|(a, b)| a != b) {
                    Some(offset) => {
                        println!(
                            "FAIL: {} differs after roundtrip (first diff at byte {})",
                            input, offset
                        );
                        Ok(false)
                    }
                    None => {
                        println!("PASS: {} roundtripped byte-exact ({} bytes)", input, orig.len());
                        Ok(true)
                    }
                }
            })();

            // Clean up temp files in every case (pass, fail, error).
            let _ = std::fs::remove_dir_all(&tmp_dir);

            if !ok? {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
