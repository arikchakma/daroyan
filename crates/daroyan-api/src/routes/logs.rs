use axum::Json;
use axum::extract::{Query, State};
use chrono::Utc;
use daroyan_core::analytics::log_lines::{
    LogFilter, LogItem, LogQuery, LogSource, SearchTerm, list_log_context,
    list_log_sources, list_logs,
};
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_protocol::log::{
    SEVERITY_DEBUG, SEVERITY_ERROR, SEVERITY_INFO, SEVERITY_WARN,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::CurrentUser;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;
const CONTEXT_LINES: u32 = 20;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_logs))
        .routes(routes!(get_log_sources))
        .routes(routes!(get_log_context))
}

#[derive(Deserialize)]
struct LogsQuery {
    server_id: Option<String>,
    server_op: Option<String>,
    service: Option<String>,
    service_op: Option<String>,
    method: Option<String>,
    method_op: Option<String>,
    status: Option<String>,
    status_op: Option<String>,
    level: Option<String>,
    q: Option<String>,
    since: Option<i64>,
    until: Option<i64>,
    offset: Option<u32>,
    limit: Option<u32>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = LogPage)]
struct LogPageView {
    lines: Vec<LogItem>,
    has_more: bool,
    until: i64,
}

fn read_min_severity(level: &str) -> Result<u8> {
    match level {
        "debug" => Ok(SEVERITY_DEBUG),
        "info" => Ok(SEVERITY_INFO),
        "warn" => Ok(SEVERITY_WARN),
        "error" => Ok(SEVERITY_ERROR),
        _ => Err(Error::new(
            ErrorName::ValidationError,
            "level must be debug, info, warn or error",
        )),
    }
}

fn read_key(key: &str) -> String {
    let key = key.strip_prefix('@').unwrap_or(key);
    match key {
        "path" => "url.path",
        "status" => "http.status",
        "method" => "http.method",
        "ip" => "client.ip",
        _ => key,
    }
    .to_string()
}

fn read_term(token: String, colon: Option<usize>, negate: bool) -> Option<SearchTerm> {
    if token.is_empty() {
        return None;
    }
    let split = colon.map(|index| token.split_at(index)).filter(|(key, value)| {
        let key = key.strip_prefix('@').unwrap_or(key);
        !key.is_empty()
            && value.len() > 1
            && !value.starts_with("://")
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    });
    Some(match split {
        Some((key, value)) => SearchTerm {
            key: Some(read_key(key)),
            value: value[1..].to_string(),
            negate,
        },
        None => SearchTerm { key: None, value: token, negate },
    })
}

fn split_search(q: &str) -> Vec<SearchTerm> {
    let mut terms = Vec::new();
    let mut chars = q.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            return terms;
        }
        let negate = chars.next_if_eq(&'-').is_some();
        let mut token = String::new();
        let mut colon = None;
        let mut is_quoted = false;
        while let Some(c) = chars.next_if(|c| is_quoted || !c.is_whitespace()) {
            match c {
                '"' => is_quoted = !is_quoted,
                ':' if !is_quoted && colon.is_none() => {
                    colon = Some(token.len());
                    token.push(c);
                }
                _ => token.push(c),
            }
        }
        terms.extend(read_term(token, colon, negate));
    }
}

fn read_list(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn read_filter(values: Option<String>, op: Option<String>) -> LogFilter {
    LogFilter {
        values: read_list(values),
        negate: op.as_deref() == Some("not"),
    }
}

#[utoipa::path(
    get,
    path = "/v1/logs",
    params(
        ("server_id" = Option<String>, Query, description = "Server IDs, comma separated"),
        ("server_op" = Option<String>, Query, description = "`not` excludes the servers"),
        ("service" = Option<String>, Query, description = "Sources, comma separated"),
        ("service_op" = Option<String>, Query, description = "`not` excludes the sources"),
        ("method" = Option<String>, Query, description = "HTTP methods, comma separated"),
        ("method_op" = Option<String>, Query, description = "`not` excludes the methods"),
        ("status" = Option<String>, Query, description = "HTTP status classes like 4xx, comma separated"),
        ("status_op" = Option<String>, Query, description = "`not` excludes the status classes"),
        ("level" = Option<String>, Query, description = "The lowest level shown: debug, info, warn or error"),
        ("q" = Option<String>, Query, description = "Words match the message or the attributes. `key:value` matches an attribute, `*` is a wildcard, and a leading `-` excludes. Quote phrases with spaces"),
        ("since" = Option<i64>, Query, description = "Only lines at or after this time in ms"),
        ("until" = Option<i64>, Query, description = "Only lines at or before this time in ms. Keep it the same while paging"),
        ("offset" = Option<u32>, Query),
        ("limit" = Option<u32>, Query)
    ),
    responses((status = 200, body = LogPageView))
)]
async fn get_logs(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<LogsQuery>,
) -> Result<Json<LogPageView>> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let until = query.until.unwrap_or_else(|| Utc::now().timestamp_millis());
    let min_severity = query.level.as_deref().map(read_min_severity).transpose()?;
    let mut status_classes = read_filter(query.status, query.status_op);
    for value in &mut status_classes.values {
        *value = value.trim_end_matches(['x', 'X']).to_string();
    }
    let mut lines = list_logs(
        &state.analytics,
        LogQuery {
            servers: read_filter(query.server_id, query.server_op),
            services: read_filter(query.service, query.service_op),
            methods: read_filter(query.method, query.method_op),
            status_classes,
            min_severity,
            terms: split_search(query.q.as_deref().unwrap_or_default()),
            since: query.since,
            until,
            offset: query.offset.unwrap_or(0),
            limit: limit + 1,
        },
    )
    .await?;
    let has_more = lines.len() > limit as usize;
    lines.truncate(limit as usize);
    Ok(Json(LogPageView { lines, has_more, until }))
}

#[utoipa::path(get, path = "/v1/logs/sources", responses((status = 200, body = Vec<LogSource>)))]
async fn get_log_sources(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<LogSource>>> {
    Ok(Json(list_log_sources(&state.analytics).await?))
}

#[derive(Deserialize)]
struct ContextQuery {
    server_id: String,
    service: String,
    at: i64,
}

#[utoipa::path(
    get,
    path = "/v1/logs/context",
    params(("server_id" = String, Query), ("service" = String, Query), ("at" = i64, Query)),
    responses((status = 200, body = Vec<LogItem>))
)]
async fn get_log_context(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<ContextQuery>,
) -> Result<Json<Vec<LogItem>>> {
    Ok(Json(
        list_log_context(
            &state.analytics,
            query.server_id,
            query.service,
            query.at,
            CONTEXT_LINES,
        )
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::split_search;

    fn read(q: &str) -> Vec<(Option<String>, String, bool)> {
        split_search(q)
            .into_iter()
            .map(|term| (term.key, term.value, term.negate))
            .collect()
    }

    #[test]
    fn splits_github_style_search() {
        assert_eq!(
            read(
                r#"payment -path:/v1/health @userId:42 -"GET /v1" status:5* http://x.io"#
            ),
            [
                (None, "payment".to_string(), false),
                (Some("url.path".to_string()), "/v1/health".to_string(), true),
                (Some("userId".to_string()), "42".to_string(), false),
                (None, "GET /v1".to_string(), true),
                (Some("http.status".to_string()), "5*".to_string(), false),
                (None, "http://x.io".to_string(), false),
            ]
        );
        assert_eq!(
            read(r#"@broken msg:"two words" -"#),
            [
                (None, "@broken".to_string(), false),
                (Some("msg".to_string()), "two words".to_string(), false),
            ]
        );
    }
}
