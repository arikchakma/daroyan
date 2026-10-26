use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;
use daroyan_core::error::{ApiError, ErrorName};
use serde::Serialize;

#[derive(Serialize)]
struct ErrorBody {
    name: ErrorName,
    status: u16,
    message: String,
}

pub async fn render_errors(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let Some(error) = response.extensions_mut().remove::<ApiError>() else {
        return response;
    };
    let status = error.name.status();
    let body = ErrorBody {
        name: error.name,
        status: status.as_u16(),
        message: error.message,
    };
    let (mut parts, _) = response.into_parts();
    parts.status = status;
    parts
        .headers
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(serde_json::to_vec(&body).unwrap_or_default()))
}
