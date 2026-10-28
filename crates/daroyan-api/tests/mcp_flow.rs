mod common;

use std::net::SocketAddr;

use axum::Router;
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

async fn rpc(
    client: &Client,
    url: &str,
    token: &str,
    method: &str,
    params: Value,
) -> Value {
    let response = client
        .post(url)
        .bearer_auth(token)
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.unwrap()
}

async fn call(
    client: &Client,
    url: &str,
    token: &str,
    name: &str,
    arguments: Value,
) -> Value {
    let reply = rpc(
        client,
        url,
        token,
        "tools/call",
        json!({ "name": name, "arguments": arguments }),
    )
    .await;
    reply["result"].clone()
}

fn read_json(result: &Value) -> Value {
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn serves_tools_over_mcp_with_a_bearer_token() {
    let dir = std::env::temp_dir().join(format!("daroyan-mcp-{}", std::process::id()));
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
    let anonymous = Client::new();
    let base = serve(build_app(state)).await;
    let url = format!("{base}/mcp");

    let created: Value = http
        .post(format!("{base}/v1/mcp-tokens"))
        .json(&json!({ "name": "claude" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = created["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("dy_mcp_"));

    let missing = anonymous.post(&url).json(&json!({})).send().await.unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    let cookie_only = http
        .post(&url)
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
        .send()
        .await
        .unwrap();
    assert_eq!(cookie_only.status(), StatusCode::UNAUTHORIZED);
    let wrong = anonymous
        .post(&url)
        .bearer_auth("dy_mcp_nope")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    let direct = anonymous
        .post(format!("{base}/v1/install-tokens"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(direct.status(), StatusCode::UNAUTHORIZED);
    let stream = anonymous.get(&url).bearer_auth(&token).send().await.unwrap();
    assert_eq!(stream.status(), StatusCode::METHOD_NOT_ALLOWED);

    let init = rpc(
        &anonymous,
        &url,
        &token,
        "initialize",
        json!({ "protocolVersion": "2025-06-18" }),
    )
    .await;
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "daroyan");
    let notified = anonymous
        .post(&url)
        .bearer_auth(&token)
        .json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .send()
        .await
        .unwrap();
    assert_eq!(notified.status(), StatusCode::ACCEPTED);

    let listed = rpc(&anonymous, &url, &token, "tools/list", json!({})).await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 52);
    let find =
        |name: &str| tools.iter().find(|tool| tool["name"] == name).unwrap().clone();
    assert!(
        find("search_logs")["inputSchema"]["properties"]["q"]["description"].is_string()
    );
    let create_check = find("create_uptime_check");
    assert!(create_check["inputSchema"]["properties"]["target"].is_object());
    assert_eq!(create_check["annotations"]["readOnlyHint"], false);
    assert_eq!(
        find("search_logs")["inputSchema"]["properties"]["since"]["type"],
        json!(["integer", "string"])
    );
    assert_eq!(find("get_overview")["annotations"]["readOnlyHint"], true);
    assert!(!listed.to_string().contains("$ref"));

    let created = call(
        &anonymous,
        &url,
        &token,
        "create_uptime_check",
        json!({ "name": "Postgres", "kind": "tcp", "target": "127.0.0.1:1", "paused": true }),
    )
    .await;
    assert_eq!(created["isError"], false, "{created}");
    let check_id = read_json(&created)["id"].as_str().unwrap().to_string();

    let fetched = call(
        &anonymous,
        &url,
        &token,
        "get_uptime_check",
        json!({ "check_id": check_id }),
    )
    .await;
    assert_eq!(read_json(&fetched)["name"], "Postgres");
    let listed = call(&anonymous, &url, &token, "list_uptime_checks", json!({})).await;
    assert!(listed.to_string().contains("Postgres"));

    let invalid = call(
        &anonymous,
        &url,
        &token,
        "create_uptime_check",
        json!({ "name": "Bad", "kind": "ftp", "target": "x" }),
    )
    .await;
    assert_eq!(invalid["isError"], true);
    let no_id = call(&anonymous, &url, &token, "get_uptime_check", json!({})).await;
    assert_eq!(no_id["isError"], true);
    let unknown =
        rpc(&anonymous, &url, &token, "tools/call", json!({ "name": "nope" })).await;
    assert_eq!(unknown["error"]["code"], -32602);

    let deleted = call(
        &anonymous,
        &url,
        &token,
        "delete_uptime_check",
        json!({ "check_id": check_id }),
    )
    .await;
    assert_eq!(deleted["isError"], false, "{deleted}");

    let install =
        call(&anonymous, &url, &token, "create_install_command", json!({})).await;
    let install = read_json(&install);
    let install_token = install["token"].as_str().unwrap();
    assert!(
        install["command"]
            .as_str()
            .unwrap()
            .contains(&format!("daroyan.example/install.sh | sudo DAROYAN_API_URL=https://daroyan.example DAROYAN_TOKEN={install_token} sh"))
    );

    let cron = call(
        &anonymous,
        &url,
        &token,
        "create_cron",
        json!({ "name": "Backup", "schedule_kind": "cron", "expression": "0 3 * * *" }),
    )
    .await;
    let cron = read_json(&cron);
    let ping_key = cron["ping_key"].as_str().unwrap();
    assert_eq!(cron["ping_url"], format!("https://daroyan.example/p/{ping_key}"));
    let preview = call(
        &anonymous,
        &url,
        &token,
        "preview_cron_schedule",
        json!({ "expression": "0 3 * * *" }),
    )
    .await;
    assert!(
        read_json(&preview)["next_runs"][0]
            .as_str()
            .unwrap()
            .ends_with("03:00:00.000Z")
    );

    let recent =
        call(&anonymous, &url, &token, "search_logs", json!({ "since": "1h" })).await;
    assert_eq!(recent["isError"], false, "{recent}");
    let dated = call(
        &anonymous,
        &url,
        &token,
        "search_logs",
        json!({ "since": "2026-01-01T00:00:00Z", "until": 1_900_000_000_000_i64 }),
    )
    .await;
    assert_eq!(dated["isError"], false, "{dated}");
    let vague =
        call(&anonymous, &url, &token, "search_logs", json!({ "since": "soon" })).await;
    assert_eq!(vague["isError"], true);

    let overview = call(&anonymous, &url, &token, "get_overview", json!({})).await;
    assert_eq!(overview["isError"], false, "{overview}");
    let overview = read_json(&overview);
    assert_eq!(overview["needs_attention"], false, "{overview}");
    assert_eq!(overview["watching"]["crons"], 1);

    let tokens: Value = http
        .get(format!("{base}/v1/mcp-tokens"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        tokens[0]["last_used_at"].is_number() || tokens[0]["last_used_at"].is_string()
    );
    let token_id = tokens[0]["id"].as_str().unwrap();
    let revoked = http
        .delete(format!("{base}/v1/mcp-tokens/{token_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    let after = anonymous
        .post(&url)
        .bearer_auth(&token)
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }))
        .send()
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
}
