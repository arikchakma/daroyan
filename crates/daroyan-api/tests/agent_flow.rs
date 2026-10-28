mod common;

use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use daroyan_api::app::{build_app, open_state};
use daroyan_core::config::Config;
use daroyan_core::token::hash_token;
use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

fn make_report(with_host: bool) -> Value {
    json!({
        "type": "report",
        "host": with_host.then(|| json!({
            "name": "worker-1", "labels": ["production"], "hostname": "worker-1",
            "platform": "Ubuntu", "platform_version": "24.04", "kernel": "6.8.0", "arch": "x86_64",
            "cpu": "EPYC", "cpu_cores": 8, "mem_total": 16_000, "swap_total": 2_000,
            "private_ip": "10.0.1.21", "boot_time": 1, "version": "0.1.0"
        })),
        "state": {
            "taken_at": chrono::Utc::now().timestamp_millis(), "uptime": 100, "cpu": 34.0,
            "load1": 1.0, "load5": 0.5, "load15": 0.2, "mem_used": 8_000, "swap_used": 0,
            "process_count": 120, "tcp_conn_count": 10, "udp_conn_count": 2,
            "disks": [{"mount": "/", "filesystem": "ext4", "disk_total": 100, "disk_used": 40,
                       "inode_total": 10, "inode_used": 1, "read_speed": 5, "write_speed": 6}],
            "nics": [{"name": "eth0", "net_in_speed": 7, "net_out_speed": 8,
                      "net_in_transfer": 70, "net_out_transfer": 80}]
        },
        "processes": [{"name": "nginx", "status": "running", "count": 2, "started_at": 1,
                       "cpu": 1.0, "mem_used": 10, "read_speed": 0, "write_speed": 0, "controllable": true},
                      {"name": "cron", "status": "running", "count": 1, "started_at": 1,
                       "cpu": 0.0, "mem_used": 1, "read_speed": 0, "write_speed": 0, "controllable": false}],
        "ports": [],
        "events": [{"at": 1, "kind": "agent_started", "version": "0.1.0"}]
    })
}

#[tokio::test]
async fn enrolls_reports_and_runs_commands() {
    let dir = std::env::temp_dir().join(format!("daroyan-test-{}", std::process::id()));
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
        app_url: None,
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
    for file in ["daroyan.db", "daroyan.duckdb"] {
        let mode = std::fs::metadata(dir.join(file)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{file}");
    }
    let http = common::sign_in(&state.database).await;
    let database = state.database.clone();
    let analytics = state.analytics.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(
            listener,
            build_app(state).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
    });

    let unauthorized =
        Client::new().get(format!("{base}/v1/servers")).send().await.unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let script = Client::new()
        .get(format!("{base}/install.sh"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(script.contains(&format!(
        "/releases/download/v{}/daroyan-$ARCH-linux-musl",
        env!("CARGO_PKG_VERSION")
    )));

    let install: Value = http
        .post(format!("{base}/v1/install-tokens"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let install_token = install["token"].as_str().unwrap();
    assert!(install_token.starts_with("dy_agent_"));

    let host = make_report(true)["host"].clone();
    let rejected = http
        .post(format!("{base}/v1/agent/enroll"))
        .bearer_auth("dy_agent_wrong")
        .json(&json!({ "host": host }))
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);

    let enrolled: Value = http
        .post(format!("{base}/v1/agent/enroll"))
        .bearer_auth(install_token)
        .json(&json!({ "host": host }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let server_id = enrolled["server_id"].as_str().unwrap().to_string();
    let server_token = enrolled["token"].as_str().unwrap();

    let reused = http
        .post(format!("{base}/v1/agent/enroll"))
        .bearer_auth(install_token)
        .json(&json!({ "host": host }))
        .send()
        .await
        .unwrap();
    assert_eq!(reused.status(), StatusCode::UNAUTHORIZED);
    for max_uses in [0, 101] {
        let invalid = http
            .post(format!("{base}/v1/install-tokens"))
            .json(&json!({ "max_uses": max_uses }))
            .send()
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    }
    let fleet: Value = http
        .post(format!("{base}/v1/install-tokens"))
        .json(&json!({ "max_uses": 2 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(fleet["max_uses"], 2);
    let fleet_hash = hash_token(fleet["token"].as_str().unwrap());
    assert!(database.claim_install_token(&fleet_hash).await.unwrap());
    assert!(database.claim_install_token(&fleet_hash).await.unwrap());
    assert!(!database.claim_install_token(&fleet_hash).await.unwrap());

    let forever: Value = http
        .post(format!("{base}/v1/install-tokens"))
        .json(&json!({ "name": "ansible", "expires_in_hours": null, "max_uses": null }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(forever["name"], "ansible");
    assert!(forever["expires_at"].is_null());
    assert!(forever["max_uses"].is_null());
    let forever_hash = hash_token(forever["token"].as_str().unwrap());
    for _ in 0..150 {
        assert!(database.claim_install_token(&forever_hash).await.unwrap());
    }
    let listed: Value = http
        .get(format!("{base}/v1/install-tokens"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let listed = listed.as_array().unwrap();
    assert!(listed.iter().all(|token| token["token"].is_null()));
    assert!(listed.iter().all(|token| token["id"] != fleet["id"]));
    let forever_row = listed.iter().find(|token| token["id"] == forever["id"]).unwrap();
    assert_eq!(forever_row["uses"], 150);
    assert!(forever_row["last_used_at"].is_string());

    let revoke_url =
        format!("{base}/v1/install-tokens/{}", fleet["id"].as_str().unwrap());
    let revoked = http.delete(&revoke_url).send().await.unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    let gone = http.delete(&revoke_url).send().await.unwrap();
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);

    let mut request = format!("{}/v1/agent/connect", base.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {server_token}").parse().unwrap());
    let (mut agent, _) = connect_async(request).await.unwrap();
    agent
        .send(Message::text(make_report(true).to_string()))
        .await
        .unwrap();

    let servers = wait_for(|| async {
        let servers: Value = http
            .get(format!("{base}/v1/servers"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        (servers[0]["status"] == "healthy").then_some(servers)
    })
    .await;
    assert_eq!(servers[0]["name"], "worker-1");
    assert_eq!(servers[0]["private_ip"], "10.0.1.21");
    assert_eq!(servers[0]["mem_percent"], 50.0);
    assert_eq!(servers[0]["disk_percent"], 40.0);

    let mut spoofed = make_report(true);
    spoofed["host"]["name"] = json!("prod-db-1");
    spoofed["host"]["labels"] = json!(["production", "spoofed"]);
    agent.send(Message::text(spoofed.to_string())).await.unwrap();
    let servers = wait_for(|| async {
        let servers: Value = http
            .get(format!("{base}/v1/servers"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        (servers[0]["labels"].to_string().contains("spoofed")).then_some(servers)
    })
    .await;
    assert_eq!(servers[0]["name"], "worker-1");

    let member = common::sign_in_member(&database, "viewer").await;
    let rename = |client: &Client, name: &str| {
        client
            .patch(format!("{base}/v1/servers/{server_id}"))
            .json(&json!({ "name": name }))
            .send()
    };
    assert_eq!(rename(&member, "mine").await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(rename(&http, " ").await.unwrap().status(), StatusCode::BAD_REQUEST);
    let renamed: Value =
        rename(&http, "worker-east").await.unwrap().json().await.unwrap();
    assert_eq!(renamed["name"], "worker-east");

    let unwatched = http
        .post(format!("{base}/v1/servers/{server_id}/commands"))
        .json(&json!({ "action": "restart_process", "name": "redis" }))
        .send()
        .await
        .unwrap();
    assert_eq!(unwatched.status(), StatusCode::BAD_REQUEST);

    let uncontrollable = http
        .post(format!("{base}/v1/servers/{server_id}/commands"))
        .json(&json!({ "action": "restart_process", "name": "cron" }))
        .send()
        .await
        .unwrap();
    assert_eq!(uncontrollable.status(), StatusCode::BAD_REQUEST);

    let command: Value = http
        .post(format!("{base}/v1/servers/{server_id}/commands"))
        .json(&json!({ "action": "restart_process", "name": "nginx" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(command["status"], "sent");
    let command_id = command["id"].as_str().unwrap();

    let received = loop {
        if let Message::Text(text) = agent.next().await.unwrap().unwrap() {
            break serde_json::from_str::<Value>(&text).unwrap();
        }
    };
    assert_eq!(received["action"], "restart_process");
    assert_eq!(received["name"], "nginx");
    agent
        .send(Message::text(
            json!({ "type": "command_result", "id": command_id, "ok": true, "error": null })
                .to_string(),
        ))
        .await
        .unwrap();

    wait_for(|| async {
        let command: Value = http
            .get(format!("{base}/v1/servers/{server_id}/commands/{command_id}"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        (command["status"] == "succeeded").then_some(())
    })
    .await;

    analytics.flush().await.unwrap();
    let metrics: Value = http
        .get(format!("{base}/v1/servers/{server_id}/metrics?range=1h"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(metrics["points"][0]["cpu"], 34.0);
    assert_eq!(metrics["points"][0]["net_in_speed"], 7.0);

    let events: Value = http
        .get(format!("{base}/v1/servers/{server_id}/events"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let started = events
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "agent_started")
        .unwrap();
    assert_eq!(started["data"]["version"], "0.1.0");
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "agent_connected")
    );

    let fleet: Value = http
        .get(format!("{base}/v1/overview/events"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(fleet["buckets"].as_array().unwrap().len(), 48);
    assert!(
        fleet["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["label"] != "Online"),
        "a first connect is not a recovery: {fleet}"
    );
    let restart = fleet["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["outcome"] == "neutral")
        .unwrap_or_else(|| panic!("no command event: {fleet}"));
    assert_eq!(restart["label"], "Restarted");
    assert_eq!(restart["detail"], "by Ops");

    agent
        .send(Message::text(
            json!({"type": "logs", "dropped": 2, "lines": [
                {"at": 1_000, "service": "Linkity API", "stream": "stdout", "severity": 9, "message": "GET /health 200"},
                {"at": 2_000, "service": "Linkity API", "stream": "stderr", "severity": 17, "message": "SES SendEmail failed", "attributes": {"attempt": 2}}
            ]})
            .to_string(),
        ))
        .await
        .unwrap();
    let errors: Value = wait_for(|| async {
        analytics.flush().await.unwrap();
        let page: Value = http
            .get(format!("{base}/v1/logs?server_id={server_id}&level=error&q=sendemail"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        (page["lines"].as_array().is_some_and(|lines| !lines.is_empty())).then_some(page)
    })
    .await;
    assert_eq!(errors["lines"][0]["level"], "error");
    assert_eq!(errors["lines"][0]["attributes"]["attempt"], 2);
    assert_eq!(errors["has_more"], false);
    let by_attribute: Value = http
        .get(format!("{base}/v1/logs?q=%40attempt%3A2"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(by_attribute["lines"].as_array().unwrap().len(), 1);
    let all: Value = http
        .get(format!("{base}/v1/logs?server_id={server_id}&limit=2"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(all["lines"].as_array().unwrap().len(), 2);
    assert_eq!(all["has_more"], true);
    assert_eq!(all["lines"][0]["service"], "Daroyan agent");
    let sources: Value = http
        .get(format!("{base}/v1/logs/sources"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(sources.as_array().unwrap().len(), 2);

    assert!(common::count_rows(&database, "commands", "server_id", &server_id).await > 0);
    let deleted = http
        .delete(format!("{base}/v1/servers/{server_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        common::count_rows(&database, "commands", "server_id", &server_id).await,
        0
    );

    let mut guesses = 0;
    loop {
        guesses += 1;
        let guess = Client::new()
            .get(format!("{base}/v1/servers"))
            .header("cookie", "daroyan_session=wrong")
            .send()
            .await
            .unwrap();
        if guess.status() == StatusCode::TOO_MANY_REQUESTS {
            break;
        }
        assert_eq!(guess.status(), StatusCode::UNAUTHORIZED);
        assert!(guesses <= 10, "the API never blocked the guesses");
    }
    let blocked = http.get(format!("{base}/v1/servers")).send().await.unwrap();
    assert_eq!(blocked.status(), StatusCode::TOO_MANY_REQUESTS);
    let body: Value = blocked.json().await.unwrap();
    assert_eq!(body["name"], "rate_limit_exceeded");

    remove_dir(dir);
}

async fn wait_for<T, F, Fut>(mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    for _ in 0..50 {
        if let Some(value) = check().await {
            return value;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for the condition");
}

fn remove_dir(dir: PathBuf) {
    std::fs::remove_dir_all(dir).ok();
}
