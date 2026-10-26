use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use daroyan_core::auth::{Role, is_email_allowed};
use daroyan_core::db::entities::members::Model as Member;
use daroyan_core::db::ids::MemberId;
use daroyan_core::db::queries::members::NewMember;
use daroyan_core::error::{Error, ErrorName, Result};
use daroyan_core::id::{Prefix, new_id};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::extract::{AdminUser, CurrentUser};
use crate::views::user::UserView;

const MAX_EMAIL_LENGTH: usize = 254;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_members, invite_member))
        .routes(routes!(update_member, remove_member))
}

#[derive(Serialize, ToSchema)]
#[schema(as = Member)]
struct MemberView {
    id: Option<String>,
    email: String,
    #[schema(example = "member")]
    role: String,
    is_locked: bool,
    user: Option<UserView>,
    inviter_id: Option<String>,
    #[serde(with = "daroyan_core::time::option")]
    invited_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, ToSchema)]
struct InviteMemberBody {
    email: String,
    #[schema(example = "member")]
    role: String,
}

#[derive(Deserialize, ToSchema)]
struct UpdateMemberBody {
    #[schema(example = "admin")]
    role: String,
}

fn read_role(role: &str) -> Result<Role> {
    Role::parse(role)
        .ok_or_else(|| Error::new(ErrorName::ValidationError, "Pick admin or member"))
}

fn invited_view(member: Member) -> MemberView {
    MemberView {
        id: Some(member.id),
        email: member.email,
        role: member.role,
        is_locked: false,
        user: None,
        inviter_id: member.inviter_id,
        invited_at: Some(member.created_at),
    }
}

fn acting_admin(admin: &CurrentUser) -> Option<&str> {
    (!admin.access.is_locked).then_some(admin.user.email.as_str())
}

fn no_longer_admin() -> Error {
    Error::new(ErrorName::Forbidden, "You are no longer an admin")
}

async fn find_changeable(
    state: &AppState,
    admin: &CurrentUser,
    member_id: &MemberId,
) -> Result<Member> {
    let member = state
        .database
        .find_member(member_id)
        .await?
        .ok_or_else(|| Error::new(ErrorName::NotFound, "Member not found"))?;
    if member.email == admin.user.email {
        return Err(Error::new(
            ErrorName::Forbidden,
            "Ask another admin to change your own access",
        ));
    }
    if is_email_allowed(&state.config.allowed_emails, &member.email) {
        return Err(Error::new(
            ErrorName::Forbidden,
            "This person is an admin through ALLOWED_EMAILS",
        ));
    }
    Ok(member)
}

#[utoipa::path(get, path = "/v1/members", responses((status = 200, body = Vec<MemberView>)))]
async fn list_members(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<MemberView>>> {
    let allowed = &state.config.allowed_emails;
    let mut members: HashMap<String, Member> = state
        .database
        .list_members()
        .await?
        .into_iter()
        .map(|member| (member.email.clone(), member))
        .collect();
    let mut views = Vec::new();
    for user in state.database.list_users().await? {
        let is_locked = is_email_allowed(allowed, &user.email);
        let member = members.remove(&user.email);
        if !is_locked && member.is_none() {
            continue;
        }
        let role = if is_locked {
            Role::Admin.as_str().to_string()
        } else {
            member.as_ref().map(|member| member.role.clone()).unwrap_or_default()
        };
        views.push(MemberView {
            id: member.as_ref().map(|member| member.id.clone()),
            email: user.email.clone(),
            role,
            is_locked,
            inviter_id: member.as_ref().and_then(|member| member.inviter_id.clone()),
            invited_at: member.map(|member| member.created_at),
            user: Some(UserView {
                id: user.id,
                email: user.email,
                name: user.name,
                avatar_url: user.avatar_url,
            }),
        });
    }
    let mut invited: Vec<Member> = members.into_values().collect();
    invited.sort_by_key(|member| member.created_at);
    views.extend(invited.into_iter().map(invited_view));
    Ok(Json(views))
}

#[utoipa::path(
    post,
    path = "/v1/members",
    request_body = InviteMemberBody,
    responses((status = 201, body = MemberView))
)]
async fn invite_member(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(body): Json<InviteMemberBody>,
) -> Result<(StatusCode, Json<MemberView>)> {
    admin.require_session()?;
    let email = body.email.trim().to_lowercase();
    let is_email = email.len() <= MAX_EMAIL_LENGTH
        && email.split_once('@').is_some_and(|(name, host)| {
            !name.is_empty() && host.contains('.') && !host.contains('@')
        })
        && !email.contains(char::is_whitespace);
    if !is_email {
        return Err(Error::new(ErrorName::ValidationError, "Enter an email address"));
    }
    let role = read_role(&body.role)?;
    if is_email_allowed(&state.config.allowed_emails, &email)
        || state.database.find_member_by_email(&email).await?.is_some()
    {
        return Err(Error::new(
            ErrorName::Conflict,
            format!("{email} can already sign in"),
        ));
    }
    let member = state
        .database
        .create_member(NewMember {
            id: new_id(Prefix::Member),
            email,
            role: role.as_str().to_string(),
            inviter_id: admin.user.id,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(invited_view(member))))
}

#[utoipa::path(
    patch,
    path = "/v1/members/{member_id}",
    params(("member_id" = String, Path)),
    request_body = UpdateMemberBody,
    responses((status = 204))
)]
async fn update_member(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(member_id): Path<MemberId>,
    Json(body): Json<UpdateMemberBody>,
) -> Result<StatusCode> {
    admin.require_session()?;
    let role = read_role(&body.role)?;
    find_changeable(&state, &admin, &member_id).await?;
    if !state
        .database
        .update_member_role(&member_id, role.as_str(), acting_admin(&admin))
        .await?
    {
        return Err(no_longer_admin());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/v1/members/{member_id}",
    params(("member_id" = String, Path)),
    responses((status = 204))
)]
async fn remove_member(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(member_id): Path<MemberId>,
) -> Result<StatusCode> {
    admin.require_session()?;
    let member = find_changeable(&state, &admin, &member_id).await?;
    if !state.database.delete_member(&member, acting_admin(&admin)).await? {
        return Err(no_longer_admin());
    }
    Ok(StatusCode::NO_CONTENT)
}
