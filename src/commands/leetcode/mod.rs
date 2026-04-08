pub mod log;
pub mod next;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum LeetcodeCommand {
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
}

pub async fn run(cmd: LeetcodeCommand) -> Result<(), String> {
    match cmd {
        LeetcodeCommand::Log { id, rating, force } => log::run(id, rating, force),
        LeetcodeCommand::Next { count } => next::run(count),
    }
}
