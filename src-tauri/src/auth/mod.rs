// Authentication module for PDF.dk Flow
// Login against the website's API, token storage, session validation.

use crate::api::api_base;
use crate::config::{self, AuthConfig};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("Token expired")]
    TokenExpired,
    #[error("PRO subscription required")]
    ProRequired,
    #[error("Keyring error: {0}")]
    Keyring(String),
    #[error("Server error: {0}")]
    ServerError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthState {
    pub is_authenticated: bool,
    pub is_pro: bool,
    pub user: Option<User>,
    pub token: Option<String>,
    pub plan: Option<String>,
    pub jobs_limit: Option<i32>,
    pub jobs_used: Option<i32>,
    pub jobs_remaining: Option<i32>,
    pub max_file_size_mb: Option<i32>,
    pub is_unlimited: Option<bool>,
}

impl AuthState {
    /// Plan names that count as paid for the app's badge
    pub fn apply_plan(&mut self, plan: &str) {
        self.plan = Some(plan.to_string());
        self.is_pro = matches!(plan.to_lowercase().as_str(), "pro" | "team" | "enterprise" | "superadmin");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: i64,
    pub email: String,
    pub name: Option<String>,
    #[serde(default)]
    pub is_superadmin: bool,
    pub role: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

// API response shapes (Laravel backend)
#[derive(Debug, Deserialize)]
struct LoginResponse {
    success: bool,
    message: Option<String>,
    data: Option<LoginData>,
}

#[derive(Debug, Deserialize)]
struct LoginData {
    user: ApiUser,
    token: String,
}

#[derive(Debug, Deserialize)]
struct ApiUser {
    id: i64,
    email: String,
    name: Option<String>,
    #[serde(default)]
    is_superadmin: bool,
    role: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    subscription_plan: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserResponse {
    success: bool,
    data: Option<UserData>,
}

#[derive(Debug, Deserialize)]
struct UserData {
    user: ApiUser,
}

fn user_from(api: ApiUser) -> (User, Option<String>) {
    let plan = api.subscription_plan.clone();
    (
        User {
            id: api.id,
            email: api.email,
            name: api.name,
            is_superadmin: api.is_superadmin || api.role.as_deref() == Some("superadmin"),
            role: api.role,
            avatar_url: api.avatar_url,
        },
        plan,
    )
}

/// POST /api/auth/login
pub async fn login(email: &str, password: &str) -> Result<AuthState, AuthError> {
    let client = crate::api::http_client();
    let response = client
        .post(format!("{}/auth/login", api_base()))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;
    tracing::debug!("Login response status: {}", status);

    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AuthError::InvalidCredentials);
    }
    if !status.is_success() {
        // the API explains 403s (e.g. "your organisation requires Microsoft login")
        let msg = serde_json::from_str::<LoginResponse>(&body).ok().and_then(|r| r.message);
        return Err(AuthError::ServerError(msg.unwrap_or_else(|| format!("Server returned {}", status))));
    }

    let login_response: LoginResponse = serde_json::from_str(&body)
        .map_err(|e| AuthError::ServerError(format!("Failed to parse response: {}", e)))?;
    if !login_response.success {
        return Err(AuthError::InvalidCredentials);
    }
    let data = login_response.data.ok_or(AuthError::InvalidCredentials)?;
    let (user, plan) = user_from(data.user);

    let mut state = AuthState {
        is_authenticated: true,
        is_pro: user.is_superadmin,
        user: Some(user),
        token: Some(data.token),
        ..Default::default()
    };
    if let Some(p) = plan {
        state.apply_plan(&p);
    }
    Ok(state)
}

/// GET /api/auth/me — validate a saved token and fetch the user.
/// (v0.2 called /api/user, which no longer exists: every saved session
/// looked expired.)
pub async fn validate_token(token: &str) -> Result<AuthState, AuthError> {
    let client = crate::api::http_client();
    let response = client
        .get(format!("{}/auth/me", api_base()))
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AuthError::TokenExpired);
    }
    if !response.status().is_success() {
        return Err(AuthError::ServerError(format!("Server returned {}", response.status())));
    }

    let body = response.text().await?;
    let user_response: UserResponse = serde_json::from_str(&body)
        .map_err(|e| AuthError::ServerError(format!("Failed to parse response: {}", e)))?;
    if !user_response.success {
        return Err(AuthError::TokenExpired);
    }
    let data = user_response.data.ok_or(AuthError::TokenExpired)?;
    let (user, plan) = user_from(data.user);

    let mut state = AuthState {
        is_authenticated: true,
        is_pro: user.is_superadmin,
        user: Some(user),
        token: Some(token.to_string()),
        ..Default::default()
    };
    if let Some(p) = plan {
        state.apply_plan(&p);
    }
    Ok(state)
}

/// POST /api/auth/logout — revoke the desktop token on the server so it stops working
/// everywhere, not only on this machine (0.3.4). Best effort: offline logout still succeeds.
pub async fn revoke_token(token: &str) -> Result<(), AuthError> {
    let client = crate::api::http_client();
    let response = client
        .post(format!("{}/auth/logout", api_base()))
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .send()
        .await?;
    if response.status().is_success() || response.status() == reqwest::StatusCode::UNAUTHORIZED {
        Ok(())
    } else {
        Err(AuthError::ServerError(format!("Server returned {}", response.status())))
    }
}

pub fn save_token(token: &str) -> Result<(), AuthError> {
    let mut cfg = config::load_config().map_err(|e| AuthError::Keyring(e.to_string()))?;
    if cfg.auth.is_none() {
        cfg.auth = Some(AuthConfig::default());
    }
    if let Some(ref mut auth) = cfg.auth {
        auth.token = Some(token.to_string());
    }
    config::save_config(&cfg).map_err(|e| AuthError::Keyring(e.to_string()))?;
    Ok(())
}

pub fn load_token() -> Result<String, AuthError> {
    let cfg = config::load_config().map_err(|e| AuthError::Keyring(e.to_string()))?;
    cfg.auth
        .and_then(|a| a.token)
        .ok_or_else(|| AuthError::Keyring("No saved token".to_string()))
}

pub fn clear_token() -> Result<(), AuthError> {
    let mut cfg = config::load_config().map_err(|e| AuthError::Keyring(e.to_string()))?;
    if let Some(ref mut auth) = cfg.auth {
        auth.token = None;
    }
    config::save_config(&cfg).map_err(|e| AuthError::Keyring(e.to_string()))?;
    Ok(())
}

/// "Remember me" keeps the e-mail only. Up to 0.3.3 the password went into the config file in
/// plain text; 0.3.4 never writes it and wipes any stored one on load (see `load_credentials`).
pub fn save_credentials(email: &str, _password: &str) -> Result<(), AuthError> {
    let mut cfg = config::load_config().map_err(|e| AuthError::Keyring(e.to_string()))?;
    if cfg.auth.is_none() {
        cfg.auth = Some(AuthConfig::default());
    }
    if let Some(ref mut auth) = cfg.auth {
        auth.email = Some(email.to_string());
        auth.password = None;
    }
    config::save_config(&cfg).map_err(|e| AuthError::Keyring(e.to_string()))?;
    Ok(())
}

pub fn load_credentials() -> Result<(String, String), AuthError> {
    let mut cfg = config::load_config().map_err(|e| AuthError::Keyring(e.to_string()))?;
    // one-time cleanup of a password saved by 0.3.3 or older
    if cfg.auth.as_ref().and_then(|a| a.password.as_ref()).is_some() {
        if let Some(ref mut auth) = cfg.auth {
            auth.password = None;
        }
        let _ = config::save_config(&cfg);
    }
    let auth = cfg.auth.ok_or_else(|| AuthError::Keyring("No saved credentials".to_string()))?;
    let email = auth.email.ok_or_else(|| AuthError::Keyring("No saved email".to_string()))?;
    Ok((email, String::new()))
}

pub fn clear_credentials() -> Result<(), AuthError> {
    let mut cfg = config::load_config().map_err(|e| AuthError::Keyring(e.to_string()))?;
    if let Some(ref mut auth) = cfg.auth {
        auth.email = None;
        auth.password = None;
    }
    config::save_config(&cfg).map_err(|e| AuthError::Keyring(e.to_string()))?;
    Ok(())
}
