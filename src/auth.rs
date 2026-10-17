use crate::db::{self, Db, User};
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const COOKIE_NAME: &str = "zo_session";
const COOKIE_MAX_AGE: i64 = 7 * 24 * 3600;
/// Binds a Google sign-in to the browser that started it (login-CSRF protection).
pub const OAUTH_STATE_COOKIE: &str = "zo_oauth_state";
const OAUTH_STATE_PATH: &str = "/api/auth/google";
const OAUTH_STATE_MAX_AGE: i64 = 600;

pub struct AuthState {
    pub db: Db,
    pub jwt_secret: String,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub base_url: String,
    /// `AppConfig::dev_login_enabled`: explicit opt-in on a loopback-only server.
    pub dev_login: bool,
    /// `AGENTRIX_OWNER_EMAILS` (ADR-005): the only accounts that may sign in to this instance.
    pub owner_emails: Vec<String>,
}

impl AuthState {
    /// One instance per person: sign-in is refused for every other account, and for everyone
    /// while the allowlist is empty.
    pub fn is_owner(&self, email: &str) -> bool {
        let email = email.trim();
        !email.is_empty() && self.owner_emails.iter().any(|o| o.eq_ignore_ascii_case(email))
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn err(status: StatusCode, msg: &str) -> impl IntoResponse {
    (
        status,
        Json(ErrorBody {
            error: msg.to_string(),
        }),
    )
}

fn set_session_cookie(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let cookie =
        format!("{COOKIE_NAME}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={COOKIE_MAX_AGE}");
    headers.insert(header::SET_COOKIE, cookie.parse().unwrap());
    headers
}

fn clear_session_cookie() -> HeaderMap {
    let mut headers = HeaderMap::new();
    let cookie = format!("{COOKIE_NAME}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    headers.insert(header::SET_COOKIE, cookie.parse().unwrap());
    headers
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

pub fn create_token(user_id: uuid::Uuid, secret: &str) -> anyhow::Result<String> {
    let exp = chrono::Utc::now() + chrono::Duration::days(7);
    let claims = Claims {
        sub: user_id.to_string(),
        exp: exp.timestamp() as usize,
    };
    Ok(jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    )?)
}

pub fn verify_token(token: &str, secret: &str) -> Option<uuid::Uuid> {
    let data = jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .ok()?;
    data.claims.sub.parse().ok()
}

pub async fn me(State(state): State<Arc<AuthState>>, headers: HeaderMap) -> impl IntoResponse {
    let user_id = match extract_user_id(&headers, &state.jwt_secret) {
        Some(id) => id,
        None => return err(StatusCode::UNAUTHORIZED, "Not authenticated").into_response(),
    };
    match db::find_user_by_id(&state.db, user_id).await.ok().flatten() {
        Some(user) => Json(user).into_response(),
        None => err(StatusCode::NOT_FOUND, "User not found").into_response(),
    }
}

/// The signed-in user: the subject of a valid JWT from the session cookie, else from an
/// `Authorization: Bearer` header. This is the only identity function — `JwtTrustAssigner` grants
/// Known from it and the routes act as the user it returns, so trust and identity cannot disagree
/// (an invalid cookie no longer hides a valid Bearer token, or vice versa).
pub fn extract_user_id(headers: &HeaderMap, secret: &str) -> Option<uuid::Uuid> {
    extract_cookie_token(headers)
        .and_then(|t| verify_token(&t, secret))
        .or_else(|| {
            let bearer = headers
                .get("authorization")?
                .to_str()
                .ok()?
                .strip_prefix("Bearer ")?;
            verify_token(bearer, secret)
        })
}

fn extract_cookie_token(headers: &HeaderMap) -> Option<String> {
    cookie_value(headers, COOKIE_NAME)
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookies = headers.get("cookie")?.to_str().ok()?;
    for part in cookies.split(';') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix(&format!("{name}="))
            && !val.is_empty()
        {
            return Some(val.to_string());
        }
    }
    None
}

fn oauth_state_cookie(value: &str, max_age: i64) -> header::HeaderValue {
    format!("{OAUTH_STATE_COOKIE}={value}; HttpOnly; SameSite=Lax; Path={OAUTH_STATE_PATH}; Max-Age={max_age}")
        .parse()
        .unwrap()
}

/// Length-independent of where the first difference is; the state is a random nonce.
fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Deserialize)]
pub struct OAuthCallback {
    pub code: String,
    pub state: Option<String>,
}

pub async fn google_redirect(State(state): State<Arc<AuthState>>) -> impl IntoResponse {
    let (client_id, _) = match (&state.google_client_id, &state.google_client_secret) {
        (Some(id), Some(s)) => (id.clone(), s.clone()),
        _ => {
            return err(StatusCode::NOT_IMPLEMENTED, "Google OAuth not configured").into_response()
        }
    };
    let redirect_uri = format!("{}/api/auth/google/callback", state.base_url);
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=email%20profile&access_type=offline&state={}",
        client_id,
        urlencoding::encode(&redirect_uri),
        nonce
    );
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, oauth_state_cookie(&nonce, OAUTH_STATE_MAX_AGE));
    (headers, Redirect::temporary(&url)).into_response()
}

pub async fn google_callback(
    State(state): State<Arc<AuthState>>,
    headers: HeaderMap,
    Query(params): Query<OAuthCallback>,
) -> impl IntoResponse {
    let (client_id, client_secret) = match (&state.google_client_id, &state.google_client_secret) {
        (Some(id), Some(s)) => (id.clone(), s.clone()),
        _ => {
            return err(StatusCode::NOT_IMPLEMENTED, "Google OAuth not configured").into_response()
        }
    };
    // The callback must come back to the browser that started the sign-in, so a code minted for
    // someone else's account cannot be planted in this one.
    let expected = cookie_value(&headers, OAUTH_STATE_COOKIE);
    if !matches!((&expected, &params.state), (Some(a), Some(b)) if same_secret(a, b)) {
        let mut headers = HeaderMap::new();
        headers.insert(header::SET_COOKIE, oauth_state_cookie("", 0));
        return (
            StatusCode::BAD_REQUEST,
            headers,
            Json(ErrorBody { error: "Sign-in state did not match — start again".into() }),
        )
            .into_response();
    }
    let redirect_uri = format!("{}/api/auth/google/callback", state.base_url);

    let client = reqwest::Client::new();
    let token_res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", params.code.as_str()),
            ("client_id", &client_id),
            ("client_secret", &client_secret),
            ("redirect_uri", &redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await;

    let token_body: serde_json::Value = match token_res {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => return err(StatusCode::BAD_GATEWAY, &e.to_string()).into_response(),
        },
        Err(e) => return err(StatusCode::BAD_GATEWAY, &e.to_string()).into_response(),
    };

    let access_token = match token_body["access_token"].as_str() {
        Some(t) => t.to_string(),
        None => return err(StatusCode::BAD_GATEWAY, "No access token from Google").into_response(),
    };

    let userinfo: serde_json::Value = match client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(&access_token)
        .send()
        .await
    {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => return err(StatusCode::BAD_GATEWAY, &e.to_string()).into_response(),
        },
        Err(e) => return err(StatusCode::BAD_GATEWAY, &e.to_string()).into_response(),
    };

    let google_id = userinfo["id"].as_str().unwrap_or_default();
    let email = userinfo["email"].as_str().unwrap_or_default();
    let name = userinfo["name"].as_str();
    let avatar = userinfo["picture"].as_str();
    if userinfo["verified_email"].as_bool() != Some(true) || !state.is_owner(email) {
        tracing::warn!("Google sign-in refused: account is not on AGENTRIX_OWNER_EMAILS");
        return err(StatusCode::FORBIDDEN, "This Agentrix OS instance belongs to someone else")
            .into_response();
    }

    let user = match db::find_user_by_provider(&state.db, "google", google_id)
        .await
        .ok()
        .flatten()
    {
        Some(u) => u,
        None => match db::find_user_by_email(&state.db, email)
            .await
            .ok()
            .flatten()
        {
            Some(u) => u,
            None => match db::create_user(
                &state.db,
                email,
                name,
                "google",
                Some(google_id),
                avatar,
            )
            .await
            {
                Ok(u) => u,
                Err(e) => {
                    return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response()
                }
            },
        },
    };

    let jwt = create_token(user.id, &state.jwt_secret).unwrap();
    let mut headers = set_session_cookie(&jwt);
    headers.append(header::SET_COOKIE, oauth_state_cookie("", 0));
    headers.insert(header::LOCATION, "/".parse().unwrap());
    (StatusCode::TEMPORARY_REDIRECT, headers).into_response()
}

pub async fn logout() -> impl IntoResponse {
    (
        clear_session_cookie(),
        Json(serde_json::json!({"ok": true})),
    )
}

#[derive(Serialize)]
pub struct AuthProviders {
    pub google: bool,
    pub dev: bool,
}

pub async fn providers(State(state): State<Arc<AuthState>>) -> Json<AuthProviders> {
    let google = state.google_client_id.is_some() && state.google_client_secret.is_some();
    Json(AuthProviders {
        google,
        dev: state.dev_login && !google,
    })
}

#[derive(Deserialize)]
pub struct DevLoginRequest {
    pub email: Option<String>,
}

/// Local-only sign-in when Google OAuth is not configured (M9 dev validation). Off unless
/// `AGENTRIX_DEV_LOGIN=true` on a loopback server: it trusts whatever email it is given.
pub async fn dev_login(
    State(state): State<Arc<AuthState>>,
    Json(body): Json<DevLoginRequest>,
) -> impl IntoResponse {
    if !state.dev_login {
        return err(
            StatusCode::NOT_FOUND,
            "Dev login is off (set AGENTRIX_DEV_LOGIN=true on a loopback HOST and BASE_URL)",
        )
        .into_response();
    }
    if state.google_client_id.is_some() {
        return err(
            StatusCode::NOT_FOUND,
            "Dev login disabled when Google OAuth is configured",
        )
        .into_response();
    }

    let email = body
        .email
        .filter(|e| !e.trim().is_empty())
        .or_else(|| state.owner_emails.first().cloned())
        .unwrap_or_else(|| "dev@localhost".into());
    if !state.is_owner(&email) {
        return err(StatusCode::FORBIDDEN, "Not an owner of this instance (AGENTRIX_OWNER_EMAILS)")
            .into_response();
    }

    let user = match db::find_user_by_email(&state.db, &email).await.ok().flatten() {
        Some(u) => u,
        None => match db::create_user(
            &state.db,
            &email,
            Some("Dev User"),
            "dev",
            Some(&email),
            None,
        )
        .await
        {
            Ok(u) => u,
            Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
        },
    };

    let jwt = match create_token(user.id, &state.jwt_secret) {
        Ok(t) => t,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    };

    let mut headers = set_session_cookie(&jwt);
    headers.insert(axum::http::header::CONTENT_TYPE, "application/json".parse().unwrap());
    (headers, Json(user)).into_response()
}

#[derive(Serialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub user: Option<User>,
}