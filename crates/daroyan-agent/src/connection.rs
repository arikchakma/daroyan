use std::collections::VecDeque;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use daroyan_protocol::host::Host;
use daroyan_protocol::log::{LogBatch, LogLine};
use daroyan_protocol::message::{AgentMessage, ApiMessage};
use daroyan_protocol::report::Report;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use log::{info, warn};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::mpsc::Receiver;
use tokio::time::{Instant, sleep, sleep_until, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::command::{HeldProcesses, run_command};
use crate::config::Config;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const RECONNECT_MAX: Duration = Duration::from_secs(60);
const QUEUE_LIMIT: usize = 60;
const LOG_QUEUE_BYTES: usize = 10 * 1024 * 1024;
const LOG_CHUNK_BYTES: usize = 256 * 1024;
const LOG_CHUNK_LINES: usize = 4_000;
const LINE_OVERHEAD: usize = 16;

#[derive(Default)]
pub struct Outbox {
    queue: VecDeque<Report>,
    last_host: Option<Host>,
    logs: VecDeque<LogBatch>,
    log_bytes: usize,
    log_dropped: u64,
}

fn measure_line(line: &LogLine) -> usize {
    serde_json::to_vec(line).map_or(line.message.len(), |json| json.len()) + LINE_OVERHEAD
}

fn measure_batch(batch: &LogBatch) -> usize {
    batch.lines.iter().map(measure_line).sum()
}

impl Outbox {
    fn push_logs(&mut self, batch: LogBatch) {
        self.log_bytes += measure_batch(&batch);
        self.logs.push_back(batch);
        while self.log_bytes > LOG_QUEUE_BYTES {
            let Some(oldest) = self.logs.pop_front() else { break };
            self.log_bytes -= measure_batch(&oldest);
            self.log_dropped += oldest.lines.len() as u64 + oldest.dropped;
        }
    }

    fn take_log_chunk(&mut self) -> Option<LogBatch> {
        if self.logs.is_empty() && self.log_dropped == 0 {
            return None;
        }
        let mut chunk = LogBatch {
            lines: Vec::new(),
            dropped: std::mem::take(&mut self.log_dropped),
        };
        let mut bytes = 0;
        while let Some(batch) = self.logs.front_mut() {
            chunk.dropped += std::mem::take(&mut batch.dropped);
            let mut fits = 0;
            let mut is_full = false;
            for line in &batch.lines {
                let size = measure_line(line);
                let count = chunk.lines.len() + fits;
                if count > 0
                    && (bytes + size > LOG_CHUNK_BYTES || count >= LOG_CHUNK_LINES)
                {
                    is_full = true;
                    break;
                }
                bytes += size;
                fits += 1;
            }
            chunk.lines.extend(batch.lines.drain(..fits));
            if is_full {
                self.log_bytes -= bytes;
                return Some(chunk);
            }
            self.logs.pop_front();
        }
        self.log_bytes -= bytes;
        Some(chunk)
    }

    fn push(&mut self, report: Report) {
        if let Some(host) = &report.host {
            self.last_host = Some(host.clone());
        }
        self.queue.push_back(report);
        if self.queue.len() > QUEUE_LIMIT {
            self.queue.pop_front();
            warn!(
                "dropped the oldest report, the API has been unreachable for 10 minutes"
            );
        }
    }

    async fn flush<S>(
        &mut self,
        sink: &mut SplitSink<WebSocketStream<S>, Message>,
        host_sent: &mut bool,
    ) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        while let Some(report) = self.queue.front_mut() {
            if !*host_sent && report.host.is_none() {
                report.host.clone_from(&self.last_host);
            }
            send_message(sink, &AgentMessage::Report(report.clone())).await?;
            self.queue.pop_front();
            *host_sent = true;
        }
        while let Some(chunk) = self.take_log_chunk() {
            send_message(sink, &AgentMessage::Logs(chunk)).await?;
        }
        Ok(())
    }
}

pub async fn run_connection(
    config: &Config,
    token: &str,
    held: &HeldProcesses,
    mut reports: Receiver<Report>,
    mut logs: Receiver<LogBatch>,
) -> Result<()> {
    let url = build_connect_url(&config.api.url)?;
    let mut outbox = Outbox::default();
    let mut backoff = Duration::from_secs(1);

    loop {
        match connect(&url, token).await {
            Ok(ws) => {
                info!("connected to {url}");
                backoff = Duration::from_secs(1);
                match run_session(ws, config, held, &mut reports, &mut logs, &mut outbox)
                    .await
                {
                    Ok(()) => return Ok(()),
                    Err(error) => warn!("lost the connection: {error:#}"),
                }
            }
            Err(error) => warn!("connect to {url} failed: {error:#}"),
        }

        let retry_at = sleep(backoff);
        tokio::pin!(retry_at);
        loop {
            tokio::select! {
                () = &mut retry_at => break,
                report = reports.recv() => match report {
                    Some(report) => outbox.push(report),
                    None => return Ok(()),
                },
                Some(batch) = logs.recv() => outbox.push_logs(batch),
            }
        }
        backoff = (backoff * 2).min(RECONNECT_MAX);
    }
}

async fn connect(
    url: &str,
    token: &str,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
    let mut request = url.into_client_request()?;
    request
        .headers_mut()
        .insert(AUTHORIZATION, format!("Bearer {token}").parse()?);
    let (ws, _) = timeout(CONNECT_TIMEOUT, connect_async(request))
        .await
        .map_err(|_| anyhow!("timed out"))??;
    Ok(ws)
}

pub async fn run_session<S>(
    ws: WebSocketStream<S>,
    config: &Config,
    held: &HeldProcesses,
    reports: &mut Receiver<Report>,
    logs: &mut Receiver<LogBatch>,
    outbox: &mut Outbox,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut sink, mut stream) = ws.split();
    let mut host_sent = false;
    let mut last_seen = Instant::now();

    loop {
        outbox.flush(&mut sink, &mut host_sent).await?;
        tokio::select! {
            report = reports.recv() => match report {
                Some(report) => outbox.push(report),
                None => return Ok(()),
            },
            Some(batch) = logs.recv() => outbox.push_logs(batch),
            () = sleep_until(last_seen + IDLE_TIMEOUT) => {
                bail!("no message from the API in {} seconds", IDLE_TIMEOUT.as_secs());
            }
            message = stream.next() => {
                last_seen = Instant::now();
                match message.context("the API closed the connection")?? {
                    Message::Text(text) => match serde_json::from_str::<ApiMessage>(&text) {
                        Ok(ApiMessage::Command(command)) => {
                            let result = run_command(command, config, held).await;
                            send_message(&mut sink, &AgentMessage::CommandResult(result)).await?;
                        }
                        Err(error) => warn!("ignored a message from the API: {error}"),
                    },
                    Message::Ping(_) => sink.flush().await?,
                    Message::Close(_) => bail!("the API closed the connection"),
                    _ => {}
                }
            }
        }
    }
}

async fn send_message<S>(
    sink: &mut SplitSink<WebSocketStream<S>, Message>,
    message: &AgentMessage,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    sink.send(Message::text(serde_json::to_string(message)?)).await?;
    Ok(())
}

fn build_connect_url(api_url: &str) -> Result<String> {
    let base = api_url.trim_end_matches('/');
    let ws_base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        bail!("api.url must start with https:// or http://");
    };
    Ok(format!("{ws_base}/v1/agent/connect"))
}

#[cfg(test)]
mod tests {
    use daroyan_protocol::message::CommandResult;
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

    use super::*;
    use crate::collect::Collector;

    fn make_batch(count: usize, size: usize, dropped: u64) -> LogBatch {
        let line = LogLine {
            at: 1,
            service: "api".to_string(),
            stream: daroyan_protocol::log::LogStream::Stdout,
            severity: 9,
            message: "x".repeat(size),
            attributes: serde_json::Map::default(),
            trace_id: None,
            span_id: None,
            raw: None,
        };
        LogBatch { lines: vec![line; count], dropped }
    }

    #[test]
    fn merges_queued_logs_into_bounded_chunks() {
        let mut outbox = Outbox::default();
        for _ in 0..300 {
            outbox.push_logs(make_batch(10, 100, 0));
        }
        outbox.push_logs(make_batch(0, 0, 4));
        let mut chunks = Vec::new();
        while let Some(chunk) = outbox.take_log_chunk() {
            chunks.push(chunk);
        }
        let lines: usize = chunks.iter().map(|chunk| chunk.lines.len()).sum();
        assert_eq!(lines, 3_000);
        assert!(chunks.len() < 5, "{} chunks", chunks.len());
        assert!(chunks.iter().all(|chunk| chunk.lines.len() <= LOG_CHUNK_LINES));
        assert_eq!(chunks.iter().map(|chunk| chunk.dropped).sum::<u64>(), 4);
        assert_eq!(outbox.log_bytes, 0);
    }

    #[test]
    fn keeps_chunks_with_attributes_under_the_message_limit() {
        let mut outbox = Outbox::default();
        let mut batch = make_batch(6_000, 20, 0);
        for line in &mut batch.lines {
            line.raw = Some("r".repeat(250));
            for key in ["time", "rid", "user_id", "board", "trace"] {
                line.attributes.insert(key.to_string(), "v".repeat(40).into());
            }
        }
        outbox.push_logs(batch);
        let mut lines = 0;
        while let Some(chunk) = outbox.take_log_chunk() {
            let size =
                serde_json::to_vec(&AgentMessage::Logs(chunk.clone())).unwrap().len();
            assert!(size < 512 * 1024, "{size} bytes");
            lines += chunk.lines.len();
        }
        assert_eq!(lines, 6_000);
    }

    #[test]
    fn drops_the_oldest_logs_past_the_queue_limit() {
        let mut outbox = Outbox::default();
        for _ in 0..30 {
            outbox.push_logs(make_batch(100, 4 * 1024, 0));
        }
        assert!(outbox.log_bytes <= LOG_QUEUE_BYTES);
        assert!(outbox.log_dropped > 0);
    }

    #[test]
    fn builds_the_connect_url() {
        assert_eq!(
            build_connect_url("https://daroyan.com/").unwrap(),
            "wss://daroyan.com/v1/agent/connect"
        );
        assert_eq!(
            build_connect_url("http://127.0.0.1:8900").unwrap(),
            "ws://127.0.0.1:8900/v1/agent/connect"
        );
        assert!(build_connect_url("daroyan.com").is_err());
    }

    #[tokio::test]
    #[allow(clippy::result_large_err)]
    async fn sends_reports_and_answers_commands() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/v1/agent/connect", listener.local_addr().unwrap());
        let api = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws =
                accept_hdr_async(tcp, |request: &Request, response: Response| {
                    assert_eq!(request.headers()["authorization"], "Bearer dy_server_x");
                    Ok(response)
                })
                .await
                .unwrap();
            ws.send(Message::text(
                r#"{"type":"command","id":"c1","action":"restart_process","name":"sshd"}"#,
            ))
            .await
            .unwrap();
            let mut seen = Vec::new();
            while seen.len() < 2 {
                if let Message::Text(text) = ws.next().await.unwrap().unwrap() {
                    seen.push(serde_json::from_str::<AgentMessage>(&text).unwrap());
                }
            }
            seen
        });

        let config = Config::default();
        let held = HeldProcesses::default();
        let collector = Collector::new();
        let host = crate::collect::host::collect_host(
            &config.server,
            &collector.system,
            &collector.networks,
        );
        let mut outbox = Outbox::default();
        outbox.push(Report {
            host: Some(host.clone()),
            state: collector.collect_state(&config),
            processes: Vec::new(),
            ports: Vec::new(),
            events: Vec::new(),
        });
        outbox.queue.front_mut().unwrap().host = None;
        let (_reports_tx, mut reports) = mpsc::channel(1);
        let (_logs_tx, mut logs) = mpsc::channel(1);
        let ws = connect(&url, "dy_server_x").await.unwrap();

        let seen = tokio::select! {
            biased;
            seen = api => seen.unwrap(),
            result = run_session(ws, &config, &held, &mut reports, &mut logs, &mut outbox) => {
                panic!("the session ended: {result:?}")
            }
        };

        let report = seen.iter().find_map(|message| match message {
            AgentMessage::Report(report) => Some(report),
            AgentMessage::CommandResult(_) | AgentMessage::Logs(_) => None,
        });
        assert_eq!(report.unwrap().host.as_ref(), Some(&host));
        assert!(seen.contains(&AgentMessage::CommandResult(CommandResult {
            id: "c1".to_string(),
            ok: false,
            error: Some("commands are turned off on this server".to_string()),
        })));
    }
}
