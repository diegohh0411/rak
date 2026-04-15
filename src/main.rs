mod commands;
mod config;
mod leetcode;
mod recorder;
mod history;
mod leitner;
mod stt;
mod analyzer;

use clap::{Parser, Subcommand};
use commands::{add, analyze, init, log, next, pull, push, record, scrape, transcribe};

#[derive(Parser)]
#[command(name = "rak", about = "Rust Application Killer — internship application workflows")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add a custom interview problem folder
    Add {
        /// Problem slug (e.g. c3ai-strings-and-targets)
        slug: String,
    },
    /// Bootstrap rak.toml, .env and .gitignore in the current directory
    Init,
    /// Record a problem attempt with a rating
    Log {
        /// LeetCode problem ID
        id: String,
        /// Self-assessed rating (1-5):
        ///   5 — Solved perfectly, no issues
        ///   4 — Solved with minor hesitation
        ///   3 — Solved but had to consult external syntax reference
        ///   2 — Struggled significantly, needed major help
        ///   1 — Couldn't solve it without AI/Web giving me the answer
        #[arg(verbatim_doc_comment)]
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
    /// Record a voice note for a problem attempt
    Record {
        /// LeetCode problem ID
        id: String,
        /// Restart numbering from attempt-1
        #[arg(long, short)]
        force: bool,
    },
    /// Transcribe voice note recordings for a problem
    Transcribe {
        /// LeetCode problem ID
        id: String,
        /// Override default transcription provider
        #[arg(long, short)]
        provider: Option<String>,
        /// Re-transcribe all recordings even if .md exists
        #[arg(long, short)]
        force: bool,
    },
    /// Analyze problem solution using AI
    Analyze {
        /// LeetCode problem ID
        id: String,
        /// Override default analysis provider
        #[arg(long, short)]
        provider: Option<String>,
        /// Overwrite existing analysis.md
        #[arg(long, short)]
        force: bool,
    },
    /// Fetch a LeetCode problem and scaffold it locally
    Pull {
        /// Problem ID (numeric), slug, or "today". Omit for interactive TUI.
        qid: Option<String>,
        /// Force-refresh the problem list cache
        #[arg(long, short)]
        refresh: bool,
    },
    /// Submit solution to LeetCode and display the result
    Push {
        /// LeetCode problem ID (numeric or slug)
        id: String,
    },
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    stt::init_providers();
    analyzer::init_providers();

    let cli = Cli::parse();

    let result = match cli.command {
        Command::Add { slug } => add::run(slug),
        Command::Init => init::run(),
        Command::Log { id, rating, force } => log::run(id, rating, force),
        Command::Next { count } => next::run(count),
        Command::Scrape { url, output } => scrape::run(url, output).await,
        Command::Record { id, force } => record::run(id, force),
        Command::Transcribe { id, provider, force } => transcribe::run(id, provider, force),
        Command::Analyze {
            id,
            provider,
            force,
        } => analyze::run(id, provider, force),
        Command::Pull { qid, refresh } => pull::run(qid, refresh),
        Command::Push { id } => push::run(id),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
