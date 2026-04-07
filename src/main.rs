mod commands;

use clap::{Parser, Subcommand};
use commands::{init, scrape};

#[derive(Parser)]
#[command(name = "rak", about = "Rust Application Killer — internship app workflows")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Bootstrap .env and .gitignore in the current directory
    Init,
    /// Scrape a URL to markdown via headless Chrome
    Scrape {
        /// URL to scrape
        url: String,
        /// Write output to a file instead of stdout
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    // Best-effort: load .env from current dir, no error if missing
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    let result = match cli.command {
        Command::Init => init::run(),
        Command::Scrape { url, output } => scrape::run(url, output).await,
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
