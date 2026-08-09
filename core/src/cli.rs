use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cortex", about = "Experimental lossless text compressor")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Compress a file
    Compress {
        input: String,
        output: Option<String>,
    },
    /// Decompress a file
    Decompress {
        input: String,
        output: Option<String>,
    },
}
