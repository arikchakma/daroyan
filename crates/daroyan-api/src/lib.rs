pub mod app;
mod cookies;
mod extract;
mod layers;
mod mcp;
mod routes;
mod views;

use std::sync::Arc;

use daroyan_core::agents::Agents;
use daroyan_core::alerts::connect::ConnectStates;
use daroyan_core::analytics::Analytics;
use daroyan_core::archive::Archive;
use daroyan_core::backup::Backups;
use daroyan_core::config::Config;
use daroyan_core::crons::Crons;
use daroyan_core::db::Database;
use daroyan_core::limits::{Limits, LogBudgets};
use daroyan_core::servers::hosts::HostCache;
use daroyan_core::uptime::Uptime;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub database: Database,
    pub analytics: Analytics,
    pub archive: Option<Archive>,
    pub agents: Agents,
    pub limits: Limits,
    pub log_budgets: LogBudgets,
    pub metric_budgets: LogBudgets,
    pub hosts: HostCache,
    pub connects: ConnectStates,
    pub uptime: Uptime,
    pub crons: Crons,
}

impl AppState {
    pub fn backups(&self) -> Backups {
        Backups::new(self.config.backup_dir.clone(), self.archive.clone())
    }
}
