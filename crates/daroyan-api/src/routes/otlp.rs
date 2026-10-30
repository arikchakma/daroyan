use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use chrono::Utc;
use daroyan_core::analytics::log_lines::{LogRow, count_otlp_records, to_otlp_log_rows};
use daroyan_core::analytics::spans::to_span_rows;
use daroyan_core::db::entities::ingest_tokens::Model as IngestToken;
use daroyan_core::error::{Error, ErrorName};
use daroyan_core::token::hash_token;
use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use prost::Message;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tower_http::decompression::RequestDecompressionLayer;

use crate::AppState;

const MAX_BODY: usize = 16 * 1024 * 1024;
const MAX_LOG_RECORDS: usize = 10_000;
const PROTOBUF: &str = "application/x-protobuf";
const JSON: &str = "application/json";
const TOUCH_AFTER_SECS: i64 = 60;
const BUFFER_RETRY_AFTER: Duration = Duration::from_secs(5);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/otlp/v1/traces", post(export_traces))
        .route("/v1/otlp/v1/logs", post(export_logs))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(RequestDecompressionLayer::new())
}

#[derive(Clone, Copy)]
enum Encoding {
    Protobuf,
    Json,
}

impl Encoding {
    fn read(headers: &HeaderMap) -> Option<Self> {
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
            PROTOBUF => Some(Self::Protobuf),
            JSON => Some(Self::Json),
            _ => None,
        }
    }

    fn decode<T: Message + Default + DeserializeOwned>(
        self,
        body: Bytes,
    ) -> Result<T, OtlpError> {
        let decoded = match self {
            Self::Json => {
                serde_json::from_slice(&body).map_err(|error| error.to_string())
            }
            Self::Protobuf => T::decode(body).map_err(|error| error.to_string()),
        };
        decoded.map_err(|error| {
            OtlpError::InvalidData(format!("The OTLP body is not valid: {error}"))
        })
    }

    fn encode<T: Message + Serialize>(self, status: StatusCode, message: &T) -> Response {
        match self {
            Self::Json => {
                let body =
                    serde_json::to_string(message).unwrap_or_else(|_| "{}".to_string());
                (status, [(header::CONTENT_TYPE, JSON)], body).into_response()
            }
            Self::Protobuf => {
                (status, [(header::CONTENT_TYPE, PROTOBUF)], message.encode_to_vec())
                    .into_response()
            }
        }
    }
}

// `google.rpc.Status`, which OTLP/HTTP requires as the body of every failure.
#[derive(Clone, PartialEq, Message, Serialize)]
struct RpcStatus {
    #[prost(int32, tag = "1")]
    code: i32,
    #[prost(string, tag = "2")]
    message: String,
}

#[derive(Debug)]
enum OtlpError {
    InvalidData(String),
    TooLarge(String),
    Unauthorized(String),
    Throttled { message: String, retry_after: Duration },
    Unavailable { message: String, retry_after: Duration },
    Internal,
}

impl OtlpError {
    fn into_response(self, encoding: Encoding) -> Response {
        // Codes from google.rpc.Code. Only 429 and 503 tell the client to retry.
        let (status, code, message, retry_after) = match self {
            Self::InvalidData(message) => (StatusCode::BAD_REQUEST, 3, message, None),
            Self::TooLarge(message) => (StatusCode::PAYLOAD_TOO_LARGE, 3, message, None),
            Self::Unauthorized(message) => (StatusCode::UNAUTHORIZED, 16, message, None),
            Self::Throttled { message, retry_after } => {
                (StatusCode::TOO_MANY_REQUESTS, 8, message, Some(retry_after))
            }
            Self::Unavailable { message, retry_after } => {
                (StatusCode::SERVICE_UNAVAILABLE, 14, message, Some(retry_after))
            }
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                13,
                "Something went wrong. Please try again later.".to_string(),
                None,
            ),
        };

        let mut response = encoding.encode(status, &RpcStatus { code, message });
        if let Some(retry_after) = retry_after {
            let seconds = retry_after.as_secs_f64().ceil().max(1.0) as u64;
            response.headers_mut().insert(header::RETRY_AFTER, seconds.into());
        }
        response
    }
}

impl From<Error> for OtlpError {
    fn from(error: Error) -> Self {
        let error = error.into_api();
        match error.name {
            ErrorName::ValidationError => Self::InvalidData(error.message),
            ErrorName::Unauthorized => Self::Unauthorized(error.message),
            ErrorName::RateLimitExceeded => Self::Unavailable {
                message: error.message,
                retry_after: BUFFER_RETRY_AFTER,
            },
            _ => Self::Internal,
        }
    }
}

impl From<BytesRejection> for OtlpError {
    fn from(rejection: BytesRejection) -> Self {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            return Self::TooLarge(format!(
                "The body is over {} MB after decompression",
                MAX_BODY / 1024 / 1024
            ));
        }

        Self::InvalidData(rejection.body_text())
    }
}

fn read_bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

async fn authorize(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<IngestToken, OtlpError> {
    let token = read_bearer(headers).ok_or_else(|| {
        OtlpError::Unauthorized(
            "Send an ingest token as Authorization: Bearer <token>".to_string(),
        )
    })?;
    let stored = state
        .database
        .find_ingest_token_by_hash(&hash_token(token))
        .await
        .map_err(Error::from)?
        .ok_or_else(|| {
            OtlpError::Unauthorized("The ingest token is not valid".to_string())
        })?;

    let now = Utc::now();
    if stored
        .last_used_at
        .is_none_or(|last| now - last > chrono::Duration::seconds(TOUCH_AFTER_SECS))
    {
        state
            .database
            .touch_ingest_token(&stored.id, now)
            .await
            .map_err(Error::from)?;
    }
    Ok(stored)
}

fn unsupported_media_type() -> Response {
    Encoding::Json.encode(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        &RpcStatus {
            code: 3,
            message: "Send application/x-protobuf or application/json".to_string(),
        },
    )
}

async fn export_traces(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let Some(encoding) = Encoding::read(&headers) else {
        return unsupported_media_type();
    };

    accept_traces(&state, &headers, encoding, body)
        .await
        .unwrap_or_else(|error| error.into_response(encoding))
}

async fn accept_traces(
    state: &AppState,
    headers: &HeaderMap,
    encoding: Encoding,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, OtlpError> {
    authorize(state, headers).await?;
    let request: ExportTraceServiceRequest = encoding.decode(body?)?;

    let hosts = state.hosts.read(&state.database).await.map_err(Error::from)?;
    let (spans, exceptions) =
        to_span_rows(&request, &hosts, Utc::now().timestamp_millis());
    state.analytics.buffer_spans(spans, exceptions)?;

    Ok(encoding.encode(StatusCode::OK, &ExportTraceServiceResponse::default()))
}

async fn export_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let Some(encoding) = Encoding::read(&headers) else {
        return unsupported_media_type();
    };

    accept_logs(&state, &headers, encoding, body)
        .await
        .unwrap_or_else(|error| error.into_response(encoding))
}

async fn accept_logs(
    state: &AppState,
    headers: &HeaderMap,
    encoding: Encoding,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, OtlpError> {
    let token = authorize(state, headers).await?;
    let request: ExportLogsServiceRequest = encoding.decode(body?)?;

    let count = count_otlp_records(&request);
    if count > MAX_LOG_RECORDS {
        return Err(OtlpError::TooLarge(format!(
            "Send at most {MAX_LOG_RECORDS} log records a request, not {count}"
        )));
    }
    if !state.analytics.has_log_room() {
        return Err(OtlpError::Unavailable {
            message: "Too many log records at once. Retry in a few seconds".to_string(),
            retry_after: BUFFER_RETRY_AFTER,
        });
    }

    let hosts = state.hosts.read(&state.database).await.map_err(Error::from)?;
    let rows = to_otlp_log_rows(&request, &hosts, Utc::now().timestamp_millis());
    let size = rows.iter().map(LogRow::size).sum();
    if let Err(retry_after) = state.log_budgets.spend(&token.id, size) {
        if let Err(error) = state
            .database
            .add_throttled_log_records(&token.id, count as u64)
            .await
        {
            tracing::warn!(token_id = %token.id, %error, "count throttled log records");
        }

        return Err(OtlpError::Throttled {
            message: format!(
                "This token sent more than {} KB of logs this minute",
                state.config.otlp_log_budget / 1024
            ),
            retry_after,
        });
    }
    state.analytics.buffer_log_rows(rows)?;

    Ok(encoding.encode(StatusCode::OK, &ExportLogsServiceResponse::default()))
}
