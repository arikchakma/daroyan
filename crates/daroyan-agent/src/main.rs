mod args;
mod check;
mod collect;
mod command;
mod config;
mod connection;
mod enroll;
mod grants;
#[cfg(target_os = "linux")]
mod io_helper;
mod logger;
mod logs;
mod service;
mod start;

use anyhow::Result;
use args::{Cli, Command};
use clap::Parser;

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Start(args) => start::start(&args),
        Command::Stop => service::control_agent("stop"),
        Command::Restart => service::control_agent("restart"),
        Command::Status => service::control_agent("status"),
        #[cfg(target_os = "linux")]
        Command::IoHelper => io_helper::serve(),
        #[cfg(not(target_os = "linux"))]
        Command::IoHelper => anyhow::bail!("the disk I/O helper only runs on Linux"),
        Command::Grants(args) => grants::write_grants(&args),
    }
}
