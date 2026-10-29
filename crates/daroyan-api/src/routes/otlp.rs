use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use chrono::{Duration, Utc};
use daroyan_core::analytics::log_lines::{LogRow, to_otlp_log_rows};
use daroyan_core::analytics::spans::to_span_rows;
use daroyan_core::db::entities::ingest_tokens::Model as IngestToken;
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::servers::hosts::HostIndex;
use daroyan_core::token::hash_token;
use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsPartialSuccess, ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use prost::Message;
use serde::de::DeserializeOwned;
use tower_http::decompression::RequestDecompressionLayer;

use crate::AppState;

const MAX_BODY: usize = 8 * 1024 * 1024;
const PROTOBUF: &str = "application/x-protobuf";
const JSON: &str = "application/json";
const TOUCH_AFTER_SECS: i64 = 60;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/otlp/v1/traces", post(export_traces))
        .route("/v1/otlp/v1/logs", post(export_logs))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(RequestDecompressionLayer::new())
}

fn read_bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

async fn authorize(state: &AppState, headers: &HeaderMap) -> Result<IngestToken> {
    let token = read_bearer(headers).ok_or_else(|| {
        Error::new(
            ErrorName::Unauthorized,
            "Send an ingest token as Authorization: Bearer <token>",
        )
    })?;
    let stored = state
        .database
        .find_ingest_token_by_hash(&hash_token(token))
        .await?
        .ok_or_else(|| {
            Error::new(ErrorName::Unauthorized, "The ingest token is not valid")
        })?;
    let now = Utc::now();
    if stored
        .last_used_at
        .is_none_or(|last| now - last > Duration::seconds(TOUCH_AFTER_SECS))
    {
        state.database.touch_ingest_token(&stored.id, now).await?;
    }
    Ok(stored)
}

fn is_json(headers: &HeaderMap) -> Option<bool> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or(PROTOBUF)
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match content_type.as_str() {
        PROTOBUF => Some(false),
        JSON => Some(true),
        _ => None,
    }
}

fn unsupported_media_type() -> Response {
    (
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "Send application/x-protobuf or application/json",
    )
        .into_response()
}

fn decode<T: Message + Default + DeserializeOwned>(
    is_json: bool,
    body: Bytes,
) -> Result<T> {
    if is_json {
        return serde_json::from_slice(&body).map_err(|error| {
            Error::new(
                ErrorName::ValidationError,
                format!("The OTLP JSON is not valid: {error}"),
            )
        });
    }

    T::decode(body).map_err(|error| {
        Error::new(
            ErrorName::ValidationError,
            format!("The OTLP protobuf is not valid: {error}"),
        )
    })
}

fn encode<T: Message + serde::Serialize>(is_json: bool, response: &T) -> Response {
    if is_json {
        let body = serde_json::to_string(response).unwrap_or_else(|_| "{}".to_string());
        return ([(header::CONTENT_TYPE, JSON)], body).into_response();
    }

    ([(header::CONTENT_TYPE, PROTOBUF)], response.encode_to_vec()).into_response()
}

async fn read_hosts(state: &AppState) -> Result<HostIndex> {
    let servers = state.database.list_servers().await?;
    Ok(HostIndex::from(servers.as_slice()))
}

async fn export_traces(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    authorize(&state, &headers).await?;
    let Some(is_json) = is_json(&headers) else {
        return Ok(unsupported_media_type());
    };

    let request: ExportTraceServiceRequest = decode(is_json, body)?;
    let hosts = read_hosts(&state).await?;
    let (spans, exceptions) =
        to_span_rows(&request, &hosts, Utc::now().timestamp_millis());
    state.analytics.buffer_spans(spans, exceptions)?;

    Ok(encode(is_json, &ExportTraceServiceResponse::default()))
}

async fn export_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let token = authorize(&state, &headers).await?;
    let Some(is_json) = is_json(&headers) else {
        return Ok(unsupported_media_type());
    };

    let request: ExportLogsServiceRequest = decode(is_json, body)?;
    let hosts = read_hosts(&state).await?;
    let rows = to_otlp_log_rows(&request, &hosts, Utc::now().timestamp_millis());
    let (rows, dropped) = state.log_budgets.keep(&token.id, rows, LogRow::size);
    state.analytics.buffer_log_rows(rows)?;

    let mut response = ExportLogsServiceResponse::default();
    if dropped > 0 {
        state.database.add_dropped_log_records(&token.id, dropped).await?;
        response.partial_success = Some(ExportLogsPartialSuccess {
            rejected_log_records: i64::try_from(dropped).unwrap_or(i64::MAX),
            error_message: format!(
                "Dropped {dropped} log records over the budget of {} KB a minute for this token",
                state.config.otlp_log_budget / 1024
            ),
        });
    }

    Ok(encode(is_json, &response))
}
