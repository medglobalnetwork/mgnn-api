use axum::{
    Router,
    routing::{get, post, put},
    Json,
    extract::{State, Path, Query},
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json, ok_message};
use crate::extractors::AuthUser;

// ──────────────────────────────────────────────
// Admin / POC Engine  (Screens 143–150)
// Platform admin: user mgmt, moderation,
// analytics, config, feature flags, audit, POC
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Screen 143 – Admin Dashboard
        .route("/dashboard", get(admin_dashboard))
        .route("/dashboard/health", get(system_health))
        // Screen 144 – User Management
        .route("/users", get(list_users).put(bulk_update_users))
        .route("/users/search", get(search_users))
        .route("/users/:id", get(get_user_detail).put(update_user).delete(delete_user))
        .route("/users/:id/suspend", post(suspend_user))
        .route("/users/:id/unsuspend", post(unsuspend_user))
        .route("/users/:id/ban", post(ban_user))
        .route("/users/:id/unban", post(unban_user))
        .route("/users/:id/verify", post(admin_verify_user))
        // Screen 145 – Content Moderation
        .route("/moderation/reports", get(list_reports).post(create_report))
        .route("/moderation/reports/:report_id", get(get_report_detail).put(update_report))
        .route("/moderation/reports/:report_id/dismiss", post(dismiss_report))
        .route("/moderation/reports/:report_id/action", post(moderate_action))
        .route("/moderation/queue", get(moderation_queue))
        // Screen 146 – Trust Review Queue
        .route("/trust/verifications", get(list_verification_requests))
        .route("/trust/verifications/:id", get(get_verification_detail))
        .route("/trust/verifications/:id/approve", post(approve_verification))
        .route("/trust/verifications/:id/reject", post(reject_verification))
        .route("/trust/verifications/:id/request-info", post(request_verification_info))
        .route("/trust/stats", get(trust_review_stats))
        // Screen 147 – Platform Analytics
        .route("/analytics/overview", get(analytics_overview))
        .route("/analytics/users", get(analytics_users))
        .route("/analytics/content", get(analytics_content))
        .route("/analytics/growth", get(analytics_growth))
        // Screen 148 – System Configuration
        .route("/config", get(get_platform_config).put(update_platform_config))
        .route("/config/announcements", get(list_announcements).post(create_announcement))
        .route("/config/announcements/:id", put(update_announcement).delete(delete_announcement))
        // Screen 149 – Audit Log Viewer
        .route("/audit-logs", get(list_audit_logs))
        .route("/audit-logs/:id", get(get_audit_log_detail))
        .route("/audit-logs/export", post(export_audit_logs))
        // Screen 149b – Feature Flags
        .route("/flags", get(list_feature_flags).post(create_feature_flag))
        .route("/flags/:id", get(get_feature_flag).put(update_feature_flag).delete(delete_feature_flag))
        .route("/flags/:id/toggle", post(toggle_feature_flag))
        // Screen 150 – POC Dashboard
        .route("/poc", get(poc_dashboard))
        .route("/poc/deployments", get(list_poc_deployments).post(create_poc_deployment))
        .route("/poc/deployments/:id", get(get_poc_deployment).put(update_poc_deployment).delete(delete_poc_deployment))
        .route("/poc/deployments/:id/promote", post(promote_poc))
        .route("/poc/deployments/:id/rollback", post(rollback_poc))
}

// ─── Request Bodies ───────────────────────────────────────

#[derive(Deserialize)]
pub struct UserListQuery {
    pub status: Option<String>,
    pub role: Option<String>,
    pub search: Option<String>,
    #[allow(dead_code)]
    pub sort: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct UserSearchQuery {
    pub q: String,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct BulkUpdatePayload {
    pub user_ids: Vec<String>,
    pub action: String, // suspend, unsuspend, ban, unban, delete
}

#[derive(Deserialize)]
pub struct ReportListQuery {
    pub status: Option<String>,
    pub category: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct CreateReportPayload {
    pub target_type: String,  // post, comment, message, profile
    pub target_id: String,
    pub category: String,     // spam, harassment, misinformation, hate_speech, other
    pub reason: String,
    pub details: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateReportPayload {
    pub status: Option<String>,  // pending, reviewing, resolved, dismissed
    pub resolution_note: Option<String>,
}

#[derive(Deserialize)]
pub struct ModerateActionPayload {
    pub action: String,       // warn, mute, remove_content, suspend_user, ban_user
    #[allow(dead_code)]
    pub duration_hours: Option<i64>,
    pub reason: String,
}

#[derive(Deserialize)]
pub struct AnalyticsQuery {
    pub period: Option<String>,  // 24h, 7d, 30d, 90d
}

#[derive(Deserialize)]
pub struct UpdateConfigPayload {
    pub settings: Value,
}

#[derive(Deserialize)]
pub struct AnnouncementPayload {
    pub title: String,
    pub body: String,
    pub priority: Option<String>,  // low, normal, high, critical
    pub target_audience: Option<String>,  // all, professionals, students, admins
    pub expires_at: Option<String>,
}

#[derive(Deserialize)]
pub struct AuditLogQuery {
    pub user_id: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct VerificationQuery {
    pub status: Option<String>,  // pending, approved, rejected, needs_info
    pub verification_type: Option<String>,  // identity, education, license, organization, research
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct RejectVerificationPayload {
    pub rejection_reason: String,
    pub rejection_details: Option<String>,
}

#[derive(Deserialize)]
pub struct RequestInfoPayload {
    pub message: String,
    pub requested_documents: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct FeatureFlagPayload {
    pub name: String,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub rollout_percentage: Option<f64>,
    pub allowed_roles: Option<Vec<String>>,
    pub allowed_users: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct UpdateFlagPayload {
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub rollout_percentage: Option<f64>,
    pub allowed_roles: Option<Vec<String>>,
    pub allowed_users: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct PocDeploymentPayload {
    pub name: String,
    pub environment: String,  // staging, canary, production
    pub config: Option<Value>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdatePocPayload {
    pub config: Option<Value>,
    pub status: Option<String>,  // active, paused, stopped
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct PromotePocPayload {
    pub target: String,  // canary, production
    pub rollout_percentage: Option<f64>,
}

#[derive(Deserialize)]
pub struct ExportAuditPayload {
    pub from: Option<String>,
    pub to: Option<String>,
    pub format: Option<String>,  // csv, json
}

// ─── Helpers ─────────────────────────────────────────────

/// Verify the caller is a platform admin (role = "admin" in profiles table).
/// Check if user has admin privileges. Accepts:
/// - role = "admin" (general admin)
/// - role = "super_admin" (full access)
/// - account_type = "admin" (legacy admin flag)
pub async fn require_admin(state: &SharedState, profile_id: &str) -> Result<(), ApiError> {
    let url = format!(
        "{}/rest/v1/profiles?id=eq.{profile_id}&select=role,account_type",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let rows: Vec<Value> = res.json().await.unwrap_or_default();
    let row = rows.first().ok_or_else(|| ApiError::Forbidden("Profile not found".into()))?;

    let role = row.get("role").and_then(|v| v.as_str()).unwrap_or("");
    let account_type = row.get("account_type").and_then(|v| v.as_str()).unwrap_or("");

    if role == "admin" || role == "super_admin" || account_type == "admin" {
        return Ok(());
    }

    Err(ApiError::Forbidden("Admin access required".into()))
}

// ─── Screen 143: Admin Dashboard ────────────────────────

async fn admin_dashboard(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Total users
    let users_url = format!("{}/rest/v1/profiles?select=id", state.rest_url());
    let total_users = if let Ok(r) = state.http.get(&users_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = r.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    // Active today (last 24h logins)
    let active_url = format!(
        "{}/rest/v1/sessions?created_at=gx.{}&select=id",
        state.rest_url(),
        (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339()
    );
    let active_today = if let Ok(r) = state.http.get(&active_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = r.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    // Pending moderation reports
    let reports_url = format!(
        "{}/rest/v1/moderation_reports?status=eq.pending&select=id",
        state.rest_url()
    );
    let pending_reports = if let Ok(r) = state.http.get(&reports_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = r.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    // Total posts
    let posts_url = format!("{}/rest/v1/posts?select=id", state.rest_url());
    let total_posts = if let Ok(r) = state.http.get(&posts_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = r.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    // Active feature flags
    let flags_url = format!(
        "{}/rest/v1/feature_flags?enabled=eq.true&select=id",
        state.rest_url()
    );
    let active_flags = if let Ok(r) = state.http.get(&flags_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = r.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    // Active POC deployments
    let poc_url = format!(
        "{}/rest/v1/poc_deployments?status=eq.active&select=id",
        state.rest_url()
    );
    let active_pocs = if let Ok(r) = state.http.get(&poc_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = r.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    ok_json(json!({
        "total_users": total_users,
        "active_today": active_today,
        "pending_reports": pending_reports,
        "total_posts": total_posts,
        "active_flags": active_flags,
        "active_pocs": active_pocs,
        "quick_actions": [
            {"id": "users", "label": "Manage Users", "icon": "users", "count": total_users},
            {"id": "moderation", "label": "Moderation Queue", "icon": "shield-alert", "count": pending_reports},
            {"id": "analytics", "label": "Platform Analytics", "icon": "chart-bar"},
            {"id": "flags", "label": "Feature Flags", "icon": "flag", "count": active_flags},
            {"id": "poc", "label": "POC Deployments", "icon": "rocket", "count": active_pocs},
            {"id": "config", "label": "System Config", "icon": "cog"},
        ]
    }))
}

async fn system_health(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Check Supabase connectivity
    let db_start = std::time::Instant::now();
    let db_ok = state.http.get(format!("{}/rest/v1/profiles?select=id&limit=1", state.rest_url()))
        .headers(state.supabase_headers()).send().await.is_ok();
    let db_latency_ms = db_start.elapsed().as_millis() as u64;

    ok_json(json!({
        "database": {
            "status": if db_ok { "healthy" } else { "unreachable" },
            "latency_ms": db_latency_ms
        },
        "server": {
            "status": "running",
            "uptime": "ok"
        },
        "overall": if db_ok { "healthy" } else { "degraded" }
    }))
}

// ─── Screen 144: User Management ────────────────────────

async fn list_users(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<UserListQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let mut url = format!("{}/rest/v1/profiles?order=created_at.desc", state.rest_url());

    if let Some(ref status) = query.status {
        url.push_str(&format!("&account_status=eq.{status}"));
    }
    if let Some(ref role) = query.role {
        url.push_str(&format!("&role=eq.{role}"));
    }
    if let Some(ref search) = query.search {
        url.push_str(&format!("&or=(full_name.ilike.*{search}*,email.ilike.*{search}*)"));
    }

    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    url.push_str(&format!("&limit={limit}&offset={offset}"));

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let users: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "users": users,
        "total": users.len(),
        "limit": limit,
        "offset": offset
    }))
}

async fn search_users(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<UserSearchQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let limit = query.limit.unwrap_or(20);
    let url = format!(
        "{}/rest/v1/profiles?or=(full_name.ilike.*{q}*,email.ilike.*{q}*)&limit={limit}",
        state.rest_url(), q = query.q, limit = limit
    );

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let users: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "users": users, "total": users.len() }))
}

async fn get_user_detail(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let url = format!(
        "{}/rest/v1/profiles?id=eq.{user_id}&select=*,profile_experiences(*),profile_education(*)",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let profiles: Vec<Value> = res.json().await.unwrap_or_default();
    let profile = profiles.first().ok_or(ApiError::NotFound("User not found".into()))?;

    // Get recent sessions
    let sess_url = format!(
        "{}/rest/v1/sessions?user_id=eq.{user_id}&order=created_at.desc&limit=5",
        state.rest_url()
    );
    let sessions = if let Ok(r) = state.http.get(&sess_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default()
    } else { vec![] };

    // Get report count
    let reports_url = format!(
        "{}/rest/v1/moderation_reports?target_user_id=eq.{user_id}&select=id",
        state.rest_url()
    );
    let report_count = if let Ok(r) = state.http.get(&reports_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    let mut result = profile.clone();
    result["recent_sessions"] = json!(sessions);
    result["report_count"] = json!(report_count);

    ok_json(result)
}

async fn update_user(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
    Json(payload): Json<Value>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let res = state.http.patch(format!("{}/rest/v1/profiles?id=eq.{user_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&payload)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update user".into()));
    }

    // Audit log
    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "update_user",
            "target_type": "user",
            "target_id": user_id,
            "details": payload
        }))
        .send().await;

    ok_message("User updated")
}

async fn delete_user(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Prevent self-deletion
    if *uid == user_id {
        return Err(ApiError::BadRequest("Cannot delete your own admin account".into()));
    }

    // Soft delete: set status to deleted
    let res = state.http.patch(format!("{}/rest/v1/profiles?id=eq.{user_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({ "account_status": "deleted", "deleted_at": chrono::Utc::now().to_rfc3339() }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete user".into()));
    }

    // Invalidate all sessions
    let _ = state.http.delete(format!("{}/rest/v1/sessions?user_id=eq.{user_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;

    // Audit log
    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "delete_user",
            "target_type": "user",
            "target_id": user_id
        }))
        .send().await;

    ok_message("User deleted")
}

async fn suspend_user(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    if *uid == user_id {
        return Err(ApiError::BadRequest("Cannot suspend yourself".into()));
    }

    let res = state.http.patch(format!("{}/rest/v1/profiles?id=eq.{user_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({ "account_status": "suspended" }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to suspend user".into()));
    }

    // Invalidate all sessions
    let _ = state.http.delete(format!("{}/rest/v1/sessions?user_id=eq.{user_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;

    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "suspend_user",
            "target_type": "user",
            "target_id": user_id
        }))
        .send().await;

    ok_message("User suspended")
}

async fn unsuspend_user(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let res = state.http.patch(format!("{}/rest/v1/profiles?id=eq.{user_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({ "account_status": "active" }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to unsuspend user".into()));
    }

    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "unsuspend_user",
            "target_type": "user",
            "target_id": user_id
        }))
        .send().await;

    ok_message("User unsuspended")
}

async fn ban_user(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    if *uid == user_id {
        return Err(ApiError::BadRequest("Cannot ban yourself".into()));
    }

    let res = state.http.patch(format!("{}/rest/v1/profiles?id=eq.{user_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({ "account_status": "banned" }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to ban user".into()));
    }

    // Invalidate all sessions
    let _ = state.http.delete(format!("{}/rest/v1/sessions?user_id=eq.{user_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;

    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "ban_user",
            "target_type": "user",
            "target_id": user_id
        }))
        .send().await;

    ok_message("User banned")
}

async fn unban_user(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let res = state.http.patch(format!("{}/rest/v1/profiles?id=eq.{user_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({ "account_status": "active" }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to unban user".into()));
    }

    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "unban_user",
            "target_type": "user",
            "target_id": user_id
        }))
        .send().await;

    ok_message("User unbanned")
}

async fn admin_verify_user(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let res = state.http.patch(format!("{}/rest/v1/profiles?id=eq.{user_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({ "verified": true, "verified_at": chrono::Utc::now().to_rfc3339() }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to verify user".into()));
    }

    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "verify_user",
            "target_type": "user",
            "target_id": user_id
        }))
        .send().await;

    ok_message("User verified")
}

async fn bulk_update_users(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<BulkUpdatePayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let status = match payload.action.as_str() {
        "suspend" => "suspended",
        "unsuspend" => "active",
        "ban" => "banned",
        "unban" => "active",
        "delete" => "deleted",
        _ => return Err(ApiError::BadRequest("Invalid action".into())),
    };

    let mut success_count = 0;
    for user_id in &payload.user_ids {
        if *uid == *user_id { continue; } // skip self

        let res = state.http.patch(format!("{}/rest/v1/profiles?id=eq.{user_id}", state.rest_url()))
            .headers(state.supabase_headers())
            .json(&json!({ "account_status": status }))
            .send().await;

        if let Ok(r) = res {
            if r.status().is_success() {
                success_count += 1;
                if status != "active" {
                    let _ = state.http.delete(format!("{}/rest/v1/sessions?user_id=eq.{user_id}", state.rest_url()))
                        .headers(state.supabase_headers()).send().await;
                }
            }
        }
    }

    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": format!("bulk_{}", payload.action),
            "target_type": "user",
            "details": { "count": success_count, "total": payload.user_ids.len() }
        }))
        .send().await;

    ok_json(json!({
        "action": payload.action,
        "success_count": success_count,
        "total": payload.user_ids.len()
    }))
}

// ─── Screen 145: Content Moderation ─────────────────────

async fn list_reports(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<ReportListQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let mut url = format!("{}/rest/v1/moderation_reports?order=created_at.desc", state.rest_url());

    if let Some(ref status) = query.status {
        url.push_str(&format!("&status=eq.{status}"));
    }
    if let Some(ref category) = query.category {
        url.push_str(&format!("&category=eq.{category}"));
    }

    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    url.push_str(&format!("&limit={limit}&offset={offset}"));

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let reports: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "reports": reports, "total": reports.len() }))
}

async fn create_report(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<CreateReportPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let body = json!({
        "reporter_id": uid,
        "target_type": payload.target_type,
        "target_id": payload.target_id,
        "category": payload.category,
        "reason": payload.reason,
        "details": payload.details,
        "status": "pending"
    });

    let res = state.http.post(format!("{}/rest/v1/moderation_reports", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "report_id": data.get("id").and_then(|v| v.as_str()),
        "message": "Report submitted"
    }))
}

async fn get_report_detail(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(report_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let url = format!("{}/rest/v1/moderation_reports?id=eq.{report_id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let reports: Vec<Value> = res.json().await.unwrap_or_default();
    let report = reports.first().ok_or(ApiError::NotFound("Report not found".into()))?;

    // Enrich with target content
    let mut result = report.clone();
    let target_type = report.get("target_type").and_then(|v| v.as_str()).unwrap_or("");
    let target_id = report.get("target_id").and_then(|v| v.as_str()).unwrap_or("");

    let content_url = match target_type {
        "post" => Some(format!("{}/rest/v1/posts?id=eq.{target_id}", state.rest_url())),
        "comment" => Some(format!("{}/rest/v1/post_comments?id=eq.{target_id}", state.rest_url())),
        "profile" => Some(format!("{}/rest/v1/profiles?id=eq.{target_id}", state.rest_url())),
        _ => None,
    };

    if let Some(c_url) = content_url {
        if let Ok(c_res) = state.http.get(&c_url).headers(state.supabase_headers()).send().await {
            if let Ok(c_data) = c_res.json::<Vec<Value>>().await {
                if let Some(content) = c_data.first() {
                    result["target_content"] = content.clone();
                }
            }
        }
    }

    ok_json(result)
}

async fn update_report(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(report_id): Path<String>,
    Json(payload): Json<UpdateReportPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let mut body = serde_json::Map::new();
    if let Some(status) = payload.status { body.insert("status".into(), json!(status)); }
    if let Some(note) = payload.resolution_note { body.insert("resolution_note".into(), json!(note)); }
    if body.contains_key("status") {
        body.insert("reviewed_by".into(), json!(uid));
        body.insert("reviewed_at".into(), json!(chrono::Utc::now().to_rfc3339()));
    }

    let res = state.http.patch(format!("{}/rest/v1/moderation_reports?id=eq.{report_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!(body))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update report".into()));
    }

    ok_message("Report updated")
}

async fn dismiss_report(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(report_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let res = state.http.patch(format!("{}/rest/v1/moderation_reports?id=eq.{report_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "status": "dismissed",
            "reviewed_by": uid,
            "reviewed_at": chrono::Utc::now().to_rfc3339()
        }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to dismiss report".into()));
    }

    ok_message("Report dismissed")
}

async fn moderate_action(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(report_id): Path<String>,
    Json(payload): Json<ModerateActionPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Get report to find target
    let report_url = format!("{}/rest/v1/moderation_reports?id=eq.{report_id}", state.rest_url());
    let report_res = state.http.get(&report_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let reports: Vec<Value> = report_res.json().await.unwrap_or_default();
    let report = reports.first().ok_or(ApiError::NotFound("Report not found".into()))?;

    let target_user_id = report.get("target_user_id").and_then(|v| v.as_str()).unwrap_or("");
    let target_type = report.get("target_type").and_then(|v| v.as_str()).unwrap_or("");
    let target_id = report.get("target_id").and_then(|v| v.as_str()).unwrap_or("");

    // Execute moderation action
    match payload.action.as_str() {
        "warn" => {
            // Create warning notification for user
            let _ = state.http.post(format!("{}/rest/v1/notifications", state.rest_url()))
                .headers(state.supabase_headers())
                .json(&json!({
                    "user_id": target_user_id,
                    "type": "moderation_warning",
                    "title": "Content Warning",
                    "body": payload.reason,
                    "metadata": json!({ "report_id": report_id })
                }))
                .send().await;
        }
        "remove_content" => {
            let del_url = match target_type {
                "post" => Some(format!("{}/rest/v1/posts?id=eq.{target_id}", state.rest_url())),
                "comment" => Some(format!("{}/rest/v1/post_comments?id=eq.{target_id}", state.rest_url())),
                _ => None,
            };
            if let Some(url) = del_url {
                let _ = state.http.delete(&url).headers(state.supabase_headers()).send().await;
            }
        }
        "suspend_user" => {
            let _ = state.http.patch(format!("{}/rest/v1/profiles?id=eq.{target_user_id}", state.rest_url()))
                .headers(state.supabase_headers())
                .json(&json!({ "account_status": "suspended" }))
                .send().await;
            let _ = state.http.delete(format!("{}/rest/v1/sessions?user_id=eq.{target_user_id}", state.rest_url()))
                .headers(state.supabase_headers()).send().await;
        }
        "ban_user" => {
            let _ = state.http.patch(format!("{}/rest/v1/profiles?id=eq.{target_user_id}", state.rest_url()))
                .headers(state.supabase_headers())
                .json(&json!({ "account_status": "banned" }))
                .send().await;
            let _ = state.http.delete(format!("{}/rest/v1/sessions?user_id=eq.{target_user_id}", state.rest_url()))
                .headers(state.supabase_headers()).send().await;
        }
        _ => return Err(ApiError::BadRequest("Invalid moderation action".into())),
    }

    // Update report status
    let _ = state.http.patch(format!("{}/rest/v1/moderation_reports?id=eq.{report_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "status": "resolved",
            "resolution": payload.action,
            "resolution_note": payload.reason,
            "reviewed_by": uid,
            "reviewed_at": chrono::Utc::now().to_rfc3339()
        }))
        .send().await;

    // Audit log
    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "moderate_action",
            "target_type": target_type,
            "target_id": target_id,
            "details": {
                "action": payload.action,
                "reason": payload.reason,
                "report_id": report_id
            }
        }))
        .send().await;

    ok_json(json!({
        "action": payload.action,
        "message": format!("Moderation action '{}' applied", payload.action)
    }))
}

async fn moderation_queue(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let url = format!(
        "{}/rest/v1/moderation_reports?status=eq.pending&order=created_at.asc&limit=50",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let reports: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Get counts by category
    let cats = vec!["spam", "harassment", "misinformation", "hate_speech", "other"];
    let mut category_counts = serde_json::Map::new();
    for cat in cats {
        let cat_url = format!(
            "{}/rest/v1/moderation_reports?status=eq.pending&category=eq.{cat}&select=id",
            state.rest_url()
        );
        let count = if let Ok(r) = state.http.get(&cat_url).headers(state.supabase_headers()).send().await {
            r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
        } else { 0 };
        category_counts.insert(cat.to_string(), json!(count));
    }

    ok_json(json!({
        "queue": reports,
        "total_pending": reports.len(),
        "by_category": category_counts
    }))
}

// ─── Screen 146: Trust Review Queue ─────────────────────
// Admin reviews user-submitted verification requests
// (identity, education, license, organization, research)

/// GET /admin/trust/verifications?status=&verification_type=&limit=&offset=
/// List verification requests with optional filters
async fn list_verification_requests(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<VerificationQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let mut filters = Vec::new();
    if let Some(ref s) = query.status {
        filters.push(format!("status=eq.{s}"));
    }
    if let Some(ref vt) = query.verification_type {
        filters.push(format!("verification_type=eq.{vt}"));
    }
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    filters.push(format!("limit={limit}"));
    filters.push(format!("offset={offset}"));
    filters.push("order=created_at.desc".to_string());

    let qs = filters.join("&");
    let url = format!(
        "{}/rest/v1/verifications?{qs}&select=*,profiles!verifications_profile_id_fkey(full_name,email,avatar_url)",
        state.rest_url()
    );

    let resp = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::BadRequest(format!("Failed to fetch verifications: {e}")))?;

    let records: Vec<Value> = resp.json().await
        .map_err(|e| ApiError::BadRequest(format!("Failed to parse verifications: {e}")))?;

    // Count by status
    let mut status_counts = std::collections::HashMap::new();
    for r in &records {
        let st = r["status"].as_str().unwrap_or("unknown");
        *status_counts.entry(st.to_string()).or_insert(0i64) += 1;
    }

    ok_json(json!({
        "verifications": records,
        "total": records.len(),
        "by_status": status_counts
    }))
}

/// GET /admin/trust/verifications/:id
/// Get detailed verification request with profile info
async fn get_verification_detail(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let url = format!(
        "{}/rest/v1/verifications?id=eq.{id}&select=*,profiles!verifications_profile_id_fkey(full_name,email,avatar_url,headline,primary_category),profiles!verifications_reviewed_by_fkey(full_name)",
        state.rest_url()
    );

    let resp = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::BadRequest(format!("Failed to fetch verification: {e}")))?;

    let records: Vec<Value> = resp.json().await
        .map_err(|e| ApiError::BadRequest(format!("Failed to parse verification: {e}")))?;

    records.first()
        .ok_or_else(|| ApiError::NotFound("Verification request not found".into()))
        .and_then(|r| ok_json(json!(r)))
}

/// POST /admin/trust/verifications/:id/approve
/// Approve a verification request
async fn approve_verification(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Fetch current verification
    let url = format!("{}/rest/v1/verifications?id=eq.{id}&select=*,profiles!verifications_profile_id_fkey(full_name)", state.rest_url());
    let resp = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::BadRequest(format!("Failed to fetch verification: {e}")))?;
    let records: Vec<Value> = resp.json().await.unwrap_or_default();
    let record = records.first().ok_or_else(|| ApiError::NotFound("Verification not found".into()))?;

    if record["status"].as_str() == Some("verified") {
        return Err(ApiError::BadRequest("Already approved".into()));
    }

    let profile_id = record["profile_id"].as_str().unwrap_or("");
    let vtype = record["verification_type"].as_str().unwrap_or("");

    // Update verification status
    let body = json!({
        "status": "verified",
        "reviewed_by": uid,
        "reviewed_at": chrono::Utc::now().to_rfc3339()
    });
    let update_url = format!("{}/rest/v1/verifications?id=eq.{id}", state.rest_url());
    state.http.patch(&update_url).headers(state.supabase_headers())
        .json(&body).send().await
        .map_err(|e| ApiError::BadRequest(format!("Failed to update verification: {e}")))?;

    // If identity verification approved, mark profile as verified
    if vtype == "identity" {
        let prof_body = json!({ "profile_verified": true });
        let prof_url = format!("{}/rest/v1/profiles?id=eq.{profile_id}", state.rest_url());
        state.http.patch(&prof_url).headers(state.supabase_headers())
            .json(&prof_body).send().await.ok();
    }

    // Log audit
    let audit_body = json!({
        "actor_id": uid,
        "action": "trust.verification.approved",
        "target_type": "verification",
        "target_id": id,
        "metadata": json!({
            "profile_id": profile_id,
            "verification_type": vtype,
            "admin": uid
        })
    });
    let audit_url = format!("{}/rest/v1/audit_logs", state.rest_url());
    state.http.post(&audit_url).headers(state.supabase_headers())
        .json(&audit_body).send().await.ok();

    ok_message("Verification approved successfully")
}

/// POST /admin/trust/verifications/:id/reject
/// Reject a verification request
async fn reject_verification(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<RejectVerificationPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Fetch current verification
    let url = format!("{}/rest/v1/verifications?id=eq.{id}&select=*", state.rest_url());
    let resp = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::BadRequest(format!("Failed to fetch verification: {e}")))?;
    let records: Vec<Value> = resp.json().await.unwrap_or_default();
    let record = records.first().ok_or_else(|| ApiError::NotFound("Verification not found".into()))?;

    if record["status"].as_str() == Some("rejected") {
        return Err(ApiError::BadRequest("Already rejected".into()));
    }

    let profile_id = record["profile_id"].as_str().unwrap_or("");
    let vtype = record["verification_type"].as_str().unwrap_or("");

    let body = json!({
        "status": "rejected",
        "reviewed_by": uid,
        "reviewed_at": chrono::Utc::now().to_rfc3339(),
        "rejection_reason": payload.rejection_reason
    });
    let update_url = format!("{}/rest/v1/verifications?id=eq.{id}", state.rest_url());
    state.http.patch(&update_url).headers(state.supabase_headers())
        .json(&body).send().await
        .map_err(|e| ApiError::BadRequest(format!("Failed to reject verification: {e}")))?;

    // Log audit
    let audit_body = json!({
        "actor_id": uid,
        "action": "trust.verification.rejected",
        "target_type": "verification",
        "target_id": id,
        "metadata": json!({
            "profile_id": profile_id,
            "verification_type": vtype,
            "reason": payload.rejection_reason,
            "details": payload.rejection_details,
            "admin": uid
        })
    });
    let audit_url = format!("{}/rest/v1/audit_logs", state.rest_url());
    state.http.post(&audit_url).headers(state.supabase_headers())
        .json(&audit_body).send().await.ok();

    ok_message("Verification rejected")
}

/// POST /admin/trust/verifications/:id/request-info
/// Request additional information from user
async fn request_verification_info(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<RequestInfoPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Fetch current verification
    let url = format!("{}/rest/v1/verifications?id=eq.{id}&select=*", state.rest_url());
    let resp = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::BadRequest(format!("Failed to fetch verification: {e}")))?;
    let records: Vec<Value> = resp.json().await.unwrap_or_default();
    let record = records.first().ok_or_else(|| ApiError::NotFound("Verification not found".into()))?;

    let profile_id = record["profile_id"].as_str().unwrap_or("");

    // Update verification with info request
    let body = json!({
        "status": "pending",
        "reviewed_by": uid,
        "reviewed_at": chrono::Utc::now().to_rfc3339(),
        "notes": payload.message
    });
    let update_url = format!("{}/rest/v1/verifications?id=eq.{id}", state.rest_url());
    state.http.patch(&update_url).headers(state.supabase_headers())
        .json(&body).send().await
        .map_err(|e| ApiError::BadRequest(format!("Failed to update verification: {e}")))?;

    // Create notification to user
    let notif_body = json!({
        "user_id": profile_id,
        "type": "verification_info_requested",
        "title": "Additional information needed",
        "body": payload.message,
        "metadata": json!({
            "verification_id": id,
            "requested_documents": payload.requested_documents
        })
    });
    let notif_url = format!("{}/rest/v1/notifications", state.rest_url());
    state.http.post(&notif_url).headers(state.supabase_headers())
        .json(&notif_body).send().await.ok();

    ok_message("Information request sent to user")
}

/// GET /admin/trust/stats
/// Trust review queue statistics
async fn trust_review_stats(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Fetch all verifications for stats
    let url = format!(
        "{}/rest/v1/verifications?select=status,verification_type,created_at",
        state.rest_url()
    );
    let resp = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::BadRequest(format!("Failed to fetch stats: {e}")))?;
    let records: Vec<Value> = resp.json().await.unwrap_or_default();

    let mut by_status = std::collections::HashMap::new();
    let mut by_type = std::collections::HashMap::new();

    for r in &records {
        let st = r["status"].as_str().unwrap_or("unknown");
        let vt = r["verification_type"].as_str().unwrap_or("unknown");
        *by_status.entry(st.to_string()).or_insert(0i64) += 1;
        *by_type.entry(vt.to_string()).or_insert(0i64) += 1;
    }

    // Average review time for resolved items
    let resolved: Vec<&Value> = records.iter()
        .filter(|r| matches!(r["status"].as_str(), Some("verified") | Some("rejected")))
        .collect();

    ok_json(json!({
        "total": records.len(),
        "by_status": by_status,
        "by_type": by_type,
        "pending_count": by_status.get("pending").unwrap_or(&0),
        "resolved_count": resolved.len() as i64,
        "avg_reviewer_load": if !records.is_empty() {
            records.len() as f64 / 5.0 // rough estimate across 5 active reviewers
        } else { 0.0 }
    }))
}

// ─── Screen 147: Platform Analytics ─────────────────────

async fn analytics_overview(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let period = query.period.unwrap_or_else(|| "30d".to_string());
    let days = match period.as_str() {
        "24h" => 1,
        "7d" => 7,
        "30d" => 30,
        "90d" => 90,
        _ => 30,
    };
    let since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();

    // New users in period
    let new_users_url = format!(
        "{}/rest/v1/profiles?created_at=gx.{since}&select=id",
        state.rest_url()
    );
    let new_users = if let Ok(r) = state.http.get(&new_users_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    // New posts in period
    let new_posts_url = format!(
        "{}/rest/v1/posts?created_at=gx.{since}&select=id",
        state.rest_url()
    );
    let new_posts = if let Ok(r) = state.http.get(&new_posts_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    // New connections in period
    let new_conns_url = format!(
        "{}/rest/v1/connections?created_at=gx.{since}&select=id",
        state.rest_url()
    );
    let new_connections = if let Ok(r) = state.http.get(&new_conns_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    // Active users (sessions in period)
    let active_url = format!(
        "{}/rest/v1/sessions?created_at=gx.{since}&select=user_id",
        state.rest_url()
    );
    let active_users = if let Ok(r) = state.http.get(&active_url).headers(state.supabase_headers()).send().await {
        let sessions: Vec<Value> = r.json().await.unwrap_or_default();
        // Count unique user_ids
        let mut unique = std::collections::HashSet::new();
        for s in &sessions {
            if let Some(uid) = s.get("user_id").and_then(|v| v.as_str()) {
                unique.insert(uid.to_string());
            }
        }
        unique.len() as i64
    } else { 0 };

    // Total counts
    let total_users_url = format!("{}/rest/v1/profiles?select=id", state.rest_url());
    let total_users = if let Ok(r) = state.http.get(&total_users_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    let total_posts_url = format!("{}/rest/v1/posts?select=id", state.rest_url());
    let total_posts = if let Ok(r) = state.http.get(&total_posts_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    ok_json(json!({
        "period": period,
        "new_users": new_users,
        "new_posts": new_posts,
        "new_connections": new_connections,
        "active_users": active_users,
        "total_users": total_users,
        "total_posts": total_posts,
        "engagement_rate": if total_users > 0 { (active_users as f64 / total_users as f64 * 100.0).min(100.0) } else { 0.0 }
    }))
}

async fn analytics_users(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let period = query.period.unwrap_or_else(|| "30d".to_string());
    let days = match period.as_str() {
        "24h" => 1,
        "7d" => 7,
        "30d" => 30,
        "90d" => 90,
        _ => 30,
    };
    let _since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();

    // By role
    let roles = vec!["professional", "student", "admin"];
    let mut by_role = serde_json::Map::new();
    for role in roles {
        let url = format!(
            "{}/rest/v1/profiles?role=eq.{role}&select=id",
            state.rest_url()
        );
        let count = if let Ok(r) = state.http.get(&url).headers(state.supabase_headers()).send().await {
            r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
        } else { 0 };
        by_role.insert(role.to_string(), json!(count));
    }

    // By status
    let statuses = vec!["active", "suspended", "banned"];
    let mut by_status = serde_json::Map::new();
    for status in statuses {
        let url = format!(
            "{}/rest/v1/profiles?account_status=eq.{status}&select=id",
            state.rest_url()
        );
        let count = if let Ok(r) = state.http.get(&url).headers(state.supabase_headers()).send().await {
            r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
        } else { 0 };
        by_status.insert(status.to_string(), json!(count));
    }

    // By primary category (top 5)
    let categories_url = format!(
        "{}/rest/v1/profiles?primary_category=not.is.null&select=primary_category",
        state.rest_url()
    );
    let by_category = if let Ok(r) = state.http.get(&categories_url).headers(state.supabase_headers()).send().await {
        let profiles: Vec<Value> = r.json().await.unwrap_or_default();
        let mut counts = std::collections::HashMap::new();
        for p in &profiles {
            if let Some(cat) = p.get("primary_category").and_then(|v| v.as_str()) {
                *counts.entry(cat.to_string()).or_insert(0) += 1;
            }
        }
        let mut sorted: Vec<(String, i64)> = counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(5);
        sorted.into_iter().map(|(k, v)| (k, json!(v))).collect::<serde_json::Map<String, Value>>()
    } else {
        serde_json::Map::new()
    };

    ok_json(json!({
        "period": period,
        "by_role": by_role,
        "by_status": by_status,
        "by_category": by_category
    }))
}

async fn analytics_content(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let period = query.period.unwrap_or_else(|| "30d".to_string());
    let days = match period.as_str() {
        "24h" => 1,
        "7d" => 7,
        "30d" => 30,
        "90d" => 90,
        _ => 30,
    };
    let since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();

    let new_posts_url = format!("{}/rest/v1/posts?created_at=gx.{since}&select=id", state.rest_url());
    let new_posts = if let Ok(r) = state.http.get(&new_posts_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    let new_comments_url = format!("{}/rest/v1/post_comments?created_at=gx.{since}&select=id", state.rest_url());
    let new_comments = if let Ok(r) = state.http.get(&new_comments_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    let reactions_url = format!("{}/rest/v1/post_reactions?created_at=gx.{since}&select=id", state.rest_url());
    let new_reactions = if let Ok(r) = state.http.get(&reactions_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    ok_json(json!({
        "period": period,
        "new_posts": new_posts,
        "new_comments": new_comments,
        "new_reactions": new_reactions,
        "total_interactions": new_comments + new_reactions
    }))
}

async fn analytics_growth(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Last 7 days daily new users
    let mut daily_users = Vec::new();
    for i in (0..7).rev() {
        let day_start = (chrono::Utc::now() - chrono::Duration::days(i)).format("%Y-%m-%dT00:00:00").to_string();
        let day_end = (chrono::Utc::now() - chrono::Duration::days(i - 1)).format("%Y-%m-%dT00:00:00").to_string();
        let url = format!(
            "{}/rest/v1/profiles?created_at=gte.{day_start}&created_at=lt.{day_end}&select=id",
            state.rest_url()
        );
        let count = if let Ok(r) = state.http.get(&url).headers(state.supabase_headers()).send().await {
            r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
        } else { 0 };
        daily_users.push(json!({
            "date": day_start.split('T').next().unwrap_or(""),
            "count": count
        }));
    }

    // Last 7 days daily new posts
    let mut daily_posts = Vec::new();
    for i in (0..7).rev() {
        let day_start = (chrono::Utc::now() - chrono::Duration::days(i)).format("%Y-%m-%dT00:00:00").to_string();
        let day_end = (chrono::Utc::now() - chrono::Duration::days(i - 1)).format("%Y-%m-%dT00:00:00").to_string();
        let url = format!(
            "{}/rest/v1/posts?created_at=gte.{day_start}&created_at=lt.{day_end}&select=id",
            state.rest_url()
        );
        let count = if let Ok(r) = state.http.get(&url).headers(state.supabase_headers()).send().await {
            r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
        } else { 0 };
        daily_posts.push(json!({
            "date": day_start.split('T').next().unwrap_or(""),
            "count": count
        }));
    }

    ok_json(json!({
        "daily_users": daily_users,
        "daily_posts": daily_posts
    }))
}

// ─── Screen 147: System Configuration ───────────────────

async fn get_platform_config(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let url = format!("{}/rest/v1/platform_config?order=key.asc", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let configs: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Convert array to key-value map
    let mut settings = serde_json::Map::new();
    for c in &configs {
        if let Some(key) = c.get("key").and_then(|v| v.as_str()) {
            let value = c.get("value").cloned().unwrap_or(json!(null));
            settings.insert(key.to_string(), value);
        }
    }

    ok_json(json!({ "settings": settings }))
}

async fn update_platform_config(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<UpdateConfigPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Upsert each config key
    if let Some(obj) = payload.settings.as_object() {
        for (key, value) in obj {
            let body = json!({
                "key": key,
                "value": value,
                "updated_by": uid,
                "updated_at": chrono::Utc::now().to_rfc3339()
            });
            let _ = state.http.post(format!("{}/rest/v1/platform_config", state.rest_url()))
                .headers(state.supabase_headers())
                .header("Prefer", "resolution=merge-duplicates")
                .json(&body)
                .send().await;
        }
    }

    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "update_platform_config",
            "target_type": "config",
            "details": payload.settings
        }))
        .send().await;

    ok_message("Platform configuration updated")
}

async fn list_announcements(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let url = format!("{}/rest/v1/announcements?order=created_at.desc", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let announcements: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "announcements": announcements, "total": announcements.len() }))
}

async fn create_announcement(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<AnnouncementPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let body = json!({
        "title": payload.title,
        "body": payload.body,
        "priority": payload.priority.unwrap_or_else(|| "normal".to_string()),
        "target_audience": payload.target_audience.unwrap_or_else(|| "all".to_string()),
        "expires_at": payload.expires_at,
        "created_by": uid
    });

    let res = state.http.post(format!("{}/rest/v1/announcements", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "announcement_id": data.get("id").and_then(|v| v.as_str()),
        "message": "Announcement created"
    }))
}

async fn update_announcement(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let res = state.http.patch(format!("{}/rest/v1/announcements?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&payload)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update announcement".into()));
    }

    ok_message("Announcement updated")
}

async fn delete_announcement(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let res = state.http.delete(format!("{}/rest/v1/announcements?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete announcement".into()));
    }

    ok_message("Announcement deleted")
}

// ─── Screen 148: Audit Log Viewer ───────────────────────

async fn list_audit_logs(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<AuditLogQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let mut url = format!("{}/rest/v1/admin_audit_logs?order=created_at.desc", state.rest_url());

    if let Some(ref user_id) = query.user_id {
        url.push_str(&format!("&admin_id=eq.{user_id}"));
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

    ok_json(json!({ "logs": logs, "total": logs.len(), "limit": limit, "offset": offset }))
}

async fn get_audit_log_detail(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(log_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let url = format!("{}/rest/v1/admin_audit_logs?id=eq.{log_id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let logs: Vec<Value> = res.json().await.unwrap_or_default();
    let log = logs.first().ok_or(ApiError::NotFound("Audit log not found".into()))?;

    // Enrich with admin profile info
    let mut result = log.clone();
    if let Some(admin_id) = log.get("admin_id").and_then(|v| v.as_str()) {
        let p_url = format!(
            "{}/rest/v1/profiles?id=eq.{admin_id}&select=id,full_name,email",
            state.rest_url()
        );
        if let Ok(p_res) = state.http.get(&p_url).headers(state.supabase_headers()).send().await {
            if let Ok(p_data) = p_res.json::<Vec<Value>>().await {
                if let Some(profile) = p_data.first() {
                    result["admin_profile"] = profile.clone();
                }
            }
        }
    }

    ok_json(result)
}

async fn export_audit_logs(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<ExportAuditPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let mut url = format!("{}/rest/v1/admin_audit_logs?order=created_at.desc", state.rest_url());

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

    let format = payload.format.unwrap_or_else(|| "json".to_string());

    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "export_audit_logs",
            "target_type": "audit_logs",
            "details": { "format": format, "count": logs.len() }
        }))
        .send().await;

    ok_json(json!({
        "format": format,
        "count": logs.len(),
        "data": logs
    }))
}

// ─── Screen 149: Feature Flags ──────────────────────────

async fn list_feature_flags(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let url = format!("{}/rest/v1/feature_flags?order=name.asc", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let flags: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "flags": flags, "total": flags.len() }))
}

async fn create_feature_flag(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<FeatureFlagPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Check name uniqueness
    let check_url = format!(
        "{}/rest/v1/feature_flags?name=eq.{}&select=id",
        state.rest_url(), payload.name
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await;
    if let Ok(r) = check_res {
        if let Ok(rows) = r.json::<Vec<Value>>().await {
            if !rows.is_empty() {
                return Err(ApiError::Conflict("Feature flag name already exists".into()));
            }
        }
    }

    let body = json!({
        "name": payload.name,
        "description": payload.description,
        "enabled": payload.enabled.unwrap_or(false),
        "rollout_percentage": payload.rollout_percentage.unwrap_or(0.0),
        "allowed_roles": payload.allowed_roles,
        "allowed_users": payload.allowed_users,
        "created_by": uid
    });

    let res = state.http.post(format!("{}/rest/v1/feature_flags", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "flag_id": data.get("id").and_then(|v| v.as_str()),
        "message": "Feature flag created"
    }))
}

async fn get_feature_flag(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(flag_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let url = format!("{}/rest/v1/feature_flags?id=eq.{flag_id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let flags: Vec<Value> = res.json().await.unwrap_or_default();
    let flag = flags.first().ok_or(ApiError::NotFound("Feature flag not found".into()))?;

    ok_json(flag.clone())
}

async fn update_feature_flag(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(flag_id): Path<String>,
    Json(payload): Json<UpdateFlagPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let mut body = serde_json::Map::new();
    if let Some(desc) = payload.description { body.insert("description".into(), json!(desc)); }
    if let Some(enabled) = payload.enabled { body.insert("enabled".into(), json!(enabled)); }
    if let Some(pct) = payload.rollout_percentage { body.insert("rollout_percentage".into(), json!(pct)); }
    if let Some(roles) = payload.allowed_roles { body.insert("allowed_roles".into(), json!(roles)); }
    if let Some(users) = payload.allowed_users { body.insert("allowed_users".into(), json!(users)); }

    if body.is_empty() {
        return Err(ApiError::BadRequest("No fields to update".into()));
    }

    body.insert("updated_by".into(), json!(uid));
    body.insert("updated_at".into(), json!(chrono::Utc::now().to_rfc3339()));

    let res = state.http.patch(format!("{}/rest/v1/feature_flags?id=eq.{flag_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!(body))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update feature flag".into()));
    }

    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "update_feature_flag",
            "target_type": "feature_flag",
            "target_id": flag_id,
            "details": json!(body)
        }))
        .send().await;

    ok_message("Feature flag updated")
}

async fn delete_feature_flag(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(flag_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let res = state.http.delete(format!("{}/rest/v1/feature_flags?id=eq.{flag_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete feature flag".into()));
    }

    ok_message("Feature flag deleted")
}

async fn toggle_feature_flag(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(flag_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Get current state
    let url = format!("{}/rest/v1/feature_flags?id=eq.{flag_id}&select=enabled,name", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let flags: Vec<Value> = res.json().await.unwrap_or_default();
    let flag = flags.first().ok_or(ApiError::NotFound("Feature flag not found".into()))?;

    let current = flag.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let new_val = !current;

    let patch = state.http.patch(format!("{}/rest/v1/feature_flags?id=eq.{flag_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "enabled": new_val,
            "updated_by": uid,
            "updated_at": chrono::Utc::now().to_rfc3339()
        }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !patch.status().is_success() {
        return Err(ApiError::Internal("Failed to toggle feature flag".into()));
    }

    ok_json(json!({
        "flag_id": flag_id,
        "name": flag.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        "enabled": new_val,
        "message": format!("Feature flag {}", if new_val { "enabled" } else { "disabled" })
    }))
}

// ─── Screen 150: POC Dashboard ─────────────────────────

async fn poc_dashboard(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // All deployments
    let url = format!("{}/rest/v1/poc_deployments?order=created_at.desc", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let deployments: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Count by status
    let mut status_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for d in &deployments {
        let status = d.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
        *status_map.entry(status.to_string()).or_insert(0) += 1;
    }
    let status_counts: serde_json::Map<String, Value> = status_map.into_iter()
        .map(|(k, v)| (k, json!(v)))
        .collect();

    ok_json(json!({
        "deployments": deployments,
        "total": deployments.len(),
        "by_status": status_counts
    }))
}

async fn list_poc_deployments(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let url = format!("{}/rest/v1/poc_deployments?order=created_at.desc", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let deployments: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "deployments": deployments, "total": deployments.len() }))
}

async fn create_poc_deployment(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<PocDeploymentPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let body = json!({
        "name": payload.name,
        "environment": payload.environment,
        "config": payload.config,
        "description": payload.description,
        "status": "active",
        "created_by": uid
    });

    let res = state.http.post(format!("{}/rest/v1/poc_deployments", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "deployment_id": data.get("id").and_then(|v| v.as_str()),
        "message": "POC deployment created"
    }))
}

async fn get_poc_deployment(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(deploy_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let url = format!("{}/rest/v1/poc_deployments?id=eq.{deploy_id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let deployments: Vec<Value> = res.json().await.unwrap_or_default();
    let deployment = deployments.first().ok_or(ApiError::NotFound("Deployment not found".into()))?;

    ok_json(deployment.clone())
}

async fn update_poc_deployment(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(deploy_id): Path<String>,
    Json(payload): Json<UpdatePocPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let mut body = serde_json::Map::new();
    if let Some(config) = payload.config { body.insert("config".into(), config); }
    if let Some(status) = payload.status { body.insert("status".into(), json!(status)); }
    if let Some(desc) = payload.description { body.insert("description".into(), json!(desc)); }
    body.insert("updated_by".into(), json!(uid));
    body.insert("updated_at".into(), json!(chrono::Utc::now().to_rfc3339()));

    let res = state.http.patch(format!("{}/rest/v1/poc_deployments?id=eq.{deploy_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!(body))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update deployment".into()));
    }

    ok_message("POC deployment updated")
}

async fn delete_poc_deployment(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(deploy_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    let res = state.http.delete(format!("{}/rest/v1/poc_deployments?id=eq.{deploy_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete deployment".into()));
    }

    ok_message("POC deployment deleted")
}

async fn promote_poc(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(deploy_id): Path<String>,
    Json(payload): Json<PromotePocPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Get current deployment
    let url = format!("{}/rest/v1/poc_deployments?id=eq.{deploy_id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let deployments: Vec<Value> = res.json().await.unwrap_or_default();
    let deployment = deployments.first().ok_or(ApiError::NotFound("Deployment not found".into()))?;

    let current_env = deployment.get("environment").and_then(|v| v.as_str()).unwrap_or("");

    // Validate promotion path
    let valid = match (current_env, payload.target.as_str()) {
        ("staging", "canary") => true,
        ("canary", "production") => true,
        ("staging", "production") => true,
        _ => false,
    };
    if !valid {
        return Err(ApiError::BadRequest(format!(
            "Cannot promote from {current_env} to {}", payload.target
        )));
    }

    let mut body = json!({
        "environment": payload.target,
        "promoted_by": uid,
        "promoted_at": chrono::Utc::now().to_rfc3339()
    });
    if let Some(pct) = payload.rollout_percentage {
        body["rollout_percentage"] = json!(pct);
    }

    let patch_res = state.http.patch(format!("{}/rest/v1/poc_deployments?id=eq.{deploy_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !patch_res.status().is_success() {
        return Err(ApiError::Internal("Failed to promote deployment".into()));
    }

    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "promote_poc",
            "target_type": "poc_deployment",
            "target_id": deploy_id,
            "details": {
                "from": current_env,
                "to": payload.target,
                "rollout_percentage": payload.rollout_percentage
            }
        }))
        .send().await;

    ok_json(json!({
        "deployment_id": deploy_id,
        "from": current_env,
        "to": payload.target,
        "message": format!("Deployment promoted to {}", payload.target)
    }))
}

async fn rollback_poc(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(deploy_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    require_admin(&state, uid).await?;

    // Get current deployment
    let url = format!("{}/rest/v1/poc_deployments?id=eq.{deploy_id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let deployments: Vec<Value> = res.json().await.unwrap_or_default();
    let deployment = deployments.first().ok_or(ApiError::NotFound("Deployment not found".into()))?;

    let current_env = deployment.get("environment").and_then(|v| v.as_str()).unwrap_or("");

    // Rollback path
    let target_env = match current_env {
        "production" => "canary",
        "canary" => "staging",
        _ => return Err(ApiError::BadRequest("Cannot rollback from staging".into())),
    };

    let patch_res = state.http.patch(format!("{}/rest/v1/poc_deployments?id=eq.{deploy_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "environment": target_env,
            "status": "paused",
            "updated_by": uid,
            "updated_at": chrono::Utc::now().to_rfc3339()
        }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !patch_res.status().is_success() {
        return Err(ApiError::Internal("Failed to rollback deployment".into()));
    }

    let _ = state.http.post(format!("{}/rest/v1/admin_audit_logs", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "admin_id": uid,
            "action": "rollback_poc",
            "target_type": "poc_deployment",
            "target_id": deploy_id,
            "details": { "from": current_env, "to": target_env }
        }))
        .send().await;

    ok_json(json!({
        "deployment_id": deploy_id,
        "from": current_env,
        "to": target_env,
        "message": format!("Deployment rolled back to {target_env}")
    }))
}
