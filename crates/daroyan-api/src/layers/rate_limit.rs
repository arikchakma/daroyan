use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::AppState;
use crate::layers::client_ip::ClientIp;

pub async fn limit_globally(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if !path.starts_with("/v1/") && !path.starts_with("/p/") {
        return next.run(request).await;
    }
    match state.limits.check_global(ip) {
        Ok(()) => next.run(request).await,
        Err(error) => error.into_response(),
    }
}
