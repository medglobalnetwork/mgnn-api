use axum::{
    Router,
    routing::{get, post},
    Json,
    extract::{State, Path, Query},
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json, ok_message};
use crate::extractors::AuthUser;
use crate::modules::admin::require_admin;

// ──────────────────────────────────────────────
// Background Jobs Engine
// Job queue, scheduling, status tracking
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/status", get(get_jobs_status))
        .route("/", get(list_jobs).post(create_job))
        .route("/:id", get(get_job_detail).put(update_job).delete(delete_job))
        .route("/:id/cancel", post(cancel_job))
        .route("/:id/retry", post(retry_job))
        .route("/scheduled", get(list_scheduled))
        .route("/failed", get(list_failed))
}

#[derive(Deserialize)]
pub struct CreateJobPayload {
    pub job_type: String,
    pub payload: Option<Value>,
    pub scheduled_at: Option<String>,
    pub priority: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateJobPayload {
    pub status: Option<String>,
    pub payload: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct JobQuery {
    pub status: Option<String>,
    pub job_type: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ─── Status ────────────────────────────────────────────

async fn get_jobs_status(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    require_admin(&state, &auth.claims.profile_id).await?;
    // Count by status
    let statuses = vec!["pending", "running", "completed", "failed", "cancelled"];
    let mut counts = serde_json::Map::new();
    let mut total: i64 = 0;
    for status in statuses {
        let url = format!(
            "{}/rest/v1/background_jobs?status=eq.{status}&select=id",
            state.rest_url()
        );
        let count = if let Ok(r) = state.http.get(&url).headers(state.supabase_headers()).send().await {
            r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
        } else { 0 };
        counts.insert(status.to_string(), json!(count));
        total += count;
    }

    ok_json(json!({
        "total_jobs": total,
        "by_status": counts
    }))
}

// ─── Jobs CRUD ─────────────────────────────────────────

async fn list_jobs(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<JobQuery>,
) -> ApiResult {
    require_admin(&state, &auth.claims.profile_id).await?;
    let mut url = format!("{}/rest/v1/background_jobs?order=created_at.desc", state.rest_url());
    if let Some(ref status) = query.status { url.push_str(&format!("&status=eq.{status}")); }
    if let Some(ref jt) = query.job_type { url.push_str(&format!("&job_type=eq.{jt}")); }
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    url.push_str(&format!("&limit={limit}&offset={offset}"));

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let jobs: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "jobs": jobs, "total": jobs.len() }))
}

async fn create_job(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<CreateJobPayload>,
) -> ApiResult {
    let body = json!({
        "job_type": payload.job_type,
        "payload": payload.payload,
        "scheduled_at": payload.scheduled_at,
        "priority": payload.priority.unwrap_or_else(|| "normal".to_string()),
        "status": "pending",
        "created_by": auth.claims.profile_id
    });
    let res = state.http.post(format!("{}/rest/v1/background_jobs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "job_id": data.get("id").and_then(|v| v.as_str()), "message": "Job created" }))
}

async fn get_job_detail(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(job_id): Path<String>,
) -> ApiResult {
    require_admin(&state, &auth.claims.profile_id).await?;
    let url = format!("{}/rest/v1/background_jobs?id=eq.{job_id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let jobs: Vec<Value> = res.json().await.unwrap_or_default();
    let job = jobs.first().ok_or(ApiError::NotFound("Job not found".into()))?;
    ok_json(job.clone())
}

async fn update_job(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(job_id): Path<String>,
    Json(payload): Json<UpdateJobPayload>,
) -> ApiResult {
    require_admin(&state, &auth.claims.profile_id).await?;
    let mut body = serde_json::Map::new();
    if let Some(status) = payload.status { body.insert("status".into(), json!(status)); }
    if let Some(p) = payload.payload { body.insert("payload".into(), p); }
    if let Some(r) = payload.result { body.insert("result".into(), r); }
    if let Some(e) = payload.error { body.insert("error".into(), json!(e)); }

    let res = state.http.patch(format!("{}/rest/v1/background_jobs?id=eq.{job_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!(body))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to update job".into())); }
    ok_message("Job updated")
}

async fn delete_job(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(job_id): Path<String>,
) -> ApiResult {
    require_admin(&state, &auth.claims.profile_id).await?;
    let res = state.http.delete(format!("{}/rest/v1/background_jobs?id=eq.{job_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to delete job".into())); }
    ok_message("Job deleted")
}

async fn cancel_job(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(job_id): Path<String>,
) -> ApiResult {
    require_admin(&state, &auth.claims.profile_id).await?;
    let res = state.http.patch(format!("{}/rest/v1/background_jobs?id=eq.{job_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({ "status": "cancelled", "cancelled_at": chrono::Utc::now().to_rfc3339() }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to cancel job".into())); }
    ok_message("Job cancelled")
}

async fn retry_job(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(job_id): Path<String>,
) -> ApiResult {
    require_admin(&state, &auth.claims.profile_id).await?;
    let res = state.http.patch(format!("{}/rest/v1/background_jobs?id=eq.{job_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "status": "pending",
            "error": null,
            "retry_count": 1,
            "retried_at": chrono::Utc::now().to_rfc3339()
        }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to retry job".into())); }
    ok_message("Job requeued for retry")
}

// ─── Filtered Lists ────────────────────────────────────

async fn list_scheduled(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    require_admin(&state, &auth.claims.profile_id).await?;
    let url = format!(
        "{}/rest/v1/background_jobs?status=eq.pending&scheduled_at=not.is.null&order=scheduled_at.asc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let jobs: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "jobs": jobs, "total": jobs.len() }))
}

async fn list_failed(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    require_admin(&state, &auth.claims.profile_id).await?;
    let url = format!(
        "{}/rest/v1/background_jobs?status=eq.failed&order=created_at.desc&limit=100",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let jobs: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "jobs": jobs, "total": jobs.len() }))
}
