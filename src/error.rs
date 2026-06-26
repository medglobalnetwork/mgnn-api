use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Unified API error type for all MGN handlers.
#[derive(Debug)]
#[allow(dead_code)]
pub enum ApiError {
    /// 400 Bad Request — invalid input, validation failure.
    BadRequest(String),
    /// 401 Unauthorized — missing or invalid auth.
    Unauthorized(String),
    /// 403 Forbidden — insufficient permissions.
    Forbidden(String),
    /// 404 Not Found — resource does not exist.
    NotFound(String),
    /// 409 Conflict — duplicate, already exists.
    Conflict(String),
    /// 422 Unprocessable — semantic validation failure.
    Unprocessable(String),
    /// 429 Too Many Requests — rate limited.
    RateLimited(String),
    /// 500 Internal — database failure, unexpected error.
    Internal(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::BadRequest(m) => write!(f, "Bad request: {m}"),
            ApiError::Unauthorized(m) => write!(f, "Unauthorized: {m}"),
            ApiError::Forbidden(m) => write!(f, "Forbidden: {m}"),
            ApiError::NotFound(m) => write!(f, "Not found: {m}"),
            ApiError::Conflict(m) => write!(f, "Conflict: {m}"),
            ApiError::Unprocessable(m) => write!(f, "Unprocessable: {m}"),
            ApiError::RateLimited(m) => write!(f, "Rate limited: {m}"),
            ApiError::Internal(m) => write!(f, "Internal error: {m}"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ApiError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            ApiError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
            ApiError::Unprocessable(m) => (StatusCode::UNPROCESSABLE_ENTITY, m.clone()),
            ApiError::RateLimited(m) => (StatusCode::TOO_MANY_REQUESTS, m.clone()),
            ApiError::Internal(m) => {
                tracing::error!("Internal error: {m}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".into())
            }
        };
        let body = Json(json!({
            "success": false,
            "error": message,
            "status": status.as_u16(),
        }));
        (status, body).into_response()
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError::Internal(format!("HTTP client error: {e}"))
    }
}

/// Convenience alias used by all handlers.
pub type ApiResult = Result<axum::response::Response, ApiError>;

/// Shorthand to return a success JSON response.
pub fn ok_json(data: serde_json::Value) -> ApiResult {
    Ok(Json(json!({ "success": true, "data": data })).into_response())
}

/// Shorthand to return a success JSON response with a message.
pub fn ok_message(msg: &str) -> ApiResult {
    Ok(Json(json!({ "success": true, "message": msg })).into_response())
}
