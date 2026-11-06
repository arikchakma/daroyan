use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use daroyan_protocol::host::Host;
use daroyan_protocol::report::{Event, EventKind, Report};
use log::{info, warn};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc::Sender;
use tokio::time::timeout;

use crate::args::StartArgs;
use crate::check::docker_events::DockerEvents;
use crate::check::port::check_ports;
use crate::check::process::ProcessWatch;
use crate::collect::host::collect_host;
use crate::collect::top::find_top;
use crate::collect::{Collector, now_millis};
use crate::command::HeldProcesses;
use crate::config::{Config, Source, load_config};
use crate::connection::run_connection;
use crate::enroll::load_or_enroll;
use crate::logger::start_logger;
use crate::logs::{LogFollowers, collect_logs};

const INTERVAL: Duration = Duration::from_secs(10);
const STOP_WAIT: Duration = Duration::from_secs(3);
const CHILD_WAIT: Duration = Duration::from_secs(1);

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
    let events = config
        .processes
        .iter()
        .any(|check| matches!(check.source, Source::Docker(_)))
        .then(DockerEvents::start);

    let (reports_tx, reports_rx) = tokio::sync::mpsc::channel(8);
    let collect_config = Arc::clone(&config);
    let collect_held = Arc::clone(&held);
    let collect_events = events.clone();
    thread::spawn(move || {
        collect_reports(
            collector,
            &collect_config,
            collect_held,
            collect_events,
            &reports_tx,
        );
    });

    let (logs_tx, logs_rx) = tokio::sync::mpsc::channel(32);
    let logs_config = Arc::clone(&config);
    let followers = LogFollowers::default();
    let logs_followers = followers.clone();
    thread::spawn(move || collect_logs(&logs_config, &logs_tx, &logs_followers, events));

    let (stopping_tx, stopping) = tokio::sync::watch::channel(false);
    let connection =
        run_connection(&config, &server.token, &held, reports_rx, logs_rx, stopping);
    tokio::pin!(connection);

    tokio::select! {
        result = &mut connection => return result,
        result = wait_for_stop() => result?,
    }

    let _ = stopping_tx.send(true);
    followers.stop();
    let drained = timeout(STOP_WAIT, &mut connection).await;
    let _ = timeout(CHILD_WAIT, wait_for_children(&followers)).await;
    followers.kill_left();

    match drained {
        Ok(result) => result,
        Err(_) => {
            warn!("stopped before all logs were sent");
            Ok(())
        }
    }
}

async fn wait_for_children(followers: &LogFollowers) {
    while followers.has_children() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_for_stop() -> Result<()> {
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;

    tokio::select! {
        _ = terminate.recv() => {}
        _ = interrupt.recv() => {}
    }
    info!("stopping");

    Ok(())
}

fn collect_reports(
    mut collector: Collector,
    config: &Config,
    held: HeldProcesses,
    events: Option<DockerEvents>,
    reports: &Sender<Report>,
) {
    let mut processes =
        ProcessWatch::new(config.processes.clone(), config.agent.commands, held, events);
    let mut events = vec![Event {
        at: now_millis(),
        kind: EventKind::AgentStarted { version: env!("CARGO_PKG_VERSION").to_string() },
    }];
    let mut last_host: Option<Host> = None;

    loop {
        thread::sleep(INTERVAL);
        collector.refresh();

        let host = collect_host(&config.server, &collector.system, &collector.networks);
        let watched = processes.check_processes(
            &collector.system,
            &collector.process_io,
            collector.elapsed_secs,
            &mut events,
        );
        let report = Report {
            host: (last_host.as_ref() != Some(&host)).then(|| host.clone()),
            state: collector.collect_state(config),
            top: find_top(&collector.system, processes.families()),
            processes: watched,
            ports: check_ports(&config.ports),
            events: std::mem::take(&mut events),
        };
        last_host = Some(host);
        if reports.blocking_send(report).is_err() {
            return;
        }
    }
}
