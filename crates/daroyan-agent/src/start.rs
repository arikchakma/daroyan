use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use daroyan_protocol::host::Host;
use daroyan_protocol::report::{Event, EventKind, Report};
use log::info;
use tokio::sync::mpsc::Sender;

use crate::args::StartArgs;
use crate::check::port::check_ports;
use crate::check::process::ProcessWatch;
use crate::collect::host::collect_host;
use crate::collect::{Collector, now_millis};
use crate::command::HeldProcesses;
use crate::config::{Config, load_config};
use crate::connection::run_connection;
use crate::enroll::load_or_enroll;
use crate::logger::start_logger;
use crate::logs::collect_logs;

const INTERVAL: Duration = Duration::from_secs(10);

pub fn start(args: &StartArgs) -> Result<()> {
    let config = load_config(&args.config)?;
    let _logger = start_logger(
        args.log_level.as_deref().unwrap_or(&config.agent.log_level),
        &config.agent.log_path,
    )?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run(config))
}

async fn run(config: Config) -> Result<()> {
    let collector = Collector::new();
    let host = collect_host(&config.server, &collector.system, &collector.networks);
    let server = load_or_enroll(&config, &host).await?;
    info!("reporting as {} ({}) to {}", host.name, server.server_id, config.api.url);

    let config = Arc::new(config);
    let held = HeldProcesses::default();
    let (reports_tx, reports_rx) = tokio::sync::mpsc::channel(8);
    let collect_config = Arc::clone(&config);
    let collect_held = Arc::clone(&held);
    thread::spawn(move || {
        collect_reports(collector, &collect_config, collect_held, &reports_tx);
    });
    let (logs_tx, logs_rx) = tokio::sync::mpsc::channel(32);
    let logs_config = Arc::clone(&config);
    thread::spawn(move || collect_logs(&logs_config, &logs_tx));

    run_connection(&config, &server.token, &held, reports_rx, logs_rx).await
}

fn collect_reports(
    mut collector: Collector,
    config: &Config,
    held: HeldProcesses,
    reports: &Sender<Report>,
) {
    let mut processes =
        ProcessWatch::new(config.processes.clone(), config.agent.commands, held);
    let mut events = vec![Event {
        at: now_millis(),
        kind: EventKind::AgentStarted { version: env!("CARGO_PKG_VERSION").to_string() },
    }];
    let mut last_host: Option<Host> = None;

    loop {
        thread::sleep(INTERVAL);
        collector.refresh();

        let host = collect_host(&config.server, &collector.system, &collector.networks);
        let report = Report {
            host: (last_host.as_ref() != Some(&host)).then(|| host.clone()),
            state: collector.collect_state(config),
            processes: processes.check_processes(
                &collector.system,
                collector.elapsed_secs,
                &mut events,
            ),
            ports: check_ports(&config.ports),
            events: std::mem::take(&mut events),
        };
        last_host = Some(host);
        if reports.blocking_send(report).is_err() {
            return;
        }
    }
}
