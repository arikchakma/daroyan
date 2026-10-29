mod common;

use std::io::Write;
use std::net::SocketAddr;

use axum::Router;
use daroyan_api::app::{build_app, open_state};
use daroyan_core::config::Config;
use daroyan_core::db::queries::servers::NewServer;
use flate2::Compression;
use flate2::write::GzEncoder;
use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value as AnyValueKind;
use opentelemetry_proto::tonic::common::v1::{
    AnyValue, InstrumentationScope, KeyValue, KeyValueList,
};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};
use prost::Message;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio::net::TcpListener;

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

fn now_nanos() -> u64 {
    u64::try_from(chrono::Utc::now().timestamp_nanos_opt().unwrap()).unwrap()
}

fn sample_request(host_name: &str, records: Vec<LogRecord>) -> ExportLogsServiceRequest {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: vec![
                    text("service.name", "inboxjot-api"),
                    text("host.name", host_name),
                ],
                ..Default::default()
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: "inboxjot".to_string(),
                    ..Default::default()
                }),
                log_records: records,
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
}

#[tokio::test]
async fn ingests_otlp_logs_and_links_them_to_traces() {
    let dir =
        std::env::temp_dir().join(format!("daroyan-otlp-logs-{}", std::process::id()));
    let state = open_state(Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        database_url: format!("sqlite://{}?mode=rwc", dir.join("daroyan.db").display()),
        duckdb_path: dir.join("daroyan.duckdb"),
        retention_days: 30,
        log_retention_days: 7,
        otlp_log_budget: 4 * 1024,
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
    state
        .database
        .create_server(NewServer {
            id: "srv_inboxjot".to_string(),
            name: "Inboxjot Server".to_string(),
            labels: json!([]),
            host: json!({
                "name": "Inboxjot Server", "labels": [], "hostname": "inboxjot",
                "platform": "ubuntu", "platform_version": "24.04", "kernel": "6.8",
                "arch": "x86_64", "cpu": "", "cpu_cores": 2, "mem_total": 0,
                "swap_total": 0, "private_ip": null, "boot_time": 0, "version": "0.0.5",
            }),
            token_hash: "unused".to_string(),
        })
        .await
        .unwrap();
    let analytics = state.analytics.clone();
    let http = common::sign_in(&state.database).await;
    let anonymous = Client::new();
    let base = serve(build_app(state)).await;
    let endpoint = format!("{base}/v1/otlp/v1/logs");

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

    let now = now_nanos();
    let body = sample_request(
        "inboxjot",
        vec![
            LogRecord {
                time_unix_nano: now - 2_000_000_000,
                severity_number: 17,
                body: Some(AnyValue {
                    value: Some(AnyValueKind::StringValue("SES SendEmail failed".into())),
                }),
                attributes: vec![text("email.id", "eml_1")],
                trace_id: vec![0xab; 16],
                span_id: vec![1; 8],
                ..Default::default()
            },
            LogRecord {
                observed_time_unix_nano: now - 1_000_000_000,
                severity_text: "WARNING".to_string(),
                body: Some(AnyValue {
                    value: Some(AnyValueKind::KvlistValue(KeyValueList {
                        values: vec![text("event", "slow job")],
                    })),
                }),
                ..Default::default()
            },
        ],
    )
    .encode_to_vec();

    let missing = anonymous.post(&endpoint).body(body.clone()).send().await.unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    let wrong = anonymous
        .post(&endpoint)
        .bearer_auth("dy_ingest_wrong")
        .body(body.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    let unsupported = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .header("content-type", "text/plain")
        .body("hi")
        .send()
        .await
        .unwrap();
    assert_eq!(unsupported.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(&body).unwrap();
    let accepted = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .header("content-type", "application/x-protobuf")
        .header("content-encoding", "gzip")
        .body(gzip.finish().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    let response =
        ExportLogsServiceResponse::decode(accepted.bytes().await.unwrap()).unwrap();
    assert_eq!(response.partial_success, None);

    let json_request = sample_request(
        "4f1c2a9b7e3d",
        vec![LogRecord {
            time_unix_nano: now,
            body: Some(AnyValue {
                value: Some(AnyValueKind::StringValue("worker started".into())),
            }),
            ..Default::default()
        }],
    );
    let accepted = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .json(&json_request)
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    assert_eq!(accepted.headers()["content-type"], "application/json");
    analytics.flush().await.unwrap();

    let page: Value = http
        .get(format!("{base}/v1/logs?service=inboxjot-api"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let lines = page["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 3);

    assert_eq!(lines[0]["message"], "worker started");
    assert_eq!(lines[0]["server_id"], "otlp");
    assert_eq!(lines[0]["level"], "info");

    assert_eq!(lines[1]["message"], r#"{"event":"slow job"}"#);
    assert_eq!(lines[1]["level"], "warn");
    assert_eq!(lines[1]["server_id"], "srv_inboxjot");

    assert_eq!(lines[2]["message"], "SES SendEmail failed");
    assert_eq!(lines[2]["stream"], "otlp");
    assert_eq!(lines[2]["severity"], 17);
    assert_eq!(lines[2]["trace_id"], "ab".repeat(16));
    assert_eq!(lines[2]["span_id"], "01".repeat(8));
    assert_eq!(lines[2]["attributes"]["email.id"], "eml_1");
    assert_eq!(lines[2]["attributes"]["host.name"], "inboxjot");
    assert_eq!(lines[2]["attributes"]["otel.scope.name"], "inboxjot");
    assert!(lines[2]["raw"].as_str().unwrap().contains("SES SendEmail failed"));

    let traced: Value = http
        .get(format!("{base}/v1/logs?q=trace_id:{}", "ab".repeat(16)))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(traced["lines"].as_array().unwrap().len(), 1);

    let sources: Value = http
        .get(format!("{base}/v1/logs/sources"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(sources.as_array().unwrap().contains(&json!({
        "server_id": "otlp",
        "service": "inboxjot-api",
    })));

    let noisy = sample_request(
        "inboxjot",
        (0..100)
            .map(|_| LogRecord {
                time_unix_nano: now,
                body: Some(AnyValue {
                    value: Some(AnyValueKind::StringValue("x".repeat(100))),
                }),
                ..Default::default()
            })
            .collect(),
    );
    let partial = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .json(&noisy)
        .send()
        .await
        .unwrap();
    assert_eq!(partial.status(), StatusCode::OK);
    let partial: Value = partial.json().await.unwrap();
    let rejected = partial["partialSuccess"]["rejectedLogRecords"].as_i64().unwrap();
    assert!(rejected > 0 && rejected < 100, "{rejected}");
    assert!(
        partial["partialSuccess"]["errorMessage"]
            .as_str()
            .unwrap()
            .contains("over the budget")
    );

    let tokens: Value = http
        .get(format!("{base}/v1/ingest-tokens"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(tokens[0]["dropped_log_records"], rejected);

    let spans = ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![
                    text("service.name", "inboxjot-api"),
                    text("host.name", "inboxjot"),
                ],
                ..Default::default()
            }),
            scope_spans: vec![ScopeSpans {
                spans: vec![Span {
                    trace_id: vec![0xab; 16],
                    span_id: vec![1; 8],
                    name: "POST /v1/emails".to_string(),
                    start_time_unix_nano: now - 2_000_000_000,
                    end_time_unix_nano: now - 1_900_000_000,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }],
    };
    let accepted = anonymous
        .post(format!("{base}/v1/otlp/v1/traces"))
        .bearer_auth(&token)
        .json(&spans)
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    analytics.flush().await.unwrap();

    for (server_id, count) in [("srv_inboxjot", 1), ("srv_other", 0)] {
        let traces: Value = http
            .get(format!("{base}/v1/traces?server_id={server_id}"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let traces = traces.as_array().unwrap();
        assert_eq!(traces.len(), count);
        if let Some(trace) = traces.first() {
            assert_eq!(trace["server_id"], "srv_inboxjot");
        }
    }

    std::fs::remove_dir_all(dir).ok();
}
