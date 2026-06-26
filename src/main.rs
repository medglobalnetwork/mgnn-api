use axum::{Router, http::StatusCode};
use axum::extract::{Json, Path};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber;
use tracing::info;
use std::env;

use crate::config::Config;
use crate::error::ApiError;
use crate::state::AppState;

mod modules;
mod router;
mod state;
mod error;
mod config;
mod extractors;

#[derive(Deserialize, Debug)]
struct SyncPayload {
    auth_id: Option<String>,
    firebase_uid: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    full_name: Option<String>,
    provider: String,
}

#[derive(Serialize)]
struct ApiResponse {
    status: String,
    message: String,
}

#[derive(Serialize)]
struct StatsResponse {
    professionals: i64,
    students: i64,
    organizations: i64,
    active_jobs: i64,
    courses: i64,
    new_users_today: i64,
    google_users: i64,
    email_users: i64,
    phone_users: i64,
    active_users: i64,
}

#[derive(Deserialize, Debug)]
struct OnboardingPayload {
    id: String,
    account_type: String,
    primary_category: String, 
    sub_category: String,
    name: String,
    country: String,
    city: String,
    headline: String,
    bio: String,
    interests: Option<Vec<String>>,
    secondary_roles: Option<Vec<String>>,
    profile_score: Option<i32>,
    badge_color: Option<String>,
}

#[derive(Deserialize, Debug)]
struct VerificationSubmitPayload {
    user_id: String,
    document_type: String,
    document_url: String,
}

#[derive(Deserialize, Debug)]
struct AdminApprovalPayload {
    user_id: String,
    badge_color: String,
}

#[tokio::main]
async fn main() -> Result<(), ApiError> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env();
    let state = AppState::new(config);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", axum::routing::get(health_check))
        .route("/auth/sync", axum::routing::post(handle_sync))
        .route("/auth/onboarding", axum::routing::post(handle_onboarding))
        .route("/stats", axum::routing::get(get_stats))
        .route("/verification/submit", axum::routing::post(submit_verification))
        .route("/admin/verifications", axum::routing::get(get_pending_verifications))
        .route("/admin/verifications/approve/:id", axum::routing::post(approve_verification))
        .route("/admin/verifications/reject/:id", axum::routing::post(reject_verification))
        .nest("/v1", router::api_routes())
        .layer(cors)
        .with_state(Arc::new(state));

    let port: u16 = env::var("PORT").unwrap_or_else(|_| "8000".into()).parse().unwrap_or(8000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
    Ok(())
}

async fn health_check() -> &'static str {
    "MGN Rust API is running!"
}

async fn handle_sync(Json(payload): Json<SyncPayload>) -> (StatusCode, Json<ApiResponse>) {
    let supabase_url = env::var("SUPABASE_URL").unwrap_or_default();
    let supabase_key = env::var("SUPABASE_SERVICE_ROLE_KEY").unwrap_or_default();

    if supabase_url.is_empty() || supabase_key.is_empty() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse { status: "error".into(), message: "Server misconfiguration".into() }));
    }

    let client = reqwest::Client::new();
    let mut h = reqwest::header::HeaderMap::new();
    h.insert("apikey", supabase_key.parse().unwrap());
    h.insert("Authorization", format!("Bearer {}", supabase_key).parse().unwrap());
    h.insert("Content-Type", "application/json".parse().unwrap());
    h.insert("Prefer", "resolution=merge-duplicates".parse().unwrap()); 

    // Insert into profiles
    let profile_payload = json!({
        "id": payload.auth_id,
        "auth_id": payload.auth_id,
        "firebase_uid": payload.firebase_uid,
        "email": payload.email,
        "phone": payload.phone,
        "full_name": payload.full_name,
        "provider": payload.provider,
    });

    let on_conflict = if payload.auth_id.is_some() { "auth_id" } else { "firebase_uid" };

    let res = client.post(format!("{}/rest/v1/profiles?on_conflict={}", supabase_url, on_conflict))
        .headers(h)
        .json(&profile_payload)
        .send()
        .await;

    match res {
        Ok(response) => {
            if response.status().is_success() {
                (StatusCode::OK, Json(ApiResponse { status: "success".into(), message: "Profile synced".into() }))
            } else {
                let err = response.text().await.unwrap_or_default();
                tracing::error!("Sync error: {}", err);
                (StatusCode::BAD_REQUEST, Json(ApiResponse { status: "error".into(), message: "Failed to sync profile".into() }))
            }
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse { status: "error".into(), message: "Database connection failed".into() }))
    }
}

async fn handle_onboarding(Json(payload): Json<OnboardingPayload>) -> (StatusCode, Json<ApiResponse>) {
    let supabase_url = env::var("SUPABASE_URL").unwrap_or_default();
    let supabase_key = env::var("SUPABASE_SERVICE_ROLE_KEY").unwrap_or_default();
    let client = reqwest::Client::new();
    let mut h = reqwest::header::HeaderMap::new();
    h.insert("apikey", supabase_key.parse().unwrap());
    h.insert("Authorization", format!("Bearer {}", supabase_key).parse().unwrap());
    h.insert("Content-Type", "application/json".parse().unwrap());
    h.insert("Prefer", "resolution=merge-duplicates".parse().unwrap()); 

    // Update profile
    let profile_payload = json!({
        "full_name": payload.name,
        "bio": payload.bio,
        "city": payload.city,
        "country": payload.country,
        "headline": payload.headline,
        "interests": payload.interests.unwrap_or_default(),
        "account_type": payload.account_type,
        "primary_category": payload.primary_category,
        "sub_category": payload.sub_category,
        "subcategory": payload.sub_category,
        "secondary_roles": payload.secondary_roles.unwrap_or_default(),
        "completion_score": payload.profile_score.unwrap_or(0),
        "badge_color": payload.badge_color.unwrap_or_else(|| "gray".to_string()),
        "profile_completed": true,
        "onboarding_score": 100
    });

    let _ = client.patch(format!("{}/rest/v1/profiles?id=eq.{}", supabase_url, payload.id))
        .headers(h.clone())
        .json(&profile_payload)
        .send()
        .await;

    // Insert into professional_identities
    let identity_payload = json!({
        "user_id": payload.id,
        "identity_type": payload.primary_category
    });

    let _ = client.post(format!("{}/rest/v1/professional_identities", supabase_url))
        .headers(h.clone())
        .json(&identity_payload)
        .send()
        .await;

    (StatusCode::OK, Json(ApiResponse { status: "success".into(), message: "Onboarding completed".into() }))
}

async fn get_stats() -> (StatusCode, Json<StatsResponse>) {
    let supabase_url = env::var("SUPABASE_URL").unwrap_or_default();
    let supabase_key = env::var("SUPABASE_SERVICE_ROLE_KEY").unwrap_or_default();
    let client = reqwest::Client::new();
    let mut h = reqwest::header::HeaderMap::new();
    h.insert("apikey", supabase_key.parse().unwrap());
    h.insert("Authorization", format!("Bearer {}", supabase_key).parse().unwrap());
    h.insert("Prefer", "count=exact,head=true".parse().unwrap()); 

    let get_count = |url: String| {
        let client = client.clone();
        let headers = h.clone();
        async move {
            if let Ok(res) = client.head(&url).headers(headers).send().await {
                if let Some(range) = res.headers().get("content-range") {
                    let range_str = range.to_str().unwrap_or("");
                    if let Some(count_str) = range_str.split('/').last() {
                        return count_str.parse::<i64>().unwrap_or(0);
                    }
                }
            }
            0
        }
    };

    let professionals = get_count(format!("{}/rest/v1/profiles?account_type=eq.professional", supabase_url)).await;
    let students = get_count(format!("{}/rest/v1/profiles?account_type=eq.student", supabase_url)).await;
    let orgs = get_count(format!("{}/rest/v1/profiles?account_type=eq.organization", supabase_url)).await;
    let jobs = get_count(format!("{}/rest/v1/jobs?status=eq.open", supabase_url)).await;
    let courses = get_count(format!("{}/rest/v1/courses", supabase_url)).await;

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let new_users_today = get_count(format!("{}/rest/v1/profiles?created_at=gte.{}T00:00:00Z", supabase_url, today)).await;
    let google_users = get_count(format!("{}/rest/v1/profiles?provider=eq.google", supabase_url)).await;
    let email_users = get_count(format!("{}/rest/v1/profiles?provider=eq.email", supabase_url)).await;
    let phone_users = get_count(format!("{}/rest/v1/profiles?provider=eq.firebase", supabase_url)).await;
    let active_users = get_count(format!("{}/rest/v1/profiles?status=eq.active", supabase_url)).await;

    (StatusCode::OK, Json(StatsResponse {
        professionals,
        students,
        organizations: orgs,
        active_jobs: jobs,
        courses,
        new_users_today,
        google_users,
        email_users,
        phone_users,
        active_users,
    }))
}

async fn submit_verification(Json(payload): Json<VerificationSubmitPayload>) -> (StatusCode, Json<ApiResponse>) {
    let supabase_url = env::var("SUPABASE_URL").unwrap_or_default();
    let supabase_key = env::var("SUPABASE_SERVICE_ROLE_KEY").unwrap_or_default();
    let client = reqwest::Client::new();
    let mut h = reqwest::header::HeaderMap::new();
    h.insert("apikey", supabase_key.parse().unwrap());
    h.insert("Authorization", format!("Bearer {}", supabase_key).parse().unwrap());
    h.insert("Content-Type", "application/json".parse().unwrap());

    let req_payload = json!({
        "user_id": payload.user_id,
        "document_type": payload.document_type,
        "document_url": payload.document_url,
        "status": "Pending"
    });

    let res = client.post(format!("{}/rest/v1/verification_requests", supabase_url))
        .headers(h)
        .json(&req_payload)
        .send()
        .await;

    match res {
        Ok(r) if r.status().is_success() => (StatusCode::OK, Json(ApiResponse { status: "success".into(), message: "Submitted".into() })),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse { status: "error".into(), message: "Failed to submit".into() }))
    }
}

async fn get_pending_verifications() -> (StatusCode, Json<serde_json::Value>) {
    let supabase_url = env::var("SUPABASE_URL").unwrap_or_default();
    let supabase_key = env::var("SUPABASE_SERVICE_ROLE_KEY").unwrap_or_default();
    let client = reqwest::Client::new();
    let mut h = reqwest::header::HeaderMap::new();
    h.insert("apikey", supabase_key.parse().unwrap());
    h.insert("Authorization", format!("Bearer {}", supabase_key).parse().unwrap());

    let url = format!("{}/rest/v1/verification_requests?status=eq.Pending&select=*,profiles(full_name,email,account_type,primary_category)", supabase_url);
    if let Ok(res) = client.get(&url).headers(h).send().await {
        if let Ok(json) = res.json::<serde_json::Value>().await {
            return (StatusCode::OK, Json(json));
        }
    }
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!([])))
}

async fn approve_verification(Path(id): Path<String>, Json(payload): Json<AdminApprovalPayload>) -> (StatusCode, Json<ApiResponse>) {
    let supabase_url = env::var("SUPABASE_URL").unwrap_or_default();
    let supabase_key = env::var("SUPABASE_SERVICE_ROLE_KEY").unwrap_or_default();
    let client = reqwest::Client::new();
    let mut h = reqwest::header::HeaderMap::new();
    h.insert("apikey", supabase_key.parse().unwrap());
    h.insert("Authorization", format!("Bearer {}", supabase_key).parse().unwrap());
    h.insert("Content-Type", "application/json".parse().unwrap());

    let _ = client.patch(format!("{}/rest/v1/verification_requests?id=eq.{}", supabase_url, id))
        .headers(h.clone())
        .json(&json!({"status": "Approved", "reviewed_at": chrono::Utc::now().to_rfc3339()}))
        .send().await;

    let _ = client.patch(format!("{}/rest/v1/profiles?id=eq.{}", supabase_url, payload.user_id))
        .headers(h)
        .json(&json!({"verified": true, "badge_color": payload.badge_color}))
        .send().await;

    (StatusCode::OK, Json(ApiResponse { status: "success".into(), message: "Approved".into() }))
}

async fn reject_verification(Path(id): Path<String>) -> (StatusCode, Json<ApiResponse>) {
    let supabase_url = env::var("SUPABASE_URL").unwrap_or_default();
    let supabase_key = env::var("SUPABASE_SERVICE_ROLE_KEY").unwrap_or_default();
    let client = reqwest::Client::new();
    let mut h = reqwest::header::HeaderMap::new();
    h.insert("apikey", supabase_key.parse().unwrap());
    h.insert("Authorization", format!("Bearer {}", supabase_key).parse().unwrap());
    h.insert("Content-Type", "application/json".parse().unwrap());

    let _ = client.patch(format!("{}/rest/v1/verification_requests?id=eq.{}", supabase_url, id))
        .headers(h)
        .json(&json!({"status": "Rejected", "reviewed_at": chrono::Utc::now().to_rfc3339()}))
        .send().await;

    (StatusCode::OK, Json(ApiResponse { status: "success".into(), message: "Rejected".into() }))
}
