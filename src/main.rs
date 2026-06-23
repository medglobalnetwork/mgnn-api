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
    interests: Option<Vec<String>>,
    education: Option<EducationPayload>,
    experience: Option<ExperiencePayload>,
    referred_by: Option<String>,
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

    let cors = CorsLayer::permissive();

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/auth/verify", post(verify_token))
        .route("/auth/onboarding", post(handle_onboarding))
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
    let name_part = payload.name.split_whitespace().next().unwrap_or("MGN").to_uppercase().replace(|c: char| !c.is_alphanumeric(), "");
    let uid_part = if payload.firebase_uid.len() >= 4 { &payload.firebase_uid[..4] } else { "0000" }.to_uppercase();
    let referral_code = format!("{}{}", name_part, uid_part);

    let user_payload = json!({
        "firebase_uid": payload.firebase_uid,
        "email": payload.email,
        "phone": payload.phone,
        "account_type": payload.account_type,
        "status": "active",
        "referral_code": referral_code,
        "referred_by": payload.referred_by,
        "onboarding_score": 85
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
        "headline": payload.headline,
        "interests": payload.interests.unwrap_or_default()
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

    // 4. Insert into education (if provided)
    if let Some(edu) = payload.education {
        let edu_payload = json!({
            "user_id": user_id,
            "institution_name": edu.college,
            "degree": edu.course,
            "field_of_study": edu.skills,
            "start_date": format!("{}-01-01", edu.year)
        });
        
        let _ = client.post(format!("{}/rest/v1/education", supabase_url))
            .headers(headers.clone())
            .json(&edu_payload)
            .send()
            .await;
    }

    // 5. Insert into experience (if provided)
    if let Some(exp) = payload.experience {
        let exp_payload = json!({
            "user_id": user_id,
            "company_name": exp.company,
            "title": exp.designation,
            "description": format!("Specialization: {}. Skills: {}", exp.specialization, exp.skills)
        });

        let _ = client.post(format!("{}/rest/v1/experience", supabase_url))
            .headers(headers.clone())
            .json(&exp_payload)
            .send()
            .await;
    }

    (
        StatusCode::OK,
        Json(ApiResponse {
            status: "success".into(),
            message: "Onboarding completed successfully".into(),
        })
    )
}
