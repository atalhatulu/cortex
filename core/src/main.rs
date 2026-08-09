mod cli;

use clap::Parser;
use cli::{Cli, Commands};
use cortex::{compress_file, decompress_file};

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compress { input, output } => {
            let out_file = output.unwrap_or_else(|| format!("{}.crx", input));
            let stats = compress_file(&input, &out_file)?;
            let ratio = stats.output_size as f64 / stats.input_size.max(1) as f64 * 100.0;
            let speed = stats.input_size as f64 / 1_000_000.0 / stats.elapsed.as_secs_f64().max(1e-9);
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
        Commands::Decompress { input, output, .. } => {
            let out_file = output.unwrap_or_else(|| {
                if input.ends_with(".crx") {
                    input[..input.len()-4].to_string()
                } else {
                    format!("{}.out", input)
                }
            });
            let stats = decompress_file(&input, &out_file)?;
            println!(
                "decompressed {} -> {} bytes in {:.2}s using {} blocks",
                stats.input_size,
                stats.output_size,
                stats.elapsed.as_secs_f64(),
                     stats.chunks
            );
        }
    }

    Ok(())
}
