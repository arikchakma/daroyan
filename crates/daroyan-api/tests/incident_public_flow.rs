mod common;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use daroyan_api::app::{build_app, open_state};
use daroyan_core::alerts::engine::AlertEngine;
use daroyan_core::config::Config;
use daroyan_core::incidents::run_maintenance;
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

async fn send(request: reqwest::RequestBuilder, expected: StatusCode) -> Value {
    let response = request.send().await.unwrap();
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, expected, "{body}");
    body
}

fn doc(text: &str) -> Value {
    json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": text }] }] })
}

async fn wait_for(received: &Arc<Mutex<Vec<Value>>>, text: &str) {
    for _ in 0..100 {
        if received
            .lock()
            .unwrap()
            .iter()
            .any(|body| body.to_string().contains(text))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no notification with {text}");
}

#[tokio::test]
async fn publishes_incidents_and_maintenance_on_status_pages() {
    let dir = std::env::temp_dir()
        .join(format!("daroyan-incident-public-{}", std::process::id()));
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
    let http = common::sign_in(&state.database).await;
    let anonymous = Client::new();
    let database = state.database.clone();
    let mut engine = AlertEngine::new(
        state.database.clone(),
        state.agents.clone(),
        state.analytics.clone(),
        state.config.app_url.clone(),
    );
    let base = serve(build_app(state)).await;
    let site = serve(Router::new().route("/health", get(|| async { "ok" }))).await;
    let received = Arc::new(Mutex::new(Vec::<Value>::new()));
    let store = Arc::clone(&received);
    let hook = serve(Router::new().route(
        "/hook",
        post(move |Json(body): Json<Value>| async move {
            store.lock().unwrap().push(body);
        }),
    ))
    .await;

    let integration = send(
        http.post(format!("{base}/v1/integrations")).json(&json!({
            "name": "Ops hook", "config": { "kind": "webhook", "url": format!("{hook}/hook") }
        })),
        StatusCode::CREATED,
    )
    .await;
    let channel_id = integration["id"].as_str().unwrap().to_string();
    let check = send(
        http.post(format!("{base}/v1/uptime-checks")).json(&json!({
            "name": "api-prod-health", "kind": "http", "target": format!("{site}/health")
        })),
        StatusCode::CREATED,
    )
    .await;
    let check_id = check["id"].as_str().unwrap().to_string();
    let run = http
        .post(format!("{base}/v1/uptime-checks/{check_id}/run"))
        .send()
        .await
        .unwrap();
    assert!(run.status().is_success());
    let cron = send(
        http.post(format!("{base}/v1/crons")).json(&json!({
            "name": "contacts.sync", "schedule_kind": "cron", "expression": "*/5 * * * *"
        })),
        StatusCode::CREATED,
    )
    .await;
    let cron_id = cron["id"].as_str().unwrap().to_string();
    let key = cron["ping_key"].as_str().unwrap().to_string();
    let page = send(
        http.post(format!("{base}/v1/status-pages")).json(&json!({
            "name": "Acme", "slug": "acme",
            "sections": [{ "name": "Core", "components": [
                { "kind": "check", "id": check_id, "name": "Public API" },
                { "kind": "cron", "id": cron_id, "name": "Contact sync" }
            ] }]
        })),
        StatusCode::CREATED,
    )
    .await;
    let page_id = page["id"].as_str().unwrap().to_string();

    let incident = send(
        http.post(format!("{base}/v1/incidents")).json(&json!({
            "kind": "incident",
            "title": "API is slow",
            "severity": "critical",
            "lead_id": "usr_test",
            "components": [{ "kind": "check", "id": check_id, "status": "degraded" }],
            "related": [{ "kind": "cron", "id": cron_id }],
            "channel_ids": [channel_id],
            "update": { "status": "investigating", "body": doc("Requests take <5> seconds.") }
        })),
        StatusCode::CREATED,
    )
    .await;
    let incident_id = incident["id"].as_str().unwrap().to_string();
    wait_for(&received, "[Critical] API is slow — Investigating").await;
    send(
        http.post(format!("{base}/v1/incidents/{incident_id}/updates"))
            .json(&json!({ "kind": "note", "body": doc("Secret kernel note") })),
        StatusCode::CREATED,
    )
    .await;
    send(
        http.post(format!("{base}/v1/incidents/{incident_id}/updates")).json(&json!({
            "kind": "update", "status": "identified", "body": doc("Found it."), "notify": false
        })),
        StatusCode::CREATED,
    )
    .await;

    let public =
        send(anonymous.get(format!("{base}/v1/status/acme")), StatusCode::OK).await;
    assert_eq!(public["status"], "major");
    let active = &public["active_incidents"][0];
    assert_eq!(active["title"], "API is slow");
    assert_eq!(active["status"], "identified");
    assert_eq!(
        active["components"],
        json!([{ "name": "Public API", "status": "degraded" }])
    );
    assert_eq!(active["latest_update"]["body"], doc("Found it."));
    assert_eq!(public["past_incidents"][0]["id"], incident_id.as_str());
    assert_eq!(public["sections"][0]["components"][0]["status"], "degraded");
    let text = public.to_string();
    for internal in [
        "Secret kernel note",
        "Ops",
        "api-prod-health",
        "contacts.sync",
        &check_id,
        &cron_id,
    ] {
        assert!(!text.contains(internal), "leaked {internal}");
    }

    let detail = send(
        anonymous.get(format!("{base}/v1/status/acme/incidents/{incident_id}")),
        StatusCode::OK,
    )
    .await;
    assert_eq!(detail["updates"].as_array().unwrap().len(), 2);
    assert!(!detail.to_string().contains("Secret kernel note"));
    assert!(detail["postmortem"].is_null());

    let rss = anonymous
        .get(format!("{base}/v1/status/acme/feed.xml"))
        .send()
        .await
        .unwrap();
    assert_eq!(rss.status(), StatusCode::OK);
    assert!(
        rss.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/xml")
    );
    let rss = rss.text().await.unwrap();
    assert!(rss.contains("<title>API is slow</title>"));
    assert!(rss.contains("&lt;p&gt;Found it.&lt;/p&gt;"));
    assert!(rss.contains("https://daroyan.example/s/acme/incidents/"));
    let legacy_rss = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(format!("{base}/v1/status/acme/feed.rss"))
        .send()
        .await
        .unwrap();
    assert_eq!(legacy_rss.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(legacy_rss.headers()["location"], "/v1/status/acme/feed.xml");
    let atom = anonymous
        .get(format!("{base}/v1/status/acme/feed.atom"))
        .send()
        .await
        .unwrap();
    assert!(
        atom.text()
            .await
            .unwrap()
            .contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">")
    );

    let edit = json!({
        "title": "API is slow",
        "severity": "critical",
        "components": [{ "kind": "check", "id": check_id, "status": "degraded" }],
        "channel_ids": [channel_id],
        "hidden_page_ids": [page_id],
    });
    send(
        http.put(format!("{base}/v1/incidents/{incident_id}")).json(&edit),
        StatusCode::OK,
    )
    .await;
    send(
        anonymous.get(format!("{base}/v1/status/acme/incidents/{incident_id}")),
        StatusCode::NOT_FOUND,
    )
    .await;
    let public =
        send(anonymous.get(format!("{base}/v1/status/acme")), StatusCode::OK).await;
    assert!(public["active_incidents"].as_array().unwrap().is_empty());
    assert_eq!(public["status"], "operational");
    let mut edit = edit;
    edit["hidden_page_ids"] = json!([]);
    send(
        http.put(format!("{base}/v1/incidents/{incident_id}")).json(&edit),
        StatusCode::OK,
    )
    .await;

    send(
        http.post(format!("{base}/v1/incidents/{incident_id}/updates")).json(
            &json!({ "kind": "update", "status": "resolved", "body": doc("Fixed.") }),
        ),
        StatusCode::CREATED,
    )
    .await;
    wait_for(&received, "API is slow — Resolved").await;
    send(
        http.put(format!("{base}/v1/incidents/{incident_id}/postmortem"))
            .json(&json!({ "postmortem": doc("A kernel update.") })),
        StatusCode::OK,
    )
    .await;
    send(
        http.delete(format!("{base}/v1/incidents/{incident_id}/postmortem")),
        StatusCode::BAD_REQUEST,
    )
    .await;
    send(
        http.post(format!("{base}/v1/incidents/{incident_id}/postmortem"))
            .json(&json!({ "notify": false })),
        StatusCode::OK,
    )
    .await;
    let detail = send(
        anonymous.get(format!("{base}/v1/status/acme/incidents/{incident_id}")),
        StatusCode::OK,
    )
    .await;
    assert_eq!(detail["postmortem"], doc("A kernel update."));
    let unpublished = send(
        http.delete(format!("{base}/v1/incidents/{incident_id}/postmortem")),
        StatusCode::OK,
    )
    .await;
    assert!(unpublished["postmortem_published_at"].is_null());
    let detail = send(
        anonymous.get(format!("{base}/v1/status/acme/incidents/{incident_id}")),
        StatusCode::OK,
    )
    .await;
    assert!(detail["postmortem"].is_null());
    let timeline = send(
        http.get(format!("{base}/v1/incidents/{incident_id}/updates")),
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        timeline[0].pointer("/body/content/0/content/0/text").unwrap(),
        "Unpublished the postmortem."
    );

    let now = Utc::now();
    let maintenance = send(
        http.post(format!("{base}/v1/incidents")).json(&json!({
            "kind": "maintenance",
            "title": "Database upgrade",
            "starts_at": (now + chrono::Duration::minutes(30)).to_rfc3339(),
            "ends_at": (now + chrono::Duration::minutes(90)).to_rfc3339(),
            "components": [{ "kind": "cron", "id": cron_id }],
            "channel_ids": [channel_id],
            "update": { "body": doc("We are upgrading Postgres.") }
        })),
        StatusCode::CREATED,
    )
    .await;
    let maintenance_id = maintenance["id"].as_str().unwrap().to_string();
    assert_eq!(maintenance["remind_before"], true);
    let public =
        send(anonymous.get(format!("{base}/v1/status/acme")), StatusCode::OK).await;
    assert_eq!(public["maintenance"][0]["status"], "scheduled");
    assert_eq!(public["maintenance"][0]["components"][0]["name"], "Contact sync");
    assert_eq!(public["status"], "operational");

    run_maintenance(&database, Some("https://daroyan.example"), now)
        .await
        .unwrap();
    wait_for(&received, "Starts at").await;
    run_maintenance(&database, None, now + chrono::Duration::minutes(31))
        .await
        .unwrap();
    let public =
        send(anonymous.get(format!("{base}/v1/status/acme")), StatusCode::OK).await;
    assert_eq!(public["maintenance"][0]["status"], "in_progress");
    assert_eq!(public["sections"][0]["components"][1]["status"], "maintenance");
    assert_eq!(public["status"], "maintenance");
    run_maintenance(&database, None, now + chrono::Duration::minutes(91))
        .await
        .unwrap();
    let finished =
        send(http.get(format!("{base}/v1/incidents/{maintenance_id}")), StatusCode::OK)
            .await;
    assert_eq!(finished["status"], "completed");
    assert!(finished["resolved_at"].is_string());
    let public =
        send(anonymous.get(format!("{base}/v1/status/acme")), StatusCode::OK).await;
    assert!(public["maintenance"].as_array().unwrap().is_empty());
    let timeline = send(
        http.get(format!("{base}/v1/incidents/{maintenance_id}/updates")),
        StatusCode::OK,
    )
    .await;
    let statuses: Vec<&str> = timeline
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["completed", "in_progress", "scheduled"]);

    send(
        http.post(format!("{base}/v1/alert-rules")).json(&json!({
            "name": "Sync failed",
            "condition": { "kind": "cron_failed" },
            "scope": { "kind": "crons", "cron_ids": [cron_id] },
            "declare_incident": true,
        })),
        StatusCode::BAD_REQUEST,
    )
    .await;
    let rule = send(
        http.post(format!("{base}/v1/alert-rules")).json(&json!({
            "name": "Sync failed",
            "condition": { "kind": "cron_failed" },
            "scope": { "kind": "crons", "cron_ids": [cron_id] },
            "integration_ids": [channel_id],
            "declare_incident": true,
            "declare_severity": "major",
            "declare_hidden_page_ids": [],
        })),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(rule["declare_incident"], true);
    assert_eq!(rule["declare_severity"], "major");
    let rule_id = rule["id"].as_str().unwrap().to_string();
    let ping = anonymous.post(format!("{base}/p/{key}")).send().await.unwrap();
    assert!(ping.status().is_success());
    let failed = anonymous.post(format!("{base}/p/{key}/fail")).send().await.unwrap();
    assert!(failed.status().is_success());
    engine.check_alerts(Utc::now()).await.unwrap();
    engine.check_alerts(Utc::now()).await.unwrap();
    let incidents = send(http.get(format!("{base}/v1/incidents")), StatusCode::OK).await;
    let declared: Vec<&Value> = incidents
        .as_array()
        .unwrap()
        .iter()
        .filter(|incident| incident["title"] == "Sync failed on contacts.sync")
        .collect();
    assert_eq!(declared.len(), 1);
    assert_eq!(declared[0]["severity"], "major");
    assert_eq!(declared[0]["components"][0]["status"], "outage");
    assert_eq!(declared[0]["related"][0]["id"], rule_id.as_str());
    wait_for(&received, "[Major] Sync failed on contacts.sync — Investigating").await;

    let alerts =
        send(http.get(format!("{base}/v1/alerts?status=firing")), StatusCode::OK).await;
    let alert_id = alerts[0]["id"].as_str().unwrap().to_string();
    send(
        http.post(format!("{base}/v1/incidents")).json(&json!({
            "kind": "incident", "title": "Sync broken", "severity": "minor",
            "alert_id": "alr_missing",
            "update": { "status": "investigating", "body": doc("Looking.") }
        })),
        StatusCode::BAD_REQUEST,
    )
    .await;
    let from_alert = send(
        http.post(format!("{base}/v1/incidents")).json(&json!({
            "kind": "incident", "title": "Sync broken", "severity": "minor",
            "alert_id": alert_id,
            "update": { "status": "investigating", "body": doc("Looking.") }
        })),
        StatusCode::CREATED,
    )
    .await;
    let from_alert_id = from_alert["id"].as_str().unwrap();
    let timeline = send(
        http.get(format!("{base}/v1/incidents/{from_alert_id}/updates")),
        StatusCode::OK,
    )
    .await;
    assert_eq!(timeline[0]["kind"], "update");
    assert_eq!(timeline[1]["kind"], "automatic");
    assert_eq!(
        timeline[1].pointer("/body/content/0/content/0/text").unwrap(),
        "Sync failed fired on contacts.sync. Ops declared this incident from the alert."
    );

    let hidden = send(
        http.put(format!("{base}/v1/status-pages/{page_id}")).json(&json!({
            "name": "Acme", "slug": "acme", "is_public": false,
            "sections": page["sections"],
        })),
        StatusCode::OK,
    )
    .await;
    assert_eq!(hidden["is_public"], false);
    send(anonymous.get(format!("{base}/v1/status/acme/feed.xml")), StatusCode::NOT_FOUND)
        .await;
    send(
        anonymous.get(format!("{base}/v1/status/acme/incidents/{incident_id}")),
        StatusCode::NOT_FOUND,
    )
    .await;
}
