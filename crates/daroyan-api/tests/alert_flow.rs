mod common;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use daroyan_api::app::{build_app, open_state};
use daroyan_core::alerts::engine::AlertEngine;
use daroyan_core::config::Config;
use futures_util::SinkExt;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

fn make_report(cpu: f64) -> Value {
    json!({
        "type": "report",
        "host": {
            "name": "worker-1", "labels": ["production"], "hostname": "worker-1",
            "platform": "Ubuntu", "platform_version": "24.04", "kernel": "6.8.0", "arch": "x86_64",
            "cpu": "EPYC", "cpu_cores": 8, "mem_total": 16_000, "swap_total": 2_000,
            "private_ip": "10.0.1.21", "boot_time": 1, "version": "0.1.0"
        },
        "state": {
            "taken_at": Utc::now().timestamp_millis(), "uptime": 100, "cpu": cpu,
            "load1": 1.0, "load5": 0.5, "load15": 0.2, "mem_used": 8_000, "swap_used": 0,
            "process_count": 120, "tcp_conn_count": 10, "udp_conn_count": 2,
            "disks": [], "nics": []
        },
        "processes": [],
        "ports": [],
        "events": []
    })
}

#[tokio::test]
async fn fires_and_resolves_alerts_through_a_webhook() {
    let dir = std::env::temp_dir().join(format!("daroyan-alerts-{}", std::process::id()));
    let state = open_state(Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        database_url: format!("sqlite://{}?mode=rwc", dir.join("daroyan.db").display()),
        duckdb_path: dir.join("daroyan.duckdb"),
        retention_days: 30,
        log_retention_days: 7,
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
    let http = common::sign_in(&state.database).await;
    let database = state.database.clone();
    let analytics = state.analytics.clone();
    let mut engine = AlertEngine::new(
        state.database.clone(),
        state.agents.clone(),
        state.analytics.clone(),
        state.config.app_url.clone(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(
            listener,
            build_app(state).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
    });

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
    let events = || -> Vec<String> {
        received
            .lock()
            .unwrap()
            .iter()
            .map(|body| body["event"].as_str().unwrap_or_default().to_string())
            .collect()
    };

    let providers: Value = http
        .get(format!("{base}/v1/integrations/providers"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(providers["slack"], false);
    let manifest_url =
        reqwest::Url::parse(providers["slack_manifest_url"].as_str().unwrap()).unwrap();
    let manifest: Value = manifest_url
        .query_pairs()
        .find(|(key, _)| key == "manifest_json")
        .map(|(_, value)| serde_json::from_str(&value).unwrap())
        .unwrap();
    assert_eq!(
        manifest["oauth_config"]["redirect_urls"][0],
        "https://daroyan.example/v1/integrations/slack/callback"
    );

    let rejected = http.post(format!("{base}/v1/integrations"))
        .json(&json!({ "name": "Ops", "config": {
            "kind": "slack", "api_url": "https://slack.com", "bot_token": "pasted", "channel_id": "C1", "channel_name": "ops"
        } }))
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);

    let integration: Value = http.post(format!("{base}/v1/integrations"))
        .json(&json!({ "name": "Ops hook", "config": { "kind": "webhook", "url": hook_url } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let integration_id = integration["id"].as_str().unwrap().to_string();
    assert_eq!(integration["kind"], "webhook");

    let tested: Value = http
        .post(format!("{base}/v1/integrations/{integration_id}/test"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(tested["last_status"], "ok");
    assert_eq!(events(), ["test"]);

    let rule: Value = http
        .post(format!("{base}/v1/alert-rules"))
        .json(&json!({
            "name": "High CPU",
            "condition": { "kind": "cpu", "above": 30.0 },
            "for_secs": 60,
            "scope": { "kind": "label", "label": "production" },
            "integration_ids": [integration_id]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(rule["summary"], "CPU above 30% for 1m");

    let install: Value = http
        .post(format!("{base}/v1/install-tokens"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let enrolled: Value = http
        .post(format!("{base}/v1/agent/enroll"))
        .bearer_auth(install["token"].as_str().unwrap())
        .json(&json!({ "host": make_report(0.0)["host"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut request = format!("{}/v1/agent/connect", base.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {}", enrolled["token"].as_str().unwrap())
            .parse()
            .unwrap(),
    );
    let (mut agent, _) = connect_async(request).await.unwrap();
    agent
        .send(Message::text(make_report(34.0).to_string()))
        .await
        .unwrap();
    wait_for_cpu(&http, &base, 34.0).await;

    let now = Utc::now();
    engine.check_alerts(now).await.unwrap();
    assert!(list_alerts(&http, &base, "firing").await.is_empty());
    engine
        .check_alerts(now + chrono::Duration::seconds(61))
        .await
        .unwrap();
    let firing = list_alerts(&http, &base, "firing").await;
    assert_eq!(firing.len(), 1);
    assert_eq!(firing[0]["rule_name"], "High CPU");
    assert_eq!(firing[0]["target_name"], "worker-1");
    assert_eq!(firing[0]["subject"], "CPU");
    assert_eq!(firing[0]["value"], "34%");
    wait_until(|| events().len() == 2).await;
    let rule_id = rule["id"].as_str().unwrap();
    let deliveries: Vec<Value> = http
        .get(format!("{base}/v1/alert-rules/{rule_id}/deliveries"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0]["status"], "sent");
    assert_eq!(deliveries[0]["message"], "Critical: High CPU · worker-1 · CPU: 34%");
    let backtest: Value = http
        .post(format!("{base}/v1/alert-rules/backtest"))
        .json(&json!({
            "condition": { "kind": "port_closed" },
            "scope": { "kind": "all" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(backtest["is_supported"], false);
    let firing_body = received.lock().unwrap()[1].clone();
    assert_eq!(firing_body["event"], "alert.firing");
    assert_eq!(firing_body["title"], "Critical: High CPU");
    assert!(
        firing_body["url"]
            .as_str()
            .unwrap()
            .starts_with("https://daroyan.example/servers/")
    );

    engine
        .check_alerts(now + chrono::Duration::seconds(75))
        .await
        .unwrap();
    assert_eq!(list_alerts(&http, &base, "firing").await.len(), 1);

    agent
        .send(Message::text(make_report(10.0).to_string()))
        .await
        .unwrap();
    wait_for_cpu(&http, &base, 10.0).await;
    engine
        .check_alerts(now + chrono::Duration::seconds(90))
        .await
        .unwrap();
    assert!(list_alerts(&http, &base, "firing").await.is_empty());
    assert_eq!(list_alerts(&http, &base, "resolved").await.len(), 1);
    wait_until(|| events().len() == 3).await;
    assert_eq!(events(), ["test", "alert.firing", "alert.resolved"]);

    let log_rule: Value = http
        .post(format!("{base}/v1/alert-rules"))
        .json(&json!({
            "name": "API Errors",
            "condition": { "kind": "logs", "level": "error", "count": 3, "within_secs": 300, "service": "Linkity API", "text": "ses" },
            "scope": { "kind": "all" },
            "integration_ids": [integration_id]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        log_rule["summary"],
        "3+ error lines in 5m from Linkity API matching \"ses\""
    );
    let at = Utc::now().timestamp_millis();
    let line = |offset: i64, severity: u8, message: &str| json!({"at": at + offset, "service": "Linkity API", "stream": "stderr", "severity": severity, "message": message});
    agent
        .send(Message::text(
            json!({"type": "logs", "lines": [
                line(0, 17, "SES SendEmail failed"),
                line(1, 17, "SES SendEmail failed"),
                line(2, 9, "SES SendEmail queued"),
                line(3, 17, "database timeout"),
            ]})
            .to_string(),
        ))
        .await
        .unwrap();
    wait_until_async(|| async {
        analytics.flush().await.unwrap();
        let page: Value = http
            .get(format!("{base}/v1/logs"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        page["lines"].as_array().unwrap().len() == 4
    })
    .await;
    engine
        .check_alerts(now + chrono::Duration::seconds(100))
        .await
        .unwrap();
    assert!(list_alerts(&http, &base, "firing").await.is_empty());

    agent
        .send(Message::text(
            json!({"type": "logs", "lines": [line(4, 17, "SES SendEmail failed again")]})
                .to_string(),
        ))
        .await
        .unwrap();
    wait_until_async(|| async {
        analytics.flush().await.unwrap();
        let page: Value = http
            .get(format!("{base}/v1/logs"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        page["lines"].as_array().unwrap().len() == 5
    })
    .await;
    engine
        .check_alerts(now + chrono::Duration::seconds(105))
        .await
        .unwrap();
    let firing = list_alerts(&http, &base, "firing").await;
    assert_eq!(firing.len(), 1);
    assert_eq!(firing[0]["subject"], "Linkity API");
    assert_eq!(firing[0]["value"], "3 error lines in 5m");
    wait_until(|| events().len() == 4).await;

    let log_backtest: Value = http
        .post(format!("{base}/v1/alert-rules/backtest"))
        .json(&json!({
            "condition": { "kind": "logs", "level": "error", "count": 3, "within_secs": 300, "service": "Linkity API", "text": "ses" },
            "scope": { "kind": "all" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(log_backtest["is_supported"], true);

    engine
        .check_alerts(now + chrono::Duration::seconds(500))
        .await
        .unwrap();
    assert!(list_alerts(&http, &base, "firing").await.is_empty());
    wait_until(|| events().len() == 5).await;
    assert_eq!(events().last().unwrap(), "alert.resolved");
    assert!(
        common::count_rows(
            &database,
            "alert_deliveries",
            "rule_id",
            log_rule["id"].as_str().unwrap()
        )
        .await
            > 0
    );
    assert!(
        common::count_rows(
            &database,
            "alerts",
            "rule_id",
            log_rule["id"].as_str().unwrap()
        )
        .await
            > 0
    );
    let deleted = http
        .delete(format!("{base}/v1/alert-rules/{}", log_rule["id"].as_str().unwrap()))
        .send()
        .await
        .unwrap();
    assert!(deleted.status().is_success());
    assert_eq!(
        common::count_rows(
            &database,
            "alert_deliveries",
            "rule_id",
            log_rule["id"].as_str().unwrap()
        )
        .await,
        0
    );
    assert_eq!(
        common::count_rows(
            &database,
            "alerts",
            "rule_id",
            log_rule["id"].as_str().unwrap()
        )
        .await,
        0
    );

    let offline: Value = http
        .post(format!("{base}/v1/alert-rules"))
        .json(&json!({
            "name": "Offline",
            "condition": { "kind": "offline" },
            "scope": { "kind": "all" },
            "integration_ids": []
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    agent.close(None).await.unwrap();
    wait_until_async(|| async {
        let servers: Value = http
            .get(format!("{base}/v1/servers"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        servers[0]["status"] == "down"
    })
    .await;
    engine
        .check_alerts(now + chrono::Duration::seconds(120))
        .await
        .unwrap();
    let firing = list_alerts(&http, &base, "firing").await;
    assert_eq!(firing.len(), 1);
    assert_eq!(firing[0]["rule_id"], offline["id"]);

    let disabled = http
        .patch(format!("{base}/v1/alert-rules/{}", offline["id"].as_str().unwrap()))
        .json(&json!({ "enabled": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::OK);
    engine
        .check_alerts(now + chrono::Duration::seconds(135))
        .await
        .unwrap();
    assert!(list_alerts(&http, &base, "firing").await.is_empty());

    std::fs::remove_dir_all(dir).ok();
}

async fn list_alerts(http: &Client, base: &str, status: &str) -> Vec<Value> {
    http.get(format!("{base}/v1/alerts?status={status}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

async fn wait_for_cpu(http: &Client, base: &str, cpu: f64) {
    wait_until_async(|| async {
        let servers: Value = http
            .get(format!("{base}/v1/servers"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        servers[0]["cpu"].as_f64() == Some(cpu)
    })
    .await;
}

async fn wait_until(mut check: impl FnMut() -> bool) {
    wait_until_async(|| std::future::ready(check())).await;
}

async fn wait_until_async<F, Fut>(mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    for _ in 0..50 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for the condition");
}
