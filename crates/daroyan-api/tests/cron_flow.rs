mod common;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::routing::post;
use axum::{Json, Router};
use chrono::{Duration as ChronoDuration, Utc};
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

async fn read(http: &Client, url: String) -> Value {
    http.get(url).send().await.unwrap().json().await.unwrap()
}

async fn ping(base: &str, key: &str, action: &str, body: &str) -> StatusCode {
    Client::new()
        .post(format!("{base}/p/{key}{action}"))
        .body(body.to_string())
        .send()
        .await
        .unwrap()
        .status()
}

async fn wait_for_hooks(received: &Mutex<Vec<Value>>, count: usize) -> Vec<Value> {
    for _ in 0..50 {
        if received.lock().unwrap().len() >= count {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    received.lock().unwrap().clone()
}

#[tokio::test]
async fn tracks_runs_failures_misses_and_timeouts() {
    let dir = std::env::temp_dir().join(format!("daroyan-crons-{}", std::process::id()));
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
    let database = state.database.clone();
    let crons = state.crons.clone();
    let mut engine = AlertEngine::new(
        state.database.clone(),
        state.agents.clone(),
        state.analytics.clone(),
        state.config.app_url.clone(),
    );
    let base = serve(build_app(state)).await;

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

    for body in [
        json!({ "name": "contacts.sync", "schedule_kind": "cron", "expression": "*/5 * * *" }),
        json!({ "name": "contacts.sync", "schedule_kind": "every", "interval_secs": 10 }),
        json!({ "name": "contacts.sync", "schedule_kind": "cron", "expression": "@hourly", "time_zone": "Mars/Base" }),
        json!({ "name": " ", "schedule_kind": "cron", "expression": "@hourly" }),
        json!({ "name": "contacts.sync", "schedule_kind": "cron", "expression": "@hourly", "server_id": "srv_missing" }),
    ] {
        let response = http
            .post(format!("{base}/v1/crons"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }

    let created = http
        .post(format!("{base}/v1/crons"))
        .json(&json!({
            "name": "contacts.sync",
            "schedule_kind": "cron",
            "expression": "*/5 * * * *",
            "grace_secs": 120,
            "max_duration_secs": 600,
            "command": "php artisan contacts:sync"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let cron: Value = created.json().await.unwrap();
    let cron_id = cron["id"].as_str().unwrap().to_string();
    let key = cron["ping_key"].as_str().unwrap().to_string();
    assert_eq!(cron["status"], "new");
    assert_eq!(cron["time_zone"], "UTC");
    assert!(cron["next_expected_at"].as_i64().unwrap() > Utc::now().timestamp_millis());

    for rule in [
        json!({ "name": "contacts.sync Missed", "condition": { "kind": "cron_missed" } }),
        json!({ "name": "contacts.sync Failed", "condition": { "kind": "cron_failed" } }),
    ] {
        let mut rule = rule;
        rule["scope"] = json!({ "kind": "crons", "cron_ids": [cron_id] });
        rule["integration_ids"] = json!([integration["id"]]);
        let response = http
            .post(format!("{base}/v1/alert-rules"))
            .json(&rule)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let wrong_scope = http
        .post(format!("{base}/v1/alert-rules"))
        .json(&json!({
            "name": "Wrong",
            "condition": { "kind": "cron_failed" },
            "scope": { "kind": "checks", "check_ids": ["chk_1"] },
            "integration_ids": [integration["id"]]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_scope.status(), StatusCode::BAD_REQUEST);

    let preview = read(
        &http,
        format!(
            "{base}/v1/crons/preview?expression=0%202%20*%20*%20*&time_zone=Asia/Dhaka"
        ),
    )
    .await;
    let next_runs = preview["next_runs"].as_array().unwrap();
    assert_eq!(next_runs.len(), 3);
    assert_eq!(
        next_runs[1].as_i64().unwrap() - next_runs[0].as_i64().unwrap(),
        86_400_000
    );
    let bad_preview = http
        .get(format!("{base}/v1/crons/preview?expression=nope"))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_preview.status(), StatusCode::BAD_REQUEST);

    assert_eq!(ping(&base, "nope", "", "").await, StatusCode::NOT_FOUND);
    assert_eq!(ping(&base, &key, "/log", "").await, StatusCode::NOT_FOUND);
    assert_eq!(ping(&base, &key, "/256", "").await, StatusCode::NOT_FOUND);
    let head = Client::new()
        .head(format!("{base}/p/{key}/start"))
        .send()
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::OK);
    let running = read(&http, format!("{base}/v1/crons/{cron_id}")).await;
    assert_eq!(running["status"], "running");
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(ping(&base, &key, "/0", "synced 42 contacts\n").await, StatusCode::OK);

    let ok = read(&http, format!("{base}/v1/crons/{cron_id}")).await;
    assert_eq!(ok["status"], "ok");
    assert!(ok["run_started_at"].is_null());
    assert!(ok["last_duration_ms"].as_u64().unwrap() >= 20);
    assert_eq!(ok["runs"][0]["status"], "ok");

    let body = format!("connecting\npsql: connection refused\n{}", "x".repeat(20_000));
    assert_eq!(ping(&base, &key, "/1", &body).await, StatusCode::OK);
    let failed = read(&http, format!("{base}/v1/crons/{cron_id}")).await;
    assert_eq!(failed["status"], "failed");
    engine.check_alerts(Utc::now()).await.unwrap();
    let alerts: Vec<Value> = read(&http, format!("{base}/v1/alerts?status=firing"))
        .await
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0]["cron_id"], cron_id.as_str());
    assert_eq!(alerts[0]["target_name"], "contacts.sync");
    assert!(
        alerts[0]["value"]
            .as_str()
            .unwrap()
            .starts_with("Failed: Exit code 1: ")
    );
    let hooks = wait_for_hooks(&received, 1).await;
    assert_eq!(hooks[0]["data"]["cron"]["id"], cron_id.as_str());

    let mute = http
        .post(format!("{base}/v1/alert-mutes"))
        .json(&json!({ "rule_id": alerts[0]["rule_id"], "cron_id": cron_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(mute.status(), StatusCode::CREATED);
    let mute: Value = mute.json().await.unwrap();
    assert_eq!(mute["cron_id"], cron_id.as_str());
    assert_eq!(mute["creator_id"], "usr_test");
    assert_eq!(mute["creator"]["name"], "Ops");
    let mutes = read(&http, format!("{base}/v1/alert-mutes")).await;
    assert_eq!(mutes[0]["creator"]["email"], "ops@daroyan.test");

    assert_eq!(ping(&base, &key, "", "").await, StatusCode::OK);
    engine.check_alerts(Utc::now()).await.unwrap();
    let firing = read(&http, format!("{base}/v1/alerts?status=firing")).await;
    assert!(firing.as_array().unwrap().is_empty());

    let later = Utc::now() + ChronoDuration::minutes(16);
    crons.check_crons(later).await.unwrap();
    let late = read(&http, format!("{base}/v1/crons/{cron_id}")).await;
    assert_eq!(late["status"], "late");
    assert!(late["next_expected_at"].as_i64().unwrap() > Utc::now().timestamp_millis());
    crons.check_crons(later).await.unwrap();
    engine.check_alerts(later).await.unwrap();
    let missed: Vec<Value> = read(&http, format!("{base}/v1/alerts?status=firing"))
        .await
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(missed.len(), 1);
    assert!(
        missed[0]["value"]
            .as_str()
            .unwrap()
            .starts_with("Missed: No ping within the 2m grace period")
    );

    assert_eq!(ping(&base, &key, "", "").await, StatusCode::OK);
    let stats = read(&http, format!("{base}/v1/crons/{cron_id}/stats")).await;
    let recent = stats["recent"].as_array().unwrap();
    let statuses: Vec<&str> =
        recent.iter().map(|run| run["status"].as_str().unwrap()).collect();
    assert_eq!(statuses.iter().filter(|status| **status == "late").count(), 1);
    let missed_runs = statuses.iter().filter(|status| **status == "missed").count();
    assert!((2..=3).contains(&missed_runs), "{statuses:?}");
    assert_eq!(&statuses[statuses.len() - 3..], ["ok", "failed", "ok"]);
    assert_eq!(recent[recent.len() - 2]["exit_code"], 1);
    assert_eq!(recent[recent.len() - 2]["output"].as_str().unwrap().len(), 10_000);
    assert_eq!(recent[recent.len() - 1]["output"], "synced 42 contacts");
    assert_eq!(stats["bars"].as_array().unwrap().len(), recent.len());
    assert_eq!(stats["stats"]["runs_today"], recent.len());
    let failures = stats["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 2);
    assert_eq!(failures[0]["kind"], "missed");
    assert!(failures[0]["resolved_at"].is_string());
    assert_eq!(failures[1]["cause"], format!("Exit code 1: {}", "x".repeat(200)));

    let fleet = read(&http, format!("{base}/v1/overview/events")).await;
    let labels: Vec<&str> = fleet["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["title"] == "contacts.sync")
        .map(|event| event["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"Failed") && labels.contains(&"Recovered"), "{labels:?}");
    assert!(
        fleet["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["label"] != "Firing"),
        "a cron alert repeats the cron failure: {fleet}"
    );
    let problem_events = fleet["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["outcome"] == "problem")
        .count();
    assert_eq!(fleet["problems"].as_u64(), u64::try_from(problem_events).ok());

    assert_eq!(ping(&base, &key, "/start?rid=run-1", "").await, StatusCode::OK);
    crons
        .check_crons(Utc::now() + ChronoDuration::minutes(11))
        .await
        .unwrap();
    let timed_out = read(&http, format!("{base}/v1/crons/{cron_id}")).await;
    assert_eq!(timed_out["status"], "failed");
    assert!(timed_out["run_started_at"].is_null());
    let stats = read(&http, format!("{base}/v1/crons/{cron_id}/stats")).await;
    let timeout = stats["recent"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["output"] == "Still running after 10m")
        .unwrap();
    assert_eq!(timeout["status"], "failed");
    assert!(timeout["duration_ms"].as_u64().unwrap() >= 600_000);

    assert_eq!(ping(&base, &key, "/start?rid=a", "").await, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(ping(&base, &key, "/start?rid=b", "").await, StatusCode::OK);
    assert_eq!(ping(&base, &key, "/0?rid=a", "first").await, StatusCode::OK);
    let overlapping = read(&http, format!("{base}/v1/crons/{cron_id}")).await;
    assert_eq!(overlapping["status"], "running");
    assert_eq!(ping(&base, &key, "/0?rid=b", "second").await, StatusCode::OK);
    assert_eq!(ping(&base, &key, "/start", "").await, StatusCode::OK);
    assert_eq!(ping(&base, &key, "/0", "third").await, StatusCode::OK);
    let finished = read(&http, format!("{base}/v1/crons/{cron_id}")).await;
    assert_eq!(finished["status"], "ok");
    assert!(finished["run_started_at"].is_null());
    let stats = read(&http, format!("{base}/v1/crons/{cron_id}/stats")).await;
    let run_of = |output: &str| {
        stats["recent"]
            .as_array()
            .unwrap()
            .iter()
            .find(|run| run["output"] == output)
            .cloned()
            .unwrap()
    };
    let (first, second, third) = (run_of("first"), run_of("second"), run_of("third"));
    assert!(first["duration_ms"].as_u64().unwrap() >= 80);
    assert!(first["started_at"].as_i64() < second["started_at"].as_i64());
    assert!(
        second["duration_ms"].as_u64().unwrap() < first["duration_ms"].as_u64().unwrap()
    );
    assert!(third["duration_ms"].is_u64());

    let paused = http
        .put(format!("{base}/v1/crons/{cron_id}"))
        .json(&json!({
            "name": "contacts.sync",
            "schedule_kind": "every",
            "interval_secs": 300,
            "paused": true
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(paused["status"], "paused");
    assert_eq!(paused["grace_secs"], 120);
    assert_eq!(paused["ping_key"], key.as_str());

    let other: Value = http
        .post(format!("{base}/v1/crons"))
        .json(&json!({ "name": "reports.nightly", "schedule_kind": "every", "interval_secs": 3600 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let other_id = other["id"].as_str().unwrap().to_string();
    for (name, scope) in [
        ("Both crons", json!({ "kind": "crons", "cron_ids": [cron_id, other_id] })),
        ("All crons", json!({ "kind": "all" })),
    ] {
        let response = http
            .post(format!("{base}/v1/alert-rules"))
            .json(&json!({
                "name": name,
                "condition": { "kind": "cron_missed" },
                "scope": scope,
                "integration_ids": [integration["id"]]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    assert!(
        common::count_rows(&database, "cron_failures", "cron_id", &cron_id).await > 0
    );
    let deleted = http
        .delete(format!("{base}/v1/crons/{cron_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        common::count_rows(&database, "cron_failures", "cron_id", &cron_id).await,
        0
    );
    assert_eq!(ping(&base, &key, "", "").await, StatusCode::NOT_FOUND);
    let rules: Vec<Value> = read(&http, format!("{base}/v1/alert-rules"))
        .await
        .as_array()
        .unwrap()
        .clone();
    let names: Vec<&str> =
        rules.iter().map(|rule| rule["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"All crons"));
    let both = rules.iter().find(|rule| rule["name"] == "Both crons").unwrap();
    assert_eq!(both["scope"], json!({ "kind": "crons", "cron_ids": [other_id] }));
    std::fs::remove_dir_all(dir).ok();
}
