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
// Trust Engine  (Screens 79–84)
// Endorsements, verification badges,
// trust scores, and peer recommendations
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Screen 79 – Trust Score
        .route("/score/:user_id", get(get_trust_score))
        // Screen 80 – Verification Requests
        .route("/verify", post(request_verification))
        .route("/verify/:id", get(get_verification_status).put(update_verification))
        // Screen 81 – Endorsements Received
        .route("/endorsements/received", get(get_received_endorsements))
        // Screen 82 – Endorsements Given
        .route("/endorsements/given", get(get_given_endorsements))
        // Screen 83 – Give / Remove Endorsement
        .route("/endorsements", post(give_endorsement).delete(remove_endorsement))
        // Screen 84 – Verification Badge Status
        .route("/badges/:user_id", get(get_verification_badges))
}

// ─── Request Bodies ───────────────────────────────────────

#[derive(Deserialize)]
pub struct RequestVerificationPayload {
    pub verification_type: String,  // email, phone, identity, credential, employer
    pub proof_url: Option<String>,
    pub notes: Option<String>,
}

#[derive(Deserialize)]
pub struct GiveEndorsementPayload {
    pub endorsed_id: String,
    pub skill: String,
    pub comment: Option<String>,
    pub endorsement_type: Option<String>,  // skill, character, work_ethic, expertise
}

#[derive(Deserialize)]
pub struct EndorsementQuery {
    pub skill: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ─── Screen 79: Trust Score ─────────────────────────────

async fn get_trust_score(
    State(state): State<SharedState>,
    _auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    // Count endorsements received
    let endorse_url = format!(
        "{}/rest/v1/endorsements?endorsed_id=eq.{user_id}&select=id",
        state.rest_url()
    );
    let endorse_res = state.http.get(&endorse_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let endorsements: Vec<Value> = endorse_res.json().await.unwrap_or_default();
    let endorsement_count = endorsements.len() as i64;

    // Count unique endorsers
    let unique_url = format!(
        "{}/rest/v1/endorsements?endorsed_id=eq.{user_id}&select=endorser_id",
        state.rest_url()
    );
    let unique_res = state.http.get(&unique_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let unique_endorsers: Vec<Value> = unique_res.json().await.unwrap_or_default();
    let unique_count = unique_endorsers.iter()
        .filter_map(|e| e.get("endorser_id").and_then(|v| v.as_str()))
        .collect::<std::collections::HashSet<_>>()
        .len() as i64;

    // Count verified badges
    let badge_url = format!(
        "{}/rest/v1/verifications?profile_id=eq.{user_id}&status=eq.verified&select=id",
        state.rest_url()
    );
    let badge_res = state.http.get(&badge_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let badges: Vec<Value> = badge_res.json().await.unwrap_or_default();
    let verification_count = badges.len() as i64;

    // Count connections
    let conn_url = format!(
        "{}/rest/v1/connections?status=eq.accepted&(requester_id=eq.{user_id}|addressee_id=eq.{user_id})&select=id",
        state.rest_url()
    );
    let conn_count = if let Ok(c_res) = state.http.get(&conn_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = c_res.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    // Calculate composite trust score (0-100)
    let base_score = 20.0; // Everyone starts at 20
    let endorse_score = (endorsement_count as f64 * 5.0).min(30.0);
    let unique_score = (unique_count as f64 * 3.0).min(20.0);
    let verification_score = (verification_count as f64 * 10.0).min(20.0);
    let connection_score = (conn_count as f64 * 0.5).min(10.0);
    let trust_score = (base_score + endorse_score + unique_score + verification_score + connection_score).min(100.0);

    // Get skill breakdown
    let skills_url = format!(
        "{}/rest/v1/endorsements?endorsed_id=eq.{user_id}&select=skill,endorsement_type",
        state.rest_url()
    );
    let skills_res = state.http.get(&skills_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let skills_data: Vec<Value> = skills_res.json().await.unwrap_or_default();

    let mut skill_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for s in &skills_data {
        if let Some(skill) = s.get("skill").and_then(|v| v.as_str()) {
            *skill_map.entry(skill.to_string()).or_insert(0) += 1;
        }
    }

    let skill_breakdown: Vec<Value> = skill_map.iter()
        .map(|(k, v)| json!({ "skill": k, "count": v }))
        .collect();

    ok_json(json!({
        "trust_score": trust_score,
        "endorsement_count": endorsement_count,
        "unique_endorsers": unique_count,
        "verification_count": verification_count,
        "connection_count": conn_count,
        "skill_breakdown": skill_breakdown,
        "level": if trust_score >= 80.0 { "expert" } else if trust_score >= 50.0 { "established" } else if trust_score >= 30.0 { "growing" } else { "new" }
    }))
}

// ─── Screen 80: Verification Requests ───────────────────

async fn request_verification(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<RequestVerificationPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Check if already pending
    let check_url = format!(
        "{}/rest/v1/verifications?profile_id=eq.{uid}&verification_type=eq.{}&status=eq.pending",
        state.rest_url(), payload.verification_type
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let existing: Vec<Value> = check_res.json().await.unwrap_or_default();
    if !existing.is_empty() {
        return Err(ApiError::BadRequest("Verification request already pending for this type".into()));
    }

    let body = json!({
        "profile_id": uid,
        "verification_type": payload.verification_type,
        "proof_url": payload.proof_url,
        "notes": payload.notes,
        "status": "pending"
    });

    let res = state.http.post(format!("{}/rest/v1/verifications", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "verification_id": data.get("id").and_then(|v| v.as_str()),
        "status": "pending",
        "message": "Verification request submitted"
    }))
}

async fn get_verification_status(
    State(state): State<SharedState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let url = format!("{}/rest/v1/verifications?id=eq.{id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let rows: Vec<Value> = res.json().await.unwrap_or_default();
    let verification = rows.first().ok_or(ApiError::NotFound("Verification not found".into()))?;

    ok_json(verification.clone())
}

async fn update_verification(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify this is the owner's verification
    let check_url = format!(
        "{}/rest/v1/verifications?id=eq.{id}&profile_id=eq.{uid}",
        state.rest_url()
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let existing: Vec<Value> = check_res.json().await.unwrap_or_default();
    if existing.is_empty() {
        return Err(ApiError::NotFound("Verification not found or not owned by you".into()));
    }

    let mut body = serde_json::Map::new();
    if let Some(notes) = payload.get("notes").and_then(|v| v.as_str()) {
        body.insert("notes".into(), json!(notes));
    }
    if let Some(proof) = payload.get("proof_url").and_then(|v| v.as_str()) {
        body.insert("proof_url".into(), json!(proof));
    }

    if body.is_empty() {
        return Err(ApiError::BadRequest("No fields to update".into()));
    }

    let res = state.http.patch(format!("{}/rest/v1/verifications?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!(body))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update verification".into()));
    }

    ok_message("Verification updated")
}

// ─── Screens 81–82: Endorsements Received / Given ──────

async fn get_received_endorsements(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<EndorsementQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let mut url = format!(
        "{}/rest/v1/endorsements?endorsed_id=eq.{uid}&order=created_at.desc",
        state.rest_url()
    );

    if let Some(ref skill) = query.skill {
        url.push_str(&format!("&skill=eq.{skill}"));
    }

    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    url.push_str(&format!("&limit={limit}&offset={offset}"));

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let endorsements: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Enrich with endorser profiles
    let mut enriched = Vec::new();
    for e in &endorsements {
        let mut endorsed = e.clone();
        if let Some(endorser_id) = e.get("endorser_id").and_then(|v| v.as_str()) {
            let p_url = format!(
                "{}/rest/v1/profiles?id=eq.{endorser_id}&select=id,full_name,avatar_url,headline",
                state.rest_url()
            );
            if let Ok(p_res) = state.http.get(&p_url).headers(state.supabase_headers()).send().await {
                if let Ok(p_data) = p_res.json::<Vec<Value>>().await {
                    if let Some(p) = p_data.first() {
                        endorsed["endorser"] = p.clone();
                    }
                }
            }
        }
        enriched.push(endorsed);
    }

    ok_json(json!({
        "endorsements": enriched,
        "total": enriched.len()
    }))
}

async fn get_given_endorsements(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<EndorsementQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let mut url = format!(
        "{}/rest/v1/endorsements?endorser_id=eq.{uid}&order=created_at.desc",
        state.rest_url()
    );

    if let Some(ref skill) = query.skill {
        url.push_str(&format!("&skill=eq.{skill}"));
    }

    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    url.push_str(&format!("&limit={limit}&offset={offset}"));

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let endorsements: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Enrich with endorsed profiles
    let mut enriched = Vec::new();
    for e in &endorsements {
        let mut endorsed = e.clone();
        if let Some(endorsed_id) = e.get("endorsed_id").and_then(|v| v.as_str()) {
            let p_url = format!(
                "{}/rest/v1/profiles?id=eq.{endorsed_id}&select=id,full_name,avatar_url,headline",
                state.rest_url()
            );
            if let Ok(p_res) = state.http.get(&p_url).headers(state.supabase_headers()).send().await {
                if let Ok(p_data) = p_res.json::<Vec<Value>>().await {
                    if let Some(p) = p_data.first() {
                        endorsed["endorsed_profile"] = p.clone();
                    }
                }
            }
        }
        enriched.push(endorsed);
    }

    ok_json(json!({
        "endorsements": enriched,
        "total": enriched.len()
    }))
}

// ─── Screen 83: Give / Remove Endorsement ──────────────

async fn give_endorsement(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<GiveEndorsementPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if *uid == payload.endorsed_id {
        return Err(ApiError::BadRequest("Cannot endorse yourself".into()));
    }

    if payload.skill.trim().is_empty() {
        return Err(ApiError::BadRequest("Skill name is required".into()));
    }

    // Check for duplicate endorsement
    let check_url = format!(
        "{}/rest/v1/endorsements?endorser_id=eq.{uid}&endorsed_id=eq.{}&skill=eq.{}",
        state.rest_url(), payload.endorsed_id, payload.skill
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let existing: Vec<Value> = check_res.json().await.unwrap_or_default();
    if !existing.is_empty() {
        return Err(ApiError::BadRequest("You have already endorsed this person for this skill".into()));
    }

    let end_type = payload.endorsement_type.unwrap_or_else(|| "skill".to_string());
    let body = json!({
        "endorser_id": uid,
        "endorsed_id": payload.endorsed_id,
        "skill": payload.skill,
        "comment": payload.comment,
        "endorsement_type": end_type
    });

    let res = state.http.post(format!("{}/rest/v1/endorsements", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "endorsement_id": data.get("id").and_then(|v| v.as_str()),
        "message": "Endorsement given"
    }))
}

async fn remove_endorsement(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<Value>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let endorsed_id = payload.get("endorsed_id").and_then(|v| v.as_str())
        .ok_or(ApiError::BadRequest("endorsed_id is required".into()))?;
    let skill = payload.get("skill").and_then(|v| v.as_str())
        .ok_or(ApiError::BadRequest("skill is required".into()))?;

    let url = format!(
        "{}/rest/v1/endorsements?endorser_id=eq.{uid}&endorsed_id=eq.{endorsed_id}&skill=eq.{skill}",
        state.rest_url()
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to remove endorsement".into()));
    }

    ok_message("Endorsement removed")
}

// ─── Screen 84: Verification Badge Status ──────────────

async fn get_verification_badges(
    State(state): State<SharedState>,
    _auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult {
    let url = format!(
        "{}/rest/v1/verifications?profile_id=eq.{user_id}&order=verified_at.desc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let verifications: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let verified: Vec<Value> = verifications.iter()
        .filter(|v| v.get("status").and_then(|s| s.as_str()) == Some("verified"))
        .cloned()
        .collect();

    let pending: Vec<Value> = verifications.iter()
        .filter(|v| v.get("status").and_then(|s| s.as_str()) == Some("pending"))
        .cloned()
        .collect();

    ok_json(json!({
        "verified_badges": verified,
        "pending_verifications": pending,
        "total_verified": verified.len(),
        "total_pending": pending.len(),
        "badge_types": verified.iter()
            .filter_map(|v| v.get("verification_type").and_then(|t| t.as_str()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    }))
}
