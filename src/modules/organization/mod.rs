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
// Organization Engine  (Screens 85–92)
// Org profiles, teams, members, invites,
// and organization analytics
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Screen 85 – Organization List & Create
        .route("/organizations", get(get_organizations).post(create_organization))
        // Screen 86 – Organization Detail
        .route("/organizations/:id", get(get_organization_detail).put(update_organization).delete(delete_organization))
        // Screen 87 – Organization Members
        .route("/organizations/:id/members", get(get_org_members).post(add_org_member).delete(remove_org_member))
        // Screen 88 – Organization Invites
        .route("/organizations/:id/invites", get(get_org_invites).post(send_invite))
        .route("/organizations/:id/invites/:invite_id", put(update_invite).delete(cancel_invite))
        .route("/invites/:token/accept", post(accept_invite))
        // Screen 89 – Teams within Organization
        .route("/organizations/:id/teams", get(get_teams).post(create_team))
        .route("/organizations/:id/teams/:team_id", get(get_team_detail).put(update_team).delete(delete_team))
        .route("/organizations/:id/teams/:team_id/members", get(get_team_members).post(add_team_member).delete(remove_team_member))
        // Screen 90 – Organization Analytics
        .route("/organizations/:id/analytics", get(get_org_analytics))
        // Screen 91 – Organization Settings
        .route("/organizations/:id/settings", get(get_org_settings).put(update_org_settings))
        // Screen 92 – My Organizations
        .route("/my-organizations", get(get_my_organizations))
}

// ─── Request Bodies ───────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateOrgPayload {
    pub name: String,
    pub description: Option<String>,
    pub org_type: Option<String>,  // hospital, clinic, research, education, other
    pub website: Option<String>,
    pub logo_url: Option<String>,
    pub location: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateOrgPayload {
    pub name: Option<String>,
    pub description: Option<String>,
    pub website: Option<String>,
    pub logo_url: Option<String>,
    pub location: Option<String>,
}

#[derive(Deserialize)]
pub struct AddMemberPayload {
    pub user_id: String,
    pub role: Option<String>,  // owner, admin, member
    pub department: Option<String>,
    pub title: Option<String>,
}

#[derive(Deserialize)]
pub struct InvitePayload {
    pub email: String,
    pub role: Option<String>,
    pub department: Option<String>,
    pub message: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTeamPayload {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTeamPayload {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct AddTeamMemberPayload {
    pub user_id: String,
    pub role: Option<String>,  // lead, member
}

#[derive(Deserialize)]
pub struct OrgQuery {
    pub org_type: Option<String>,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct UpdateInvitePayload {
    pub status: String,  // accepted, declined
}

// ─── Helpers ─────────────────────────────────────────────

async fn check_org_role(state: &SharedState, org_id: &str, user_id: &str, min_role: &str) -> Result<bool, ApiError> {
    let url = format!(
        "{}/rest/v1/organization_members?organization_id=eq.{org_id}&profile_id=eq.{user_id}&select=role",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let rows: Vec<Value> = res.json().await.unwrap_or_default();

    if let Some(member) = rows.first() {
        let role = member.get("role").and_then(|v| v.as_str()).unwrap_or("");
        return Ok(match min_role {
            "owner" => role == "owner",
            "admin" => role == "owner" || role == "admin",
            _ => true,
        });
    }
    Ok(false)
}

// ─── Screens 85–86: Organizations CRUD ──────────────────

async fn get_organizations(
    State(state): State<SharedState>,
    Query(query): Query<OrgQuery>,
) -> ApiResult {
    let mut url = format!(
        "{}/rest/v1/organizations?order=created_at.desc",
        state.rest_url()
    );

    if let Some(ref org_type) = query.org_type {
        url.push_str(&format!("&org_type=eq.{org_type}"));
    }
    if let Some(ref search) = query.search {
        url.push_str(&format!("&name=ilike.*{search}*"));
    }

    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    url.push_str(&format!("&limit={limit}&offset={offset}"));

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let orgs: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "organizations": orgs,
        "total": orgs.len()
    }))
}

async fn create_organization(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<CreateOrgPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest("Organization name is required".into()));
    }

    let body = json!({
        "name": payload.name,
        "description": payload.description,
        "org_type": payload.org_type.unwrap_or_else(|| "other".to_string()),
        "website": payload.website,
        "logo_url": payload.logo_url,
        "location": payload.location,
        "created_by": uid
    });

    let res = state.http.post(format!("{}/rest/v1/organizations", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    let org_id = data.get("id").and_then(|v| v.as_str()).unwrap_or("");

    // Add creator as owner
    let member_body = json!({
        "organization_id": org_id,
        "profile_id": uid,
        "role": "owner"
    });
    let _ = state.http.post(format!("{}/rest/v1/organization_members", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&member_body)
        .send().await;

    ok_json(json!({
        "organization_id": org_id,
        "message": "Organization created"
    }))
}

async fn get_organization_detail(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult {
    let url = format!("{}/rest/v1/organizations?id=eq.{id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let orgs: Vec<Value> = res.json().await.unwrap_or_default();
    let org = orgs.first().ok_or(ApiError::NotFound("Organization not found".into()))?;

    // Get member count
    let count_url = format!("{}/rest/v1/organization_members?organization_id=eq.{id}&select=id", state.rest_url());
    let member_count = if let Ok(c_res) = state.http.get(&count_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = c_res.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    // Get team count
    let team_count_url = format!("{}/rest/v1/teams?organization_id=eq.{id}&select=id", state.rest_url());
    let team_count = if let Ok(tc_res) = state.http.get(&team_count_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = tc_res.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    let mut result = org.clone();
    result["member_count"] = json!(member_count);
    result["team_count"] = json!(team_count);

    ok_json(result)
}

async fn update_organization(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<UpdateOrgPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only org admins can update the organization".into()));
    }

    let mut body = serde_json::Map::new();
    if let Some(name) = payload.name { body.insert("name".into(), json!(name)); }
    if let Some(desc) = payload.description { body.insert("description".into(), json!(desc)); }
    if let Some(website) = payload.website { body.insert("website".into(), json!(website)); }
    if let Some(logo) = payload.logo_url { body.insert("logo_url".into(), json!(logo)); }
    if let Some(loc) = payload.location { body.insert("location".into(), json!(loc)); }

    if body.is_empty() {
        return Err(ApiError::BadRequest("No fields to update".into()));
    }

    let res = state.http.patch(format!("{}/rest/v1/organizations?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!(body))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update organization".into()));
    }

    ok_message("Organization updated")
}

async fn delete_organization(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "owner").await? {
        return Err(ApiError::Forbidden("Only the organization owner can delete it".into()));
    }

    // Clean up all related data
    let _ = state.http.delete(format!("{}/rest/v1/organization_members?organization_id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let _ = state.http.delete(format!("{}/rest/v1/organization_invites?organization_id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let _ = state.http.delete(format!("{}/rest/v1/teams?organization_id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let res = state.http.delete(format!("{}/rest/v1/organizations?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete organization".into()));
    }

    ok_message("Organization deleted")
}

// ─── Screen 87: Organization Members ────────────────────

async fn get_org_members(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult {
    let url = format!(
        "{}/rest/v1/organization_members?organization_id=eq.{id}&order=joined_at.asc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let members: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Enrich with profile info
    let mut enriched = Vec::new();
    for m in &members {
        let mut member = m.clone();
        if let Some(pid) = m.get("profile_id").and_then(|v| v.as_str()) {
            let p_url = format!(
                "{}/rest/v1/profiles?id=eq.{pid}&select=id,full_name,avatar_url,headline,primary_category",
                state.rest_url()
            );
            if let Ok(p_res) = state.http.get(&p_url).headers(state.supabase_headers()).send().await {
                if let Ok(p_data) = p_res.json::<Vec<Value>>().await {
                    if let Some(p) = p_data.first() {
                        member["profile"] = p.clone();
                    }
                }
            }
        }
        enriched.push(member);
    }

    ok_json(json!({ "members": enriched, "total": enriched.len() }))
}

async fn add_org_member(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<AddMemberPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can add members".into()));
    }

    let role = payload.role.unwrap_or_else(|| "member".to_string());
    let body = json!({
        "organization_id": id,
        "profile_id": payload.user_id,
        "role": role,
        "department": payload.department,
        "title": payload.title
    });

    let res = state.http.post(format!("{}/rest/v1/organization_members", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to add member".into()));
    }

    ok_message("Member added to organization")
}

async fn remove_org_member(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<AddMemberPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Allow self-removal or admin-removal
    if *uid != payload.user_id && !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can remove other members".into()));
    }

    // Prevent removing the owner
    let check_url = format!(
        "{}/rest/v1/organization_members?organization_id=eq.{id}&profile_id=eq.{}&role=eq.owner",
        state.rest_url(), payload.user_id
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await;
    if let Ok(res) = check_res {
        if let Ok(rows) = res.json::<Vec<Value>>().await {
            if !rows.is_empty() && *uid != payload.user_id {
                return Err(ApiError::Forbidden("Cannot remove the organization owner".into()));
            }
        }
    }

    let url = format!(
        "{}/rest/v1/organization_members?organization_id=eq.{id}&profile_id=eq.{}",
        state.rest_url(), payload.user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to remove member".into()));
    }

    ok_message("Member removed from organization")
}

// ─── Screen 88: Organization Invites ────────────────────

async fn get_org_invites(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can view invites".into()));
    }

    let url = format!(
        "{}/rest/v1/organization_invites?organization_id=eq.{id}&order=created_at.desc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let invites: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "invites": invites, "total": invites.len() }))
}

async fn send_invite(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<InvitePayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can send invites".into()));
    }

    // Check for existing pending invite
    let check_url = format!(
        "{}/rest/v1/organization_invites?organization_id=eq.{id}&email=eq.{}&status=eq.pending",
        state.rest_url(), payload.email
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await;
    if let Ok(res) = check_res {
        if let Ok(rows) = res.json::<Vec<Value>>().await {
            if !rows.is_empty() {
                return Err(ApiError::BadRequest("Invite already pending for this email".into()));
            }
        }
    }

    let token = uuid::Uuid::new_v4().to_string();
    let role = payload.role.unwrap_or_else(|| "member".to_string());
    let body = json!({
        "organization_id": id,
        "invited_by": uid,
        "email": payload.email,
        "role": role,
        "department": payload.department,
        "message": payload.message,
        "token": token,
        "status": "pending"
    });

    let res = state.http.post(format!("{}/rest/v1/organization_invites", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "invite_id": data.get("id").and_then(|v| v.as_str()),
        "token": token,
        "message": "Invite sent"
    }))
}

async fn update_invite(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((id, invite_id)): Path<(String, String)>,
    Json(payload): Json<UpdateInvitePayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can manage invites".into()));
    }

    let res = state.http.patch(format!(
        "{}/rest/v1/organization_invites?id=eq.{invite_id}&organization_id=eq.{id}",
        state.rest_url()
    ))
    .headers(state.supabase_headers())
    .json(&json!({ "status": payload.status }))
    .send().await
    .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update invite".into()));
    }

    ok_message("Invite updated")
}

async fn cancel_invite(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((id, invite_id)): Path<(String, String)>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can cancel invites".into()));
    }

    let res = state.http.delete(format!(
        "{}/rest/v1/organization_invites?id=eq.{invite_id}&organization_id=eq.{id}",
        state.rest_url()
    ))
    .headers(state.supabase_headers()).send().await
    .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to cancel invite".into()));
    }

    ok_message("Invite cancelled")
}

async fn accept_invite(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(token): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Find invite by token
    let url = format!(
        "{}/rest/v1/organization_invites?token=eq.{token}&status=eq.pending",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let invites: Vec<Value> = res.json().await.unwrap_or_default();
    let invite = invites.first().ok_or(ApiError::NotFound("Invite not found or already used".into()))?;

    let org_id = invite.get("organization_id").and_then(|v| v.as_str())
        .ok_or(ApiError::Internal("Invite missing organization_id".into()))?;
    let role = invite.get("role").and_then(|v| v.as_str()).unwrap_or("member");
    let invite_id = invite.get("id").and_then(|v| v.as_str()).unwrap_or("");

    // Check not already a member
    let member_check = format!(
        "{}/rest/v1/organization_members?organization_id=eq.{org_id}&profile_id=eq.{uid}&select=id",
        state.rest_url()
    );
    let m_res = state.http.get(&member_check).headers(state.supabase_headers()).send().await;
    if let Ok(r) = m_res {
        if let Ok(rows) = r.json::<Vec<Value>>().await {
            if !rows.is_empty() {
                return Err(ApiError::BadRequest("You are already a member of this organization".into()));
            }
        }
    }

    // Add as member
    let member_body = json!({
        "organization_id": org_id,
        "profile_id": uid,
        "role": role
    });
    let _ = state.http.post(format!("{}/rest/v1/organization_members", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&member_body)
        .send().await;

    // Update invite status
    let _ = state.http.patch(format!(
        "{}/rest/v1/organization_invites?id=eq.{invite_id}",
        state.rest_url()
    ))
    .headers(state.supabase_headers())
    .json(&json!({ "status": "accepted" }))
    .send().await;

    ok_json(json!({
        "organization_id": org_id,
        "message": "Invite accepted. You are now a member."
    }))
}

// ─── Screen 89: Teams ──────────────────────────────────

async fn get_teams(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult {
    let url = format!(
        "{}/rest/v1/teams?organization_id=eq.{id}&order=created_at.desc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let teams: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Enrich with member count
    let mut enriched = Vec::new();
    for t in &teams {
        let mut team = t.clone();
        let team_id = t.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let count_url = format!(
            "{}/rest/v1/team_members?team_id=eq.{team_id}&select=id",
            state.rest_url()
        );
        if let Ok(c_res) = state.http.get(&count_url).headers(state.supabase_headers()).send().await {
            if let Ok(rows) = c_res.json::<Vec<Value>>().await {
                team["member_count"] = json!(rows.len());
            }
        }
        enriched.push(team);
    }

    ok_json(json!({ "teams": enriched, "total": enriched.len() }))
}

async fn create_team(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<CreateTeamPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can create teams".into()));
    }

    let body = json!({
        "organization_id": id,
        "name": payload.name,
        "description": payload.description,
        "created_by": uid
    });

    let res = state.http.post(format!("{}/rest/v1/teams", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let team_id = data.get("id").and_then(|v| v.as_str()).unwrap_or("");

    // Add creator as team lead
    let _ = state.http.post(format!("{}/rest/v1/team_members", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({ "team_id": team_id, "profile_id": uid, "role": "lead" }))
        .send().await;

    ok_json(json!({
        "team_id": team_id,
        "message": "Team created"
    }))
}

async fn get_team_detail(
    State(state): State<SharedState>,
    Path((id, team_id)): Path<(String, String)>,
) -> ApiResult {
    let url = format!("{}/rest/v1/teams?id=eq.{team_id}&organization_id=eq.{id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let teams: Vec<Value> = res.json().await.unwrap_or_default();
    let team = teams.first().ok_or(ApiError::NotFound("Team not found".into()))?;

    let mut result = team.clone();

    // Get member count
    let count_url = format!("{}/rest/v1/team_members?team_id=eq.{team_id}&select=id", state.rest_url());
    let member_count = if let Ok(c_res) = state.http.get(&count_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = c_res.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    result["member_count"] = json!(member_count);
    ok_json(result)
}

async fn update_team(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((id, team_id)): Path<(String, String)>,
    Json(payload): Json<UpdateTeamPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can update teams".into()));
    }

    let mut body = serde_json::Map::new();
    if let Some(name) = payload.name { body.insert("name".into(), json!(name)); }
    if let Some(desc) = payload.description { body.insert("description".into(), json!(desc)); }

    if body.is_empty() {
        return Err(ApiError::BadRequest("No fields to update".into()));
    }

    let res = state.http.patch(format!(
        "{}/rest/v1/teams?id=eq.{team_id}&organization_id=eq.{id}",
        state.rest_url()
    ))
    .headers(state.supabase_headers())
    .json(&json!(body))
    .send().await
    .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update team".into()));
    }

    ok_message("Team updated")
}

async fn delete_team(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((id, team_id)): Path<(String, String)>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can delete teams".into()));
    }

    let _ = state.http.delete(format!("{}/rest/v1/team_members?team_id=eq.{team_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let res = state.http.delete(format!(
        "{}/rest/v1/teams?id=eq.{team_id}&organization_id=eq.{id}",
        state.rest_url()
    ))
    .headers(state.supabase_headers()).send().await
    .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete team".into()));
    }

    ok_message("Team deleted")
}

async fn get_team_members(
    State(state): State<SharedState>,
    Path((_id, team_id)): Path<(String, String)>,
) -> ApiResult {
    let url = format!(
        "{}/rest/v1/team_members?team_id=eq.{team_id}&order=joined_at.asc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let members: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let mut enriched = Vec::new();
    for m in &members {
        let mut member = m.clone();
        if let Some(pid) = m.get("profile_id").and_then(|v| v.as_str()) {
            let p_url = format!(
                "{}/rest/v1/profiles?id=eq.{pid}&select=id,full_name,avatar_url,headline",
                state.rest_url()
            );
            if let Ok(p_res) = state.http.get(&p_url).headers(state.supabase_headers()).send().await {
                if let Ok(p_data) = p_res.json::<Vec<Value>>().await {
                    if let Some(p) = p_data.first() {
                        member["profile"] = p.clone();
                    }
                }
            }
        }
        enriched.push(member);
    }

    ok_json(json!({ "members": enriched, "total": enriched.len() }))
}

async fn add_team_member(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((_id, team_id)): Path<(String, String)>,
    Json(payload): Json<AddTeamMemberPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &_id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can add team members".into()));
    }

    let role = payload.role.unwrap_or_else(|| "member".to_string());
    let body = json!({
        "team_id": team_id,
        "profile_id": payload.user_id,
        "role": role
    });

    let res = state.http.post(format!("{}/rest/v1/team_members", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to add team member".into()));
    }

    ok_message("Member added to team")
}

async fn remove_team_member(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((_id, team_id)): Path<(String, String)>,
    Json(payload): Json<AddTeamMemberPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &_id, uid, "admin").await? && *uid != payload.user_id {
        return Err(ApiError::Forbidden("Only admins can remove team members or self".into()));
    }

    let url = format!(
        "{}/rest/v1/team_members?team_id=eq.{team_id}&profile_id=eq.{}",
        state.rest_url(), payload.user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to remove team member".into()));
    }

    ok_message("Member removed from team")
}

// ─── Screen 90: Organization Analytics ──────────────────

async fn get_org_analytics(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can view analytics".into()));
    }

    // Member count
    let member_url = format!("{}/rest/v1/organization_members?organization_id=eq.{id}&select=id", state.rest_url());
    let member_count = if let Ok(r) = state.http.get(&member_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = r.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    // Team count
    let team_url = format!("{}/rest/v1/teams?organization_id=eq.{id}&select=id", state.rest_url());
    let team_count = if let Ok(r) = state.http.get(&team_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = r.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    // Pending invites
    let invite_url = format!(
        "{}/rest/v1/organization_invites?organization_id=eq.{id}&status=eq.pending&select=id",
        state.rest_url()
    );
    let pending_invites = if let Ok(r) = state.http.get(&invite_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = r.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    ok_json(json!({
        "member_count": member_count,
        "team_count": team_count,
        "pending_invites": pending_invites,
        "engagement_rate": if member_count > 0 { (team_count as f64 / member_count as f64 * 100.0).min(100.0) } else { 0.0 }
    }))
}

// ─── Screen 91: Organization Settings ───────────────────

async fn get_org_settings(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can view settings".into()));
    }

    let url = format!("{}/rest/v1/organizations?id=eq.{id}&select=*,org_settings(*)", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let orgs: Vec<Value> = res.json().await.unwrap_or_default();
    let org = orgs.first().ok_or(ApiError::NotFound("Organization not found".into()))?;

    ok_json(org.clone())
}

async fn update_org_settings(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if !check_org_role(&state, &id, uid, "admin").await? {
        return Err(ApiError::Forbidden("Only admins can update settings".into()));
    }

    // Update org-level settings
    let res = state.http.patch(format!("{}/rest/v1/organizations?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&payload)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update settings".into()));
    }

    ok_message("Organization settings updated")
}

// ─── Screen 92: My Organizations ───────────────────────

async fn get_my_organizations(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/organization_members?profile_id=eq.{uid}&select=*,organizations(*)",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let memberships: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Enrich with org details
    let mut organizations = Vec::new();
    for m in &memberships {
        let mut org_with_role = m.get("organizations").cloned().unwrap_or(json!(null));
        if let Some(role) = m.get("role").and_then(|v| v.as_str()) {
            org_with_role["my_role"] = json!(role);
        }
        if let Some(dept) = m.get("department").and_then(|v| v.as_str()) {
            org_with_role["my_department"] = json!(dept);
        }
        if let Some(title) = m.get("title").and_then(|v| v.as_str()) {
            org_with_role["my_title"] = json!(title);
        }
        organizations.push(org_with_role);
    }

    ok_json(json!({
        "organizations": organizations,
        "total": organizations.len()
    }))
}
