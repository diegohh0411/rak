mod commands;
mod config;
mod history;
mod leitner;
mod stt;

use clap::{Parser, Subcommand};
use commands::{init, log, next, scrape};

#[derive(Parser)]
#[command(name = "rak", about = "Rust Application Killer — internship application workflows")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Bootstrap rak.toml, .env and .gitignore in the current directory
    Init,
    /// Record a problem attempt with a rating
    Log {
        /// LeetCode problem ID
        id: String,
        /// Self-assessed rating (1-5)
        rating: u8,
        /// Replace today's attempt if one already exists
        #[arg(long)]
        force: bool,
    },
    /// Show problems due for review
    Next {
        /// Number of problems to show
        #[arg(short, long, default_value_t = 10)]
        count: usize,
    },
    /// Scrape a URL to markdown via headless Chrome
    Scrape {
        url: String,
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    let result = match cli.command {
        Command::Init => init::run(),
        Command::Log { id, rating, force } => log::run(id, rating, force),
        Command::Next { count } => next::run(count),
        Command::Scrape { url, output } => scrape::run(url, output).await,
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
