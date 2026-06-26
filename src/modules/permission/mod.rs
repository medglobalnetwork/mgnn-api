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

// ──────────────────────────────────────────────
// Permission Engine  (RBAC)
// Roles, permissions, role assignments
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Roles
        .route("/roles", get(get_roles).post(create_role))
        .route("/roles/:id", get(get_role_detail).put(update_role).delete(delete_role))
        .route("/roles/:id/permissions", get(get_role_permissions).put(assign_role_permissions))
        // Permissions
        .route("/permissions", get(list_permissions).post(create_permission))
        .route("/permissions/:id", delete(delete_permission))
        // User role assignments
        .route("/user-roles", get(get_user_roles).post(assign_user_role))
        .route("/user-roles/:id", delete(revoke_user_role))
        // Feature flags (public evaluation)
        .route("/feature-flags", get(list_client_flags))
        .route("/feature-flags/:name/evaluate", post(evaluate_flag))
}

#[derive(Deserialize)]
pub struct CreateRolePayload {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRolePayload {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct AssignPermissionsPayload {
    pub permission_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct CreatePermissionPayload {
    pub name: String,
    pub description: Option<String>,
    pub resource: String,
    pub action: String,
}

#[derive(Deserialize)]
pub struct AssignUserRolePayload {
    pub user_id: String,
    pub role_id: String,
}

#[derive(Deserialize)]
pub struct EvaluateFlagPayload {
    pub user_id: Option<String>,
    #[allow(dead_code)]
    pub role: Option<String>,
}

// ─── Roles ─────────────────────────────────────────────

async fn get_roles(
    State(state): State<SharedState>,
) -> ApiResult {
    let url = format!("{}/rest/v1/roles?order=name.asc", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let roles: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "roles": roles }))
}

async fn create_role(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<CreateRolePayload>,
) -> ApiResult {
    let body = json!({
        "name": payload.name,
        "description": payload.description,
        "created_by": auth.claims.profile_id
    });
    let res = state.http.post(format!("{}/rest/v1/roles", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "role_id": data.get("id").and_then(|v| v.as_str()), "message": "Role created" }))
}

async fn get_role_detail(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult {
    let url = format!("{}/rest/v1/roles?id=eq.{id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let roles: Vec<Value> = res.json().await.unwrap_or_default();
    let role = roles.first().ok_or(ApiError::NotFound("Role not found".into()))?;
    ok_json(role.clone())
}

async fn update_role(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateRolePayload>,
) -> ApiResult {
    let mut body = serde_json::Map::new();
    if let Some(name) = payload.name { body.insert("name".into(), json!(name)); }
    if let Some(desc) = payload.description { body.insert("description".into(), json!(desc)); }
    if body.is_empty() { return Err(ApiError::BadRequest("No fields to update".into())); }

    let res = state.http.patch(format!("{}/rest/v1/roles?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!(body))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to update role".into())); }
    ok_message("Role updated")
}

async fn delete_role(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult {
    let res = state.http.delete(format!("{}/rest/v1/roles?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to delete role".into())); }
    ok_message("Role deleted")
}

// ─── Role Permissions ──────────────────────────────────

async fn get_role_permissions(
    State(state): State<SharedState>,
    Path(role_id): Path<String>,
) -> ApiResult {
    let url = format!(
        "{}/rest/v1/role_permissions?role_id=eq.{role_id}&select=*,permissions(*)",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let perms: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "permissions": perms }))
}

async fn assign_role_permissions(
    State(state): State<SharedState>,
    Path(role_id): Path<String>,
    Json(payload): Json<AssignPermissionsPayload>,
) -> ApiResult {
    // Delete existing then insert new
    let _ = state.http.delete(format!("{}/rest/v1/role_permissions?role_id=eq.{role_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;

    for perm_id in &payload.permission_ids {
        let _ = state.http.post(format!("{}/rest/v1/role_permissions", state.rest_url()))
            .headers(state.supabase_headers())
            .json(&json!({ "role_id": role_id, "permission_id": perm_id }))
            .send().await;
    }
    ok_message("Role permissions updated")
}

// ─── Permissions CRUD ──────────────────────────────────

async fn list_permissions(
    State(state): State<SharedState>,
) -> ApiResult {
    let url = format!("{}/rest/v1/permissions?order=resource.asc,action.asc", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let perms: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "permissions": perms }))
}

async fn create_permission(
    State(state): State<SharedState>,
    Json(payload): Json<CreatePermissionPayload>,
) -> ApiResult {
    let body = json!({
        "name": payload.name,
        "description": payload.description,
        "resource": payload.resource,
        "action": payload.action
    });
    let res = state.http.post(format!("{}/rest/v1/permissions", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "permission_id": data.get("id").and_then(|v| v.as_str()), "message": "Permission created" }))
}

async fn delete_permission(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult {
    let res = state.http.delete(format!("{}/rest/v1/permissions?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to delete permission".into())); }
    ok_message("Permission deleted")
}

// ─── User Role Assignments ─────────────────────────────

async fn get_user_roles(
    State(state): State<SharedState>,
) -> ApiResult {
    let url = format!("{}/rest/v1/user_roles?select=*,roles(*)", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let user_roles: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "user_roles": user_roles }))
}

async fn assign_user_role(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<AssignUserRolePayload>,
) -> ApiResult {
    let body = json!({
        "user_id": payload.user_id,
        "role_id": payload.role_id,
        "assigned_by": auth.claims.profile_id
    });
    let res = state.http.post(format!("{}/rest/v1/user_roles", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to assign role".into())); }
    ok_message("Role assigned to user")
}

async fn revoke_user_role(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult {
    let res = state.http.delete(format!("{}/rest/v1/user_roles?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to revoke role".into())); }
    ok_message("Role revoked from user")
}

// ─── Client Feature Flag Evaluation ────────────────────

async fn list_client_flags(
    State(state): State<SharedState>,
) -> ApiResult {
    let url = format!("{}/rest/v1/feature_flags?enabled=eq.true&select=name,description,rollout_percentage", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let flags: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "flags": flags }))
}

async fn evaluate_flag(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Json(payload): Json<EvaluateFlagPayload>,
) -> ApiResult {
    let url = format!("{}/rest/v1/feature_flags?name=eq.{name}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let flags: Vec<Value> = res.json().await.unwrap_or_default();
    let flag = flags.first().ok_or(ApiError::NotFound("Feature flag not found".into()))?;

    let enabled = flag.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let rollout = flag.get("rollout_percentage").and_then(|v| v.as_f64()).unwrap_or(0.0);

    // Simple rollout evaluation
    let allowed = if !enabled {
        false
    } else if rollout >= 100.0 {
        true
    } else if rollout <= 0.0 {
        false
    } else {
        // Deterministic hash-based rollout
        let user_id = payload.user_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let hash = user_id.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        ((hash % 100) as f64) < rollout
    };

    ok_json(json!({
        "name": name,
        "enabled": allowed,
        "rollout_percentage": rollout
    }))
}
