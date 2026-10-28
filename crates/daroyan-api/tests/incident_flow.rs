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

async fn public_status(http: &Client, base: &str) -> Value {
    send(http.get(format!("{base}/v1/status/acme")), StatusCode::OK).await
}

#[tokio::test]
async fn declares_updates_and_resolves_incidents() {
    let dir =
        std::env::temp_dir().join(format!("daroyan-incidents-{}", std::process::id()));
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
        slack_signing_secret: None,
        discord_client_id: None,
        discord_client_secret: None,
        telegram_bot_token: None,
        allow_private_targets: true,
    })
    .await
    .unwrap();
    let http = common::sign_in(&state.database).await;
    let database = state.database.clone();
    let anonymous = Client::new();
    let base = serve(build_app(state)).await;
    let site = serve(Router::new().route("/health", get(|| async { "ok" }))).await;

    let unauthorized =
        anonymous.get(format!("{base}/v1/incidents")).send().await.unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let users = send(http.get(format!("{base}/v1/users")), StatusCode::OK).await;
    assert_eq!(users[0]["id"], "usr_test");

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
    let page = send(
        http.post(format!("{base}/v1/status-pages")).json(&json!({
            "name": "Acme", "slug": "acme",
            "sections": [{ "name": "API", "components": [{ "kind": "check", "id": check_id, "name": "Public API" }] }]
        })),
        StatusCode::CREATED,
    )
    .await;
    let page_id = page["id"].as_str().unwrap().to_string();
    assert_eq!(public_status(&anonymous, &base).await["status"], "operational");

    let component = json!([{ "kind": "check", "id": check_id, "status": "partial" }]);
    for body in [
        json!({ "kind": "incident", "title": " ", "severity": "major", "update": { "status": "investigating", "body": doc("Looking") } }),
        json!({ "kind": "incident", "title": "API slow", "severity": "huge", "update": { "status": "investigating", "body": doc("Looking") } }),
        json!({ "kind": "incident", "title": "API slow", "severity": "major", "update": { "status": "scheduled", "body": doc("Looking") } }),
        json!({ "kind": "incident", "title": "API slow", "severity": "major", "update": { "status": "investigating", "body": doc(" ") } }),
        json!({ "kind": "incident", "title": "API slow", "severity": "major", "components": [{ "kind": "check", "id": "chk_missing", "status": "outage" }], "update": { "status": "investigating", "body": doc("Looking") } }),
        json!({ "kind": "incident", "title": "API slow", "severity": "major", "components": [{ "kind": "check", "id": check_id, "status": "broken" }], "update": { "status": "investigating", "body": doc("Looking") } }),
        json!({ "kind": "incident", "title": "API slow", "severity": "major", "lead_id": "usr_missing", "update": { "status": "investigating", "body": doc("Looking") } }),
        json!({ "kind": "maintenance", "title": "Upgrade", "starts_at": "2030-01-01T10:00:00Z", "ends_at": "2030-01-01T09:00:00Z", "update": { "body": doc("Upgrading") } }),
        json!({ "kind": "other", "title": "API slow", "update": { "body": doc("Looking") } }),
    ] {
        send(
            http.post(format!("{base}/v1/incidents")).json(&body),
            StatusCode::BAD_REQUEST,
        )
        .await;
    }

    let incident = send(
        http.post(format!("{base}/v1/incidents")).json(&json!({
            "kind": "incident",
            "title": "API is slow",
            "severity": "major",
            "lead_id": "usr_test",
            "components": component,
            "related": [{ "kind": "check", "id": check_id }],
            "responders": [{ "user_id": "usr_test", "role": "Responder" }],
            "update": { "status": "investigating", "body": doc("Requests take 5 seconds. We are looking into it.") }
        })),
        StatusCode::CREATED,
    )
    .await;
    let incident_id = incident["id"].as_str().unwrap().to_string();
    assert!(incident_id.starts_with("inc_"));
    assert_eq!(incident["status"], "investigating");
    assert_eq!(incident["lead"]["name"], "Ops");
    assert_eq!(incident["components"][0]["name"], "api-prod-health");
    assert_eq!(incident["related"][0]["kind"], "check");
    assert_eq!(incident["status_pages"][0]["id"], page_id);
    assert_eq!(incident["status_pages"][0]["is_shown"], true);
    assert_eq!(incident["public_update_count"], 1);

    let public = public_status(&anonymous, &base).await;
    assert_eq!(public["sections"][0]["components"][0]["status"], "partial");
    assert_eq!(public["status"], "partial");

    let mut edit = json!({
        "title": "API is slow",
        "severity": "critical",
        "lead_id": "usr_test",
        "components": [{ "kind": "check", "id": check_id, "status": "outage" }],
        "hidden_page_ids": [page_id],
    });
    let edited = send(
        http.put(format!("{base}/v1/incidents/{incident_id}")).json(&edit),
        StatusCode::OK,
    )
    .await;
    assert_eq!(edited["severity"], "critical");
    assert_eq!(edited["status_pages"][0]["is_shown"], false);
    assert_eq!(
        public_status(&anonymous, &base).await["sections"][0]["components"][0]["status"],
        "operational"
    );
    edit["hidden_page_ids"] = json!([]);
    send(
        http.put(format!("{base}/v1/incidents/{incident_id}")).json(&edit),
        StatusCode::OK,
    )
    .await;
    assert_eq!(public_status(&anonymous, &base).await["status"], "major");

    let note = send(
        http.post(format!("{base}/v1/incidents/{incident_id}/updates")).json(&json!({
            "kind": "note", "status": "resolved", "body": doc("Kernel 6.8 broke the network."), "is_public": true
        })),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(note["is_public"], false);
    assert!(note["status"].is_null());
    let note_id = note["id"].as_str().unwrap().to_string();
    let edited_note = send(
        http.put(format!("{base}/v1/incidents/{incident_id}/updates/{note_id}"))
            .json(&json!({ "body": doc("Kernel 6.8.0-47 broke the network.") })),
        StatusCode::OK,
    )
    .await;
    assert!(edited_note["edited_at"].is_string());
    let unsafe_link = json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [
        { "type": "text", "text": "Status", "marks": [{ "type": "link", "attrs": { "href": "javascript:alert(1)" } }] }
    ] }] });
    send(
        http.post(format!("{base}/v1/incidents/{incident_id}/updates"))
            .json(&json!({ "kind": "note", "body": unsafe_link })),
        StatusCode::BAD_REQUEST,
    )
    .await;

    send(
        http.post(format!("{base}/v1/incidents/{incident_id}/postmortem")),
        StatusCode::BAD_REQUEST,
    )
    .await;
    let resolved_update = send(
        http.post(format!("{base}/v1/incidents/{incident_id}/updates"))
            .json(&json!({
                "kind": "update", "status": "resolved", "body": doc("Fixed.")
            })),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(resolved_update["creator"]["name"], "Ops");
    assert_eq!(resolved_update["creator_id"], resolved_update["creator"]["id"]);
    let resolved =
        send(http.get(format!("{base}/v1/incidents/{incident_id}")), StatusCode::OK)
            .await;
    assert_eq!(resolved["status"], "resolved");
    assert!(resolved["resolved_at"].is_string());
    assert_eq!(resolved["public_update_count"], 2);
    assert_eq!(public_status(&anonymous, &base).await["status"], "operational");

    let timeline = send(
        http.get(format!("{base}/v1/incidents/{incident_id}/updates")),
        StatusCode::OK,
    )
    .await;
    let kinds: Vec<&str> = timeline
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["update", "note", "automatic", "automatic", "update"]);
    let mut automatic: Vec<&str> = [&timeline[2], &timeline[3]]
        .iter()
        .map(|entry| {
            entry
                .pointer("/body/content/0/content/0/text")
                .unwrap()
                .as_str()
                .unwrap()
        })
        .collect();
    automatic.sort_unstable();
    assert_eq!(
        automatic,
        [
            "Affected components are now api-prod-health (Major Outage).",
            "Severity changed to Critical."
        ]
    );
    let automatic = timeline[2]["id"].as_str().unwrap();
    send(
        http.put(format!("{base}/v1/incidents/{incident_id}/updates/{automatic}"))
            .json(&json!({ "body": doc("Changed") })),
        StatusCode::BAD_REQUEST,
    )
    .await;

    send(
        http.post(format!("{base}/v1/incidents/{incident_id}/postmortem")),
        StatusCode::BAD_REQUEST,
    )
    .await;
    let postmortem = send(
        http.put(format!("{base}/v1/incidents/{incident_id}/postmortem"))
            .json(&json!({
                "postmortem": {
                    "type": "doc",
                    "content": [
                        { "type": "heading", "attrs": { "level": 2 }, "content": [{ "type": "text", "text": "Root cause" }] },
                        { "type": "paragraph", "content": [{ "type": "text", "text": "A kernel update." }] }
                    ]
                },
                "follow_ups": [{ "body": "Pin the kernel", "owner_id": "usr_test" }]
            })),
        StatusCode::OK,
    )
    .await;
    assert_eq!(postmortem["postmortem"]["content"][0]["attrs"]["level"], 2);
    let follow_up = postmortem["follow_ups"][0].clone();
    assert!(follow_up["id"].as_str().unwrap().starts_with("fup_"));
    assert_eq!(follow_up["is_done"], false);
    let published = send(
        http.post(format!("{base}/v1/incidents/{incident_id}/postmortem")),
        StatusCode::OK,
    )
    .await;
    assert!(published["postmortem_published_at"].is_string());

    let reopened = send(
        http.post(format!("{base}/v1/incidents/{incident_id}/updates"))
            .json(&json!({
                "kind": "update", "status": "monitoring", "body": doc("It came back.")
            })),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(reopened["status"], "monitoring");
    let incident =
        send(http.get(format!("{base}/v1/incidents/{incident_id}")), StatusCode::OK)
            .await;
    assert!(incident["resolved_at"].is_null());

    let maintenance = send(
        http.post(format!("{base}/v1/incidents")).json(&json!({
            "kind": "maintenance",
            "title": "Database upgrade",
            "starts_at": "2030-01-01T09:00:00Z",
            "ends_at": "2030-01-01T10:00:00Z",
            "components": [{ "kind": "check", "id": check_id, "status": "outage" }],
            "update": { "status": "resolved", "body": doc("We are upgrading Postgres.") }
        })),
        StatusCode::CREATED,
    )
    .await;
    let maintenance_id = maintenance["id"].as_str().unwrap().to_string();
    assert_eq!(maintenance["status"], "scheduled");
    assert!(maintenance["severity"].is_null());
    assert_eq!(maintenance["components"][0]["status"], "maintenance");
    send(
        http.post(format!("{base}/v1/incidents/{incident_id}/updates"))
            .json(&json!({
                "kind": "update", "status": "resolved", "body": doc("Fixed again.")
            })),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(
        public_status(&anonymous, &base).await["sections"][0]["components"][0]["status"],
        "operational"
    );
    send(
        http.post(format!("{base}/v1/incidents/{maintenance_id}/updates"))
            .json(&json!({
                "kind": "update", "status": "in_progress", "body": doc("Started.")
            })),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(
        public_status(&anonymous, &base).await["sections"][0]["components"][0]["status"],
        "maintenance"
    );
    send(
        http.post(format!("{base}/v1/incidents/{maintenance_id}/postmortem")),
        StatusCode::BAD_REQUEST,
    )
    .await;

    let listed = send(http.get(format!("{base}/v1/incidents")), StatusCode::OK).await;
    assert_eq!(listed.as_array().unwrap().len(), 2);
    assert!(
        common::count_rows(&database, "incident_updates", "incident_id", &incident_id)
            .await
            > 0
    );
    let deleted = http
        .delete(format!("{base}/v1/incidents/{incident_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        common::count_rows(&database, "incident_updates", "incident_id", &incident_id)
            .await,
        0
    );
    send(http.get(format!("{base}/v1/incidents/{incident_id}")), StatusCode::NOT_FOUND)
        .await;
}
