use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use chrono::{Duration, Utc};
use daroyan_core::analytics::spans::to_span_rows;
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::token::hash_token;
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use prost::Message;
use tower_http::decompression::RequestDecompressionLayer;

use crate::AppState;

const MAX_BODY: usize = 8 * 1024 * 1024;
const PROTOBUF: &str = "application/x-protobuf";
const JSON: &str = "application/json";
const TOUCH_AFTER_SECS: i64 = 60;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/otlp/v1/traces", post(export_traces))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(RequestDecompressionLayer::new())
}

fn read_bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

async fn authorize(state: &AppState, headers: &HeaderMap) -> Result<()> {
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
    Ok(())
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

async fn export_traces(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    authorize(&state, &headers).await?;
    let Some(is_json) = is_json(&headers) else {
        return Ok((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Send application/x-protobuf or application/json",
        )
            .into_response());
    };
    let request = if is_json {
        serde_json::from_slice::<ExportTraceServiceRequest>(&body).map_err(|error| {
            Error::new(
                ErrorName::ValidationError,
                format!("The OTLP JSON is not valid: {error}"),
            )
        })?
    } else {
        ExportTraceServiceRequest::decode(body).map_err(|error| {
            Error::new(
                ErrorName::ValidationError,
                format!("The OTLP protobuf is not valid: {error}"),
            )
        })?
    };
    let (spans, exceptions) = to_span_rows(&request, Utc::now().timestamp_millis());
    state.analytics.buffer_spans(spans, exceptions)?;
    let response = ExportTraceServiceResponse::default();
    Ok(if is_json {
        ([(header::CONTENT_TYPE, JSON)], "{}").into_response()
    } else {
        ([(header::CONTENT_TYPE, PROTOBUF)], response.encode_to_vec()).into_response()
    })
}
