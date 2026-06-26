use axum::{
    Router,
    routing::{get, post},
    Json,
    extract::{State, Path, Query},
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json};

// ──────────────────────────────────────────────
// Audit Engine  (Screens: audit-logs)
// System-wide audit logging: log actions,
// query logs, export
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/logs", get(get_logs))
        .route("/logs/:id", get(get_log_detail))
        .route("/logs/export", post(export_logs))
}

#[derive(Deserialize)]
pub struct LogQuery {
    pub actor_id: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct ExportAuditPayload {
    pub from: Option<String>,
    pub to: Option<String>,
    pub format: Option<String>,
}

/// Internal function to log actions — backward-compatible 4-arg signature.
/// Logs to tracing (console). Modules that need DB audit can call log_action_db.
pub async fn log_action(actor_id: &str, action: &str, target_type: &str, target_id: &str) {
    tracing::info!("Audit: {actor_id} performed {action} on {target_type} ({target_id})");
}

/// Log an action to the DB audit_logs table (requires SharedState).
#[allow(dead_code)]
pub async fn log_action_db(state: &SharedState, actor_id: &str, action: &str, target_type: &str, target_id: &str) {
    let body = json!({
        "actor_id": actor_id,
        "action": action,
        "target_type": target_type,
        "target_id": target_id,
        "created_at": chrono::Utc::now().to_rfc3339()
    });
    let _ = state.http.post(format!("{}/rest/v1/audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await;
    tracing::info!("Audit (DB): {actor_id} performed {action} on {target_type} ({target_id})");
}

async fn get_logs(
    State(state): State<SharedState>,
    Query(query): Query<LogQuery>,
) -> ApiResult {
    let mut url = format!("{}/rest/v1/audit_logs?order=created_at.desc", state.rest_url());

    if let Some(ref actor_id) = query.actor_id {
        url.push_str(&format!("&actor_id=eq.{actor_id}"));
    }
    if let Some(ref action) = query.action {
        url.push_str(&format!("&action=eq.{action}"));
    }
    if let Some(ref resource_type) = query.resource_type {
        url.push_str(&format!("&target_type=eq.{resource_type}"));
    }
    if let Some(ref from) = query.from {
        url.push_str(&format!("&created_at=gx.{from}"));
    }
    if let Some(ref to) = query.to {
        url.push_str(&format!("&created_at=lx.{to}"));
    }

    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    url.push_str(&format!("&limit={limit}&offset={offset}"));

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let logs: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "logs": logs, "total": logs.len() }))
}

async fn get_log_detail(
    State(state): State<SharedState>,
    Path(log_id): Path<String>,
) -> ApiResult {
    let url = format!("{}/rest/v1/audit_logs?id=eq.{log_id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let logs: Vec<Value> = res.json().await.unwrap_or_default();
    let log = logs.first().ok_or(ApiError::NotFound("Audit log not found".into()))?;
    ok_json(log.clone())
}

async fn export_logs(
    State(state): State<SharedState>,
    Json(payload): Json<ExportAuditPayload>,
) -> ApiResult {
    let mut url = format!("{}/rest/v1/audit_logs?order=created_at.desc", state.rest_url());

    if let Some(ref from) = payload.from {
        url.push_str(&format!("&created_at=gx.{from}"));
    }
    if let Some(ref to) = payload.to {
        url.push_str(&format!("&created_at=lx.{to}"));
    }
    url.push_str("&limit=10000");

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let logs: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "format": payload.format.unwrap_or_else(|| "json".to_string()),
        "count": logs.len(),
        "data": logs
    }))
}
