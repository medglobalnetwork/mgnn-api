use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use crate::error::ApiError;
use crate::state::SharedState;

/// JWT claims embedded in every access token.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Subject — the mgn_users.id UUID.
    pub sub: String,
    /// Profile ID — the profiles.id UUID.
    pub profile_id: String,
    /// Role name (e.g., "professional", "admin").
    pub role: String,
    /// Issued-at (Unix seconds).
    pub iat: i64,
    /// Expiration (Unix seconds).
    pub exp: i64,
    /// Issuer.
    pub iss: String,
    /// Session ID for revocation tracking.
    pub sid: String,
}

/// Create a signed JWT access token.
pub fn create_access_token(
    user_id: &str,
    profile_id: &str,
    role: &str,
    session_id: &str,
    config: &crate::config::Config,
) -> Result<String, ApiError> {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        profile_id: profile_id.to_string(),
        role: role.to_string(),
        iat: now,
        exp: now + config.access_token_ttl_secs,
        iss: config.jwt_issuer.clone(),
        sid: session_id.to_string(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(format!("JWT encode error: {e}")))
}

/// Create a signed JWT refresh token (longer TTL, fewer claims).
#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshClaims {
    pub sub: String,
    pub sid: String,
    pub iat: i64,
    pub exp: i64,
    pub iss: String,
}

pub fn create_refresh_token(
    user_id: &str,
    session_id: &str,
    config: &crate::config::Config,
) -> Result<String, ApiError> {
    let now = chrono::Utc::now().timestamp();
    let claims = RefreshClaims {
        sub: user_id.to_string(),
        sid: session_id.to_string(),
        iat: now,
        exp: now + config.refresh_token_ttl_secs,
        iss: config.jwt_issuer.clone(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(format!("JWT encode error: {e}")))
}

/// Axum extractor: requires a valid Bearer token, returns Claims.
///
/// Usage: `async fn handler(claims: AuthUser) -> ...`
pub struct AuthUser {
    pub claims: Claims,
}

#[axum::async_trait]
impl FromRequestParts<SharedState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &SharedState) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized("Missing Authorization header".into()))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::Unauthorized("Invalid Authorization header format".into()))?;

        let claims = decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map(|data| data.claims)
        .map_err(|e| ApiError::Unauthorized(format!("Invalid token: {e}")))?;

        Ok(AuthUser { claims })
    }
}

/// Optional auth extractor — returns None if no token / invalid token.
#[allow(dead_code)]
pub struct OptionalAuth {
    pub claims: Option<Claims>,
}

#[axum::async_trait]
impl FromRequestParts<SharedState> for OptionalAuth {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &SharedState) -> Result<Self, Self::Rejection> {
        let claims = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|auth| auth.strip_prefix("Bearer "))
            .and_then(|token| {
                decode::<Claims>(
                    token,
                    &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
                    &Validation::default(),
                )
                .ok()
                .map(|data| data.claims)
            });

        Ok(OptionalAuth { claims })
    }
}
