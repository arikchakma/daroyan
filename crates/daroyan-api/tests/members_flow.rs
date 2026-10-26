mod common;

use std::net::SocketAddr;

use daroyan_api::app::{build_app, open_state};
use daroyan_core::auth::create_session;
use daroyan_core::config::Config;
use daroyan_core::db::ids::MemberId;
use daroyan_core::db::queries::members::NewMember;
use daroyan_core::db::queries::users::GoogleUser;
use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::net::TcpListener;

#[tokio::test]
async fn admins_invite_and_remove_members() {
    let dir =
        std::env::temp_dir().join(format!("daroyan-members-{}", std::process::id()));
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
        allowed_emails: vec!["boss@daroyan.test".to_string()],
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
    let admin = common::sign_in(&state.database).await;
    let member = common::sign_in_member(&state.database, "dev").await;
    let database = state.database.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(
            listener,
            build_app(state).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
    });

    let me: Value = member
        .get(format!("{base}/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["role"], "member");
    assert_eq!(me["email"], "dev@daroyan.test");

    let invite = |email: &str, role: &str| {
        admin
            .post(format!("{base}/v1/members"))
            .json(&json!({ "email": email, "role": role }))
            .send()
    };
    let invited = invite(" New@Daroyan.test ", "member").await.unwrap();
    assert_eq!(invited.status(), StatusCode::CREATED);
    let invited: Value = invited.json().await.unwrap();
    assert_eq!(invited["email"], "new@daroyan.test");
    assert_eq!(invited["inviter_id"], "usr_test");
    assert_eq!(
        invite("new@daroyan.test", "admin").await.unwrap().status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        invite("boss@daroyan.test", "member").await.unwrap().status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        invite("not-an-email", "member").await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        invite("x@daroyan.test", "owner").await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );

    let check =
        json!({ "name": "API", "kind": "tcp", "target": "127.0.0.1:1", "paused": true });
    let blocked = member
        .post(format!("{base}/v1/uptime-checks"))
        .json(&check)
        .send()
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    let blocked_invite = member
        .post(format!("{base}/v1/members"))
        .json(&json!({ "email": "friend@daroyan.test", "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(blocked_invite.status(), StatusCode::FORBIDDEN);
    let incident = member
        .post(format!("{base}/v1/incidents"))
        .json(&json!({ "kind": "incident", "title": "API down", "severity": "major", "update": { "status": "investigating", "body": "Looking" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(incident.status(), StatusCode::CREATED);
    for site in ["cross-site", "same-site"] {
        let forged = member
            .post(format!("{base}/v1/uptime-checks/upt_x/run"))
            .header("sec-fetch-site", site)
            .send()
            .await
            .unwrap();
        assert_eq!(forged.status(), StatusCode::FORBIDDEN);
    }
    let cross_site_read = member
        .get(format!("{base}/v1/servers"))
        .header("sec-fetch-site", "cross-site")
        .send()
        .await
        .unwrap();
    assert_eq!(cross_site_read.status(), StatusCode::OK);

    let promoted = admin
        .patch(format!("{base}/v1/members/mem_dev"))
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(promoted.status(), StatusCode::NO_CONTENT);
    let allowed = member
        .post(format!("{base}/v1/uptime-checks"))
        .json(&check)
        .send()
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::CREATED);
    let self_removed = member
        .delete(format!("{base}/v1/members/mem_dev"))
        .send()
        .await
        .unwrap();
    assert_eq!(self_removed.status(), StatusCode::FORBIDDEN);

    let members: Value = admin
        .get(format!("{base}/v1/members"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let emails: Vec<&str> = members
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["email"].as_str().unwrap())
        .collect();
    assert_eq!(emails, ["ops@daroyan.test", "dev@daroyan.test", "new@daroyan.test"]);
    assert_eq!(members[1]["role"], "admin");
    assert!(members[1]["user"].is_object());
    assert!(members[2]["user"].is_null());

    let removed = admin
        .delete(format!("{base}/v1/members/mem_dev"))
        .send()
        .await
        .unwrap();
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let after = member.get(format!("{base}/v1/servers")).send().await.unwrap();
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
    let users: Value = admin
        .get(format!("{base}/v1/users"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(users.as_array().unwrap().len(), 1);

    let created: Value = admin
        .post(format!("{base}/v1/mcp-tokens"))
        .json(&json!({ "name": "claude" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = created["token"].as_str().unwrap();
    let token_id = created["id"].as_str().unwrap();
    let anonymous = reqwest::Client::new();
    let by_token = [
        anonymous.get(format!("{base}/v1/mcp-tokens")),
        anonymous
            .post(format!("{base}/v1/mcp-tokens"))
            .json(&json!({ "name": "more" })),
        anonymous.delete(format!("{base}/v1/mcp-tokens/{token_id}")),
        anonymous
            .post(format!("{base}/v1/members"))
            .json(&json!({ "email": "sneaky@daroyan.test", "role": "admin" })),
        anonymous.delete(format!("{base}/v1/members/mem_x")),
    ];
    for request in by_token {
        let response = request.bearer_auth(token).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let read = anonymous
        .get(format!("{base}/v1/servers"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::UNAUTHORIZED);

    let stale = create_session(&database, "usr_dev", None, None).await.unwrap();
    let stale_get = || {
        anonymous
            .get(format!("{base}/v1/servers"))
            .header("cookie", format!("daroyan_session={}", stale.token))
            .send()
    };
    let reinvited = invite("dev@daroyan.test", "member").await.unwrap();
    assert_eq!(reinvited.status(), StatusCode::CREATED);
    assert_eq!(stale_get().await.unwrap().status(), StatusCode::UNAUTHORIZED);

    let fresh = create_session(&database, "usr_dev", None, None).await.unwrap();
    let fresh_get = anonymous
        .get(format!("{base}/v1/servers"))
        .header("cookie", format!("daroyan_session={}", fresh.token))
        .send()
        .await
        .unwrap();
    assert_eq!(fresh_get.status(), StatusCode::OK);
    let relinked = database
        .save_google_user(GoogleUser {
            id: "usr_other".to_string(),
            google_id: "google_other",
            email: "dev@daroyan.test",
            name: "Other",
            avatar_url: None,
        })
        .await
        .unwrap();
    assert_eq!(relinked.id, "usr_dev");
    assert_eq!(relinked.google_id, "google_other");
    let after_relink = anonymous
        .get(format!("{base}/v1/servers"))
        .header("cookie", format!("daroyan_session={}", fresh.token))
        .send()
        .await
        .unwrap();
    assert_eq!(after_relink.status(), StatusCode::UNAUTHORIZED);

    let forged_logout = admin
        .post(format!("{base}/v1/auth/logout"))
        .header("sec-fetch-site", "cross-site")
        .send()
        .await
        .unwrap();
    assert_eq!(forged_logout.status(), StatusCode::FORBIDDEN);
    let still_in = admin.get(format!("{base}/v1/auth/me")).send().await.unwrap();
    assert_eq!(still_in.status(), StatusCode::OK);

    for (id, email) in [("mem_a", "a@daroyan.test"), ("mem_b", "b@daroyan.test")] {
        database
            .create_member(NewMember {
                id: id.to_string(),
                email: email.to_string(),
                role: "admin".to_string(),
                inviter_id: "usr_test".to_string(),
            })
            .await
            .unwrap();
    }
    let (a, b) = (MemberId::from("mem_a"), MemberId::from("mem_b"));
    let demoted = database.update_member_role(&b, "member", Some("a@daroyan.test"));
    assert!(demoted.await.unwrap());
    let counter = database.update_member_role(&a, "member", Some("b@daroyan.test"));
    assert!(!counter.await.unwrap());
    let a_member = database.find_member(&a).await.unwrap().unwrap();
    assert_eq!(a_member.role, "admin");
    assert!(
        !database
            .delete_member(&a_member, Some("b@daroyan.test"))
            .await
            .unwrap()
    );
    assert!(database.find_member(&a).await.unwrap().is_some());
}
