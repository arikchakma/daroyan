mod common;

use std::net::SocketAddr;

use axum::Router;
use axum::routing::get;
use daroyan_api::app::{build_app, open_state};
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

async fn create(http: &Client, url: String, body: Value) -> Value {
    let response = http.post(url).json(&body).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED, "{body}");
    response.json().await.unwrap()
}

#[tokio::test]
async fn publishes_monitor_status_on_a_public_page() {
    let dir =
        std::env::temp_dir().join(format!("daroyan-status-pages-{}", std::process::id()));
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
    let anonymous = Client::new();
    let base = serve(build_app(state)).await;
    let site = serve(Router::new().route("/health", get(|| async { "ok" }))).await;

    let check = create(
        &http,
        format!("{base}/v1/uptime-checks"),
        json!({ "name": "api-prod-health", "kind": "http", "target": format!("{site}/health") }),
    )
    .await;
    let check_id = check["id"].as_str().unwrap().to_string();
    let run = http
        .post(format!("{base}/v1/uptime-checks/{check_id}/run"))
        .send()
        .await
        .unwrap();
    assert!(run.status().is_success());
    let cron = create(
        &http,
        format!("{base}/v1/crons"),
        json!({ "name": "contacts.sync", "schedule_kind": "cron", "expression": "*/5 * * * *" }),
    )
    .await;
    let cron_id = cron["id"].as_str().unwrap().to_string();
    let key = cron["ping_key"].as_str().unwrap().to_string();
    assert!(
        anonymous
            .post(format!("{base}/p/{key}"))
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );

    let sections = json!([
        { "name": "API", "components": [{ "kind": "check", "id": check_id, "name": "Public API" }] },
        { "name": "Background jobs", "components": [{ "kind": "cron", "id": cron_id, "name": "Contact sync" }] }
    ]);
    for (body, status) in [
        (json!({ "name": "Acme", "slug": "Acme Status" }), StatusCode::BAD_REQUEST),
        (json!({ "name": " ", "slug": "acme" }), StatusCode::BAD_REQUEST),
        (
            json!({ "name": "Acme", "slug": "acme", "logo_url": "https://acme.test/logo.png" }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "name": "Acme", "slug": "acme", "sections": [{ "name": "API", "components": [{ "kind": "check", "id": "chk_missing", "name": "API" }] }] }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "name": "Acme", "slug": "acme", "sections": [{ "name": "API", "components": [{ "kind": "server", "id": cron_id, "name": "API" }] }] }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "name": "Acme", "slug": "acme", "sections": [{ "name": "API", "components": [
                { "kind": "check", "id": check_id, "name": "API" },
                { "kind": "check", "id": check_id, "name": "API again" }
            ] }] }),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = http
            .post(format!("{base}/v1/status-pages"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{body}");
    }

    let unauthorized =
        anonymous.get(format!("{base}/v1/status-pages")).send().await.unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let page = create(
        &http,
        format!("{base}/v1/status-pages"),
        json!({ "name": "Acme", "slug": "acme", "logo_url": "data:image/png;base64,iVBORw0KGgo=", "sections": sections }),
    )
    .await;
    let page_id = page["id"].as_str().unwrap().to_string();
    assert_eq!(page["component_count"], 2);
    assert_eq!(page["status"], "operational");
    assert_eq!(page["is_public"], true);

    let taken = http
        .post(format!("{base}/v1/status-pages"))
        .json(&json!({ "name": "Other", "slug": "acme" }))
        .send()
        .await
        .unwrap();
    assert_eq!(taken.status(), StatusCode::CONFLICT);

    let response = anonymous.get(format!("{base}/v1/status/acme")).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "public, max-age=30");
    let public: Value = response.json().await.unwrap();
    assert_eq!(public["name"], "Acme");
    assert_eq!(public["status"], "operational");
    let text = public.to_string();
    for internal in ["api-prod-health", "contacts.sync", &check_id, &cron_id, &key] {
        assert!(!text.contains(internal), "leaked {internal}");
    }
    let api = &public["sections"][0]["components"][0];
    assert_eq!(api["name"], "Public API");
    assert_eq!(api["status"], "operational");
    assert_eq!(api["uptime"], 1.0);
    assert_eq!(api["days"].as_array().unwrap().len(), 90);
    assert_eq!(api["days"][89]["total"], 1);
    assert_eq!(public["sections"][1]["components"][0]["status"], "operational");

    let failed = anonymous.post(format!("{base}/p/{key}/fail")).send().await.unwrap();
    assert!(failed.status().is_success());
    let public: Value = anonymous
        .get(format!("{base}/v1/status/acme"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(public["status"], "major");
    let jobs = &public["sections"][1]["components"][0];
    assert_eq!(jobs["status"], "outage");
    assert_eq!(jobs["uptime"], 0.5);
    assert!(public["active_incidents"].as_array().unwrap().is_empty());
    assert!(public["past_incidents"].as_array().unwrap().is_empty());

    let mut body = json!({ "name": "Acme", "slug": "acme-status", "is_public": false, "sections": sections });
    let updated: Value = http
        .put(format!("{base}/v1/status-pages/{page_id}"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(updated["slug"], "acme-status");
    assert_eq!(updated["status"], "major");
    let old = anonymous.get(format!("{base}/v1/status/acme")).send().await.unwrap();
    assert_eq!(old.status(), StatusCode::NOT_FOUND);
    let hidden = anonymous
        .get(format!("{base}/v1/status/acme-status"))
        .send()
        .await
        .unwrap();
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
    let preview = http
        .get(format!("{base}/v1/status/acme-status"))
        .send()
        .await
        .unwrap();
    assert_eq!(preview.status(), StatusCode::OK);
    assert_eq!(preview.headers()["cache-control"], "private, no-store");

    let deleted = http
        .delete(format!("{base}/v1/crons/{cron_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let public: Value = http
        .get(format!("{base}/v1/status/acme-status"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(public["status"], "operational");
    assert_eq!(public["sections"].as_array().unwrap().len(), 1);
    assert!(public["maintenance"].as_array().unwrap().is_empty());
    let listed: Value = http
        .get(format!("{base}/v1/status-pages"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed[0]["status"], "operational");

    body["sections"] = json!([]);
    body["slug"] = json!("acme-status");
    let saved = http
        .put(format!("{base}/v1/status-pages/{page_id}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);

    let removed = http
        .delete(format!("{base}/v1/status-pages/{page_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let gone = http
        .get(format!("{base}/v1/status/acme-status"))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
}
