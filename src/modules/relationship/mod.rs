use axum::{
    Router,
    routing::{get, post, delete},
    Json,
    extract::{State, Path},
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json, ok_message};
use crate::extractors::AuthUser;

// ─── Request Bodies ───────────────────────────────────────

#[derive(Deserialize)]
pub struct SendRequestPayload {
    pub addressee_id: String,
}

#[derive(Deserialize)]
pub struct FollowPayload {
    pub following_id: String,
}

#[derive(Deserialize)]
pub struct BlockPayload {
    pub blocked_id: String,
}

#[derive(Deserialize)]
pub struct CreateCirclePayload {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct AddCircleMemberPayload {
    pub member_id: String,
}

// ─── Router ───────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/summary", get(get_summary))
        .route("/connections", get(get_connections))
        .route("/followers", get(get_followers))
        .route("/following", get(get_following))
        .route("/requests/incoming", get(get_incoming_requests))
        .route("/requests/sent", get(get_sent_requests))
        .route("/request", post(send_request))
        .route("/request/:id/accept", post(accept_request))
        .route("/request/:id/decline", post(decline_request))
        .route("/follow", post(follow_user))
        .route("/follow/:user_id", delete(unfollow_user))
        .route("/block", post(block_user))
        .route("/block/:user_id", delete(unblock_user))
        .route("/suggestions", get(get_suggestions))
        .route("/circles", get(get_circles).post(create_circle))
        .route("/circles/:id/members", post(add_circle_member))
        .route("/mutual/:user_id", get(get_mutual_connections))
}

// ─── Helpers ──────────────────────────────────────────────

/// Count rows matching a simple filter.
async fn count_rows(
    state: &SharedState,
    table: &str,
    filter: &str,
) -> Result<i64, ApiError> {
    let url = format!(
        "{}/rest/v1/{table}?{filter}&select=id",
        state.rest_url()
    );
    let res = state
        .http
        .head(&url)
        .headers(state.supabase_headers())
        .header("Prefer", "count=exact,head=true")
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if let Some(range) = res.headers().get("content-range") {
        let range_str = range.to_str().unwrap_or("");
        if let Some(count_str) = range_str.split('/').last() {
            return count_str.parse::<i64>().map_err(|_| ApiError::Internal("Invalid count".into()));
        }
    }
    Ok(0)
}

/// Check if a connection (any status) exists between two users.
async fn connection_exists(
    state: &SharedState,
    user_a: &str,
    user_b: &str,
) -> Result<bool, ApiError> {
    let url = format!(
        "{}/rest/v1/connections?or=(and(requester_id.eq.{user_a},addressee_id.eq.{user_b}),and(requester_id.eq.{user_b},addressee_id.eq.{user_a}))&select=id&limit=1",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let rows: Vec<Value> = res
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    Ok(!rows.is_empty())
}

/// Check if user_a has blocked user_b.
async fn is_blocked(
    state: &SharedState,
    blocker: &str,
    blocked: &str,
) -> Result<bool, ApiError> {
    let url = format!(
        "{}/rest/v1/blocks?blocker_id=eq.{blocker}&blocked_id=eq.{blocked}&select=id&limit=1",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let rows: Vec<Value> = res
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    Ok(!rows.is_empty())
}

// ─── Handlers ─────────────────────────────────────────────

/// GET /network/summary — Connection stats for current user.
async fn get_summary(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let connections = count_rows(&state, "connections", &format!(
        "status=eq.accepted&or=(requester_id.eq.{uid},addressee_id.eq.{uid})"
    )).await?;

    let followers = count_rows(&state, "follows", &format!("following_id.eq.{uid}")).await?;
    let following = count_rows(&state, "follows", &format!("follower_id.eq.{uid}")).await?;
    let pending = count_rows(&state, "connections", &format!(
        "status=eq.pending&addressee_id=eq.{uid}"
    )).await?;
    let blocked = count_rows(&state, "blocks", &format!("blocker_id=eq.{uid}")).await?;

    ok_json(json!({
        "connections": connections,
        "followers": followers,
        "following": following,
        "pending_requests": pending,
        "blocked": blocked
    }))
}

/// GET /network/connections — All accepted connections.
async fn get_connections(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Fetch connection rows
    let url = format!(
        "{}/rest/v1/connections?status=eq.accepted&or=(requester_id.eq.{uid},addressee_id.eq.{uid})&select=*",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let rows: Vec<Value> = res
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Collect the "other" user IDs
    let other_ids: Vec<String> = rows
        .iter()
        .filter_map(|r| {
            let req = r.get("requester_id")?.as_str()?;
            let add = r.get("addressee_id")?.as_str()?;
            if req == uid { Some(add.to_string()) } else { Some(req.to_string()) }
        })
        .collect();

    if other_ids.is_empty() {
        return ok_json(json!([]));
    }

    let ids_filter = other_ids.iter().map(|id| format!("id.eq.{id}")).collect::<Vec<_>>().join(",");
    let profiles_url = format!(
        "{}/rest/v1/profiles?or=({ids_filter})&select=id,full_name,avatar_url,headline,primary_category",
        state.rest_url()
    );
    let profiles_res = state
        .http
        .get(&profiles_url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let profiles: Vec<Value> = profiles_res
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!(profiles))
}

/// GET /network/followers — Users following current user.
async fn get_followers(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/follows?following_id=eq.{uid}&select=follower_id",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let rows: Vec<Value> = res
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let ids: Vec<&str> = rows.iter()
        .filter_map(|r| r.get("follower_id")?.as_str())
        .collect();

    if ids.is_empty() {
        return ok_json(json!([]));
    }

    let or_filter = ids.iter().map(|id| format!("id.eq.{id}")).collect::<Vec<_>>().join(",");
    let profiles_url = format!(
        "{}/rest/v1/profiles?or=({or_filter})&select=id,full_name,avatar_url,headline,primary_category",
        state.rest_url()
    );
    let profiles_res = state
        .http
        .get(&profiles_url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let profiles: Vec<Value> = profiles_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!(profiles))
}

/// GET /network/following — Users current user follows.
async fn get_following(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/follows?follower_id=eq.{uid}&select=following_id",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let rows: Vec<Value> = res
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let ids: Vec<&str> = rows.iter()
        .filter_map(|r| r.get("following_id")?.as_str())
        .collect();

    if ids.is_empty() {
        return ok_json(json!([]));
    }

    let or_filter = ids.iter().map(|id| format!("id.eq.{id}")).collect::<Vec<_>>().join(",");
    let profiles_url = format!(
        "{}/rest/v1/profiles?or=({or_filter})&select=id,full_name,avatar_url,headline,primary_category",
        state.rest_url()
    );
    let profiles_res = state
        .http
        .get(&profiles_url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let profiles: Vec<Value> = profiles_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!(profiles))
}

/// GET /network/requests/incoming — Pending requests to current user.
async fn get_incoming_requests(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/connections?status=eq.pending&addressee_id=eq.{uid}&select=*,profiles:requester_id(id,full_name,avatar_url,headline,primary_category)",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let rows: Vec<Value> = res
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!(rows))
}

/// GET /network/requests/sent — Pending requests from current user.
async fn get_sent_requests(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/connections?status=eq.pending&requester_id=eq.{uid}&select=*,profiles:addressee_id(id,full_name,avatar_url,headline,primary_category)",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let rows: Vec<Value> = res
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!(rows))
}

/// POST /network/request — Send a connection request.
async fn send_request(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<SendRequestPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Cannot connect to yourself
    if *uid == payload.addressee_id {
        return Err(ApiError::BadRequest("Cannot send request to yourself".into()));
    }

    // Check if blocked
    if is_blocked(&state, &payload.addressee_id, uid).await? {
        return Err(ApiError::Forbidden("This user has blocked you".into()));
    }

    // Check if blocked by you
    if is_blocked(&state, uid, &payload.addressee_id).await? {
        return Err(ApiError::BadRequest("You have blocked this user".into()));
    }

    // Check if any connection already exists
    if connection_exists(&state, uid, &payload.addressee_id).await? {
        return Err(ApiError::Conflict("A connection request already exists".into()));
    }

    let body = json!({
        "requester_id": uid,
        "addressee_id": payload.addressee_id,
        "status": "pending"
    });

    let url = format!("{}/rest/v1/connections", state.rest_url());
    let res = state
        .http
        .post(&url)
        .headers(state.supabase_headers())
        .header("Prefer", "return=representation")
        .json(&body)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!("Failed to send request: {err}")));
    }

    let row: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "id": row.first().and_then(|r| r.get("id")),
        "message": "Connection request sent"
    }))
}

/// POST /network/request/:id/accept — Accept a connection request.
async fn accept_request(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(request_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Fetch the request
    let url = format!(
        "{}/rest/v1/connections?id=eq.{request_id}&select=*",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let rows: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let row = rows.first().ok_or_else(|| ApiError::NotFound("Request not found".into()))?;

    if row.get("status").and_then(|v| v.as_str()) != Some("pending") {
        return Err(ApiError::BadRequest("Request is not pending".into()));
    }

    if row.get("addressee_id").and_then(|v| v.as_str()) != Some(uid) {
        return Err(ApiError::Forbidden("Not your request to accept".into()));
    }

    let patch_url = format!("{}/rest/v1/connections?id=eq.{request_id}", state.rest_url());
    let patch_res = state
        .http
        .patch(&patch_url)
        .headers(state.supabase_headers())
        .json(&json!({ "status": "accepted" }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !patch_res.status().is_success() {
        return Err(ApiError::Internal("Failed to accept request".into()));
    }

    ok_message("Connection request accepted")
}

/// POST /network/request/:id/decline — Decline a connection request.
async fn decline_request(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(request_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/connections?id=eq.{request_id}&select=*",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let rows: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let row = rows.first().ok_or_else(|| ApiError::NotFound("Request not found".into()))?;

    if row.get("status").and_then(|v| v.as_str()) != Some("pending") {
        return Err(ApiError::BadRequest("Request is not pending".into()));
    }

    if row.get("addressee_id").and_then(|v| v.as_str()) != Some(uid) {
        return Err(ApiError::Forbidden("Not your request to decline".into()));
    }

    let patch_url = format!("{}/rest/v1/connections?id=eq.{request_id}", state.rest_url());
    let patch_res = state
        .http
        .patch(&patch_url)
        .headers(state.supabase_headers())
        .json(&json!({ "status": "declined" }))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !patch_res.status().is_success() {
        return Err(ApiError::Internal("Failed to decline request".into()));
    }

    ok_message("Connection request declined")
}

/// POST /network/follow — Follow a user.
async fn follow_user(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<FollowPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if *uid == payload.following_id {
        return Err(ApiError::BadRequest("Cannot follow yourself".into()));
    }

    // Check if already following
    let check_url = format!(
        "{}/rest/v1/follows?follower_id=eq.{uid}&following_id=eq.{}&select=id&limit=1",
        state.rest_url(), payload.following_id
    );
    let check_res = state
        .http
        .get(&check_url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let existing: Vec<Value> = check_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    if !existing.is_empty() {
        return Err(ApiError::Conflict("Already following this user".into()));
    }

    let body = json!({
        "follower_id": uid,
        "following_id": payload.following_id
    });

    let url = format!("{}/rest/v1/follows", state.rest_url());
    let res = state
        .http
        .post(&url)
        .headers(state.supabase_headers())
        .json(&body)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!("Failed to follow: {err}")));
    }

    ok_message("User followed")
}

/// DELETE /network/follow/:user_id — Unfollow a user.
async fn unfollow_user(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/follows?follower_id=eq.{uid}&following_id=eq.{user_id}",
        state.rest_url()
    );
    let res = state
        .http
        .delete(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to unfollow".into()));
    }

    ok_message("User unfollowed")
}

/// POST /network/block — Block a user.
async fn block_user(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<BlockPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if *uid == payload.blocked_id {
        return Err(ApiError::BadRequest("Cannot block yourself".into()));
    }

    // Insert block
    let body = json!({
        "blocker_id": uid,
        "blocked_id": payload.blocked_id
    });
    let url = format!("{}/rest/v1/blocks", state.rest_url());
    let res = state
        .http
        .post(&url)
        .headers(state.supabase_headers())
        .json(&body)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!("Failed to block: {err}")));
    }

    // Remove any existing connection
    let del_conn_url = format!(
        "{}/rest/v1/connections?or=(and(requester_id.eq.{uid},addressee_id.eq.{}),and(requester_id.eq.{},addressee_id.eq.{uid}))",
        state.rest_url(), payload.blocked_id, payload.blocked_id
    );
    let _ = state
        .http
        .delete(&del_conn_url)
        .headers(state.supabase_headers())
        .send()
        .await;

    // Remove any follow relationships
    let del_follow1 = format!(
        "{}/rest/v1/follows?follower_id=eq.{uid}&following_id=eq.{}",
        state.rest_url(), payload.blocked_id
    );
    let del_follow2 = format!(
        "{}/rest/v1/follows?follower_id=eq.{}&following_id=eq.{uid}",
        state.rest_url(), payload.blocked_id
    );
    let _ = state.http.delete(&del_follow1).headers(state.supabase_headers()).send().await;
    let _ = state.http.delete(&del_follow2).headers(state.supabase_headers()).send().await;

    ok_message("User blocked")
}

/// DELETE /network/block/:user_id — Unblock a user.
async fn unblock_user(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/blocks?blocker_id=eq.{uid}&blocked_id=eq.{user_id}",
        state.rest_url()
    );
    let res = state
        .http
        .delete(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to unblock".into()));
    }

    ok_message("User unblocked")
}

/// GET /network/suggestions — Suggested profiles to connect with.
async fn get_suggestions(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Get profiles (excluding self), limited to 10
    let url = format!(
        "{}/rest/v1/profiles?id=neq.{uid}&select=id,full_name,avatar_url,headline,primary_category,city&limit=20",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let all_profiles: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    if all_profiles.is_empty() {
        return ok_json(json!([]));
    }

    // Collect IDs we already have relationships with
    let mut exclude_ids = vec![uid.to_string()];

    // Existing connections
    let conn_url = format!(
        "{}/rest/v1/connections?or=(requester_id.eq.{uid},addressee_id.eq.{uid})&select=requester_id,addressee_id",
        state.rest_url()
    );
    if let Ok(conn_res) = state.http.get(&conn_url).headers(state.supabase_headers()).send().await {
        if let Ok(conn_rows) = conn_res.json::<Vec<Value>>().await {
            for row in conn_rows {
                if let Some(id) = row.get("requester_id").and_then(|v| v.as_str()) {
                    exclude_ids.push(id.to_string());
                }
                if let Some(id) = row.get("addressee_id").and_then(|v| v.as_str()) {
                    exclude_ids.push(id.to_string());
                }
            }
        }
    }

    // Blocked users
    let block_url = format!(
        "{}/rest/v1/blocks?blocker_id=eq.{uid}&select=blocked_id",
        state.rest_url()
    );
    if let Ok(blk_res) = state.http.get(&block_url).headers(state.supabase_headers()).send().await {
        if let Ok(blk_rows) = blk_res.json::<Vec<Value>>().await {
            for row in blk_rows {
                if let Some(id) = row.get("blocked_id").and_then(|v| v.as_str()) {
                    exclude_ids.push(id.to_string());
                }
            }
        }
    }

    // Filter out excluded IDs
    let suggestions: Vec<&Value> = all_profiles
        .iter()
        .filter(|p| {
            let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
            !exclude_ids.iter().any(|e| e == id)
        })
        .take(10)
        .collect();

    ok_json(json!(suggestions))
}

/// GET /network/circles — User's circles with member counts.
async fn get_circles(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/circles?owner_id=eq.{uid}&select=*",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let circles: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // For each circle, get member count
    let mut result = Vec::new();
    for circle in &circles {
        let circle_id = circle.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let member_count = count_rows(&state, "circle_members", &format!("circle_id.eq.{circle_id}")).await?;
        let mut enriched = circle.clone();
        if let Some(obj) = enriched.as_object_mut() {
            obj.insert("member_count".into(), json!(member_count));
        }
        result.push(enriched);
    }

    ok_json(json!(result))
}

/// POST /network/circles — Create a circle.
async fn create_circle(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<CreateCirclePayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let body = json!({
        "owner_id": uid,
        "name": payload.name,
        "description": payload.description.unwrap_or_default(),
        "is_default": false
    });

    let url = format!("{}/rest/v1/circles", state.rest_url());
    let res = state
        .http
        .post(&url)
        .headers(state.supabase_headers())
        .header("Prefer", "return=representation")
        .json(&body)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!("Failed to create circle: {err}")));
    }

    let rows: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "id": rows.first().and_then(|r| r.get("id")),
        "message": "Circle created"
    }))
}

/// POST /network/circles/:id/members — Add member to circle.
async fn add_circle_member(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(circle_id): Path<String>,
    Json(payload): Json<AddCircleMemberPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify user owns the circle
    let check_url = format!(
        "{}/rest/v1/circles?id=eq.{circle_id}&owner_id=eq.{uid}&select=id",
        state.rest_url()
    );
    let check_res = state
        .http
        .get(&check_url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let existing: Vec<Value> = check_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    if existing.is_empty() {
        return Err(ApiError::NotFound("Circle not found or not owned by you".into()));
    }

    let body = json!({
        "circle_id": circle_id,
        "member_id": payload.member_id
    });

    let url = format!("{}/rest/v1/circle_members", state.rest_url());
    let res = state
        .http
        .post(&url)
        .headers(state.supabase_headers())
        .json(&body)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!("Failed to add member: {err}")));
    }

    ok_message("Member added to circle")
}

/// GET /network/mutual/:user_id — Mutual connections between current user and another.
async fn get_mutual_connections(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Get current user's connections
    let my_conn_url = format!(
        "{}/rest/v1/connections?status=eq.accepted&or=(requester_id.eq.{uid},addressee_id.eq.{uid})&select=requester_id,addressee_id",
        state.rest_url()
    );
    let my_res = state
        .http
        .get(&my_conn_url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let my_rows: Vec<Value> = my_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let my_connections: Vec<String> = my_rows.iter().filter_map(|r| {
        let req = r.get("requester_id")?.as_str()?;
        let add = r.get("addressee_id")?.as_str()?;
        if req == uid { Some(add.to_string()) } else { Some(req.to_string()) }
    }).collect();

    // Get other user's connections
    let other_conn_url = format!(
        "{}/rest/v1/connections?status=eq.accepted&or=(requester_id.eq.{user_id},addressee_id.eq.{user_id})&select=requester_id,addressee_id",
        state.rest_url()
    );
    let other_res = state
        .http
        .get(&other_conn_url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let other_rows: Vec<Value> = other_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let other_connections: Vec<String> = other_rows.iter().filter_map(|r| {
        let req = r.get("requester_id")?.as_str()?;
        let add = r.get("addressee_id")?.as_str()?;
        if req == user_id { Some(add.to_string()) } else { Some(req.to_string()) }
    }).collect();

    // Find mutuals
    let mutual_ids: Vec<String> = my_connections.iter()
        .filter(|id| other_connections.contains(id))
        .cloned()
        .collect();

    if mutual_ids.is_empty() {
        return ok_json(json!({
            "mutual_count": 0,
            "profiles": []
        }));
    }

    // Fetch profiles
    let or_filter = mutual_ids.iter().map(|id| format!("id.eq.{id}")).collect::<Vec<_>>().join(",");
    let profiles_url = format!(
        "{}/rest/v1/profiles?or=({or_filter})&select=id,full_name,avatar_url,headline,primary_category",
        state.rest_url()
    );
    let profiles_res = state
        .http
        .get(&profiles_url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let profiles: Vec<Value> = profiles_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "mutual_count": profiles.len(),
        "profiles": profiles
    }))
}
