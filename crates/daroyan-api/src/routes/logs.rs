use axum::Json;
use axum::extract::{Query, State};
use chrono::Utc;
use daroyan_core::analytics::log_lines::{
    LogBucket, LogFilter, LogItem, LogQuery, LogSearch, LogSource, SearchField,
    SearchMatch, SearchTerm, count_log_buckets, list_log_context, list_log_sources,
    list_logs,
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
const DEFAULT_VOLUME_MS: i64 = 24 * 3_600_000;
const MAX_BUCKETS: i64 = 120;
const BUCKET_STEPS_MS: [i64; 16] = [
    1_000,
    5_000,
    10_000,
    30_000,
    60_000,
    5 * 60_000,
    10 * 60_000,
    15 * 60_000,
    30 * 60_000,
    3_600_000,
    2 * 3_600_000,
    3 * 3_600_000,
    6 * 3_600_000,
    12 * 3_600_000,
    24 * 3_600_000,
    7 * 24 * 3_600_000,
];

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_logs))
        .routes(routes!(get_log_volume))
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

#[derive(Serialize, ToSchema)]
#[schema(as = LogVolume)]
struct LogVolumeView {
    bucket_ms: i64,
    since: i64,
    until: i64,
    buckets: Vec<LogBucket>,
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

struct SearchToken {
    key: Option<String>,
    value: String,
    is_quoted: bool,
    is_regex: bool,
}

fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@' | '*')
}

fn read_key_end(chars: &[char], start: usize) -> Option<usize> {
    let colon = (start..chars.len()).find(|&index| !is_key_char(chars[index]))?;
    let key: String = chars[start..colon].iter().collect();
    let key = key.strip_prefix('@').unwrap_or(&key);
    let is_key = key == "*" || (!key.is_empty() && !key.contains('*'));
    let value = chars.get(colon + 1..).unwrap_or_default();
    let has_value = value.first().is_some_and(|c| !c.is_whitespace());
    let is_url = value.starts_with(&['/', '/']);

    (chars[colon] == ':' && is_key && has_value && !is_url).then_some(colon)
}

fn read_regex_end(chars: &[char], start: usize) -> Option<usize> {
    if chars.get(start) != Some(&'/') {
        return None;
    }

    let mut index = start + 1;
    while index < chars.len() {
        match chars[index] {
            '\\' => index += 2,
            '/' if chars.get(index + 1).is_none_or(|c| c.is_whitespace()) => {
                return (index > start + 1).then_some(index);
            }
            _ => index += 1,
        }
    }
    None
}

fn read_token(chars: &[char], start: usize) -> (SearchToken, usize) {
    let colon = read_key_end(chars, start);
    let key = colon.map(|end| chars[start..end].iter().collect::<String>());
    let value_start = colon.map_or(start, |end| end + 1);

    if let Some(end) = read_regex_end(chars, value_start) {
        let value = chars[value_start + 1..end].iter().collect();
        let token = SearchToken { key, value, is_quoted: false, is_regex: true };
        return (token, end + 1);
    }

    let mut value = String::new();
    let mut is_quoted = false;
    let mut is_inside_quotes = false;
    let mut is_inside_list = colon.is_some() && chars.get(value_start) == Some(&'[');
    let mut index = value_start;
    while let Some(&c) = chars.get(index) {
        if c.is_whitespace() && !is_inside_quotes && !is_inside_list {
            break;
        }
        if c == ']' {
            is_inside_list = false;
        }

        match c {
            '"' => {
                is_quoted = true;
                is_inside_quotes = !is_inside_quotes;
            }
            '\\' if is_inside_quotes
                && matches!(chars.get(index + 1), Some('"' | '\\')) =>
            {
                index += 1;
                value.push(chars[index]);
            }
            _ => value.push(c),
        }
        index += 1;
    }

    (SearchToken { key, value, is_quoted, is_regex: false }, index)
}

fn read_number(value: &str, sign: char) -> Option<f64> {
    value.strip_prefix(sign)?.parse().ok()
}

fn read_match(token: SearchToken) -> Result<SearchMatch> {
    let SearchToken { key, value, is_quoted, is_regex } = token;

    if is_regex {
        return regex::Regex::new(&value)
            .map(|_| SearchMatch::Regex(value.clone()))
            .map_err(|_| {
                Error::new(
                    ErrorName::ValidationError,
                    format!("/{value}/ is not a valid regex"),
                )
            });
    }
    if is_quoted || key.is_none() {
        return Ok(SearchMatch::Text(value));
    }

    if let Some(list) = value.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
        let items = list
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
        return Ok(SearchMatch::OneOf(items));
    }
    if let Some(number) = read_number(&value, '>') {
        return Ok(SearchMatch::Above(number));
    }
    if let Some(number) = read_number(&value, '<') {
        return Ok(SearchMatch::Below(number));
    }

    Ok(SearchMatch::Text(value))
}

fn read_term(token: SearchToken, negate: bool) -> Result<Option<SearchTerm>> {
    if token.value.is_empty() {
        return Ok(None);
    }

    let field = match token.key.as_deref() {
        None => SearchField::Message,
        Some("*") => SearchField::Any,
        Some(key) => SearchField::Key(read_key(key)),
    };
    let matches = read_match(token)?;
    if matches == SearchMatch::OneOf(Vec::new()) {
        return Ok(None);
    }

    Ok(Some(SearchTerm { field, matches, negate }))
}

fn split_search(q: &str) -> Result<Vec<SearchTerm>> {
    let chars: Vec<char> = q.chars().collect();
    let mut terms = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index].is_whitespace() {
            index += 1;
            continue;
        }

        let negate = chars[index] == '-';
        let start = if negate { index + 1 } else { index };
        let (token, end) = read_token(&chars, start);
        index = end;

        terms.extend(read_term(token, negate)?);
    }

    Ok(terms)
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
        ("q" = Option<String>, Query, description = "Words match the message, and `*:word` matches any field. `key:value` matches an attribute, where `*` is a wildcard and `key:*` means the attribute exists. `key:[a,b]` matches either value, `key:>500` and `key:<100` compare numbers, and `/regex/` or `key:/regex/` match an RE2 regex. A leading `-` excludes. Quote text with spaces. Matching ignores case"),
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
    let offset = query.offset.unwrap_or(0);
    let search = read_search(query)?;
    let until = search.until;
    let mut lines =
        list_logs(&state.analytics, LogQuery { search, offset, limit: limit + 1 })
            .await?;
    let has_more = lines.len() > limit as usize;
    lines.truncate(limit as usize);
    Ok(Json(LogPageView { lines, has_more, until }))
}

#[utoipa::path(
    get,
    path = "/v1/logs/volume",
    params(
        ("server_id" = Option<String>, Query, description = "Server IDs, comma separated"),
        ("server_op" = Option<String>, Query, description = "`not` excludes the servers"),
        ("service" = Option<String>, Query, description = "Sources, comma separated"),
        ("service_op" = Option<String>, Query, description = "`not` excludes the sources"),
        ("method" = Option<String>, Query, description = "HTTP methods, comma separated"),
        ("method_op" = Option<String>, Query, description = "`not` excludes the methods"),
        ("status" = Option<String>, Query, description = "HTTP status classes like 4xx, comma separated"),
        ("status_op" = Option<String>, Query, description = "`not` excludes the status classes"),
        ("level" = Option<String>, Query, description = "The lowest level counted: debug, info, warn or error"),
        ("q" = Option<String>, Query, description = "The same search as /v1/logs"),
        ("since" = Option<i64>, Query, description = "Start of the range in ms. Defaults to 24 hours before until"),
        ("until" = Option<i64>, Query, description = "End of the range in ms. Defaults to now")
    ),
    responses((status = 200, body = LogVolumeView))
)]
async fn get_log_volume(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<LogsQuery>,
) -> Result<Json<LogVolumeView>> {
    let mut search = read_search(query)?;
    let until = search.until;
    let since = search.since.unwrap_or(until - DEFAULT_VOLUME_MS).min(until);
    let bucket_ms = pick_bucket_ms(until - since);
    let start = since.div_euclid(bucket_ms) * bucket_ms;
    search.since = Some(since);

    let counted = count_log_buckets(&state.analytics, search, bucket_ms).await?;
    Ok(Json(LogVolumeView {
        bucket_ms,
        since,
        until,
        buckets: fill_buckets(counted, start, until, bucket_ms),
    }))
}

fn read_search(query: LogsQuery) -> Result<LogSearch> {
    let mut status_classes = read_filter(query.status, query.status_op);
    for value in &mut status_classes.values {
        *value = value.trim_end_matches(['x', 'X']).to_string();
    }

    Ok(LogSearch {
        servers: read_filter(query.server_id, query.server_op),
        services: read_filter(query.service, query.service_op),
        methods: read_filter(query.method, query.method_op),
        status_classes,
        min_severity: query.level.as_deref().map(read_min_severity).transpose()?,
        terms: split_search(query.q.as_deref().unwrap_or_default())?,
        since: query.since,
        until: query.until.unwrap_or_else(|| Utc::now().timestamp_millis()),
    })
}

fn pick_bucket_ms(span_ms: i64) -> i64 {
    BUCKET_STEPS_MS
        .into_iter()
        .find(|step| span_ms / step < MAX_BUCKETS)
        .unwrap_or(BUCKET_STEPS_MS[BUCKET_STEPS_MS.len() - 1])
}

fn fill_buckets(
    counted: Vec<LogBucket>,
    start: i64,
    until: i64,
    bucket_ms: i64,
) -> Vec<LogBucket> {
    let mut counted = counted.into_iter().peekable();
    (start..=until)
        .step_by(bucket_ms as usize)
        .map(|at| {
            counted
                .next_if(|bucket| bucket.at == at)
                .unwrap_or(LogBucket { at, ..LogBucket::default() })
        })
        .collect()
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
    use daroyan_core::analytics::log_lines::LogBucket;

    use daroyan_core::analytics::log_lines::{SearchField, SearchMatch, SearchTerm};

    use super::{fill_buckets, pick_bucket_ms, split_search};

    fn term(field: SearchField, matches: SearchMatch, negate: bool) -> SearchTerm {
        SearchTerm { field, matches, negate }
    }

    fn key(name: &str) -> SearchField {
        SearchField::Key(name.to_string())
    }

    fn text(value: &str) -> SearchMatch {
        SearchMatch::Text(value.to_string())
    }

    #[test]
    fn splits_github_style_search() {
        assert_eq!(
            split_search(
                r#"payment -path:/v1/health @userId:42 -"GET /v1" status:5* http://x.io"#
            )
            .unwrap(),
            [
                SearchTerm::message("payment", false),
                term(key("url.path"), text("/v1/health"), true),
                term(key("userId"), text("42"), false),
                SearchTerm::message("GET /v1", true),
                term(key("http.status"), text("5*"), false),
                SearchTerm::message("http://x.io", false),
            ]
        );
        assert_eq!(
            split_search(r#"@broken msg:"two words" -"#).unwrap(),
            [
                SearchTerm::message("@broken", false),
                term(key("msg"), text("two words"), false),
            ]
        );
    }

    #[test]
    fn splits_advanced_search() {
        assert_eq!(
            split_search(
                "/time(d)? ?out/ -user_agent:/bot|crawler/ *:usr_42 service:[api, web] status:>499 duration_ms:<100 user_id:*"
            )
            .unwrap(),
            [
                term(SearchField::Message, SearchMatch::Regex("time(d)? ?out".into()), false),
                term(key("user_agent"), SearchMatch::Regex("bot|crawler".into()), true),
                term(SearchField::Any, text("usr_42"), false),
                term(key("service"), SearchMatch::OneOf(vec!["api".into(), "web".into()]), false),
                term(key("http.status"), SearchMatch::Above(499.0), false),
                term(key("duration_ms"), SearchMatch::Below(100.0), false),
                term(key("user_id"), text("*"), false),
            ]
        );
        assert_eq!(
            split_search(r#""user:42 failed" msg:">5" "say \"hi\"" path:/v1/ x:[]"#)
                .unwrap(),
            [
                SearchTerm::message("user:42 failed", false),
                term(key("msg"), text(">5"), false),
                SearchTerm::message(r#"say "hi""#, false),
                term(key("url.path"), SearchMatch::Regex("v1".into()), false),
            ]
        );
        assert!(split_search("/(unclosed/").is_err());
    }

    #[test]
    fn picks_about_a_hundred_buckets() {
        let minute = 60_000;
        let hour = 60 * minute;
        assert_eq!(pick_bucket_ms(15 * minute), 10_000);
        assert_eq!(pick_bucket_ms(hour), 60_000);
        assert_eq!(pick_bucket_ms(24 * hour), 15 * minute);
        assert_eq!(pick_bucket_ms(7 * 24 * hour), 2 * hour);
        assert_eq!(pick_bucket_ms(0), 1_000);
    }

    #[test]
    fn fills_empty_buckets_with_zeros() {
        let error = LogBucket { at: 20, error: 3, ..LogBucket::default() };
        let buckets = fill_buckets(vec![error.clone()], 10, 35, 10);
        assert_eq!(
            buckets,
            [
                LogBucket { at: 10, ..LogBucket::default() },
                error,
                LogBucket { at: 30, ..LogBucket::default() },
            ]
        );
    }
}
