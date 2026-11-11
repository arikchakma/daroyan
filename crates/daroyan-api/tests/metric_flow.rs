mod common;

use std::net::SocketAddr;
use std::path::Path;

use axum::Router;
use daroyan_api::AppState;
use daroyan_api::app::{build_app, open_state};
use daroyan_core::analytics::Analytics;
use daroyan_core::config::{Config, MetricSettings};
use daroyan_core::db::queries::servers::NewServer;
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use opentelemetry_proto::tonic::common::v1::any_value::Value as AnyValueKind;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::metrics::v1::metric::Data;
use opentelemetry_proto::tonic::metrics::v1::number_data_point::Value as NumberValue;
use opentelemetry_proto::tonic::metrics::v1::{
    AggregationTemporality, Exemplar, Gauge, Histogram, HistogramDataPoint, Metric,
    NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum, Summary, SummaryDataPoint,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use prost::Message;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio::net::TcpListener;

const MINUTE_NANOS: u64 = 60_000_000_000;

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

fn request(instance: &str, metrics: Vec<Metric>) -> ExportMetricsServiceRequest {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![
                    text("service.name", "inboxjot-api"),
                    text("service.instance.id", instance),
                    text("host.name", "inboxjot"),
                ],
                ..Default::default()
            }),
            scope_metrics: vec![ScopeMetrics { metrics, ..Default::default() }],
            ..Default::default()
        }],
    }
}

fn counter(start: u64, at: u64, value: i64) -> Metric {
    Metric {
        name: "emails.sent".to_string(),
        unit: "{email}".to_string(),
        data: Some(Data::Sum(Sum {
            data_points: vec![NumberDataPoint {
                attributes: vec![text("provider", "ses")],
                start_time_unix_nano: start,
                time_unix_nano: at,
                value: Some(NumberValue::AsInt(value)),
                ..Default::default()
            }],
            aggregation_temporality: AggregationTemporality::Cumulative as i32,
            is_monotonic: true,
        })),
        ..Default::default()
    }
}

fn gauge(name: &str, routes: &[&str], at: u64) -> Metric {
    Metric {
        name: name.to_string(),
        data: Some(Data::Gauge(Gauge {
            data_points: routes
                .iter()
                .map(|route| NumberDataPoint {
                    attributes: vec![text("route", route)],
                    time_unix_nano: at,
                    value: Some(NumberValue::AsDouble(340.0)),
                    ..Default::default()
                })
                .collect(),
        })),
        ..Default::default()
    }
}

async fn open_test_state(dir: &Path, metrics: MetricSettings) -> AppState {
    let state = open_state(Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        database_url: format!("sqlite://{}?mode=rwc", dir.join("daroyan.db").display()),
        duckdb_path: dir.join("daroyan.duckdb"),
        retention_days: 30,
        log_retention_days: 7,
        otlp_log_budget: 4 * 1024,
        metrics,
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

    state
}

async fn create_token(http: &Client, base: &str) -> String {
    let created: Value = http
        .post(format!("{base}/v1/ingest-tokens"))
        .json(&json!({ "name": "inboxjot" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    created["token"].as_str().unwrap().to_string()
}

async fn send(
    client: &Client,
    endpoint: &str,
    token: &str,
    body: &ExportMetricsServiceRequest,
) -> ExportMetricsServiceResponse {
    let response = client
        .post(endpoint)
        .bearer_auth(token)
        .body(body.encode_to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    ExportMetricsServiceResponse::decode(response.bytes().await.unwrap()).unwrap()
}

async fn read_deltas(
    analytics: &Analytics,
    name: &'static str,
) -> Vec<(String, Option<f64>)> {
    analytics
        .read(move |conn| {
            conn.prepare(
                "SELECT s.instance, p.delta FROM metric_points p
                 JOIN metric_series s USING (series_id)
                 WHERE s.name = ? ORDER BY s.instance, p.taken_at",
            )?
            .query_map([name], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn stores_metrics_and_works_out_counter_changes() {
    let dir =
        std::env::temp_dir().join(format!("daroyan-metrics-{}", std::process::id()));
    let state = open_test_state(&dir, MetricSettings::default()).await;
    let analytics = state.analytics.clone();
    let http = common::sign_in(&state.database).await;
    let anonymous = Client::new();
    let base = serve(build_app(state)).await;
    let endpoint = format!("{base}/v1/otlp/v1/metrics");
    let token = create_token(&http, &base).await;

    let missing = anonymous.post(&endpoint).body(Vec::new()).send().await.unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let start = now_nanos() - 3 * MINUTE_NANOS;
    let response = send(
        &anonymous,
        &endpoint,
        &token,
        &request(
            "worker-1",
            vec![
                counter(start, start + MINUTE_NANOS, 5),
                gauge("queue.depth", &["/send"], start + MINUTE_NANOS),
                Metric {
                    name: "job.duration".to_string(),
                    unit: "s".to_string(),
                    data: Some(Data::Histogram(Histogram {
                        data_points: vec![HistogramDataPoint {
                            start_time_unix_nano: start,
                            time_unix_nano: start + MINUTE_NANOS,
                            count: 3,
                            sum: Some(4.5),
                            explicit_bounds: vec![1.0, 2.0],
                            bucket_counts: vec![1, 1, 1],
                            exemplars: vec![Exemplar {
                                trace_id: vec![0xab; 16],
                                span_id: vec![1; 8],
                                ..Default::default()
                            }],
                            ..Default::default()
                        }],
                        aggregation_temporality: AggregationTemporality::Delta as i32,
                    })),
                    ..Default::default()
                },
                Metric {
                    name: "legacy.latency".to_string(),
                    data: Some(Data::Summary(Summary {
                        data_points: vec![SummaryDataPoint::default()],
                    })),
                    ..Default::default()
                },
            ],
        ),
    )
    .await;
    let partial = response.partial_success.unwrap();
    assert_eq!(partial.rejected_data_points, 1);
    assert!(partial.error_message.contains("summaries"));

    let later = send(
        &anonymous,
        &endpoint,
        &token,
        &request("worker-1", vec![counter(start, start + 2 * MINUTE_NANOS, 12)]),
    )
    .await;
    assert_eq!(later.partial_success, None);

    let restarted = start + 2 * MINUTE_NANOS + 1;
    send(
        &anonymous,
        &endpoint,
        &token,
        &request("worker-1", vec![counter(restarted, start + 3 * MINUTE_NANOS, 2)]),
    )
    .await;
    send(
        &anonymous,
        &endpoint,
        &token,
        &request("worker-2", vec![counter(start, start + 2 * MINUTE_NANOS, 40)]),
    )
    .await;

    let json = anonymous
        .post(&endpoint)
        .bearer_auth(&token)
        .json(&request("worker-2", vec![counter(start, start + 3 * MINUTE_NANOS, 46)]))
        .send()
        .await
        .unwrap();
    assert_eq!(json.status(), StatusCode::OK);
    assert_eq!(json.headers()["content-type"], "application/json");

    analytics.flush().await.unwrap();

    assert_eq!(
        read_deltas(&analytics, "emails.sent").await,
        [
            ("worker-1".to_string(), Some(5.0)),
            ("worker-1".to_string(), Some(7.0)),
            ("worker-1".to_string(), Some(2.0)),
            ("worker-2".to_string(), Some(40.0)),
            ("worker-2".to_string(), Some(6.0)),
        ]
    );

    let histogram: (String, String, String, Option<String>) = analytics
        .read(|conn| {
            conn.query_row(
                "SELECT s.server_id, CAST(s.bounds AS VARCHAR), CAST(p.bucket_deltas AS VARCHAR), p.trace_id
                 FROM metric_points p JOIN metric_series s USING (series_id)
                 WHERE s.name = 'job.duration'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
        })
        .await
        .unwrap();
    assert_eq!(
        histogram,
        (
            "srv_inboxjot".to_string(),
            "[1.0, 2.0]".to_string(),
            "[1, 1, 1]".to_string(),
            Some("ab".repeat(16)),
        )
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn drops_new_series_past_the_limit_and_counts_them() {
    let dir = std::env::temp_dir()
        .join(format!("daroyan-metric-limits-{}", std::process::id()));
    let limits = MetricSettings {
        series_limit: 3,
        series_per_metric: 2,
        ..MetricSettings::default()
    };
    let state = open_test_state(&dir, limits).await;
    let analytics = state.analytics.clone();
    let http = common::sign_in(&state.database).await;
    let anonymous = Client::new();
    let base = serve(build_app(state)).await;
    let endpoint = format!("{base}/v1/otlp/v1/metrics");
    let token = create_token(&http, &base).await;

    let at = now_nanos();
    let response = send(
        &anonymous,
        &endpoint,
        &token,
        &request(
            "worker-1",
            vec![
                gauge("queue.depth", &["/a", "/b", "/c"], at),
                gauge("pool.in_use", &["/a", "/b"], at),
            ],
        ),
    )
    .await;
    let partial = response.partial_success.unwrap();
    assert_eq!(partial.rejected_data_points, 2);
    assert!(partial.error_message.contains("queue.depth"), "{}", partial.error_message);

    let again = send(
        &anonymous,
        &endpoint,
        &token,
        &request(
            "worker-1",
            vec![gauge("queue.depth", &["/a", "/b"], at + MINUTE_NANOS)],
        ),
    )
    .await;
    assert_eq!(again.partial_success, None);

    analytics.flush().await.unwrap();
    let points: i64 = analytics
        .read(|conn| {
            conn.query_row("SELECT count(*) FROM metric_points", [], |row| row.get(0))
        })
        .await
        .unwrap();
    assert_eq!(points, 5);

    let tokens: Value = http
        .get(format!("{base}/v1/ingest-tokens"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(tokens[0]["rejected_metric_points"], 2);

    std::fs::remove_dir_all(&dir).ok();
}
