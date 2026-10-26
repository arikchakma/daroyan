use std::collections::HashSet;

use axum::Json;
use axum::extract::State;
use daroyan_core::auth::is_email_allowed;
use daroyan_core::error::Result;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::CurrentUser;
use crate::views::user::UserView;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_users))
}

#[utoipa::path(get, path = "/v1/users", responses((status = 200, body = Vec<UserView>)))]
async fn list_users(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<UserView>>> {
    let members: HashSet<String> = state
        .database
        .list_members()
        .await?
        .into_iter()
        .map(|member| member.email)
        .collect();
    let allowed = &state.config.allowed_emails;
    Ok(Json(
        state
            .database
            .list_users()
            .await?
            .into_iter()
            .filter(|user| {
                members.contains(&user.email) || is_email_allowed(allowed, &user.email)
            })
            .map(|user| UserView {
                id: user.id,
                email: user.email,
                name: user.name,
                avatar_url: user.avatar_url,
            })
            .collect(),
    ))
}
