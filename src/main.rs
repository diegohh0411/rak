/// `eprintln!` unless the TUI has silenced output (see `quiet`).
macro_rules! say {
    ($($arg:tt)*) => {
        if !$crate::quiet::is_quiet() {
            eprintln!($($arg)*);
        }
    };
}

/// `println!` unless the TUI has silenced output (see `quiet`).
macro_rules! out {
    ($($arg:tt)*) => {
        if !$crate::quiet::is_quiet() {
            println!($($arg)*);
        }
    };
}

mod analyzer;
mod commands;
mod config;
mod credentials;
mod history;
mod judge;
mod leetcode;
mod leitner;
mod quiet;
mod recorder;
mod scaffold;
mod stt;
mod tui;

use std::io::{self, IsTerminal};

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
    command: Option<Command>,
    /// Print plain text instead of opening the terminal UI
    #[arg(long, global = true)]
    plain: bool,
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

/// The TUI screen a command opens, when it has one.
fn tui_route(cmd: &Command) -> Option<tui::Route> {
    match cmd {
        Command::Pull {
            qid: None, refresh, ..
        } => Some(tui::Route::Pull { refresh: *refresh }),
        Command::Pull {
            qid: Some(target),
            force,
            ..
        } => Some(tui::Route::PullSlug {
            target: target.clone(),
            force: *force,
        }),
        Command::Record { id, force, resume } => Some(tui::Route::Record {
            id: id.clone(),
            force: *force,
            resume: *resume,
        }),
        Command::Test {
            id,
            case,
            timeout,
            lang,
            debug,
            ..
        } => Some(tui::Route::Test {
            id: id.clone(),
            case: *case,
            timeout_ms: *timeout,
            lang: lang.clone(),
            debug: *debug,
        }),
        Command::Analyze {
            id,
            provider,
            force,
        } => Some(tui::Route::Analyze {
            id: id.clone(),
            provider: provider.clone(),
            force: *force,
        }),
        Command::Push { id } => Some(tui::Route::Push { id: id.clone() }),
        Command::Next { .. } => Some(tui::Route::Next),
        _ => None,
    }
}

fn main() {
    let _ = dotenvy::dotenv();
    stt::init_providers();
    analyzer::init_providers();
    judge::runtime::init_runtimes();

    let cli = Cli::parse();
    let interactive = !cli.plain && io::stdout().is_terminal() && io::stdin().is_terminal();

    let result = match cli.command {
        None if interactive => tui::run(tui::Route::Home),
        None => Cli::command().print_help().map_err(|e| e.to_string()),
        Some(cmd) => match tui_route(&cmd).filter(|_| interactive) {
            Some(route) => tui::run(route),
            None => run_plain(cmd),
        },
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run_plain(command: Command) -> Result<(), String> {
    match command {
        Command::Add { slug } => add::run(slug),
        Command::Init => init::run(),
        Command::Log {
            id,
            rating,
            force,
            date,
        } => log::run(id, rating, force, date),
        Command::Next { count } => next::run(count),
        // Only scraping is async. Everything else uses blocking HTTP, which
        // must not run inside a tokio runtime, so the runtime lives here.
        Command::Scrape { url, output } => tokio::runtime::Runtime::new()
            .map_err(|e| e.to_string())?
            .block_on(scrape::run(url, output)),
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
    }
}
