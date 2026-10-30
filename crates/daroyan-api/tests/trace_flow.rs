mod common;

use std::io::Write;
use std::net::SocketAddr;

use axum::Router;
use daroyan_api::app::{build_app, open_state};
use daroyan_core::config::Config;
use flate2::Compression;
use flate2::write::GzEncoder;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value as AnyValueKind;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::span::Event;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span, Status};
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

fn sample_request() -> ExportTraceServiceRequest {
    let start = now_nanos() - 300_000_000;
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![text("service.name", "checkout")],
                ..Default::default()
            }),
            scope_spans: vec![ScopeSpans {
                spans: vec![
                    Span {
                        trace_id: vec![0xab; 16],
                        span_id: vec![1; 8],
                        name: "POST /orders".to_string(),
                        kind: 2,
                        start_time_unix_nano: start,
                        end_time_unix_nano: start + 200_000_000,
                        attributes: vec![
                            text("http.request.method", "POST"),
                            text("http.route", "/orders"),
                        ],
                        status: Some(Status { code: 2, message: "boom".to_string() }),
                        events: vec![Event {
                            name: "exception".to_string(),
                            time_unix_nano: start + 150_000_000,
                            attributes: vec![
                                text("exception.type", "PaymentError"),
                                text("exception.message", "card 4242 declined"),
                                text(
                                    "exception.stacktrace",
                                    "PaymentError\n    at charge (src/pay.ts:10:3)",
                                ),
                            ],
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                    Span {
                        trace_id: vec![0xab; 16],
                        span_id: vec![2; 8],
                        parent_span_id: vec![1; 8],
                        name: "SELECT orders".to_string(),
                        kind: 3,
                        start_time_unix_nano: start + 10_000_000,
                        end_time_unix_nano: start + 60_000_000,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
}

#[tokio::test]
async fn ingests_otlp_traces_and_groups_exceptions() {
    let dir = std::env::temp_dir().join(format!("daroyan-traces-{}", std::process::id()));
    let state = open_state(Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        database_url: format!("sqlite://{}?mode=rwc", dir.join("daroyan.db").display()),
        duckdb_path: dir.join("daroyan.duckdb"),
        retention_days: 30,
        log_retention_days: 7,
        otlp_log_budget: 1024 * 1024,
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
    let http = common::sign_in(&state.database).await;
    let anonymous = Client::new();
    let base = serve(build_app(state)).await;
    let endpoint = format!("{base}/v1/otlp/v1/traces");

    let unauthorized = anonymous
        .get(format!("{base}/v1/ingest-tokens"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let empty_name = http
        .post(format!("{base}/v1/ingest-tokens"))
        .json(&json!({ "name": " " }))
        .send()
        .await
        .unwrap();
    assert_eq!(empty_name.status(), StatusCode::BAD_REQUEST);
    let created: Value = http
        .post(format!("{base}/v1/ingest-tokens"))
        .json(&json!({ "name": "checkout" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = created["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("dy_ingest_"));
    assert_eq!(created["hint"], token[token.len() - 4..]);
    let token_id = created["id"].as_str().unwrap().to_string();

    let body = sample_request().encode_to_vec();
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
    let garbage = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .header("content-type", "application/x-protobuf")
        .body(vec![0xff, 0xff, 0xff])
        .send()
        .await
        .unwrap();
    assert_eq!(garbage.status(), StatusCode::BAD_REQUEST);

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
    assert_eq!(accepted.headers()["content-type"], "application/x-protobuf");

    let mut bomb = GzEncoder::new(Vec::new(), Compression::best());
    bomb.write_all(&vec![0; 17 * 1024 * 1024]).unwrap();
    let too_large = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .header("content-type", "application/x-protobuf")
        .header("content-encoding", "gzip")
        .body(bomb.finish().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(too_large.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let mut json_request = sample_request();
    json_request.resource_spans[0].scope_spans[0].spans.truncate(1);
    json_request.resource_spans[0].scope_spans[0].spans[0].trace_id = vec![0xcd; 16];
    let accepted = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .json(&json_request)
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    analytics.flush().await.unwrap();

    let traces: Value = http
        .get(format!("{base}/v1/traces"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let traces = traces.as_array().unwrap();
    assert_eq!(traces.len(), 2);
    let trace = traces
        .iter()
        .find(|trace| trace["trace_id"] == "ab".repeat(16))
        .unwrap();
    assert_eq!(trace["service"], "checkout");
    assert_eq!(trace["name"], "POST /orders");
    assert_eq!(trace["span_count"], 2);
    assert_eq!(trace["error_count"], 1);
    assert_eq!(trace["duration_us"], 200_000);
    assert_eq!(trace["http_route"], "/orders");

    let errors: Value = http
        .get(format!("{base}/v1/traces?status=error&q=orders&min_duration_ms=100"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(errors.as_array().unwrap().len(), 2);
    let slow: Value = http
        .get(format!("{base}/v1/traces?min_duration_ms=250"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(slow.as_array().unwrap().is_empty());

    let spans: Value = http
        .get(format!("{base}/v1/traces/{}", "ab".repeat(16)))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(spans.as_array().unwrap().len(), 2);
    assert_eq!(spans[1]["parent_span_id"], "01".repeat(8));
    assert_eq!(spans[0]["events"][0]["name"], "exception");
    let missing_trace = http
        .get(format!("{base}/v1/traces/{}", "00".repeat(16)))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_trace.status(), StatusCode::NOT_FOUND);

    let services: Value = http
        .get(format!("{base}/v1/traces/services"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(services[0]["service"], "checkout");
    assert_eq!(services[0]["error_count"], 2);

    let issues: Value = http
        .get(format!("{base}/v1/issues"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(issues.as_array().unwrap().len(), 1);
    assert_eq!(issues[0]["kind"], "PaymentError");
    assert_eq!(issues[0]["count"], 2);
    let fingerprint = issues[0]["fingerprint"].as_str().unwrap();
    let events: Value = http
        .get(format!("{base}/v1/issues/{fingerprint}/events"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(events.as_array().unwrap().len(), 2);

    let status: Value = http
        .get(format!("{base}/v1/traces/status"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(status["last_span_at"].is_i64());

    let trends: Value = http
        .get(format!("{base}/v1/overview/trends"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let error_total: f64 = trends["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|point| point["value"].as_f64().unwrap())
        .sum();
    assert_eq!(error_total, 2.0);
    assert!(trends["cpu"].as_array().unwrap().is_empty());
    let listed: Value = http
        .get(format!("{base}/v1/ingest-tokens"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(listed[0]["last_used_at"].is_string());
    assert!(listed[0].get("token").is_none());

    let deleted = http
        .delete(format!("{base}/v1/ingest-tokens/{token_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let revoked = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
}
