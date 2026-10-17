mod analyzer;
mod commands;
mod config;
mod credentials;
mod history;
mod judge;
mod leetcode;
mod leitner;
mod recorder;
mod scaffold;
mod stt;

use clap::{CommandFactory, Parser, Subcommand};
use commands::{
    add, analyze, init, log, login, next, pull, push, record, scrape, test, transcribe,
};

#[derive(Parser)]
#[command(
    name = "rak",
    version,
    about = "Rust Application Killer — internship application workflows"
)]
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
        /// Replace the attempt if one already exists for the given date
        #[arg(long)]
        force: bool,
        /// Date of the attempt (YYYY-MM-DD). Defaults to today.
        #[arg(short, long)]
        date: Option<String>,
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
        /// Append the new audio to the latest attempt instead of starting a new one
        #[arg(long, short, conflicts_with = "force")]
        resume: bool,
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
        /// Re-scaffold even if the problem folder already exists (overwrites solution.cpp, testcases.txt, question.md)
        #[arg(long, short)]
        force: bool,
    },
    /// Run the solution against testcases.txt locally
    Test {
        /// LeetCode problem ID (numeric or slug)
        id: String,
        /// Test locally (the default; accepted for leetgo compatibility)
        #[arg(long, short = 'L')]
        local: bool,
        /// Only run test case N (numbered from 1)
        #[arg(long, short)]
        case: Option<usize>,
        /// Per-case time limit in milliseconds (overrides [test] timeout_ms)
        #[arg(long, short, value_name = "MS")]
        timeout: Option<u64>,
        /// Language to test when the folder has solutions in several (e.g. cpp)
        #[arg(long)]
        lang: Option<String>,
        /// Build with sanitizers and debug info to pinpoint crashes (slower)
        #[arg(long, short)]
        debug: bool,
    },
    /// Submit solution to LeetCode and display the result
    Push {
        /// LeetCode problem ID (numeric or slug)
        id: String,
    },
    /// Configure credentials for a provider
    Login {
        /// Provider to configure (e.g. chirp, elevenlabs, openrouter)
        provider: Option<String>,
    },
    /// Generate shell completion script
    Completions {
        /// Shell to generate completions for
        #[arg(short, long, default_value = "bash")]
        shell: String,
    },
}

pub fn build_cli() -> clap::Command {
    Cli::command()
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    stt::init_providers();
    analyzer::init_providers();
    judge::runtime::init_runtimes();

    let cli = Cli::parse();

    let result = match cli.command {
        Command::Add { slug } => add::run(slug),
        Command::Init => init::run(),
        Command::Log {
            id,
            rating,
            force,
            date,
        } => log::run(id, rating, force, date),
        Command::Next { count } => next::run(count),
        Command::Scrape { url, output } => scrape::run(url, output).await,
        Command::Record { id, force, resume } => record::run(id, force, resume),
        Command::Transcribe {
            id,
            provider,
            force,
        } => transcribe::run(id, provider, force),
        Command::Analyze {
            id,
            provider,
            force,
        } => analyze::run(id, provider, force),
        Command::Pull {
            qid,
            refresh,
            force,
        } => pull::run(qid, refresh, force),
        Command::Test {
            id,
            local: _,
            case,
            timeout,
            lang,
            debug,
        } => test::run(id, case, timeout, lang, debug),
        Command::Push { id } => push::run(id),
        Command::Login { provider } => login::run(provider),
        Command::Completions { shell } => commands::completions::run(&shell),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
