use axum::{Router, routing::{get, put, post, delete}, Json, extract::State};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::extractors::AuthUser;
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json, ok_message};
use crate::modules::audit;
use argon2::PasswordVerifier;

// ──────────────────────────────────────────────
// Settings & Security Engine  (Screens 135–142)
// Full Supabase-backed implementation
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Screen 135 – Settings Home
        .route("/", get(get_settings_home))
        // Screen 136 – Account Settings
        .route("/account", get(get_account).put(update_account))
        .route("/account/email", put(update_email))
        .route("/account/phone", put(update_phone))
        // Screen 137 – Privacy Center
        .route("/privacy", get(get_privacy).put(update_privacy))
        // Screen 138 – Security Center
        .route("/security", get(get_security_status))
        .route("/security/password", put(change_password))
        .route("/security/2fa", get(get_2fa_status).put(toggle_2fa))
        .route("/security/2fa/verify", post(verify_2fa))
        .route("/security/recovery", put(update_recovery))
        .route("/security/backup-codes", post(generate_backup_codes))
        // Screen 139 – Sessions & Devices
        .route("/sessions", get(get_sessions))
        .route("/sessions/:id", delete(delete_session))
        .route("/sessions", delete(delete_all_sessions))
        // Screen 140 – Notification Preferences
        .route("/notifications", get(get_notif_prefs).put(update_notif_prefs))
        .route("/notifications/quiet-hours", get(get_quiet_hours).put(update_quiet_hours))
        // Screen 141 – Connected Accounts
        .route("/connected-accounts", get(get_connected_accounts))
        .route("/connected-accounts/:provider/disconnect", post(disconnect_provider))
        .route("/connected-accounts/:provider/primary", put(set_primary_login))
        // Screen 142 – Data & Account Management
        .route("/data/export", post(request_data_export))
        .route("/data/export/:id", get(get_export_status))
        .route("/account/deactivate", post(deactivate_account))
        .route("/account/delete", post(delete_account))
}

// ══════════════════════════════════════════════
// Screen 135 – Settings Home
// ══════════════════════════════════════════════

async fn get_settings_home(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    let url = format!("{}/rest/v1/profiles?id=eq.{}&select=id,full_name,email,phone,account_type,verified", state.rest_url(), claims.claims.sub);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let profiles: serde_json::Value = res.json().await?;
    let profile = profiles.as_array().and_then(|a| a.first()).cloned().unwrap_or(json!({}));

    let privacy_url = format!("{}/rest/v1/privacy_settings?user_id=eq.{}&select=*", state.rest_url(), claims.claims.sub);
    let priv_res = state.http.get(&privacy_url).headers(state.supabase_headers()).send().await?;
    let privacy: serde_json::Value = priv_res.json().await?;

    let sec_url = format!("{}/rest/v1/security_settings?user_id=eq.{}&select=*", state.rest_url(), claims.claims.sub);
    let sec_res = state.http.get(&sec_url).headers(state.supabase_headers()).send().await?;
    let security: serde_json::Value = sec_res.json().await?;

    ok_json(json!({
        "profile": profile,
        "privacy": privacy,
        "security": security,
        "sections": [
            {"id": "account", "label": "Account", "icon": "user"},
            {"id": "security", "label": "Security", "icon": "shield"},
            {"id": "privacy", "label": "Privacy", "icon": "eye-off"},
            {"id": "notifications", "label": "Notifications", "icon": "bell"},
            {"id": "appearance", "label": "Appearance", "icon": "palette"},
            {"id": "language", "label": "Language", "icon": "globe"},
            {"id": "accessibility", "label": "Accessibility", "icon": "accessibility"},
            {"id": "connected_accounts", "label": "Connected Accounts", "icon": "link"},
            {"id": "sessions", "label": "Sessions & Devices", "icon": "monitor"},
            {"id": "data", "label": "Data & Privacy", "icon": "database"},
            {"id": "help", "label": "Help & Support", "icon": "help-circle"}
        ]
    }))
}

// ══════════════════════════════════════════════
// Screen 136 – Account Settings
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct AccountUpdate {
    first_name: Option<String>,
    last_name: Option<String>,
    preferred_name: Option<String>,
    username: Option<String>,
    headline: Option<String>,
    country: Option<String>,
    timezone: Option<String>,
}

#[derive(Deserialize)]
struct EmailUpdate {
    email: String,
    otp: String,
}

#[derive(Deserialize)]
struct PhoneUpdate {
    phone: String,
    otp: String,
}

async fn get_account(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    let url = format!(
        "{}/rest/v1/profiles?id=eq.{}&select=id,full_name,email,phone,username,headline,country,timezone,preferred_name,account_type,verified,created_at",
        state.rest_url(), claims.claims.sub
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;
    ok_json(data)
}

async fn update_account(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<AccountUpdate>,
) -> ApiResult {
    // If username is being changed, validate it
    if let Some(ref username) = payload.username {
        if username.len() < 3 || username.len() > 30 {
            return Err(ApiError::BadRequest("Username must be 3–30 characters".into()));
        }
        if !username.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(ApiError::BadRequest("Username can only contain letters, numbers, and underscores".into()));
        }
        // Check uniqueness
        let check_url = format!("{}/rest/v1/profiles?username=eq.{}&select=id", state.rest_url(), username);
        let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await?;
        let existing: serde_json::Value = check_res.json().await?;
        if let Some(arr) = existing.as_array() {
            if !arr.is_empty() && arr[0]["id"].as_str() != Some(claims.claims.sub.as_str()) {
                return Err(ApiError::Conflict("Username already taken".into()));
            }
        }
    }

    let mut update = json!({});
    if let Some(v) = payload.first_name { update["first_name"] = json!(v); }
    if let Some(v) = payload.last_name { update["last_name"] = json!(v); }
    if let Some(v) = payload.preferred_name { update["preferred_name"] = json!(v); }
    if let Some(v) = payload.username { update["username"] = json!(v); }
    if let Some(v) = payload.headline { update["headline"] = json!(v); }
    if let Some(v) = payload.country { update["country"] = json!(v); }
    if let Some(v) = payload.timezone { update["timezone"] = json!(v); }

    let url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), claims.claims.sub);
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update account".into()));
    }

    audit::log_action(&claims.claims.sub, "account.update", "profile", &claims.claims.sub).await;
    ok_message("Account updated")
}

async fn update_email(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<EmailUpdate>,
) -> ApiResult {
    // Verify OTP first
    let otp_url = format!("{}/rest/v1/otp_verifications?user_id=eq.{}&code=eq.{}&purpose=eq.email_change&used=eq.false&expires_at.gt.{}&select=id",
        state.rest_url(), claims.claims.sub, payload.otp, chrono::Utc::now().to_rfc3339());
    let otp_res = state.http.get(&otp_url).headers(state.supabase_headers()).send().await?;
    let otp_data: serde_json::Value = otp_res.json().await?;
    if otp_data.as_array().map_or(true, |a| a.is_empty()) {
        return Err(ApiError::BadRequest("Invalid or expired OTP".into()));
    }

    // Check duplicate email
    let check_url = format!("{}/rest/v1/profiles?email=eq.{}&select=id", state.rest_url(), payload.email);
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await?;
    let existing: serde_json::Value = check_res.json().await?;
    if let Some(arr) = existing.as_array() {
        if !arr.is_empty() && arr[0]["id"].as_str() != Some(claims.claims.sub.as_str()) {
            return Err(ApiError::Conflict("Email already registered".into()));
        }
    }

    let update = json!({"email": payload.email, "email_verified": true});
    let url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), claims.claims.sub);
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update email".into()));
    }

    audit::log_action(&claims.claims.sub, "account.email_change", "profile", &claims.claims.sub).await;
    ok_message("Email updated successfully")
}

async fn update_phone(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<PhoneUpdate>,
) -> ApiResult {
    let otp_url = format!("{}/rest/v1/otp_verifications?user_id=eq.{}&code=eq.{}&purpose=eq.phone_change&used=eq.false&expires_at.gt.{}&select=id",
        state.rest_url(), claims.claims.sub, payload.otp, chrono::Utc::now().to_rfc3339());
    let otp_res = state.http.get(&otp_url).headers(state.supabase_headers()).send().await?;
    let otp_data: serde_json::Value = otp_res.json().await?;
    if otp_data.as_array().map_or(true, |a| a.is_empty()) {
        return Err(ApiError::BadRequest("Invalid or expired OTP".into()));
    }

    let update = json!({"phone": payload.phone, "phone_verified": true});
    let url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), claims.claims.sub);
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update phone".into()));
    }

    audit::log_action(&claims.claims.sub, "account.phone_change", "profile", &claims.claims.sub).await;
    ok_message("Phone updated successfully")
}

// ══════════════════════════════════════════════
// Screen 137 – Privacy Center
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct PrivacyUpdate {
    profile_visibility: Option<String>,     // public, verified, connections, only_me
    contact_email_visible: Option<bool>,
    contact_phone_visible: Option<bool>,
    contact_website_visible: Option<bool>,
    contact_social_visible: Option<bool>,
    professional_qualification_visible: Option<bool>,
    professional_license_visible: Option<bool>,
    professional_experience_visible: Option<bool>,
    professional_research_visible: Option<bool>,
    professional_organization_visible: Option<bool>,
    activity_posts_visible: Option<bool>,
    activity_comments_visible: Option<bool>,
    activity_reactions_visible: Option<bool>,
    activity_followers_visible: Option<bool>,
    activity_following_visible: Option<bool>,
    activity_connections_visible: Option<bool>,
    messaging_allow_from: Option<String>,    // everyone, followers, connections, verified, nobody
    search_show_in_search: Option<bool>,
    search_allow_recommendation: Option<bool>,
    search_allow_ai_recommendations: Option<bool>,
}

async fn get_privacy(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    let url = format!("{}/rest/v1/privacy_settings?user_id=eq.{}&select=*", state.rest_url(), claims.claims.sub);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;
    let settings = data.as_array().and_then(|a| a.first().cloned()).unwrap_or(json!({
        "profile_visibility": "public",
        "contact_email_visible": false,
        "contact_phone_visible": false,
        "messaging_allow_from": "connections",
        "search_show_in_search": true
    }));
    ok_json(settings)
}

async fn update_privacy(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<PrivacyUpdate>,
) -> ApiResult {
    // Validate enum fields
    if let Some(ref v) = payload.profile_visibility {
        let valid = ["public", "verified", "connections", "only_me"];
        if !valid.contains(&v.as_str()) {
            return Err(ApiError::BadRequest("Invalid profile_visibility value".into()));
        }
    }
    if let Some(ref v) = payload.messaging_allow_from {
        let valid = ["everyone", "followers", "connections", "verified", "nobody"];
        if !valid.contains(&v.as_str()) {
            return Err(ApiError::BadRequest("Invalid messaging_allow_from value".into()));
        }
    }

    let mut update = json!({});
    if let Some(v) = payload.profile_visibility { update["profile_visibility"] = json!(v); }
    if let Some(v) = payload.contact_email_visible { update["contact_email_visible"] = json!(v); }
    if let Some(v) = payload.contact_phone_visible { update["contact_phone_visible"] = json!(v); }
    if let Some(v) = payload.contact_website_visible { update["contact_website_visible"] = json!(v); }
    if let Some(v) = payload.contact_social_visible { update["contact_social_visible"] = json!(v); }
    if let Some(v) = payload.professional_qualification_visible { update["professional_qualification_visible"] = json!(v); }
    if let Some(v) = payload.professional_license_visible { update["professional_license_visible"] = json!(v); }
    if let Some(v) = payload.professional_experience_visible { update["professional_experience_visible"] = json!(v); }
    if let Some(v) = payload.professional_research_visible { update["professional_research_visible"] = json!(v); }
    if let Some(v) = payload.professional_organization_visible { update["professional_organization_visible"] = json!(v); }
    if let Some(v) = payload.activity_posts_visible { update["activity_posts_visible"] = json!(v); }
    if let Some(v) = payload.activity_comments_visible { update["activity_comments_visible"] = json!(v); }
    if let Some(v) = payload.activity_reactions_visible { update["activity_reactions_visible"] = json!(v); }
    if let Some(v) = payload.activity_followers_visible { update["activity_followers_visible"] = json!(v); }
    if let Some(v) = payload.activity_following_visible { update["activity_following_visible"] = json!(v); }
    if let Some(v) = payload.activity_connections_visible { update["activity_connections_visible"] = json!(v); }
    if let Some(v) = payload.messaging_allow_from { update["messaging_allow_from"] = json!(v); }
    if let Some(v) = payload.search_show_in_search { update["search_show_in_search"] = json!(v); }
    if let Some(v) = payload.search_allow_recommendation { update["search_allow_recommendation"] = json!(v); }
    if let Some(v) = payload.search_allow_ai_recommendations { update["search_allow_ai_recommendations"] = json!(v); }

    // Upsert: try update first, insert if no row exists
    let url = format!("{}/rest/v1/privacy_settings?user_id=eq.{}", state.rest_url(), claims.claims.sub);
    let check_res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let existing: serde_json::Value = check_res.json().await?;

    let needs_insert = existing.as_array().map_or(true, |a| a.is_empty());

    if needs_insert {
        update["user_id"] = json!(claims.claims.sub);
        let insert_url = format!("{}/rest/v1/privacy_settings", state.rest_url());
        let res = state.http.post(&insert_url).headers(state.supabase_headers()).json(&update).send().await?;
        if !res.status().is_success() {
            return Err(ApiError::Internal("Failed to create privacy settings".into()));
        }
    } else {
        let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
        if !res.status().is_success() {
            return Err(ApiError::Internal("Failed to update privacy settings".into()));
        }
    }

    audit::log_action(&claims.claims.sub, "settings.privacy_update", "privacy_settings", &claims.claims.sub).await;
    ok_message("Privacy settings updated")
}

// ══════════════════════════════════════════════
// Screen 138 – Security Center
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct PasswordChange {
    current_password: String,
    new_password: String,
}

#[derive(Deserialize)]
struct TwoFactorToggle {
    method: String,   // authenticator, email_otp, phone_otp
    enabled: bool,
}

#[derive(Deserialize)]
struct TwoFactorVerify {
    method: String,
    code: String,
}

#[derive(Deserialize)]
struct RecoveryUpdate {
    recovery_email: Option<String>,
    recovery_phone: Option<String>,
}

async fn get_security_status(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    let url = format!("{}/rest/v1/security_settings?user_id=eq.{}&select=*", state.rest_url(), claims.claims.sub);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;

    let settings = data.as_array().and_then(|a| a.first().cloned()).unwrap_or(json!({}));

    let password_strength = settings.get("password_strength").and_then(|v| v.as_str()).unwrap_or("unknown");
    let two_fa_enabled = settings.get("two_fa_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let recovery_complete = settings.get("recovery_email").is_some() || settings.get("recovery_phone").is_some();

    let risk_level = if two_fa_enabled && recovery_complete && password_strength == "strong" {
        "low"
    } else if two_fa_enabled || password_strength == "strong" {
        "medium"
    } else {
        "high"
    };

    let mut recommendations = vec![];
    if !two_fa_enabled { recommendations.push("Enable 2FA for better security"); }
    if password_strength != "strong" { recommendations.push("Update your password to a stronger one"); }
    if !recovery_complete { recommendations.push("Add recovery email or phone"); }

    ok_json(json!({
        "password_strength": password_strength,
        "two_fa_enabled": two_fa_enabled,
        "two_fa_methods": settings.get("two_fa_methods").cloned().unwrap_or(json!([])),
        "recovery_complete": recovery_complete,
        "recovery_email_set": settings.get("recovery_email").is_some(),
        "recovery_phone_set": settings.get("recovery_phone").is_some(),
        "backup_codes_generated": settings.get("backup_codes_generated").and_then(|v| v.as_bool()).unwrap_or(false),
        "last_password_change": settings.get("last_password_change"),
        "risk_level": risk_level,
        "recommendations": recommendations
    }))
}

async fn change_password(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<PasswordChange>,
) -> ApiResult {
    // Verify current password first
    let verify_url = format!("{}/rest/v1/security_settings?user_id=eq.{}&select=password_hash", state.rest_url(), claims.claims.sub);
    let verify_res = state.http.get(&verify_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let rows: Vec<Value> = verify_res.json().await.unwrap_or_default();
    if let Some(row) = rows.first() {
        if let Some(hash_str) = row.get("password_hash").and_then(|v| v.as_str()) {
            let parsed_hash = argon2::PasswordHash::new(hash_str)
                .map_err(|e| ApiError::Internal(format!("Hash parse error: {e}")))?;
            argon2::Argon2::default()
                .verify_password(payload.current_password.as_bytes(), &parsed_hash)
                .map_err(|_| ApiError::BadRequest("Current password is incorrect".into()))?;
        }
    }

    // Validate new password strength
    if payload.new_password.len() < 8 {
        return Err(ApiError::BadRequest("Password must be at least 8 characters".into()));
    }
    let has_upper = payload.new_password.chars().any(|c| c.is_uppercase());
    let has_lower = payload.new_password.chars().any(|c| c.is_lowercase());
    let has_digit = payload.new_password.chars().any(|c| c.is_ascii_digit());
    let has_special = payload.new_password.chars().any(|c| "!@#$%^&*()_+-=[]{}|;:,.<>?".contains(c));
    let strength = if has_upper && has_lower && has_digit && has_special && payload.new_password.len() >= 12 {
        "strong"
    } else if has_upper && has_lower && has_digit {
        "medium"
    } else {
        "weak"
    };

    // Hash new password with argon2
    let salt = argon2::password_hash::SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let hash = argon2::PasswordHasher::hash_password(
        &argon2::Argon2::default(),
        payload.new_password.as_bytes(),
        &salt,
    ).map_err(|e| ApiError::Internal(format!("Password hash error: {e}")))?;

    let update = json!({
        "password_hash": hash.to_string(),
        "password_strength": strength,
        "last_password_change": chrono::Utc::now().to_rfc3339()
    });

    let url = format!("{}/rest/v1/security_settings?user_id=eq.{}", state.rest_url(), claims.claims.sub);
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update password".into()));
    }

    // Invalidate all refresh tokens except current session
    let invalidate_url = format!("{}/rest/v1/user_sessions?user_id=eq.{}&id=neq.{}", state.rest_url(), claims.claims.sub, claims.claims.sid);
    let _ = state.http.delete(&invalidate_url).headers(state.supabase_headers()).send().await;

    audit::log_action(&claims.claims.sub, "security.password_change", "security_settings", &claims.claims.sub).await;
    ok_message("Password changed successfully. Other sessions have been logged out.")
}

async fn get_2fa_status(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    let url = format!("{}/rest/v1/security_settings?user_id=eq.{}&select=two_fa_enabled,two_fa_methods", state.rest_url(), claims.claims.sub);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;
    ok_json(data)
}

async fn toggle_2fa(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<TwoFactorToggle>,
) -> ApiResult {
    let valid_methods = ["authenticator", "email_otp", "phone_otp"];
    if !valid_methods.contains(&payload.method.as_str()) {
        return Err(ApiError::BadRequest("Invalid 2FA method".into()));
    }

    // Admin accounts must have 2FA enabled
    if claims.claims.role == "admin" || claims.claims.role == "super_admin" {
        if !payload.enabled {
            return Err(ApiError::Forbidden("Admin accounts must have 2FA enabled".into()));
        }
    }

    let url = format!("{}/rest/v1/security_settings?user_id=eq.{}", state.rest_url(), claims.claims.sub);
    let get_res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let existing: serde_json::Value = get_res.json().await?;

    let mut methods: Vec<String> = existing.as_array()
        .and_then(|a| a.first())
        .and_then(|r| r.get("two_fa_methods"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    if payload.enabled {
        if !methods.contains(&payload.method) {
            methods.push(payload.method.clone());
        }
    } else {
        methods.retain(|m| m != &payload.method);
    }

    let update = json!({
        "two_fa_enabled": !methods.is_empty(),
        "two_fa_methods": methods
    });

    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update 2FA settings".into()));
    }

    audit::log_action(&claims.claims.sub, &format!("security.2fa_{}", if payload.enabled { "enable" } else { "disable" }), "security_settings", &claims.claims.sub).await;
    ok_message(if payload.enabled { "2FA method enabled" } else { "2FA method disabled" })
}

async fn verify_2fa(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<TwoFactorVerify>,
) -> ApiResult {
    let otp_url = format!("{}/rest/v1/otp_verifications?user_id=eq.{}&code=eq.{}&purpose=eq.2fa_{}&used=eq.false&expires_at.gt.{}&select=id",
        state.rest_url(), claims.claims.sub, payload.code, payload.method, chrono::Utc::now().to_rfc3339());
    let res = state.http.get(&otp_url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;
    if data.as_array().map_or(true, |a| a.is_empty()) {
        return Err(ApiError::BadRequest("Invalid or expired 2FA code".into()));
    }

    // Mark OTP as used
    if let Some(otp_id) = data[0]["id"].as_str() {
        let mark_url = format!("{}/rest/v1/otp_verifications?id=eq.{}", state.rest_url(), otp_id);
        let _ = state.http.patch(&mark_url).headers(state.supabase_headers()).json(&json!({"used": true})).send().await;
    }

    ok_message("2FA verified successfully")
}

async fn update_recovery(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<RecoveryUpdate>,
) -> ApiResult {
    let mut update = json!({});
    if let Some(v) = payload.recovery_email { update["recovery_email"] = json!(v); }
    if let Some(v) = payload.recovery_phone { update["recovery_phone"] = json!(v); }

    let url = format!("{}/rest/v1/security_settings?user_id=eq.{}", state.rest_url(), claims.claims.sub);
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update recovery info".into()));
    }

    audit::log_action(&claims.claims.sub, "security.recovery_update", "security_settings", &claims.claims.sub).await;
    ok_message("Recovery information updated")
}

async fn generate_backup_codes(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    let codes: Vec<String> = (0..10).map(|_| uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_string()).collect();

    let update = json!({
        "backup_codes": codes,
        "backup_codes_generated": true,
        "backup_codes_generated_at": chrono::Utc::now().to_rfc3339()
    });

    let url = format!("{}/rest/v1/security_settings?user_id=eq.{}", state.rest_url(), claims.claims.sub);
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to generate backup codes".into()));
    }

    audit::log_action(&claims.claims.sub, "security.backup_codes_generated", "security_settings", &claims.claims.sub).await;
    ok_json(json!({ "codes": codes, "warning": "Store these codes safely. They cannot be shown again." }))
}

// ══════════════════════════════════════════════
// Screen 139 – Sessions & Devices
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

async fn delete_session(
    State(state): State<SharedState>,
    claims: AuthUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult {
    if id == claims.claims.sid {
        return Err(ApiError::BadRequest("Cannot logout current session from here. Use /logout instead.".into()));
    }
    let url = format!("{}/rest/v1/user_sessions?id=eq.{}&user_id=eq.{}", state.rest_url(), id, claims.claims.sub);
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to logout device".into()));
    }
    audit::log_action(&claims.claims.sub, "session.logout_device", "user_sessions", &id).await;
    ok_message("Device logged out successfully")
}

async fn delete_all_sessions(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    // Delete all sessions except current
    let url = format!("{}/rest/v1/user_sessions?user_id=eq.{}&id=neq.{}", state.rest_url(), claims.claims.sub, claims.claims.sid);
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to logout all devices".into()));
    }
    audit::log_action(&claims.claims.sub, "session.logout_all", "user_sessions", &claims.claims.sub).await;
    ok_message("All other devices logged out successfully")
}

// ══════════════════════════════════════════════
// Screen 140 – Notification Preferences
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct NotifPrefsUpdate {
    // Channels
    channel_in_app: Option<bool>,
    channel_email: Option<bool>,
    channel_push: Option<bool>,
    // Categories
    cat_connections: Option<bool>,
    cat_messages: Option<bool>,
    cat_research: Option<bool>,
    cat_verification: Option<bool>,
    cat_organizations: Option<bool>,
    cat_security: Option<bool>,
    cat_announcements: Option<bool>,
    cat_marketing: Option<bool>,
    // Frequency
    frequency: Option<String>,  // instant, hourly, daily, weekly, never
}

#[derive(Deserialize)]
struct QuietHoursUpdate {
    quiet_hours_start: Option<String>,  // HH:mm
    quiet_hours_end: Option<String>,    // HH:mm
    timezone: Option<String>,
}

async fn get_notif_prefs(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    let url = format!("{}/rest/v1/notification_preferences?user_id=eq.{}&select=*", state.rest_url(), claims.claims.sub);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;
    let prefs = data.as_array().and_then(|a| a.first().cloned()).unwrap_or(json!({
        "channel_in_app": true, "channel_email": true, "channel_push": true,
        "cat_connections": true, "cat_messages": true, "cat_research": true,
        "frequency": "instant"
    }));
    ok_json(prefs)
}

async fn update_notif_prefs(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<NotifPrefsUpdate>,
) -> ApiResult {
    if let Some(ref f) = payload.frequency {
        let valid = ["instant", "hourly", "daily", "weekly", "never"];
        if !valid.contains(&f.as_str()) {
            return Err(ApiError::BadRequest("Invalid frequency value".into()));
        }
    }

    let mut update = json!({});
    if let Some(v) = payload.channel_in_app { update["channel_in_app"] = json!(v); }
    if let Some(v) = payload.channel_email { update["channel_email"] = json!(v); }
    if let Some(v) = payload.channel_push { update["channel_push"] = json!(v); }
    if let Some(v) = payload.cat_connections { update["cat_connections"] = json!(v); }
    if let Some(v) = payload.cat_messages { update["cat_messages"] = json!(v); }
    if let Some(v) = payload.cat_research { update["cat_research"] = json!(v); }
    if let Some(v) = payload.cat_verification { update["cat_verification"] = json!(v); }
    if let Some(v) = payload.cat_organizations { update["cat_organizations"] = json!(v); }
    if let Some(v) = payload.cat_security { update["cat_security"] = json!(v); }
    if let Some(v) = payload.cat_announcements { update["cat_announcements"] = json!(v); }
    if let Some(v) = payload.cat_marketing { update["cat_marketing"] = json!(v); }
    if let Some(v) = payload.frequency { update["frequency"] = json!(v); }

    // Upsert
    let url = format!("{}/rest/v1/notification_preferences?user_id=eq.{}", state.rest_url(), claims.claims.sub);
    let check = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let existing: serde_json::Value = check.json().await?;
    let needs_insert = existing.as_array().map_or(true, |a| a.is_empty());

    if needs_insert {
        update["user_id"] = json!(claims.claims.sub);
        let insert_url = format!("{}/rest/v1/notification_preferences", state.rest_url());
        let res = state.http.post(&insert_url).headers(state.supabase_headers()).json(&update).send().await?;
        if !res.status().is_success() { return Err(ApiError::Internal("Failed to create notification preferences".into())); }
    } else {
        let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
        if !res.status().is_success() { return Err(ApiError::Internal("Failed to update notification preferences".into())); }
    }

    ok_message("Notification preferences updated")
}

async fn get_quiet_hours(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    let url = format!("{}/rest/v1/notification_preferences?user_id=eq.{}&select=quiet_hours_start,quiet_hours_end,timezone", state.rest_url(), claims.claims.sub);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;
    ok_json(data)
}

async fn update_quiet_hours(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<QuietHoursUpdate>,
) -> ApiResult {
    let mut update = json!({});
    if let Some(v) = payload.quiet_hours_start { update["quiet_hours_start"] = json!(v); }
    if let Some(v) = payload.quiet_hours_end { update["quiet_hours_end"] = json!(v); }
    if let Some(v) = payload.timezone { update["timezone"] = json!(v); }

    let url = format!("{}/rest/v1/notification_preferences?user_id=eq.{}", state.rest_url(), claims.claims.sub);
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update quiet hours".into()));
    }
    ok_message("Quiet hours updated")
}

// ══════════════════════════════════════════════
// Screen 141 – Connected Accounts
// ══════════════════════════════════════════════

async fn get_connected_accounts(
    State(state): State<SharedState>,
    claims: AuthUser,
) -> ApiResult {
    let url = format!("{}/rest/v1/connected_accounts?user_id=eq.{}&select=provider,provider_account_id,connected_at,is_primary", state.rest_url(), claims.claims.sub);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;
    ok_json(data)
}

async fn disconnect_provider(
    State(state): State<SharedState>,
    claims: AuthUser,
    axum::extract::Path(provider): axum::extract::Path<String>,
) -> ApiResult {
    // Ensure at least one login method remains
    let url = format!("{}/rest/v1/connected_accounts?user_id=eq.{}&select=provider", state.rest_url(), claims.claims.sub);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;
    let count = data.as_array().map(|a| a.len()).unwrap_or(0);
    if count <= 1 {
        return Err(ApiError::Forbidden("Cannot disconnect the only authentication method".into()));
    }

    let del_url = format!("{}/rest/v1/connected_accounts?user_id=eq.{}&provider=eq.{}", state.rest_url(), claims.claims.sub, provider);
    let res = state.http.delete(&del_url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to disconnect provider".into()));
    }

    audit::log_action(&claims.claims.sub, &format!("account.disconnect_{}", provider), "connected_accounts", &claims.claims.sub).await;
    ok_message("Provider disconnected successfully")
}

async fn set_primary_login(
    State(state): State<SharedState>,
    claims: AuthUser,
    axum::extract::Path(provider): axum::extract::Path<String>,
) -> ApiResult {
    // Set all to is_primary=false
    let url = format!("{}/rest/v1/connected_accounts?user_id=eq.{}", state.rest_url(), claims.claims.sub);
    let _ = state.http.patch(&url).headers(state.supabase_headers()).json(&json!({"is_primary": false})).send().await;

    // Set selected to is_primary=true
    let prov_url = format!("{}/rest/v1/connected_accounts?user_id=eq.{}&provider=eq.{}", state.rest_url(), claims.claims.sub, provider);
    let res = state.http.patch(&prov_url).headers(state.supabase_headers()).json(&json!({"is_primary": true})).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to set primary login".into()));
    }

    ok_message("Primary login method updated")
}

// ══════════════════════════════════════════════
// Screen 142 – Data & Account Management
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct ExportRequest {
    categories: Vec<String>,  // profile, posts, research, messages, connections, settings
}

#[derive(Deserialize)]
struct DeactivateRequest {
    reason: Option<String>,
}

#[derive(Deserialize)]
struct DeleteRequest {
    password: String,
    confirmation: bool,
}

async fn request_data_export(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<ExportRequest>,
) -> ApiResult {
    let valid_categories = ["profile", "posts", "research", "messages", "connections", "settings"];
    for cat in &payload.categories {
        if !valid_categories.contains(&cat.as_str()) {
            return Err(ApiError::BadRequest(format!("Invalid export category: {}", cat)));
        }
    }

    let insert = json!({
        "user_id": claims.claims.sub,
        "categories": payload.categories,
        "status": "pending",
        "requested_at": chrono::Utc::now().to_rfc3339()
    });

    let url = format!("{}/rest/v1/data_exports", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers()).json(&insert).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to request data export".into()));
    }

    audit::log_action(&claims.claims.sub, "data.export_request", "data_exports", &claims.claims.sub).await;
    ok_message("Data export requested. You'll be notified when it's ready.")
}

async fn get_export_status(
    State(state): State<SharedState>,
    claims: AuthUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult {
    let url = format!("{}/rest/v1/data_exports?id=eq.{}&user_id=eq.{}&select=*", state.rest_url(), id, claims.claims.sub);
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let data: serde_json::Value = res.json().await?;
    ok_json(data)
}

async fn deactivate_account(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<DeactivateRequest>,
) -> ApiResult {
    let update = json!({
        "status": "deactivated",
        "deactivated_at": chrono::Utc::now().to_rfc3339(),
        "deactivation_reason": payload.reason
    });

    let url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), claims.claims.sub);
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to deactivate account".into()));
    }

    audit::log_action(&claims.claims.sub, "account.deactivate", "profile", &claims.claims.sub).await;
    ok_message("Account deactivated. You can reactivate by logging in again.")
}

async fn delete_account(
    State(state): State<SharedState>,
    claims: AuthUser,
    Json(payload): Json<DeleteRequest>,
) -> ApiResult {
    if !payload.confirmation {
        return Err(ApiError::BadRequest("You must confirm account deletion".into()));
    }

    // Verify password (re-authentication)
    let sec_url = format!("{}/rest/v1/security_settings?user_id=eq.{}&select=password_hash", state.rest_url(), claims.claims.sub);
    let sec_res = state.http.get(&sec_url).headers(state.supabase_headers()).send().await?;
    let sec_data: serde_json::Value = sec_res.json().await?;
    let hash_str = sec_data.as_array()
        .and_then(|a| a.first())
        .and_then(|r| r.get("password_hash"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let parsed_hash = argon2::PasswordHash::new(hash_str)
        .map_err(|_| ApiError::Internal("Invalid stored password hash".into()))?;
    let verified = argon2::PasswordVerifier::verify_password(
        &argon2::Argon2::default(),
        payload.password.as_bytes(),
        &parsed_hash,
    ).is_ok();

    if !verified {
        return Err(ApiError::Unauthorized("Incorrect password".into()));
    }

    // Mark for deletion with grace period (30 days)
    let update = json!({
        "status": "pending_deletion",
        "deletion_requested_at": chrono::Utc::now().to_rfc3339(),
        "deletion_scheduled_at": (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339()
    });

    let url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), claims.claims.sub);
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&update).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to schedule account deletion".into()));
    }

    audit::log_action(&claims.claims.sub, "account.delete_requested", "profile", &claims.claims.sub).await;
    ok_message("Account deletion scheduled. You have 30 days to cancel by logging in.")
}
