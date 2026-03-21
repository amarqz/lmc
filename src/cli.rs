use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lmc", about = "Let Me Check — capture, cluster, and retrieve shell command workflows")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Alias or search fragment to look up
    pub query: Option<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Save the most recent cluster with an alias
    Save {
        /// The alias name for this cluster
        alias: String,

        /// Save a specific cluster by ID instead of the most recent
        #[arg(long)]
        from: Option<i64>,
    },

    /// Generate shell hook for integration
    Init {
        /// Shell to generate hook for (zsh, bash, fish)
        shell: String,
    },

    /// Record a command (internal, called by shell hook)
    #[command(hide = true)]
    Record {
        /// The command that was executed
        #[arg(long)]
        cmd: String,

        /// Working directory
        #[arg(long)]
        dir: String,

        /// Exit code
        #[arg(long)]
        exit_code: Option<i32>,

        /// Shell session ID
        #[arg(long)]
        session_id: String,

        /// Shell name
        #[arg(long)]
        shell: String,
    },
}
