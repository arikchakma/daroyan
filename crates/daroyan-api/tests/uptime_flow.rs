mod common;

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::StatusCode as AxumStatus;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use daroyan_api::app::{build_app, open_state};
use daroyan_core::alerts::engine::AlertEngine;
use daroyan_core::config::Config;
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

async fn run(http: &Client, base: &str, check_id: &str) -> Value {
    http.post(format!("{base}/v1/uptime-checks/{check_id}/run"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn opens_and_resolves_outages_as_a_site_goes_down_and_up() {
    let dir = std::env::temp_dir().join(format!("daroyan-uptime-{}", std::process::id()));
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
    let mut engine = AlertEngine::new(
        state.database.clone(),
        state.agents.clone(),
        state.analytics.clone(),
        state.config.app_url.clone(),
    );
    let base = serve(build_app(state)).await;

    let is_up = Arc::new(AtomicBool::new(true));
    let site_state = Arc::clone(&is_up);
    let is_slow = Arc::new(AtomicBool::new(false));
    let slow_state = Arc::clone(&is_slow);
    let site = serve(
        Router::new()
            .route(
                "/old",
                get(|| async {
                    (AxumStatus::MOVED_PERMANENTLY, [("location", "/health")])
                }),
            )
            .route(
                "/slow",
                get(move || {
                    let is_slow = slow_state.load(Ordering::SeqCst);
                    async move {
                        if is_slow {
                            tokio::time::sleep(Duration::from_millis(300)).await;
                        }
                        "ok"
                    }
                }),
            )
            .route(
                "/health",
                get(move || {
                    let is_up = site_state.load(Ordering::SeqCst);
                    async move {
                        if is_up {
                            (AxumStatus::OK, "ok")
                        } else {
                            (AxumStatus::BAD_GATEWAY, "down")
                        }
                    }
                }),
            ),
    )
    .await;

    let received = Arc::new(Mutex::new(Vec::<Value>::new()));
    let hook_store = Arc::clone(&received);
    let hook = serve(Router::new().route(
        "/hook",
        post(move |Json(body): Json<Value>| async move {
            hook_store.lock().unwrap().push(body);
        }),
    ))
    .await;
    let integration: Value = http
        .post(format!("{base}/v1/integrations"))
        .json(&json!({ "name": "Ops hook", "config": { "kind": "webhook", "url": format!("{hook}/hook") } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let rejected = http
        .post(format!("{base}/v1/uptime-checks"))
        .json(&json!({ "name": "API", "kind": "http", "target": "ftp://nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);

    let long_name = http
        .post(format!("{base}/v1/uptime-checks"))
        .json(&json!({ "name": "a".repeat(101), "kind": "http", "target": "https://example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(long_name.status(), StatusCode::BAD_REQUEST);
    let long_name: Value = long_name.json().await.unwrap();
    assert_eq!(long_name["message"], "Add a name of up to 100 characters");

    let check: Value = http
        .post(format!("{base}/v1/uptime-checks"))
        .json(&json!({
            "name": "API",
            "kind": "http",
            "target": format!("{site}/old"),
            "body_contains": "ok",
            "timeout_secs": 5,
            "interval_secs": 30
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let check_id = check["id"].as_str().unwrap().to_string();
    assert_eq!(check["status"], "pending");

    let titles = || -> Vec<String> {
        received
            .lock()
            .unwrap()
            .iter()
            .map(|body| body["title"].as_str().unwrap_or_default().to_string())
            .collect()
    };
    let down_rule: Value = http
        .post(format!("{base}/v1/alert-rules"))
        .json(&json!({
            "name": "API Down",
            "condition": { "kind": "uptime_down" },
            "scope": { "kind": "checks", "check_ids": [check_id] },
            "integration_ids": [integration["id"]]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(down_rule["summary"], "Uptime check down");
    let mismatched = http
        .post(format!("{base}/v1/alert-rules"))
        .json(&json!({
            "name": "Wrong",
            "condition": { "kind": "uptime_down" },
            "scope": { "kind": "label", "label": "production" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(mismatched.status(), StatusCode::BAD_REQUEST);

    let first = run(&http, &base, &check_id).await;
    assert_eq!(first["status"], "up");
    assert_eq!(first["code"], 200);
    assert!(first["dns_ms"].is_number());
    assert!(first["connect_ms"].is_number());
    assert!(first["ttfb_ms"].is_number());
    assert!(first["tls_ms"].is_null());

    is_up.store(false, Ordering::SeqCst);
    let failed = run(&http, &base, &check_id).await;
    assert_eq!(failed["status"], "down");
    assert_eq!(failed["error"], "502 Bad Gateway");
    run(&http, &base, &check_id).await;
    engine.check_alerts(Utc::now()).await.unwrap();
    is_up.store(true, Ordering::SeqCst);
    run(&http, &base, &check_id).await;
    engine.check_alerts(Utc::now()).await.unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(titles(), ["Critical: API Down", "Resolved: API Down"]);

    let checks: Vec<Value> = http
        .get(format!("{base}/v1/uptime-checks"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(checks[0]["status"], "up");
    assert_eq!(checks[0]["uptime"], 0.5);
    assert_eq!(checks[0]["days"].as_array().unwrap().len(), 30);

    let stats: Value = http
        .get(format!("{base}/v1/uptime-checks/{check_id}/stats"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(stats["stats"]["checks_today"], 4);
    assert_eq!(stats["recent"].as_array().unwrap().len(), 4);
    let outages = stats["outages"].as_array().unwrap();
    assert_eq!(outages.len(), 1);
    assert_eq!(outages[0]["cause"], "502 Bad Gateway");
    assert!(outages[0]["resolved_at"].is_string());

    let slow_check: Value = http
        .post(format!("{base}/v1/uptime-checks"))
        .json(&json!({
            "name": "Slow API",
            "kind": "http",
            "target": format!("{site}/slow"),
            "interval_secs": 30
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let slow_id = slow_check["id"].as_str().unwrap().to_string();
    let slow_rule: Value = http
        .post(format!("{base}/v1/alert-rules"))
        .json(&json!({
            "name": "Slow Responses",
            "condition": { "kind": "response_time", "above": 1000.0 },
            "scope": { "kind": "checks", "check_ids": [slow_id] },
            "integration_ids": [integration["id"]],
            "options": {
                "warning": 200.0,
                "remind_every_secs": 300,
                "remind_times": 1,
                "severity_label": "P2",
                "warning_integration_ids": [integration["id"]]
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(slow_rule["summary"], "Response time above 1 s, warning at 200 ms");
    is_slow.store(true, Ordering::SeqCst);
    assert_eq!(run(&http, &base, &slow_id).await["status"], "up");
    let now = Utc::now();
    engine.check_alerts(now).await.unwrap();
    let alerts: Vec<Value> = http
        .get(format!("{base}/v1/alerts?status=firing"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(alerts[0]["severity"], "warning");
    assert_eq!(alerts[0]["target_name"], "Slow API");
    let checks: Vec<Value> = http
        .get(format!("{base}/v1/uptime-checks"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let slow_view = checks.iter().find(|check| check["id"] == slow_id.as_str()).unwrap();
    assert_eq!(slow_view["status"], "degraded");
    engine
        .check_alerts(now + chrono::Duration::seconds(301))
        .await
        .unwrap();
    engine
        .check_alerts(now + chrono::Duration::seconds(700))
        .await
        .unwrap();
    let slow_rule_id = slow_rule["id"].as_str().unwrap();
    http.patch(format!("{base}/v1/alert-rules/{slow_rule_id}"))
        .json(&json!({ "condition": { "kind": "response_time", "above": 250.0 } }))
        .send()
        .await
        .unwrap();
    run(&http, &base, &slow_id).await;
    engine
        .check_alerts(now + chrono::Duration::seconds(710))
        .await
        .unwrap();
    is_slow.store(false, Ordering::SeqCst);
    run(&http, &base, &slow_id).await;
    engine
        .check_alerts(now + chrono::Duration::seconds(720))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        titles()[2..],
        [
            "[P2] Warning: Slow Responses",
            "[P2] Still firing: Slow Responses",
            "[P2] Critical: Slow Responses",
            "[P2] Resolved: Slow Responses"
        ]
    );

    let deleted = http
        .delete(format!("{base}/v1/uptime-checks/{check_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    std::fs::remove_dir_all(dir).ok();
}
