use daroyan_core::auth::create_session;
use daroyan_core::db::Database;
use daroyan_core::db::queries::members::NewMember;
use daroyan_core::db::queries::users::GoogleUser;
use reqwest::Client;
use reqwest::header::{COOKIE, HeaderMap, HeaderValue};

pub async fn sign_in(database: &Database) -> Client {
    sign_in_as(database, "test", "Ops", "admin").await
}

#[allow(dead_code)]
pub async fn sign_in_member(database: &Database, name: &str) -> Client {
    sign_in_as(database, name, name, "member").await
}

async fn sign_in_as(database: &Database, key: &str, name: &str, role: &str) -> Client {
    let email = if key == "test" {
        "ops@daroyan.test".to_string()
    } else {
        format!("{key}@daroyan.test")
    };
    if database.find_member_by_email(&email).await.unwrap().is_none() {
        database
            .create_member(NewMember {
                id: format!("mem_{key}"),
                email: email.clone(),
                role: role.to_string(),
                inviter_id: "usr_test".to_string(),
            })
            .await
            .unwrap();
    }
    let user = database
        .save_google_user(GoogleUser {
            id: format!("usr_{key}"),
            google_id: &format!("google_{key}"),
            email: &email,
            name,
            avatar_url: None,
        })
        .await
        .unwrap();
    let session = create_session(database, &user.id, None, None).await.unwrap();
    let mut headers = HeaderMap::new();
    let cookie = format!("daroyan_session={}", session.token);
    headers.insert(COOKIE, HeaderValue::from_str(&cookie).unwrap());
    Client::builder().default_headers(headers).build().unwrap()
}

#[allow(dead_code)]
pub async fn count_rows(
    database: &Database,
    table: &'static str,
    column: &'static str,
    id: &str,
) -> i64 {
    use sea_orm::ConnectionTrait;
    use sea_orm::sea_query::{Asterisk, Expr, ExprTrait, Func, Query};

    let row = database
        .conn
        .query_one(
            Query::select()
                .expr_as(Func::count(Expr::col(Asterisk)), "count")
                .from(table)
                .and_where(Expr::col(column).eq(id)),
        )
        .await
        .unwrap()
        .unwrap();
    row.try_get("", "count").unwrap()
}
