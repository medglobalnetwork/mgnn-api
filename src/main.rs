use axum::{
    routing::{get, post},
    Router,
    Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber;
use std::env;

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
    total_users: i64,
    verified_professionals: i64,
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
struct EducationPayload {
    college: String,
    course: String,
    year: String,
    skills: String,
}

#[derive(Deserialize, Debug)]
struct ExperiencePayload {
    company: String,
    designation: String,
    specialization: String,
    skills: String,
}

#[derive(Deserialize, Debug)]
struct OnboardingPayload {
    id: String, // from auth
    account_type: String,
    category: String, 
    name: String,
    country: String,
    city: String,
    headline: String,
    bio: String,
    interests: Option<Vec<String>>,
    education: Option<EducationPayload>,
    experience: Option<ExperiencePayload>,
    referred_by: Option<String>,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let cors = CorsLayer::permissive();

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/auth/sync", post(handle_sync))
        .route("/auth/onboarding", post(handle_onboarding))
        .route("/stats", get(get_stats))
        .layer(cors);

    let port: u16 = env::var("PORT").unwrap_or_else(|_| "8000".into()).parse().unwrap_or(8000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
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
        "identity_type": payload.category
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

    let total_users = get_count(format!("{}/rest/v1/profiles", supabase_url)).await;
    let verified = get_count(format!("{}/rest/v1/profiles?verified=eq.true", supabase_url)).await;
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
        total_users,
        verified_professionals: verified,
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
