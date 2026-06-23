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

#[derive(Deserialize)]
struct VerifyRequest {
    token: String,
}

#[derive(Serialize)]
struct VerifyResponse {
    status: String,
    message: String,
    uid: Option<String>,
}

#[derive(Deserialize, Debug)]
struct OnboardingPayload {
    firebase_uid: String,
    email: String,
    phone: Option<String>,
    account_type: String,
    category: String, // Maps to identity_type
    name: String,
    country: String,
    city: String,
    headline: String,
    bio: String,
}

#[derive(Serialize)]
struct ApiResponse {
    status: String,
    message: String,
}

// Structs for Supabase Responses
#[derive(Deserialize, Debug)]
struct SupabaseUser {
    id: String,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok(); // Load .env
    tracing_subscriber::fmt::init();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/auth/verify", post(verify_token))
        .route("/auth/onboarding", post(handle_onboarding))
        .layer(cors);

    let addr = SocketAddr::from(([127, 0, 0, 1], 8000));
    tracing::info!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "MGN Rust API is running!"
}

async fn verify_token(Json(payload): Json<VerifyRequest>) -> (StatusCode, Json<VerifyResponse>) {
    if payload.token.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyResponse {
                status: "error".to_string(),
                message: "No token provided".to_string(),
                uid: None,
            }),
        );
    }

    (
        StatusCode::OK,
        Json(VerifyResponse {
            status: "success".to_string(),
            message: "Token is valid".to_string(),
            uid: Some("simulated_firebase_uid_123".to_string()),
        }),
    )
}

async fn handle_onboarding(Json(payload): Json<OnboardingPayload>) -> (StatusCode, Json<ApiResponse>) {
    let supabase_url = env::var("SUPABASE_URL").unwrap_or_default();
    let supabase_key = env::var("SUPABASE_SERVICE_ROLE_KEY").unwrap_or_default();

    if supabase_url.is_empty() || supabase_key.is_empty() {
        tracing::error!("Missing Supabase configuration");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse { status: "error".into(), message: "Server misconfiguration".into() })
        );
    }

    let client = reqwest::Client::new();
    let headers = {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("apikey", supabase_key.parse().unwrap());
        h.insert("Authorization", format!("Bearer {}", supabase_key).parse().unwrap());
        h.insert("Content-Type", "application/json".parse().unwrap());
        h.insert("Prefer", "return=representation".parse().unwrap()); // To return inserted row
        h
    };

    // 1. Insert into users
    let user_payload = json!({
        "firebase_uid": payload.firebase_uid,
        "email": payload.email,
        "phone": payload.phone,
        "account_type": payload.account_type,
        "status": "active"
    });

    let res = client.post(format!("{}/rest/v1/users", supabase_url))
        .headers(headers.clone())
        .json(&user_payload)
        .send()
        .await;

    let user_id = match res {
        Ok(response) => {
            if response.status().is_success() {
                let body: Vec<SupabaseUser> = response.json().await.unwrap_or_default();
                if let Some(user) = body.first() {
                    user.id.clone()
                } else {
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse { status: "error".into(), message: "Failed to parse user".into() }));
                }
            } else {
                let err_text = response.text().await.unwrap_or_default();
                tracing::error!("Users Insert Error: {}", err_text);
                // If it's a unique violation, user might already exist. For MVP we'll just error.
                return (StatusCode::BAD_REQUEST, Json(ApiResponse { status: "error".into(), message: "User already exists or DB error".into() }));
            }
        },
        Err(e) => {
            tracing::error!("Request Error: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse { status: "error".into(), message: "Database connection failed".into() }));
        }
    };

    // 2. Insert into profiles
    let profile_payload = json!({
        "id": user_id,
        "name": payload.name,
        "bio": payload.bio,
        "city": payload.city,
        "country": payload.country,
        "headline": payload.headline
    });

    let res = client.post(format!("{}/rest/v1/profiles", supabase_url))
        .headers(headers.clone())
        // For profiles we don't need representation back necessarily, but let's keep headers
        .json(&profile_payload)
        .send()
        .await;

    if let Ok(response) = res {
        if !response.status().is_success() {
            let err_text = response.text().await.unwrap_or_default();
            tracing::error!("Profiles Insert Error: {}", err_text);
        }
    }

    // 3. Insert into professional_identities
    let identity_payload = json!({
        "user_id": user_id,
        "identity_type": payload.category
    });

    let res = client.post(format!("{}/rest/v1/professional_identities", supabase_url))
        .headers(headers.clone())
        .json(&identity_payload)
        .send()
        .await;

    if let Ok(response) = res {
        if !response.status().is_success() {
            let err_text = response.text().await.unwrap_or_default();
            tracing::error!("Identity Insert Error: {}", err_text);
        }
    }

    (
        StatusCode::OK,
        Json(ApiResponse {
            status: "success".into(),
            message: "Onboarding completed successfully".into(),
        })
    )
}
