use std::net::SocketAddr;
use std::time::Duration;

use daroyan_api::app::{build_app, open_state};
use daroyan_core::alerts::engine::spawn_alert_engine;
use daroyan_core::analytics::spawn_analytics_tasks;
use daroyan_core::config::Config;
use daroyan_core::crons::spawn_cron_runner;
use daroyan_core::incidents::spawn_incident_scheduler;
use daroyan_core::uptime::spawn_uptime_runner;
use nix::sys::stat::{Mode, umask};
use tokio::net::TcpListener;
use tokio::signal;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    umask(Mode::from_bits_truncate(0o077));
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    let state = open_state(Config::from_env()?).await?;
    let analytics = state.analytics.clone();
    let agents = state.agents.clone();
    let analytics_tasks = spawn_analytics_tasks(
        analytics.clone(),
        state.config.retention_days,
        state.config.log_retention_days,
        state.config.backup_dir.clone(),
    );
    let alert_engine = spawn_alert_engine(
        state.database.clone(),
        state.agents.clone(),
        state.analytics.clone(),
        state.config.app_url.clone(),
        state.config.slack_signing_secret.is_some(),
    );
    let uptime_runner = spawn_uptime_runner(state.uptime.clone());
    let cron_runner = spawn_cron_runner(state.crons.clone());
    let incident_scheduler =
        spawn_incident_scheduler(state.database.clone(), state.config.app_url.clone());
    let limits = state.limits.clone();
    tokio::spawn(async move {
        let mut cleanup = tokio::time::interval(Duration::from_secs(60));
        loop {
            cleanup.tick().await;
            limits.forget_stale();
        }
    });

    let address = SocketAddr::from((state.config.host, state.config.port));
    let listener = TcpListener::bind(address).await?;
    tracing::info!("listening on http://{address}");

    axum::serve(
        listener,
        build_app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        wait_for_shutdown().await;
        agents.disconnect_all();
    })
    .await?;
    analytics_tasks.abort();
    alert_engine.abort();
    uptime_runner.abort();
    cron_runner.abort();
    incident_scheduler.abort();
    if let Err(error) = analytics.flush().await {
        tracing::error!(error = %error, "flush analytics on shutdown");
    }
    Ok(())
}

async fn wait_for_shutdown() {
    let interrupt = async {
        signal::ctrl_c().await.ok();
    };
    let terminate = async {
        if let Ok(mut signal) =
            signal::unix::signal(signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    tokio::select! {
        () = interrupt => {},
        () = terminate => {},
    }
}
