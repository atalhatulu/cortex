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
        /// The output file path (optional, defaults to <input>.ctx)
        output: Option<String>,
        /// Compression level. BWT mode: 1=1MB 2=4MB 3=16MB 9=64MB blocks. Zstd fast mode: zstd compression level (default 19).
        #[arg(short, long)]
        level: Option<u8>,
        /// Password for encryption (optional)
        #[arg(short, long)]
        password: Option<String>,
        /// Use Fast Mode (LZ + rANS) instead of Maximum Mode (BWT + CM)
        #[arg(long, default_value_t = false)]
        fast: bool,
        /// Use BWT+tANS balance mode (CTXT) instead of BWT+range-coder (CTX8) or Zstd fast mode (CTXF)
        #[arg(long, default_value_t = false)]
        tans: bool,
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
