use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "daroyan", version, about = "Daroyan server monitoring agent")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run the agent in the foreground
    Start(StartArgs),
    /// Stop the agent service
    Stop,
    /// Restart the agent service
    Restart,
    /// Show the agent service status
    Status,
}

#[derive(Args)]
pub struct StartArgs {
    /// Path to the config file
    #[arg(long, default_value = "/etc/daroyan/agent.yml")]
    pub config: PathBuf,
    /// Override the log level in the config
    #[arg(long)]
    pub log_level: Option<String>,
}
