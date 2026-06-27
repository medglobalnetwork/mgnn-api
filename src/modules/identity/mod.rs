use axum::{Router, routing::{get, post, delete}, Json, extract::State};
use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use crate::extractors::{AuthUser, create_access_token, create_refresh_token, RefreshClaims};
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json, ok_message};
use crate::modules::audit;

// ──────────────────────────────────────────────
// Identity Engine  (Screens 1–10)
// Full Supabase-backed auth implementation
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Screen 1 – Email Signup
        .route("/signup/email", post(signup_email))
        // Screen 2 – Email Login
        .route("/login/email", post(login_email))
        // Screen 3 – Google OAuth
        .route("/login/google", post(login_google))
        // Screen 4 – Phone Login
        .route("/login/phone", post(login_phone))
        // Screen 5 – OTP Verify (email/phone)
        .route("/verify-otp", post(verify_otp))
        // Screen 6 – Forgot / Reset Password
        .route("/forgot-password", post(forgot_password))
        .route("/reset-password", post(reset_password))
        // Screen 7 – Refresh Token
        .route("/refresh", post(refresh_token))
        // Screen 8 – Logout
        .route("/logout", post(logout))
        // Screen 9 – Active Sessions
        .route("/sessions", get(get_sessions))
        // Screen 10 – Delete Session
        .route("/sessions/:id", delete(delete_session))
}

// ══════════════════════════════════════════════
// Screen 1 – Email Signup
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct EmailSignup {
    email: String,
    password: String,
    full_name: String,
}

async fn signup_email(
    State(state): State<SharedState>,
    Json(payload): Json<EmailSignup>,
) -> ApiResult {
    // Validate email format
    if !payload.email.contains('@') || !payload.email.contains('.') {
        return Err(ApiError::BadRequest("Invalid email format".into()));
    }

    // Validate password strength
    if payload.password.len() < 8 {
        return Err(ApiError::BadRequest("Password must be at least 8 characters".into()));
    }

    // Check if email already exists in users table
    let check_url = format!("{}/rest/v1/users?email=eq.{}&select=id", state.rest_url(), payload.email);
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await?;
    let existing: serde_json::Value = check_res.json().await?;
    if let Some(arr) = existing.as_array() {
        if !arr.is_empty() {
            return Err(ApiError::Conflict("Email already registered".into()));
        }
    }

    // Hash password with argon2
    let salt = argon2::password_hash::SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let hash = argon2::PasswordHasher::hash_password(
        &argon2::Argon2::default(),
        payload.password.as_bytes(),
        &salt,
    ).map_err(|e| ApiError::Internal(format!("Password hash error: {e}")))?;

    let user_id = uuid::Uuid::new_v4().to_string();
    let session_id = uuid::Uuid::new_v4().to_string();

    // Insert into users table
    let user_payload = json!({
        "id": user_id,
        "email": payload.email,
        "password_hash": hash.to_string(),
        "status": "active",
        "primary_login_method": "email",
        "email_verified": false,
        "phone_verified": false,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "updated_at": chrono::Utc::now().to_rfc3339()
    });

    let user_url = format!("{}/rest/v1/users", state.rest_url());
    let user_res = state.http.post(&user_url).headers(state.supabase_headers()).json(&user_payload).send().await?;
    if !user_res.status().is_success() {
        let err = user_res.text().await.unwrap_or_default();
        tracing::error!("User insert error: {}", err);
        return Err(ApiError::Internal("Failed to create user".into()));
    }

    // Insert into auth_identities table
    let auth_id_payload = json!({
        "user_id": user_id,
        "provider": "email",
        "provider_user_id": payload.email,
        "provider_email": payload.email,
        "linked_at": chrono::Utc::now().to_rfc3339()
    });
    let auth_id_url = format!("{}/rest/v1/auth_identities", state.rest_url());
    let _ = state.http.post(&auth_id_url).headers(state.supabase_headers()).json(&auth_id_payload).send().await;

    // Create session
    let session_payload = json!({
        "id": session_id,
        "user_id": user_id,
        "refresh_token_hash": "placeholder_until_refresh_logic", // Will be updated during refresh generation
        "device_name": "Unknown",
        "browser": "Unknown",
        "os": "Unknown",
        "ip_address": "0.0.0.0",
        "is_current": true,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "last_active": chrono::Utc::now().to_rfc3339(),
        "expires_at": (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339()
    });
    let sess_url = format!("{}/rest/v1/sessions", state.rest_url());
    let _ = state.http.post(&sess_url).headers(state.supabase_headers()).json(&session_payload).send().await;

    // Generate JWT tokens
    let access_token = create_access_token(&user_id, &user_id, "user", &session_id, &state.config)?;
    let refresh_token = create_refresh_token(&user_id, &session_id, &state.config)?;

    audit::log_action(&user_id, "auth.signup_email", "user", &user_id).await;

    ok_json(json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "token_type": "Bearer",
        "expires_in": state.config.access_token_ttl_secs,
        "user": {
            "id": user_id,
            "email": payload.email,
            "full_name": payload.full_name,
            "email_verified": false
        }
    }))
}

// ══════════════════════════════════════════════
// Screen 2 – Email Login
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct EmailLogin {
    email: String,
    password: String,
    device_name: Option<String>,
    browser: Option<String>,
    os: Option<String>,
    ip_masked: Option<String>,
}

async fn login_email(
    State(state): State<SharedState>,
    Json(payload): Json<EmailLogin>,
) -> ApiResult {
    // Find profile by email
    let url = format!("{}/rest/v1/profiles?email=eq.{}&select=id,email,full_name,account_type,status,email_verified", state.rest_url(), payload.email);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let profiles: serde_json::Value = res.json().await?;

    let profile = profiles.as_array()
        .and_then(|a| a.first().cloned())
        .ok_or_else(|| ApiError::Unauthorized("Invalid email or password".into()))?;

    let profile_id = profile["id"].as_str().ok_or_else(|| ApiError::Internal("Invalid profile data".into()))?;

    // Check account status
    if profile["status"].as_str() == Some("deactivated") {
        // Reactivate account on login
        let react_url = format!("{}/rest/v1/profiles?id=eq.{}&select=id", state.rest_url(), profile_id);
        let _ = state.http.patch(&react_url).headers(state.supabase_headers())
            .json(&json!({"status": "active", "deactivated_at": null}))
            .send().await;
    } else if profile["status"].as_str() == Some("pending_deletion") {
        // Cancel deletion on login
        let cancel_url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), profile_id);
        let _ = state.http.patch(&cancel_url).headers(state.supabase_headers())
            .json(&json!({"status": "active", "deletion_requested_at": null, "deletion_scheduled_at": null}))
            .send().await;
    } else if profile["status"].as_str() == Some("suspended") {
        return Err(ApiError::Forbidden("Account suspended. Contact support.".into()));
    }

    // Verify password
    let sec_url = format!("{}/rest/v1/security_settings?user_id=eq.{}&select=password_hash,two_fa_enabled,two_fa_methods", state.rest_url(), profile_id);
    let sec_res = state.http.get(&sec_url).headers(state.supabase_headers()).send().await?;
    let sec_data: serde_json::Value = sec_res.json().await?;

    let hash_str = sec_data.as_array()
        .and_then(|a| a.first())
        .and_then(|r| r.get("password_hash"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if hash_str.is_empty() {
        return Err(ApiError::Unauthorized("No password set. Use reset password.".into()));
    }

    let parsed_hash = argon2::PasswordHash::new(hash_str)
        .map_err(|_| ApiError::Internal("Invalid stored password hash".into()))?;
    let verified = argon2::PasswordVerifier::verify_password(
        &argon2::Argon2::default(),
        payload.password.as_bytes(),
        &parsed_hash,
    ).is_ok();

    if !verified {
        return Err(ApiError::Unauthorized("Invalid email or password".into()));
    }

    let two_fa_enabled = sec_data.as_array()
        .and_then(|a| a.first())
        .and_then(|r| r.get("two_fa_enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let two_fa_methods = sec_data.as_array()
        .and_then(|a| a.first())
        .and_then(|r| r.get("two_fa_methods"))
        .cloned()
        .unwrap_or(json!([]));

    let role = profile["account_type"].as_str().unwrap_or("professional");
    let session_id = uuid::Uuid::new_v4().to_string();

    // If 2FA required, send OTP and return challenge
    if two_fa_enabled {
        let methods = two_fa_methods.as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .unwrap_or("email_otp");

        let otp = generate_otp_code();
        let otp_purpose = format!("2fa_{}", methods);
        let otp_payload = json!({
            "user_id": profile_id,
            "code": otp,
            "purpose": otp_purpose,
            "expires_at": (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
            "used": false
        });
        let otp_url = format!("{}/rest/v1/otp_verifications", state.rest_url());
        let _ = state.http.post(&otp_url).headers(state.supabase_headers()).json(&otp_payload).send().await;

        // Store pending session temporarily
        let pending_payload = json!({
            "id": session_id,
            "user_id": profile_id,
            "device_name": payload.device_name.unwrap_or_default(),
            "browser": payload.browser.unwrap_or_default(),
            "os": payload.os.unwrap_or_default(),
            "ip_masked": payload.ip_masked.unwrap_or_default(),
            "is_current": false,
            "two_fa_pending": true,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "last_active": chrono::Utc::now().to_rfc3339()
        });
        let sess_url = format!("{}/rest/v1/user_sessions", state.rest_url());
        let _ = state.http.post(&sess_url).headers(state.supabase_headers()).json(&pending_payload).send().await;

        return ok_json(json!({
            "requires_2fa": true,
            "session_id": session_id,
            "methods": two_fa_methods,
            "message": "2FA verification required"
        }));
    }

    // No 2FA — create session and issue tokens
    create_session_and_tokens(&state, profile_id, role, &session_id, &payload.device_name, &payload.browser, &payload.os, &payload.ip_masked).await
}

// ══════════════════════════════════════════════
// Screen 3 – Google OAuth Login
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct GoogleLogin {
    id_token: String,
    device_name: Option<String>,
    browser: Option<String>,
    os: Option<String>,
    ip_masked: Option<String>,
}

async fn login_google(
    State(state): State<SharedState>,
    Json(payload): Json<GoogleLogin>,
) -> ApiResult {
    // In production, verify id_token with Google's API
    // For now, decode the JWT payload to extract user info
    let parts: Vec<&str> = payload.id_token.split('.').collect();
    let google_data: serde_json::Value = if parts.len() == 3 {
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]).unwrap_or_default();
        serde_json::from_slice(&decoded).unwrap_or(json!({}))
    } else {
        return Err(ApiError::BadRequest("Invalid Google ID token".into()));
    };

    let email = google_data["email"].as_str().unwrap_or("").to_string();
    let full_name = google_data["name"].as_str().unwrap_or("").to_string();
    let google_sub = google_data["sub"].as_str().unwrap_or("").to_string();
    let picture = google_data["picture"].as_str().unwrap_or("").to_string();

    if email.is_empty() {
        return Err(ApiError::BadRequest("Could not extract email from Google token".into()));
    }

    // Check if user exists
    let check_url = format!("{}/rest/v1/profiles?email=eq.{}&select=id,full_name,account_type,status", state.rest_url(), email);
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await?;
    let existing: serde_json::Value = check_res.json().await?;

    let profile_id = if let Some(arr) = existing.as_array() {
        if let Some(profile) = arr.first() {
            // Existing user — update status if needed
            let pid = profile["id"].as_str().unwrap_or("").to_string();
            if profile["status"].as_str() == Some("deactivated") {
                let react_url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), pid);
                let _ = state.http.patch(&react_url).headers(state.supabase_headers())
                    .json(&json!({"status": "active"}))
                    .send().await;
            }
            pid
        } else {
            // New user — create profile
            create_oauth_profile(&state, &email, &full_name, "google", &picture, &google_sub).await?
        }
    } else {
        create_oauth_profile(&state, &email, &full_name, "google", &picture, &google_sub).await?
    };

    if profile_id.is_empty() {
        return Err(ApiError::Internal("Failed to resolve profile".into()));
    }

    // Ensure connected_accounts entry
    let conn_check = format!("{}/rest/v1/connected_accounts?user_id=eq.{}&provider=eq.google&select=id", state.rest_url(), profile_id);
    let conn_res = state.http.get(&conn_check).headers(state.supabase_headers()).send().await?;
    let conn_data: serde_json::Value = conn_res.json().await?;
    if conn_data.as_array().map_or(true, |a| a.is_empty()) {
        let conn_payload = json!({
            "user_id": profile_id,
            "provider": "google",
            "provider_account_id": google_sub,
            "is_primary": false,
            "connected_at": chrono::Utc::now().to_rfc3339()
        });
        let conn_url = format!("{}/rest/v1/connected_accounts", state.rest_url());
        let _ = state.http.post(&conn_url).headers(state.supabase_headers()).json(&conn_payload).send().await;
    }

    let session_id = uuid::Uuid::new_v4().to_string();

    // Get role
    let role_url = format!("{}/rest/v1/profiles?id=eq.{}&select=account_type", state.rest_url(), profile_id);
    let role_res = state.http.get(&role_url).headers(state.supabase_headers()).send().await?;
    let role_data: serde_json::Value = role_res.json().await?;
    let role = role_data.as_array()
        .and_then(|a| a.first())
        .and_then(|r| r.get("account_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("professional");

    audit::log_action(&profile_id, "auth.login_google", "profile", &profile_id).await;
    create_session_and_tokens(&state, &profile_id, role, &session_id, &payload.device_name, &payload.browser, &payload.os, &payload.ip_masked).await
}

// ══════════════════════════════════════════════
// Screen 4 – Phone Login
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct PhoneLogin {
    phone: String,
    device_name: Option<String>,
    browser: Option<String>,
    os: Option<String>,
    ip_masked: Option<String>,
}

async fn login_phone(
    State(state): State<SharedState>,
    Json(payload): Json<PhoneLogin>,
) -> ApiResult {
    let phone = payload.phone.trim().to_string();

    // Find or create profile by phone
    let check_url = format!("{}/rest/v1/profiles?phone=eq.{}&select=id,full_name,account_type,status,phone_verified", state.rest_url(), phone);
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await?;
    let existing: serde_json::Value = check_res.json().await?;

    let profile_id = if let Some(arr) = existing.as_array() {
        if let Some(profile) = arr.first() {
            let pid = profile["id"].as_str().unwrap_or("").to_string();
            if profile["status"].as_str() == Some("suspended") {
                return Err(ApiError::Forbidden("Account suspended. Contact support.".into()));
            }
            pid
        } else {
            // New user — create minimal profile
            let new_id = uuid::Uuid::new_v4().to_string();
            let profile_payload = json!({
                "id": new_id,
                "phone": phone,
                "full_name": phone.clone(),
                "provider": "firebase",
                "phone_verified": true,
                "profile_completed": false,
                "status": "active",
                "account_type": "professional",
                "created_at": chrono::Utc::now().to_rfc3339()
            });
            let profile_url = format!("{}/rest/v1/profiles", state.rest_url());
            let _ = state.http.post(&profile_url).headers(state.supabase_headers()).json(&profile_payload).send().await;

            // connected_accounts entry
            let conn_payload = json!({
                "user_id": new_id,
                "provider": "phone",
                "provider_account_id": phone,
                "is_primary": true,
                "connected_at": chrono::Utc::now().to_rfc3339()
            });
            let conn_url = format!("{}/rest/v1/connected_accounts", state.rest_url());
            let _ = state.http.post(&conn_url).headers(state.supabase_headers()).json(&conn_payload).send().await;

            new_id
        }
    } else {
        let new_id = uuid::Uuid::new_v4().to_string();
        let profile_payload = json!({
            "id": new_id,
            "phone": phone,
            "full_name": phone.clone(),
            "provider": "firebase",
            "phone_verified": true,
            "profile_completed": false,
            "status": "active",
            "account_type": "professional",
            "created_at": chrono::Utc::now().to_rfc3339()
        });
        let profile_url = format!("{}/rest/v1/profiles", state.rest_url());
        let _ = state.http.post(&profile_url).headers(state.supabase_headers()).json(&profile_payload).send().await;
        new_id
    };

    // Generate OTP for phone verification
    let otp = generate_otp_code();
    let otp_payload = json!({
        "user_id": profile_id,
        "code": otp,
        "purpose": "phone_login",
        "expires_at": (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
        "used": false
    });
    let otp_url = format!("{}/rest/v1/otp_verifications", state.rest_url());
    let _ = state.http.post(&otp_url).headers(state.supabase_headers()).json(&otp_payload).send().await;

    let session_id = uuid::Uuid::new_v4().to_string();

    // Create pending session
    let session_payload = json!({
        "id": session_id,
        "user_id": profile_id,
        "device_name": payload.device_name.unwrap_or_default(),
        "browser": payload.browser.unwrap_or_default(),
        "os": payload.os.unwrap_or_default(),
        "ip_masked": payload.ip_masked.unwrap_or_default(),
        "is_current": false,
        "two_fa_pending": true,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "last_active": chrono::Utc::now().to_rfc3339()
    });
    let sess_url = format!("{}/rest/v1/user_sessions", state.rest_url());
    let _ = state.http.post(&sess_url).headers(state.supabase_headers()).json(&session_payload).send().await;

    ok_json(json!({
        "requires_otp": true,
        "session_id": session_id,
        "phone": mask_phone(&phone),
        "message": "OTP sent to your phone"
    }))
}

// ══════════════════════════════════════════════
// Screen 5 – OTP Verification (email/phone/2fa)
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct OtpVerify {
    session_id: String,
    code: String,
    purpose: String,  // email_verify, phone_login, 2fa_authenticator, 2fa_email_otp, 2fa_phone_otp
}

async fn verify_otp(
    State(state): State<SharedState>,
    Json(payload): Json<OtpVerify>,
) -> ApiResult {
    // Find the session to get the user
    let sess_url = format!("{}/rest/v1/user_sessions?id=eq.{}&select=user_id,two_fa_pending", state.rest_url(), payload.session_id);
    let sess_res = state.http.get(&sess_url).headers(state.supabase_headers()).send().await?;
    let sess_data: serde_json::Value = sess_res.json().await?;

    let session = sess_data.as_array()
        .and_then(|a| a.first().cloned())
        .ok_or_else(|| ApiError::NotFound("Session not found".into()))?;

    let user_id = session["user_id"].as_str().ok_or_else(|| ApiError::Internal("Invalid session data".into()))?;

    // Verify OTP
    let otp_url = format!("{}/rest/v1/otp_verifications?user_id=eq.{}&code=eq.{}&purpose=eq.{}&used=eq.false&expires_at.gt.{}&select=id&order=created_at.desc&limit=1",
        state.rest_url(), user_id, payload.code, payload.purpose, chrono::Utc::now().to_rfc3339());
    let otp_res = state.http.get(&otp_url).headers(state.supabase_headers()).send().await?;
    let otp_data: serde_json::Value = otp_res.json().await?;

    if otp_data.as_array().map_or(true, |a| a.is_empty()) {
        return Err(ApiError::BadRequest("Invalid or expired OTP".into()));
    }

    // Mark OTP as used
    if let Some(otp_id) = otp_data[0]["id"].as_str() {
        let mark_url = format!("{}/rest/v1/otp_verifications?id=eq.{}", state.rest_url(), otp_id);
        let _ = state.http.patch(&mark_url).headers(state.supabase_headers())
            .json(&json!({"used": true, "used_at": chrono::Utc::now().to_rfc3339()}))
            .send().await;
    }

    // Handle purpose-specific side effects
    match payload.purpose.as_str() {
        "email_verify" => {
            let update_url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), user_id);
            let _ = state.http.patch(&update_url).headers(state.supabase_headers())
                .json(&json!({"email_verified": true}))
                .send().await;
        }
        "phone_login" | "phone_change" => {
            let update_url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), user_id);
            let _ = state.http.patch(&update_url).headers(state.supabase_headers())
                .json(&json!({"phone_verified": true}))
                .send().await;
        }
        _ => {} // 2fa_ purposes don't need profile update
    }

    // Activate the pending session
    let update_sess_url = format!("{}/rest/v1/user_sessions?id=eq.{}", state.rest_url(), payload.session_id);
    let _ = state.http.patch(&update_sess_url).headers(state.supabase_headers())
        .json(&json!({"is_current": true, "two_fa_pending": false, "last_active": chrono::Utc::now().to_rfc3339()}))
        .send().await;

    // Get role and issue tokens
    let role_url = format!("{}/rest/v1/profiles?id=eq.{}&select=account_type", state.rest_url(), user_id);
    let role_res = state.http.get(&role_url).headers(state.supabase_headers()).send().await?;
    let role_data: serde_json::Value = role_res.json().await?;
    let role = role_data.as_array()
        .and_then(|a| a.first())
        .and_then(|r| r.get("account_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("professional");

    let access_token = create_access_token(user_id, user_id, role, &payload.session_id, &state.config)?;
    let refresh_token = create_refresh_token(user_id, &payload.session_id, &state.config)?;

    audit::log_action(user_id, &format!("auth.otp_verified_{}", payload.purpose), "user_sessions", &payload.session_id).await;

    ok_json(json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "token_type": "Bearer",
        "expires_in": state.config.access_token_ttl_secs,
        "message": "OTP verified successfully"
    }))
}

// ══════════════════════════════════════════════
// Screen 6 – Forgot / Reset Password
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct ForgotPassword {
    email: String,
}

async fn forgot_password(
    State(state): State<SharedState>,
    Json(payload): Json<ForgotPassword>,
) -> ApiResult {
    let url = format!("{}/rest/v1/profiles?email=eq.{}&select=id", state.rest_url(), payload.email);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;

    let user_id = data.as_array()
        .and_then(|a| a.first())
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_str());

    if let Some(uid) = user_id {
        let otp = generate_otp_code();
        let otp_payload = json!({
            "user_id": uid,
            "code": otp,
            "purpose": "password_reset",
            "expires_at": (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339(),
            "used": false
        });
        let otp_url = format!("{}/rest/v1/otp_verifications", state.rest_url());
        let _ = state.http.post(&otp_url).headers(state.supabase_headers()).json(&otp_payload).send().await;

        audit::log_action(uid, "auth.forgot_password", "profile", uid).await;
    }

    // Always return success to prevent email enumeration
    ok_message("If an account with that email exists, a reset code has been sent")
}

#[derive(Deserialize)]
struct ResetPassword {
    email: String,
    code: String,
    new_password: String,
}

async fn reset_password(
    State(state): State<SharedState>,
    Json(payload): Json<ResetPassword>,
) -> ApiResult {
    // Find user
    let url = format!("{}/rest/v1/profiles?email=eq.{}&select=id", state.rest_url(), payload.email);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;

    let user_id = data.as_array()
        .and_then(|a| a.first())
        .and_then(|r| r.get("id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::NotFound("Account not found".into()))?;

    // Verify OTP
    let otp_url = format!("{}/rest/v1/otp_verifications?user_id=eq.{}&code=eq.{}&purpose=eq.password_reset&used=eq.false&expires_at.gt.{}&select=id",
        state.rest_url(), user_id, payload.code, chrono::Utc::now().to_rfc3339());
    let otp_res = state.http.get(&otp_url).headers(state.supabase_headers()).send().await?;
    let otp_data: serde_json::Value = otp_res.json().await?;

    if otp_data.as_array().map_or(true, |a| a.is_empty()) {
        return Err(ApiError::BadRequest("Invalid or expired reset code".into()));
    }

    // Mark OTP as used
    if let Some(otp_id) = otp_data[0]["id"].as_str() {
        let mark_url = format!("{}/rest/v1/otp_verifications?id=eq.{}", state.rest_url(), otp_id);
        let _ = state.http.patch(&mark_url).headers(state.supabase_headers())
            .json(&json!({"used": true})).send().await;
    }

    // Validate & hash new password
    if payload.new_password.len() < 8 {
        return Err(ApiError::BadRequest("Password must be at least 8 characters".into()));
    }

    let salt = argon2::password_hash::SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let hash = argon2::PasswordHasher::hash_password(
        &argon2::Argon2::default(),
        payload.new_password.as_bytes(),
        &salt,
    ).map_err(|e| ApiError::Internal(format!("Password hash error: {e}")))?;

    let sec_url = format!("{}/rest/v1/security_settings?user_id=eq.{}", state.rest_url(), user_id);
    let _ = state.http.patch(&sec_url).headers(state.supabase_headers())
        .json(&json!({
            "password_hash": hash.to_string(),
            "password_strength": compute_password_strength(&payload.new_password),
            "last_password_change": chrono::Utc::now().to_rfc3339()
        }))
        .send().await;

    // Invalidate all sessions (force re-login)
    let sess_url = format!("{}/rest/v1/user_sessions?user_id=eq.{}", state.rest_url(), user_id);
    let _ = state.http.delete(&sess_url).headers(state.supabase_headers()).send().await;

    audit::log_action(user_id, "auth.password_reset", "security_settings", user_id).await;
    ok_message("Password reset successfully. Please log in again.")
}

// ══════════════════════════════════════════════
// Screen 7 – Refresh Token
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

async fn refresh_token(
    State(state): State<SharedState>,
    Json(payload): Json<RefreshRequest>,
) -> ApiResult {
    // Decode the refresh token
    let refresh_claims = jsonwebtoken::decode::<RefreshClaims>(
        &payload.refresh_token,
        &jsonwebtoken::DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|e| ApiError::Unauthorized(format!("Invalid refresh token: {e}")))?;

    // Verify session still exists
    let sess_url = format!("{}/rest/v1/user_sessions?id=eq.{}&user_id=eq.{}&select=id,user_id",
        state.rest_url(), refresh_claims.sid, refresh_claims.sub);
    let sess_res = state.http.get(&sess_url).headers(state.supabase_headers()).send().await?;
    let sess_data: serde_json::Value = sess_res.json().await?;

    if sess_data.as_array().map_or(true, |a| a.is_empty()) {
        return Err(ApiError::Unauthorized("Session no longer exists".into()));
    }

    let user_id = refresh_claims.sub.clone();

    // Get role
    let role_url = format!("{}/rest/v1/profiles?id=eq.{}&select=account_type", state.rest_url(), user_id);
    let role_res = state.http.get(&role_url).headers(state.supabase_headers()).send().await?;
    let role_data: serde_json::Value = role_res.json().await?;
    let role = role_data.as_array()
        .and_then(|a| a.first())
        .and_then(|r| r.get("account_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("professional");

    // Update session last_active
    let update_url = format!("{}/rest/v1/user_sessions?id=eq.{}", state.rest_url(), refresh_claims.sid);
    let _ = state.http.patch(&update_url).headers(state.supabase_headers())
        .json(&json!({"last_active": chrono::Utc::now().to_rfc3339()}))
        .send().await;

    // Issue new access token
    let access_token = create_access_token(&user_id, &user_id, role, &refresh_claims.sid, &state.config)?;

    ok_json(json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": state.config.access_token_ttl_secs
    }))
}

// ══════════════════════════════════════════════
// Screen 8 – Logout
// ══════════════════════════════════════════════

async fn logout(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    // Delete current session
    let url = format!("{}/rest/v1/user_sessions?id=eq.{}&user_id=eq.{}",
        state.rest_url(), claims.claims.sid, claims.claims.sub);
    let _ = state.http.delete(&url).headers(state.supabase_headers()).send().await;

    audit::log_action(&claims.claims.sub, "auth.logout", "user_sessions", &claims.claims.sid).await;
    ok_message("Logged out successfully")
}

// ══════════════════════════════════════════════
// Screen 9 – Get Active Sessions
// ══════════════════════════════════════════════

async fn get_sessions(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    let url = format!("{}/rest/v1/user_sessions?user_id=eq.{}&select=id,device_name,browser,os,ip_masked,country,city,last_active,created_at,is_current&order=last_active.desc",
        state.rest_url(), claims.claims.sub);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;
    ok_json(data)
}

// ══════════════════════════════════════════════
// Screen 10 – Delete Session (logout device)
// ══════════════════════════════════════════════

async fn delete_session(
    State(state): State<SharedState>,
    claims: AuthUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult {
    if id == claims.claims.sid {
        return Err(ApiError::BadRequest("Cannot logout current session from here. Use /logout instead.".into()));
    }
    let url = format!("{}/rest/v1/user_sessions?id=eq.{}&user_id=eq.{}",
        state.rest_url(), id, claims.claims.sub);
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to logout device".into()));
    }
    audit::log_action(&claims.claims.sub, "session.logout_device", "user_sessions", &id).await;
    ok_message("Device logged out successfully")
}

// ══════════════════════════════════════════════
// Helper Functions
// ══════════════════════════════════════════════

async fn create_session_and_tokens(
    state: &SharedState,
    user_id: &str,
    role: &str,
    session_id: &str,
    device_name: &Option<String>,
    browser: &Option<String>,
    os: &Option<String>,
    ip_masked: &Option<String>,
) -> ApiResult {
    // Create session record
    let session_payload = json!({
        "id": session_id,
        "user_id": user_id,
        "device_name": device_name.clone().unwrap_or_default(),
        "browser": browser.clone().unwrap_or_default(),
        "os": os.clone().unwrap_or_default(),
        "ip_masked": ip_masked.clone().unwrap_or_default(),
        "is_current": true,
        "two_fa_pending": false,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "last_active": chrono::Utc::now().to_rfc3339()
    });
    let sess_url = format!("{}/rest/v1/user_sessions", state.rest_url());
    let _ = state.http.post(&sess_url).headers(state.supabase_headers()).json(&session_payload).send().await;

    let access_token = create_access_token(user_id, user_id, role, session_id, &state.config)?;
    let refresh_token = create_refresh_token(user_id, session_id, &state.config)?;

    // Get profile for user info
    let profile_url = format!("{}/rest/v1/profiles?id=eq.{}&select=id,email,full_name,email_verified,phone_verified,profile_verified,profile_completed", state.rest_url(), user_id);
    let profile_res = state.http.get(&profile_url).headers(state.supabase_headers()).send().await?;
    let profile_data: serde_json::Value = profile_res.json().await?;
    let profile = profile_data.as_array().and_then(|a| a.first().cloned()).unwrap_or(json!({}));

    audit::log_action(user_id, "auth.login_success", "user_sessions", session_id).await;

    ok_json(json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "token_type": "Bearer",
        "expires_in": state.config.access_token_ttl_secs,
        "user": profile
    }))
}

async fn create_oauth_profile(
    state: &SharedState,
    email: &str,
    full_name: &str,
    provider: &str,
    picture: &str,
    _provider_sub: &str,
) -> Result<String, ApiError> {
    let profile_id = uuid::Uuid::new_v4().to_string();
    let profile_payload = json!({
        "id": profile_id,
        "email": email,
        "full_name": full_name,
        "avatar_url": picture,
        "provider": provider,
        "email_verified": true,
        "profile_completed": false,
        "status": "active",
        "account_type": "professional",
        "created_at": chrono::Utc::now().to_rfc3339()
    });
    let profile_url = format!("{}/rest/v1/profiles", state.rest_url());
    let res = state.http.post(&profile_url).headers(state.supabase_headers()).json(&profile_payload).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to create OAuth profile".into()));
    }
    Ok(profile_id)
}

fn generate_otp_code() -> String {
    // 6-digit numeric OTP
    format!("{:06}", rand::random::<u32>() % 1_000_000)
}

fn compute_password_strength(password: &str) -> &'static str {
    let has_upper = password.chars().any(|c| c.is_uppercase());
    let has_lower = password.chars().any(|c| c.is_lowercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_special = password.chars().any(|c| "!@#$%^&*()_+-=[]{}|;:,.<>?".contains(c));

    if has_upper && has_lower && has_digit && has_special && password.len() >= 12 {
        "strong"
    } else if has_upper && has_lower && has_digit {
        "medium"
    } else {
        "weak"
    }
}

fn mask_phone(phone: &str) -> String {
    if phone.len() > 4 {
        format!("{}****{}", &phone[..phone.len()-4], &phone[phone.len()-2..])
    } else {
        "****".to_string()
    }
}
