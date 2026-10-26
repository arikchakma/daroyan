use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{AppendHeaders, IntoResponse, Response};
use daroyan_core::auth::{
    Callback, create_session, delete_session, get_app_url, get_safe_next_path,
    sign_in_with_google, start_google_sign_in,
};
use daroyan_core::error::Result;
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::cookies::{
    NEXT_COOKIE, SESSION_COOKIE, SIGNED_IN_COOKIE, STATE_COOKIE, VERIFIER_COOKIE,
    clear_cookie, create_session_cookies, create_short_cookie, read_cookie,
};
use crate::extract::{CurrentUser, cross_site_error, is_cross_site};
use crate::layers::client_ip::ClientIp;
use crate::views::user::{MeView, StartedSignInView, UserView};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(start_sign_in))
        .routes(routes!(finish_sign_in))
        .routes(routes!(get_me))
        .routes(routes!(sign_out))
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct StartQuery {
    next: Option<String>,
}

#[utoipa::path(
    post,
    path = "/v1/auth/google",
    params(StartQuery),
    responses((status = 200, body = StartedSignInView))
)]
async fn start_sign_in(
    State(state): State<AppState>,
    Query(query): Query<StartQuery>,
) -> Result<impl IntoResponse> {
    let started = start_google_sign_in(&state.config)?;
    let config = &state.config;
    let next = get_safe_next_path(query.next.as_deref());
    let cookies = [
        create_short_cookie(config, STATE_COOKIE, &started.state),
        create_short_cookie(config, VERIFIER_COOKIE, &started.code_verifier),
        next.map_or_else(
            || clear_cookie(NEXT_COOKIE),
            |next| create_short_cookie(config, NEXT_COOKIE, next),
        ),
    ];
    Ok((
        AppendHeaders(cookies.map(|cookie| (header::SET_COOKIE, cookie))),
        Json(StartedSignInView { authorization_url: started.url }),
    ))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

async fn complete_sign_in(
    state: &AppState,
    headers: &HeaderMap,
    ip: ClientIp,
    query: &CallbackQuery,
) -> Result<[String; 2]> {
    let saved_state = read_cookie(headers, STATE_COOKIE);
    let code_verifier = read_cookie(headers, VERIFIER_COOKIE);
    let callback = Callback {
        code: query.code.as_deref(),
        state: query.state.as_deref(),
        saved_state: saved_state.as_deref(),
        code_verifier: code_verifier.as_deref(),
    };
    let user = sign_in_with_google(&state.database, &state.config, &callback).await?;
    let user_agent =
        headers.get(header::USER_AGENT).and_then(|value| value.to_str().ok());
    let ip_address = ip.0.to_string();
    let session =
        create_session(&state.database, &user.id, user_agent, Some(&ip_address)).await?;
    Ok(create_session_cookies(&state.config, &session.token, session.expires_at))
}

#[utoipa::path(get, path = "/v1/auth/google/callback", responses((status = 302)))]
async fn finish_sign_in(
    State(state): State<AppState>,
    ip: ClientIp,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let saved_next = read_cookie(&headers, NEXT_COOKIE);
    let next = get_safe_next_path(saved_next.as_deref()).unwrap_or("/");
    let mut cookies: Vec<String> = [STATE_COOKIE, VERIFIER_COOKIE, NEXT_COOKIE]
        .map(clear_cookie)
        .to_vec();
    let location = match complete_sign_in(&state, &headers, ip, &query).await {
        Ok(session_cookies) => {
            cookies.extend(session_cookies);
            get_app_url(state.config.app_url.as_deref(), next, None)
        }
        Err(error) => {
            let error = error.into_api();
            if error.name.status().is_server_error() {
                tracing::error!(?error, "google sign-in failed");
            }
            get_app_url(state.config.app_url.as_deref(), "/login", Some(&error.message))
        }
    };
    let headers = cookies
        .into_iter()
        .map(|cookie| (header::SET_COOKIE, cookie))
        .chain([(header::LOCATION, location)]);
    (StatusCode::FOUND, AppendHeaders(headers.collect::<Vec<_>>())).into_response()
}

#[utoipa::path(get, path = "/v1/auth/me", responses((status = 200, body = MeView)))]
async fn get_me(current: CurrentUser) -> Json<MeView> {
    let user = current.user;
    Json(MeView {
        user: UserView {
            id: user.id,
            email: user.email,
            name: user.name,
            avatar_url: user.avatar_url,
        },
        role: current.access.role.as_str().to_string(),
    })
}

#[utoipa::path(post, path = "/v1/auth/logout", responses((status = 204)))]
async fn sign_out(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse> {
    if is_cross_site(&headers) {
        return Err(cross_site_error());
    }
    if let Some(token) = read_cookie(&headers, SESSION_COOKIE) {
        delete_session(&state.database, &token).await?;
    }
    let cookies = [SESSION_COOKIE, SIGNED_IN_COOKIE].map(clear_cookie);
    Ok((
        StatusCode::NO_CONTENT,
        AppendHeaders(cookies.map(|cookie| (header::SET_COOKIE, cookie))),
    ))
}
