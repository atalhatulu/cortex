use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cortex", about = "Experimental lossless text compressor")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Compress a file losslessly
    Compress {
        /// The input file to compress
        input: String,
        /// The output file path (optional, defaults to <input>.crx)
        output: Option<String>,
        /// Compression level (1: Fast, 2: Balanced, 3: Ultra)
        #[arg(short, long, default_value_t = 3)]
        level: u8,
        /// Password for encryption (optional)
        #[arg(short, long)]
        password: Option<String>,
        /// Split archive into volumes of given size in MB (0 to disable)
        #[arg(short, long, default_value_t = 0)]
        split: usize,
        /// Maximum number of threads to use (0 for all available cores)
        #[arg(short, long, default_value_t = 0)]
        threads: usize,
        /// Overwrite output file if it already exists
        #[arg(short, long, default_value_t = false)]
        force: bool,
        /// Suppress progress bar and normal output
        #[arg(short, long, default_value_t = false)]
        quiet: bool,
        /// Show detailed statistics after completion
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
    /// Decompress a file
    Decompress {
        /// The input .crx file to decompress
        input: String,
        /// The output file path (optional, defaults to original name or <input>.out)
        output: Option<String>,
        /// Password for decryption (if the archive is encrypted)
        #[arg(short, long)]
        password: Option<String>,
        /// Maximum number of threads to use (0 for all available cores)
        #[arg(short, long, default_value_t = 0)]
        threads: usize,
        /// Overwrite output file if it already exists
        #[arg(short, long, default_value_t = false)]
        force: bool,
        /// Suppress progress bar and normal output
        #[arg(short, long, default_value_t = false)]
        quiet: bool,
        /// Show detailed statistics after completion
        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
}
