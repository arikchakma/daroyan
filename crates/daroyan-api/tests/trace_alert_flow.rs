mod common;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::routing::post;
use axum::{Json, Router};
use daroyan_api::app::{build_app, open_state};
use daroyan_core::alerts::engine::AlertEngine;
use daroyan_core::config::Config;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value as AnyValueKind;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};
use prost::Message;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio::net::TcpListener;

const SERVER: i32 = 2;
const CONSUMER: i32 = 5;
const INTERNAL: i32 = 1;

async fn serve(app: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
    });
    format!("http://{address}")
}

fn text(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(AnyValueKind::StringValue(value.to_string())),
        }),
        ..Default::default()
    }
}

fn number(key: &str, value: i64) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue { value: Some(AnyValueKind::IntValue(value)) }),
        ..Default::default()
    }
}

fn now_nanos() -> u64 {
    u64::try_from(chrono::Utc::now().timestamp_nanos_opt().unwrap()).unwrap()
}

#[derive(Clone, Copy)]
struct SpanSpec {
    kind: i32,
    route: &'static str,
    duration_ms: u64,
    is_error: bool,
    http_status: Option<i64>,
}

fn make_span(index: usize, spec: &SpanSpec) -> Span {
    let start = now_nanos() - 60_000_000_000;
    let mut trace_id = vec![0; 16];
    trace_id[..8].copy_from_slice(&(index as u64 + 1).to_be_bytes());
    let parent_span_id = if spec.kind == INTERNAL { vec![9; 8] } else { Vec::new() };

    let mut attributes = vec![text("http.route", spec.route)];
    attributes
        .extend(spec.http_status.map(|code| number("http.response.status_code", code)));

    Span {
        trace_id,
        span_id: (index as u64 + 1).to_be_bytes().to_vec(),
        parent_span_id,
        name: format!("GET {}", spec.route),
        kind: spec.kind,
        start_time_unix_nano: start,
        end_time_unix_nano: start + spec.duration_ms * 1_000_000,
        attributes,
        status: spec.is_error.then(|| Status { code: 2, message: "boom".to_string() }),
        ..Default::default()
    }
}

fn make_request(
    service: &str,
    offset: usize,
    specs: &[SpanSpec],
) -> ExportTraceServiceRequest {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![text("service.name", service)],
                ..Default::default()
            }),
            scope_spans: vec![ScopeSpans {
                spans: specs
                    .iter()
                    .enumerate()
                    .map(|(index, spec)| make_span(offset + index, spec))
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
}

fn repeat(count: usize, spec: SpanSpec) -> Vec<SpanSpec> {
    vec![spec; count]
}

fn ok(kind: i32, route: &'static str, duration_ms: u64) -> SpanSpec {
    SpanSpec {
        kind,
        route,
        duration_ms,
        is_error: false,
        http_status: Some(200),
    }
}

fn failed(kind: i32, route: &'static str) -> SpanSpec {
    SpanSpec {
        kind,
        route,
        duration_ms: 40,
        is_error: true,
        http_status: None,
    }
}

fn server_error(route: &'static str) -> SpanSpec {
    SpanSpec {
        kind: SERVER,
        route,
        duration_ms: 40,
        is_error: false,
        http_status: Some(503),
    }
}

fn sample_requests() -> Vec<ExportTraceServiceRequest> {
    let api = [
        repeat(14, ok(SERVER, "/api/inbox", 100)),
        repeat(2, ok(SERVER, "/api/search", 2400)),
        repeat(2, failed(SERVER, "/api/export")),
        repeat(2, server_error("/api/export")),
        repeat(30, failed(INTERNAL, "/db/query")),
    ]
    .concat();
    let worker = [
        repeat(14, ok(CONSUMER, "send-digest", 300)),
        repeat(6, failed(CONSUMER, "send-digest")),
    ]
    .concat();
    let cron = [
        repeat(1, ok(SERVER, "/cron/sync", 50)),
        repeat(2, failed(SERVER, "/cron/sync")),
    ]
    .concat();

    vec![
        make_request("inboxjot-api", 0, &api),
        make_request("inboxjot-worker", 1000, &worker),
        make_request("inboxjot-cron", 2000, &cron),
    ]
}

#[tokio::test]
async fn alerts_on_trace_error_rate_and_latency_per_service() {
    let dir =
        std::env::temp_dir().join(format!("daroyan-trace-alerts-{}", std::process::id()));
    let state = open_state(Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        database_url: format!("sqlite://{}?mode=rwc", dir.join("daroyan.db").display()),
        duckdb_path: dir.join("daroyan.duckdb"),
        retention_days: 30,
        log_retention_days: 7,
        otlp_log_budget: 1024 * 1024,
        metrics: daroyan_core::config::MetricSettings::default(),
        backup_dir: None,
        trusted_proxies: Vec::new(),
        client_ip_header: "cf-connecting-ip".to_string(),
        app_url: Some("https://daroyan.example".to_string()),
        google_client_id: None,
        google_client_secret: None,
        allowed_emails: Vec::new(),
        slack_api_url: "https://slack.com".to_string(),
        slack_client_id: None,
        slack_client_secret: None,
        discord_client_id: None,
        discord_client_secret: None,
        telegram_bot_token: None,
        allow_private_targets: true,
    })
    .await
    .unwrap();
    let analytics = state.analytics.clone();
    let mut engine = AlertEngine::new(
        state.database.clone(),
        state.agents.clone(),
        state.analytics.clone(),
        state.config.app_url.clone(),
    );
    let http = common::sign_in(&state.database).await;
    let base = serve(build_app(state)).await;

    let received = Arc::new(Mutex::new(Vec::<Value>::new()));
    let hook_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hook_url = format!("http://{}/hook", hook_listener.local_addr().unwrap());
    let hook_store = Arc::clone(&received);
    tokio::spawn(async move {
        let app = Router::new().route(
            "/hook",
            post(move |Json(body): Json<Value>| async move {
                hook_store.lock().unwrap().push(body);
            }),
        );
        axum::serve(hook_listener, app).await
    });

    let integration: Value = http
        .post(format!("{base}/v1/integrations"))
        .json(&json!({ "name": "Ops hook", "config": { "kind": "webhook", "url": hook_url } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let created: Value = http
        .post(format!("{base}/v1/ingest-tokens"))
        .json(&json!({ "name": "inboxjot" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = created["token"].as_str().unwrap().to_string();

    for request in sample_requests() {
        let response = Client::new()
            .post(format!("{base}/v1/otlp/v1/traces"))
            .bearer_auth(&token)
            .header("content-type", "application/x-protobuf")
            .body(request.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    analytics.flush().await.unwrap();

    let errors = json!({ "kind": "trace_errors", "above": 10, "within_secs": 300 });
    let picked = http
        .post(format!("{base}/v1/alert-rules"))
        .json(&json!({
            "name": "Failing Requests",
            "condition": errors,
            "scope": { "kind": "servers", "server_ids": ["srv_1"] },
            "integration_ids": []
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(picked.status(), StatusCode::BAD_REQUEST);

    let rule: Value = http
        .post(format!("{base}/v1/alert-rules"))
        .json(&json!({
            "name": "Failing Requests",
            "condition": errors,
            "scope": { "kind": "all" },
            "integration_ids": [integration["id"]]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(rule["id"].is_string(), "{rule}");
    assert_eq!(rule["condition"]["min_requests"], 10);
    assert_eq!(rule["condition"]["spans"], "entry");

    let ends_at = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    let muted = http
        .post(format!("{base}/v1/alert-mutes"))
        .json(&json!({
            "rule_id": rule["id"],
            "server_id": "traces",
            "subject": "inboxjot-api",
            "ends_at": ends_at
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(muted.status(), StatusCode::CREATED);

    engine.check_alerts(chrono::Utc::now()).await.unwrap();

    let mut firing: Vec<Value> = http
        .get(format!("{base}/v1/alerts?status=firing"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    firing.sort_by_key(|alert| alert["subject"].as_str().unwrap().to_string());

    let summary: Vec<(&str, &str, &str, &str)> = firing
        .iter()
        .map(|alert| {
            (
                alert["server_id"].as_str().unwrap(),
                alert["target_name"].as_str().unwrap(),
                alert["subject"].as_str().unwrap(),
                alert["value"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("traces", "Traces", "inboxjot-api", "20% errors"),
            ("traces", "Traces", "inboxjot-worker", "30% errors"),
        ]
    );

    for _ in 0..50 {
        if received.lock().unwrap().len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    let hooks: Vec<Value> = received
        .lock()
        .unwrap()
        .iter()
        .filter(|hook| hook["event"] == "alert.firing")
        .cloned()
        .collect();
    assert_eq!(hooks.len(), 1, "{hooks:?}");
    let hook = &hooks[0];
    assert_eq!(hook["event"], "alert.firing");
    assert_eq!(hook["data"]["subject"], "inboxjot-worker");
    assert_eq!(hook["data"]["traces"]["name"], "Traces");
    assert_eq!(
        hook["url"],
        "https://daroyan.example/traces?service=inboxjot-worker&range=1h&view=error"
    );

    let details = hook["details"].to_string();
    assert!(details.contains("6 of 20 requests failed"), "{details}");
    assert!(details.contains("Failing routes"), "{details}");
    assert!(details.contains("6 of 20 failed (30%) · send-digest"), "{details}");
    assert!(
        details.contains(
            "https://daroyan.example/traces?service=inboxjot-worker&range=1h&view=error"
        ),
        "{details}"
    );

    let latency = json!({
        "kind": "trace_latency",
        "above": 1000,
        "percentile": "p95",
        "within_secs": 300,
        "group_by": "route",
        "min_requests": 2
    });
    let backtest: Value = http
        .post(format!("{base}/v1/alert-rules/backtest"))
        .json(&json!({ "condition": latency, "scope": { "kind": "all" } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(backtest["is_supported"], true);
    let hits: Vec<(&str, &str)> = backtest["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| (hit["subject"].as_str().unwrap(), hit["value"].as_str().unwrap()))
        .collect();
    assert_eq!(hits, [("inboxjot-api · /api/search", "p95 2.4 s")]);

    let backtest: Value = http
        .post(format!("{base}/v1/alert-rules/backtest"))
        .json(&json!({ "condition": errors, "scope": { "kind": "all" } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut hits: Vec<&str> = backtest["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["subject"].as_str().unwrap())
        .collect();
    hits.sort_unstable();
    hits.dedup();
    assert_eq!(hits, ["inboxjot-api", "inboxjot-worker"]);

    std::fs::remove_dir_all(dir).ok();
}
