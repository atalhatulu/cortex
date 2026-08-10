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
        Commands::Compress { input, output, level, password, split, threads, force, quiet, verbose } => {
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
                }
            }
        }
    }

    Ok(())
}
