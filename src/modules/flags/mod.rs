use axum::{
    Router,
    routing::{get, post},
    Json,
    extract::{State, Path},
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json};
use crate::extractors::AuthUser;

// ──────────────────────────────────────────────
// Feature Flags Engine  (Client-facing)
// Public flag evaluation for client apps.
// Admin CRUD lives in admin/mod.rs.
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/", get(list_public_flags))
        .route("/evaluate", post(evaluate_all))
        .route("/:name", post(evaluate_single))
        .route("/user-flags", get(get_user_flags))
}

#[derive(Deserialize)]
pub struct EvaluateAllPayload {
    pub user_id: Option<String>,
    pub role: Option<String>,
    #[allow(dead_code)]
    pub context: Option<Value>,
}

#[derive(Deserialize)]
pub struct EvaluateSinglePayload {
    pub user_id: Option<String>,
    pub role: Option<String>,
}

/// Evaluate if a user matches a flag's targeting rules.
fn evaluate_flag_targeting(flag: &Value, user_id: Option<&str>, role: Option<&str>) -> bool {
    let enabled = flag.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    if !enabled { return false; }

    let rollout = flag.get("rollout_percentage").and_then(|v| v.as_f64()).unwrap_or(100.0);
    if rollout >= 100.0 { return true; }
    if rollout <= 0.0 { return false; }

    // Check role targeting
    if let Some(allowed_roles) = flag.get("allowed_roles").and_then(|v| v.as_array()) {
        if let Some(user_role) = role {
            let role_allowed = allowed_roles.iter()
                .any(|r| r.as_str() == Some(user_role));
            if !role_allowed { return false; }
        }
    }

    // Check user targeting
    if let Some(allowed_users) = flag.get("allowed_users").and_then(|v| v.as_array()) {
        if let Some(uid) = user_id {
            let user_allowed = allowed_users.iter()
                .any(|u| u.as_str() == Some(uid));
            if !user_allowed { return false; }
        }
    }

    // Rollout-based evaluation
    if let Some(uid) = user_id {
        let hash = uid.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        ((hash % 100) as f64) < rollout
    } else {
        rollout >= 50.0
    }
}

async fn list_public_flags(
    State(state): State<SharedState>,
) -> ApiResult {
    let url = format!(
        "{}/rest/v1/feature_flags?enabled=eq.true&select=name,description,rollout_percentage",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let flags: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "flags": flags, "total": flags.len() }))
}

async fn evaluate_all(
    State(state): State<SharedState>,
    Json(payload): Json<EvaluateAllPayload>,
) -> ApiResult {
    let url = format!("{}/rest/v1/feature_flags?select=*", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let flags: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let mut evaluated = serde_json::Map::new();
    for flag in &flags {
        let name = flag.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let active = evaluate_flag_targeting(
            flag,
            payload.user_id.as_deref(),
            payload.role.as_deref(),
        );
        evaluated.insert(name.to_string(), json!({
            "enabled": active,
            "rollout_percentage": flag.get("rollout_percentage").and_then(|v| v.as_f64()).unwrap_or(0.0),
        }));
    }

    ok_json(json!({ "flags": evaluated }))
}

async fn evaluate_single(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Json(payload): Json<EvaluateSinglePayload>,
) -> ApiResult {
    let url = format!("{}/rest/v1/feature_flags?name=eq.{name}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let flags: Vec<Value> = res.json().await.unwrap_or_default();
    let flag = flags.first().ok_or(ApiError::NotFound("Feature flag not found".into()))?;

    let active = evaluate_flag_targeting(
        flag,
        payload.user_id.as_deref(),
        payload.role.as_deref(),
    );

    ok_json(json!({
        "name": name,
        "enabled": active,
        "rollout_percentage": flag.get("rollout_percentage").and_then(|v| v.as_f64()).unwrap_or(0.0),
    }))
}

async fn get_user_flags(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let role = &auth.claims.role;

    let url = format!("{}/rest/v1/feature_flags?select=*", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let flags: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let mut user_flags = serde_json::Map::new();
    for flag in &flags {
        let name = flag.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let active = evaluate_flag_targeting(flag, Some(uid), Some(role));
        user_flags.insert(name.to_string(), json!(active));
    }

    ok_json(json!({ "flags": user_flags }))
}
