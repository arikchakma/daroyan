use std::time::Duration;

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use sea_orm::DbErr;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorName {
    ValidationError,
    NotFound,
    Unauthorized,
    Forbidden,
    Conflict,
    RateLimitExceeded,
    UnknownError,
}

impl ErrorName {
    pub fn status(self) -> StatusCode {
        match self {
            Self::ValidationError => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::UnknownError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ApiError {
    pub name: ErrorName,
    pub message: String,
    pub retry_after: Option<Duration>,
}

impl ApiError {
    pub fn new(name: ErrorName, message: impl Into<String>) -> Self {
        Self { name, message: message.into(), retry_after: None }
    }

    pub fn unknown() -> Self {
        Self::new(
            ErrorName::UnknownError,
            "Something went wrong. Please try again later.",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = self.name.status().into_response();
        if let Some(retry_after) = self.retry_after {
            let seconds = retry_after.as_secs_f64().ceil().max(1.0) as u64;
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response.extensions_mut().insert(self);
        response
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
pub enum Error {
    Api(ApiError),
    Database(DbErr),
    Internal(anyhow::Error),
}

impl Error {
    pub fn new(name: ErrorName, message: impl Into<String>) -> Self {
        Self::Api(ApiError::new(name, message))
    }

    pub fn with_retry_after(self, retry_after: Duration) -> Self {
        match self {
            Self::Api(error) => {
                Self::Api(ApiError { retry_after: Some(retry_after), ..error })
            }
            error => error,
        }
    }

    pub fn name(&self) -> ErrorName {
        match self {
            Self::Api(error) => error.name,
            Self::Database(_) | Self::Internal(_) => ErrorName::UnknownError,
        }
    }

    pub fn into_api(self) -> ApiError {
        match self {
            Self::Api(error) => error,
            Self::Database(error) => {
                tracing::error!(error = %format!("{:#}", anyhow::Error::new(error)), "database error");
                ApiError::unknown()
            }
            Self::Internal(error) => {
                tracing::error!(error = %format!("{error:#}"), "internal error");
                ApiError::unknown()
            }
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(error) => write!(f, "{:?}: {}", error.name, error.message),
            Self::Database(error) => write!(f, "{error}"),
            Self::Internal(error) => write!(f, "{error:#}"),
        }
    }
}

impl From<ApiError> for Error {
    fn from(error: ApiError) -> Self {
        Self::Api(error)
    }
}

impl From<DbErr> for Error {
    fn from(error: DbErr) -> Self {
        Self::Database(error)
    }
}

impl From<anyhow::Error> for Error {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        self.into_api().into_response()
    }
}

macro_rules! internal_from {
    ($($source:ty),* $(,)?) => {
        $(
            impl From<$source> for Error {
                fn from(error: $source) -> Self {
                    Self::Internal(error.into())
                }
            }
        )*
    };
}

internal_from!(duckdb::Error, serde_json::Error, tokio::task::JoinError);
