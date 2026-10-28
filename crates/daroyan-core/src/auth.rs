use std::sync::LazyLock;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::db::Database;
use crate::db::entities::sessions::Model as Session;
use crate::db::entities::users::Model as User;
use crate::db::queries::sessions::NewSession;
use crate::db::queries::users::GoogleUser;
use crate::error::{Error, ErrorName, Result};
use crate::id::{Prefix, new_id};
use crate::token::hash_token;

pub const SESSION_DAYS: i64 = 30;

const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("http client with a fixed timeout builds")
});

struct GoogleClient<'a> {
    id: &'a str,
    secret: &'a str,
    redirect_uri: String,
}

fn read_google_client(config: &Config) -> Result<GoogleClient<'_>> {
    let (Some(id), Some(secret), Some(app_url)) = (
        config.google_client_id.as_deref(),
        config.google_client_secret.as_deref(),
        config.app_url.as_deref(),
    ) else {
        return Err(Error::new(
            ErrorName::ValidationError,
            "Google sign-in is not set up. Set GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET and APP_URL.",
        ));
    };
    let redirect_uri =
        format!("{}/v1/auth/google/callback", app_url.trim_end_matches('/'));
    Ok(GoogleClient { id, secret, redirect_uri })
}

fn create_random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub struct StartedSignIn {
    pub url: String,
    pub state: String,
    pub code_verifier: String,
}

pub fn start_google_sign_in(config: &Config) -> Result<StartedSignIn> {
    let client = read_google_client(config)?;
    let state = create_random_token();
    let code_verifier = create_random_token();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
    let mut url = reqwest::Url::parse(AUTHORIZE_URL).map_err(|_| {
        Error::new(ErrorName::UnknownError, "Could not build the Google URL")
    })?;
    url.query_pairs_mut()
        .append_pair("client_id", client.id)
        .append_pair("redirect_uri", &client.redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", "openid email profile")
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("prompt", "select_account")
        .append_pair("state", &state);
    Ok(StartedSignIn { url: url.to_string(), state, code_verifier })
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct GoogleProfile {
    sub: String,
    email: Option<String>,
    #[serde(default)]
    email_verified: bool,
    name: Option<String>,
    picture: Option<String>,
}

pub struct Callback<'a> {
    pub code: Option<&'a str>,
    pub state: Option<&'a str>,
    pub saved_state: Option<&'a str>,
    pub code_verifier: Option<&'a str>,
}

async fn read_google_profile(
    config: &Config,
    callback: &Callback<'_>,
) -> Result<GoogleProfile> {
    let failed =
        || Error::new(ErrorName::ValidationError, "Google sign-in failed. Try again.");
    let (Some(code), Some(saved_state), Some(code_verifier)) =
        (callback.code, callback.saved_state, callback.code_verifier)
    else {
        return Err(failed());
    };
    if callback.state != Some(saved_state) {
        return Err(failed());
    }
    let client = read_google_client(config)?;
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &client.redirect_uri),
        ("code_verifier", code_verifier),
        ("client_id", client.id),
        ("client_secret", client.secret),
    ];
    let token: TokenResponse = CLIENT
        .post(TOKEN_URL)
        .form(&form)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| failed())?
        .json()
        .await
        .map_err(|_| failed())?;
    CLIENT
        .get(USERINFO_URL)
        .bearer_auth(token.access_token)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| failed())?
        .json()
        .await
        .map_err(|_| failed())
}

pub fn is_email_allowed(allowed: &[String], email: &str) -> bool {
    let email = email.to_lowercase();
    allowed.iter().any(|item| match item.strip_prefix('@') {
        Some(domain) => email.rsplit_once('@').is_some_and(|(_, host)| host == domain),
        None => *item == email,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Admin,
    Member,
}

impl Role {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "admin" => Some(Self::Admin),
            "member" => Some(Self::Member),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    pub role: Role,
    pub is_locked: bool,
}

pub async fn find_access(
    database: &Database,
    config: &Config,
    email: &str,
) -> Result<Option<Access>> {
    if is_email_allowed(&config.allowed_emails, email) {
        return Ok(Some(Access { role: Role::Admin, is_locked: true }));
    }
    let member = database.find_member_by_email(&email.to_lowercase()).await?;
    Ok(member.map(|member| Access {
        role: Role::parse(&member.role).unwrap_or(Role::Member),
        is_locked: false,
    }))
}

pub async fn sign_in_with_google(
    database: &Database,
    config: &Config,
    callback: &Callback<'_>,
) -> Result<User> {
    let profile = read_google_profile(config, callback).await?;
    let email = profile
        .email
        .filter(|_| profile.email_verified)
        .map(|email| email.to_lowercase())
        .ok_or_else(|| {
            Error::new(
                ErrorName::Forbidden,
                "Your Google account does not have a verified email address.",
            )
        })?;
    if find_access(database, config, &email).await?.is_none() {
        return Err(Error::new(
            ErrorName::Forbidden,
            format!("{email} has not been invited. Ask an admin to invite you."),
        ));
    }
    let name = profile.name.unwrap_or_else(|| email.clone());
    let user = GoogleUser {
        id: new_id(Prefix::User),
        google_id: &profile.sub,
        email: &email,
        name: &name,
        avatar_url: profile.picture.as_deref(),
    };
    Ok(database.save_google_user(user).await?)
}

pub struct CreatedSession {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

pub async fn create_session(
    database: &Database,
    user_id: &str,
    user_agent: Option<&str>,
    ip_address: Option<&str>,
) -> Result<CreatedSession> {
    let token = create_random_token();
    let expires_at = Utc::now() + chrono::Duration::days(SESSION_DAYS);
    database
        .create_session(NewSession {
            id: new_id(Prefix::Session),
            user_id,
            token_hash: hash_token(&token),
            user_agent,
            ip_address,
            expires_at,
        })
        .await?;
    Ok(CreatedSession { token, expires_at })
}

pub async fn find_active_session(
    database: &Database,
    token: &str,
) -> Result<Option<(Session, User)>> {
    Ok(database.find_active_session(&hash_token(token)).await?)
}

pub async fn delete_session(database: &Database, token: &str) -> Result<()> {
    Ok(database.delete_session(&hash_token(token)).await?)
}

pub fn get_safe_next_path(next: Option<&str>) -> Option<&str> {
    next.filter(|next| {
        next.starts_with('/')
            && !next.starts_with("//")
            && !next.contains('\\')
            && !next.chars().any(|c| c.is_ascii_control() || c.is_whitespace())
    })
}

pub fn get_app_url(app_url: Option<&str>, path: &str, error: Option<&str>) -> String {
    let base = app_url.unwrap_or_default().trim_end_matches('/');
    let Ok(mut url) = reqwest::Url::parse(&format!("{base}{path}")) else {
        return path.to_owned();
    };
    if let Some(error) = error {
        url.query_pairs_mut().append_pair("error", error);
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::{get_safe_next_path, is_email_allowed};

    #[test]
    fn allows_listed_emails_and_domains() {
        let allowed = vec!["arik@roadmap.sh".to_owned(), "@daroyan.com".to_owned()];
        assert!(is_email_allowed(&allowed, "Arik@roadmap.sh"));
        assert!(is_email_allowed(&allowed, "ops@daroyan.com"));
        assert!(!is_email_allowed(&allowed, "ops@evildaroyan.com"));
        assert!(!is_email_allowed(&allowed, "someone@roadmap.sh"));
        assert!(!is_email_allowed(&[], "arik@roadmap.sh"));
    }

    #[test]
    fn keeps_only_app_paths() {
        assert_eq!(get_safe_next_path(Some("/servers")), Some("/servers"));
        for path in [
            "//evil.example",
            "https://evil.example",
            "servers",
            "/\\evil",
            "/\t/evil.com",
            "/\n/evil.com",
            "/ /evil.com",
        ] {
            assert_eq!(get_safe_next_path(Some(path)), None);
        }
    }
}
