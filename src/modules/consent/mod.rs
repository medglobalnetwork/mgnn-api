use axum::{
    Router,
    routing::{get, post, put},
    Json,
    extract::{State, Path},
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json, ok_message};
use crate::extractors::AuthUser;

// ──────────────────────────────────────────────
// Consent Engine  (GDPR / HIPAA)
// Consent tracking, data processing agreements,
// privacy consent, data subject requests
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // User consent management
        .route("/", get(get_my_consents))
        .route("/grant", post(grant_consent))
        .route("/:id/revoke", put(revoke_consent))
        .route("/:id", get(get_consent_detail))
        // Consent history
        .route("/history", get(consent_history))
        // Data processing agreements
        .route("/dpa", get(list_dpa).post(sign_dpa))
        // Data subject requests (GDPR)
        .route("/requests", get(list_data_requests).post(create_data_request))
        .route("/requests/:id", get(get_data_request_detail))
        // Privacy policy
        .route("/privacy-policy", get(get_privacy_policy))
}

#[derive(Deserialize)]
pub struct GrantConsentPayload {
    pub consent_type: String,  // analytics, marketing, data_sharing, research, third_party
    pub purpose: String,
    pub scope: Option<String>,
}

#[derive(Deserialize)]
pub struct RevokeConsentPayload {
    pub reason: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateDataRequestPayload {
    pub request_type: String,  // access, rectification, erasure, portability, restriction
    pub details: Option<String>,
}

#[derive(Deserialize)]
pub struct SignDpaPayload {
    pub dpa_type: String,  // hipaa, gdpr_standard, research
    pub agreed: bool,
}

// ─── User Consent Management ─────────────────────────

async fn get_my_consents(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!(
        "{}/rest/v1/user_consents?user_id=eq.{uid}&order=granted_at.desc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let consents: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "consents": consents, "total": consents.len() }))
}

async fn grant_consent(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<GrantConsentPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Check if already granted and active
    let check_url = format!(
        "{}/rest/v1/user_consents?user_id=eq.{uid}&consent_type=eq.{}&status=eq.granted",
        state.rest_url(), payload.consent_type
    );
    let check = state.http.get(&check_url).headers(state.supabase_headers()).send().await;
    if let Ok(r) = check {
        if let Ok(rows) = r.json::<Vec<Value>>().await {
            if !rows.is_empty() {
                return Err(ApiError::BadRequest("Consent already granted".into()));
            }
        }
    }

    let body = json!({
        "user_id": uid,
        "consent_type": payload.consent_type,
        "purpose": payload.purpose,
        "scope": payload.scope,
        "status": "granted",
        "granted_at": chrono::Utc::now().to_rfc3339(),
        "ip_hash": "anonymous",
    });

    let res = state.http.post(format!("{}/rest/v1/user_consents", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "consent_id": data.get("id").and_then(|v| v.as_str()),
        "message": "Consent granted"
    }))
}

async fn get_consent_detail(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(consent_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!(
        "{}/rest/v1/user_consents?id=eq.{consent_id}&user_id=eq.{uid}",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let consents: Vec<Value> = res.json().await.unwrap_or_default();
    let consent = consents.first().ok_or(ApiError::NotFound("Consent not found".into()))?;
    ok_json(consent.clone())
}

async fn revoke_consent(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(consent_id): Path<String>,
    Json(payload): Json<RevokeConsentPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let res = state.http.patch(format!(
        "{}/rest/v1/user_consents?id=eq.{consent_id}&user_id=eq.{uid}",
        state.rest_url()
    ))
    .headers(state.supabase_headers())
    .json(&json!({
        "status": "revoked",
        "revoked_at": chrono::Utc::now().to_rfc3339(),
        "revoke_reason": payload.reason,
    }))
    .send().await
    .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to revoke consent".into()));
    }

    ok_message("Consent revoked")
}

// ─── Consent History ─────────────────────────────────

async fn consent_history(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!(
        "{}/rest/v1/user_consents?user_id=eq.{uid}&order=granted_at.desc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let history: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "history": history, "total": history.len() }))
}

// ─── Data Processing Agreements ──────────────────────

async fn list_dpa(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!(
        "{}/rest/v1/data_processing_agreements?user_id=eq.{uid}",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let dpas: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "agreements": dpas }))
}

async fn sign_dpa(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<SignDpaPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let body = json!({
        "user_id": uid,
        "dpa_type": payload.dpa_type,
        "agreed": payload.agreed,
        "signed_at": chrono::Utc::now().to_rfc3339(),
    });

    let res = state.http.post(format!("{}/rest/v1/data_processing_agreements", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to sign DPA".into()));
    }

    ok_message("Data processing agreement signed")
}

// ─── Data Subject Requests (GDPR) ───────────────────

async fn list_data_requests(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!(
        "{}/rest/v1/data_subject_requests?user_id=eq.{uid}&order=created_at.desc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let requests: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "requests": requests }))
}

async fn create_data_request(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<CreateDataRequestPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Rate limit: max 1 pending request per type
    let check_url = format!(
        "{}/rest/v1/data_subject_requests?user_id=eq.{uid}&request_type=eq.{}&status=eq.pending",
        state.rest_url(), payload.request_type
    );
    let check = state.http.get(&check_url).headers(state.supabase_headers()).send().await;
    if let Ok(r) = check {
        if let Ok(rows) = r.json::<Vec<Value>>().await {
            if !rows.is_empty() {
                return Err(ApiError::BadRequest("A pending request of this type already exists".into()));
            }
        }
    }

    let body = json!({
        "user_id": uid,
        "request_type": payload.request_type,
        "details": payload.details,
        "status": "pending",
    });

    let res = state.http.post(format!("{}/rest/v1/data_subject_requests", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "request_id": data.get("id").and_then(|v| v.as_str()),
        "message": "Data subject request submitted"
    }))
}

async fn get_data_request_detail(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(request_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!(
        "{}/rest/v1/data_subject_requests?id=eq.{request_id}&user_id=eq.{uid}",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let requests: Vec<Value> = res.json().await.unwrap_or_default();
    let request = requests.first().ok_or(ApiError::NotFound("Request not found".into()))?;
    ok_json(request.clone())
}

// ─── Privacy Policy ──────────────────────────────────

async fn get_privacy_policy(
    State(state): State<SharedState>,
) -> ApiResult {
    let url = format!("{}/rest/v1/platform_config?key=eq.privacy_policy&select=value", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let configs: Vec<Value> = res.json().await.unwrap_or_default();

    let policy = configs.first()
        .and_then(|c| c.get("value"))
        .cloned()
        .unwrap_or(json!({
            "version": "1.0",
            "title": "MGN Privacy Policy",
            "content": "Privacy policy content goes here.",
            "effective_date": "2026-01-01"
        }));

    ok_json(json!({ "policy": policy }))
}
