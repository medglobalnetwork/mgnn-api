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

// ──────────────────────────────────────────────
// Events Engine
// Real-time events, SSE streaming, event logging,
// user activity tracking, push event dispatch
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Event stream (SSE)
        .route("/stream", get(event_stream))
        // Event history
        .route("/", get(list_events).post(emit_event))
        .route("/unread-count", get(unread_event_count))
        .route("/:id/ack", post(acknowledge_event))
        .route("/:id", get(get_event_detail))
        // User activity
        .route("/activity", get(list_user_activity))
        // Event types catalog
        .route("/types", get(list_event_types))
}

#[derive(Deserialize)]
pub struct EmitEventPayload {
    pub event_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub payload: Option<Value>,
    pub priority: Option<String>,
}

#[derive(Deserialize)]
pub struct ListEventsQuery {
    pub event_type: Option<String>,
    pub since: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct ListActivityQuery {
    pub limit: Option<i64>,
}

// ─── Event Stream (SSE) ─────────────────────────────

async fn event_stream(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Return SSE connection info — actual SSE requires axum::response::sse
    // For now, return a connection token and endpoint for client polling
    let url = format!(
        "{}/rest/v1/user_events?user_id=eq.{uid}&status=eq.unread&order=created_at.desc&limit=50",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let events: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "connection": {
            "type": "polling",
            "endpoint": "/api/v1/events/stream/poll",
            "interval_seconds": 30,
            "note": "Client should poll every 30s for new events"
        },
        "initial_events": events,
        "total_unread": events.len()
    }))
}

// ─── List Events ─────────────────────────────────────

async fn list_events(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<ListEventsQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let mut url = format!(
        "{}/rest/v1/user_events?user_id=eq.{uid}&order=created_at.desc",
        state.rest_url()
    );

    if let Some(ref event_type) = query.event_type {
        url.push_str(&format!("&event_type=eq.{event_type}"));
    }
    if let Some(ref since) = query.since {
        url.push_str(&format!("&created_at=gte.{since}"));
    }
    let limit = query.limit.unwrap_or(50);
    url.push_str(&format!("&limit={limit}"));

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let events: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "events": events, "total": events.len() }))
}

// ─── Emit Event ──────────────────────────────────────

async fn emit_event(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<EmitEventPayload>,
) -> ApiResult {
    let _uid = &auth.claims.profile_id;

    let body = json!({
        "user_id": _uid,
        "event_type": payload.event_type,
        "target_type": payload.target_type,
        "target_id": payload.target_id,
        "payload": payload.payload.unwrap_or(json!({})),
        "priority": payload.priority.unwrap_or_else(|| "normal".into()),
        "status": "unread",
    });

    let res = state.http.post(format!("{}/rest/v1/user_events", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "event_id": data.get("id").and_then(|v| v.as_str()),
        "message": "Event emitted"
    }))
}

// ─── Unread Count ────────────────────────────────────

async fn unread_event_count(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!(
        "{}/rest/v1/user_events?user_id=eq.{uid}&status=eq.unread&select=id",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let events: Vec<Value> = res.json().await.unwrap_or_default();

    ok_json(json!({ "unread_count": events.len() }))
}

// ─── Acknowledge Event ───────────────────────────────

async fn acknowledge_event(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(event_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let res = state.http.patch(format!(
        "{}/rest/v1/user_events?id=eq.{event_id}&user_id=eq.{uid}",
        state.rest_url()
    ))
    .headers(state.supabase_headers())
    .json(&json!({
        "status": "read",
        "read_at": chrono::Utc::now().to_rfc3339(),
    }))
    .send().await
    .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to acknowledge event".into()));
    }

    ok_message("Event acknowledged")
}

// ─── Event Detail ────────────────────────────────────

async fn get_event_detail(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(event_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!(
        "{}/rest/v1/user_events?id=eq.{event_id}&user_id=eq.{uid}",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let events: Vec<Value> = res.json().await.unwrap_or_default();
    let event = events.first().ok_or(ApiError::NotFound("Event not found".into()))?;
    ok_json(event.clone())
}

// ─── User Activity ───────────────────────────────────

async fn list_user_activity(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<ListActivityQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let limit = query.limit.unwrap_or(50);
    let url = format!(
        "{}/rest/v1/user_activity?user_id=eq.{uid}&order=created_at.desc&limit={limit}",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let activity: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "activity": activity, "total": activity.len() }))
}

// ─── Event Types Catalog ─────────────────────────────

async fn list_event_types() -> ApiResult {
    let types = json!([
        { "type": "profile.viewed", "description": "Your profile was viewed", "category": "profile" },
        { "type": "profile.commented", "description": "Someone commented on your profile", "category": "profile" },
        { "type": "connection.requested", "description": "New connection request", "category": "connections" },
        { "type": "connection.accepted", "description": "Connection request accepted", "category": "connections" },
        { "type": "connection.rejected", "description": "Connection request rejected", "category": "connections" },
        { "type": "message.received", "description": "New message received", "category": "messages" },
        { "type": "message.read", "description": "Message was read", "category": "messages" },
        { "type": "post.published", "description": "Your post was published", "category": "content" },
        { "type": "post.reacted", "description": "Someone reacted to your post", "category": "content" },
        { "type": "post.commented", "description": "New comment on your post", "category": "content" },
        { "type": "post.reported", "description": "Your post was reported", "category": "moderation" },
        { "type": "follow.new_follower", "description": "New follower", "category": "connections" },
        { "type": "endorsement.received", "description": "New endorsement received", "category": "trust" },
        { "type": "endorsement.requested", "description": "Endorsement request", "category": "trust" },
        { "type": "verification.completed", "description": "Verification completed", "category": "trust" },
        { "type": "organization.invited", "description": "Organization invitation", "category": "organization" },
        { "type": "organization.joined", "description": "Joined an organization", "category": "organization" },
        { "type": "admin.system_announcement", "description": "System announcement", "category": "admin" },
        { "type": "admin.account_warning", "description": "Account warning", "category": "admin" },
        { "type": "consent.requested", "description": "Consent request", "category": "privacy" },
        { "type": "consent.granted", "description": "Consent granted", "category": "privacy" },
        { "type": "consent.revoked", "description": "Consent revoked", "category": "privacy" },
        { "type": "security.login_new_device", "description": "Login from new device", "category": "security" },
        { "type": "security.password_changed", "description": "Password changed", "category": "security" },
        { "type": "security.2fa_enabled", "description": "Two-factor authentication enabled", "category": "security" },
        { "type": "job.completed", "description": "Background job completed", "category": "system" },
        { "type": "job.failed", "description": "Background job failed", "category": "system" },
    ]);

    ok_json(json!({ "event_types": types, "total": types.as_array().map_or(0, |a| a.len()) }))
}
