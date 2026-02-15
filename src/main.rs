use anyhow::Result;
use clap::Parser;

pub use ludwig::frame;
pub use ludwig::mark;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// File to edit
    #[arg(value_name = "FILE")]
    file: Option<String>,

    /// Create a file if it does not exist
    #[arg(short = 'c', long)]
    create: bool,

    /// No initialisation file
    #[arg(short = 'I', long)]
    init_stdin: bool,

    /// Initialisation file
    #[arg(short = 'i', long, value_name = "FILE")]
    init: Option<String>,

    /// Batch mode
    #[arg(short = 'M', long)]
    batch: bool,

    /// Use new command names
    #[arg(short = 'O', long)]
    new_cmds: bool,

    /// Open in read-only mode
    #[arg(short = 'r', long)]
    read_only: bool,

}

fn main() -> Result<()> {
    // Yet to be implemented
    let args = Args::parse();
    println!("{:?}", args);
    Ok(())
}
