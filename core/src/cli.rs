use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "cortex",
    version,
    about = "Lossless archiver — Balanced (CTXT) by default; --ratio for max compression, --fast for max speed"
)]
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
        /// The output file path (optional, defaults to <input>.ctx)
        output: Option<String>,
        /// BWT block level: 1=1MB 2=4MB 3=16MB 9=64MB (larger = better ratio, slower). Zstd fast mode: zstd level (default 19).
        #[arg(short, long)]
        level: Option<u8>,
        /// Password for encryption (optional)
        #[arg(short, long)]
        password: Option<String>,
        /// Fast mode (CTXF, zstd): max decompress speed, lower ratio
        #[arg(long, default_value_t = false)]
        fast: bool,
        /// Max compression ratio mode (CTX8, BWT + range coder). Default is Balanced (CTXT).
        #[arg(long, default_value_t = false)]
        ratio: bool,
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
        /// The input .ctx file to decompress
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
    /// Show archive header information
    Info {
        /// The input .ctx file to inspect
        input: String,
    },
    /// Verify a file survives a compress/decompress roundtrip
    Test {
        /// The input file to verify
        input: String,
    },
}
