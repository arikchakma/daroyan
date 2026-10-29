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
    assert_eq!(missing.headers()["content-type"], "application/x-protobuf");
    let wrong = anonymous
        .post(&endpoint)
        .bearer_auth("dy_ingest_wrong")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.headers()["content-type"], "application/json");
    let status: Value = wrong.json().await.unwrap();
    assert_eq!(status["code"], 16);
    assert_eq!(status["message"], "The ingest token is not valid");
    let unsupported = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .header("content-type", "text/plain")
        .body("hi")
        .send()
        .await
        .unwrap();
    assert_eq!(unsupported.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let garbage = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .json(&json!({ "resourceLogs": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(garbage.status(), StatusCode::BAD_REQUEST);
    assert_eq!(garbage.json::<Value>().await.unwrap()["code"], 3);
    let empty = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::OK);
    let crowded = sample_request("inboxjot", vec![LogRecord::default(); 10_001]);
    let crowded = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .body(crowded.encode_to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(crowded.status(), StatusCode::PAYLOAD_TOO_LARGE);

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
        vec![
            LogRecord {
                time_unix_nano: now,
                body: Some(AnyValue {
                    value: Some(AnyValueKind::StringValue("worker started".into())),
                }),
                ..Default::default()
            },
            LogRecord {
                time_unix_nano: now - 300_000_000,
                severity_number: 30,
                ..Default::default()
            },
            LogRecord {
                time_unix_nano: now - 500_000_000,
                event_name: "email.bounced".to_string(),
                attributes: vec![text("level", "error")],
                ..Default::default()
            },
        ],
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
    assert_eq!(lines.len(), 5);

    assert_eq!(lines[0]["message"], "worker started");
    assert_eq!(lines[0]["server_id"], "otlp");
    assert_eq!(lines[0]["level"], "info");

    assert_eq!(lines[1]["severity"], 17);

    assert_eq!(lines[2]["level"], "error");
    assert_eq!(lines[2]["attributes"]["event.name"], "email.bounced");

    assert_eq!(lines[3]["message"], r#"{"event":"slow job"}"#);
    assert_eq!(lines[3]["level"], "warn");
    assert_eq!(lines[3]["server_id"], "srv_inboxjot");

    assert_eq!(lines[4]["message"], "SES SendEmail failed");
    assert_eq!(lines[4]["stream"], "otlp");
    assert_eq!(lines[4]["severity"], 17);
    assert_eq!(lines[4]["trace_id"], "ab".repeat(16));
    assert_eq!(lines[4]["span_id"], "01".repeat(8));
    assert_eq!(lines[4]["attributes"]["email.id"], "eml_1");
    assert_eq!(lines[4]["attributes"].get("host.name"), None);
    assert_eq!(lines[4]["resource"]["host.name"], "inboxjot");
    assert_eq!(lines[4]["resource"]["service.name"], "inboxjot-api");
    assert_eq!(lines[4]["attributes"]["otel.scope.name"], "inboxjot");
    assert!(lines[4]["raw"].as_str().unwrap().contains("SES SendEmail failed"));

    let traced: Value = http
        .get(format!("{base}/v1/logs?q=trace_id:{}", "ab".repeat(16)))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(traced["lines"].as_array().unwrap().len(), 1);
    let by_resource: Value = http
        .get(format!("{base}/v1/logs?q=host.name:inboxjot"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(by_resource["lines"].as_array().unwrap().len(), 2);

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
    let throttled = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .json(&noisy)
        .send()
        .await
        .unwrap();
    assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after: u64 =
        throttled.headers()["retry-after"].to_str().unwrap().parse().unwrap();
    assert!((1..=60).contains(&retry_after), "{retry_after}");
    let status: Value = throttled.json().await.unwrap();
    assert_eq!(status["code"], 8);
    assert!(status["message"].as_str().unwrap().contains("4 KB of logs"));

    let tokens: Value = http
        .get(format!("{base}/v1/ingest-tokens"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(tokens[0]["throttled_log_records"], 100);

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

    let deleted = http
        .delete(format!("{base}/v1/servers/srv_inboxjot"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let kept: Value = http
        .get(format!("{base}/v1/logs?service=inboxjot-api"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let kept = kept["lines"].as_array().unwrap();
    assert_eq!(kept.len(), 5);
    assert!(kept.iter().all(|line| line["server_id"] == "otlp"));
    let traces: Value = http
        .get(format!("{base}/v1/traces"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(traces[0]["server_id"], Value::Null);

    std::fs::remove_dir_all(dir).ok();
}
