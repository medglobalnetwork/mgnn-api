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
// Localization Engine
// i18n translations, locale management,
// user language preferences
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Supported locales
        .route("/locales", get(list_locales))
        // Translation strings
        .route("/translations", get(get_translations))
        .route("/translations/:locale", get(get_locale_translations))
        // User preference
        .route("/preference", get(get_language_preference).put(update_language_preference))
        // Admin: manage translations
        .route("/admin/translations", post(upsert_translation))
        .route("/admin/translations/:locale/:key", put(update_translation_key))
}

#[derive(Deserialize)]
pub struct TranslationsQuery {
    pub keys: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct UpsertTranslationPayload {
    pub locale: String,
    pub key: String,
    pub value: String,
    pub context: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTranslationPayload {
    pub value: String,
    pub context: Option<String>,
}

#[derive(Deserialize)]
pub struct LanguagePreferencePayload {
    pub language: String,
    pub dialect: Option<String>,
}

// Built-in locales
const SUPPORTED_LOCALES: &[(&str, &str, bool)] = &[
    ("en", "English", true),
    ("hi", "Hindi", true),
    ("bn", "Bengali", false),
    ("ta", "Tamil", false),
    ("te", "Telugu", false),
    ("mr", "Marathi", false),
    ("gu", "Gujarati", false),
    ("kn", "Kannada", false),
    ("ml", "Malayalam", false),
    ("pa", "Punjabi", false),
    ("or", "Odia", false),
    ("as", "Assamese", false),
];

// ─── Supported Locales ───────────────────────────────

async fn list_locales() -> ApiResult {
    let locales: Vec<Value> = SUPPORTED_LOCALES.iter().map(|(code, name, primary)| {
        json!({
            "code": code,
            "name": name,
            "primary": primary,
            "completion": if *primary { 100 } else { 0 }
        })
    }).collect();

    ok_json(json!({ "locales": locales, "default": "en" }))
}

// ─── Translations ─────────────────────────────────────

async fn get_translations(
    State(state): State<SharedState>,
    Query(query): Query<TranslationsQuery>,
) -> ApiResult {
    let mut url = format!("{}/rest/v1/translations?locale=eq.en&order=key.asc", state.rest_url());

    if let Some(ref keys) = query.keys {
        let key_filter = keys.iter().map(|k| format!("key=eq.{k}")).collect::<Vec<_>>().join(",");
        if !key_filter.is_empty() {
            url.push_str(&format!("&or=({key_filter})"));
        }
    }

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let translations: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Convert to key-value map
    let mut map = serde_json::Map::new();
    for t in &translations {
        if let (Some(key), Some(value)) = (
            t.get("key").and_then(|v| v.as_str()),
            t.get("value").and_then(|v| v.as_str()),
        ) {
            map.insert(key.to_string(), json!(value));
        }
    }

    ok_json(json!({ "locale": "en", "translations": map }))
}

async fn get_locale_translations(
    State(state): State<SharedState>,
    Path(locale): Path<String>,
    Query(query): Query<TranslationsQuery>,
) -> ApiResult {
    let mut url = format!("{}/rest/v1/translations?locale=eq.{locale}&order=key.asc", state.rest_url());

    if let Some(ref keys) = query.keys {
        let key_filter = keys.iter().map(|k| format!("key=eq.{k}")).collect::<Vec<_>>().join(",");
        if !key_filter.is_empty() {
            url.push_str(&format!("&or=({key_filter})"));
        }
    }

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let translations: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let mut map = serde_json::Map::new();
    for t in &translations {
        if let (Some(key), Some(value)) = (
            t.get("key").and_then(|v| v.as_str()),
            t.get("value").and_then(|v| v.as_str()),
        ) {
            map.insert(key.to_string(), json!(value));
        }
    }

    // Fallback to English for missing keys
    let en_url = format!("{}/rest/v1/translations?locale=eq.en&order=key.asc", state.rest_url());
    if let Ok(r) = state.http.get(&en_url).headers(state.supabase_headers()).send().await {
        if let Ok(en_translations) = r.json::<Vec<Value>>().await {
            for t in &en_translations {
                if let (Some(key), Some(value)) = (
                    t.get("key").and_then(|v| v.as_str()),
                    t.get("value").and_then(|v| v.as_str()),
                ) {
                    map.entry(key.to_string()).or_insert_with(|| json!(value));
                }
            }
        }
    }

    ok_json(json!({ "locale": locale.as_str(), "translations": map }))
}

// ─── User Language Preference ─────────────────────────

async fn get_language_preference(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!(
        "{}/rest/v1/profiles?id=eq.{uid}&select=language,dialect",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let profiles: Vec<Value> = res.json().await.unwrap_or_default();
    let profile = profiles.first().ok_or(ApiError::NotFound("Profile not found".into()))?;

    ok_json(json!({
        "language": profile.get("language").and_then(|v| v.as_str()).unwrap_or("en"),
        "dialect": profile.get("dialect").and_then(|v| v.as_str()).unwrap_or(""),
    }))
}

async fn update_language_preference(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<LanguagePreferencePayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let res = state.http.patch(format!("{}/rest/v1/profiles?id=eq.{uid}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!({
            "language": payload.language,
            "dialect": payload.dialect,
        }))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update language preference".into()));
    }

    ok_message("Language preference updated")
}

// ─── Admin: Manage Translations ──────────────────────

async fn upsert_translation(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<UpsertTranslationPayload>,
) -> ApiResult {
    let _uid = &auth.claims.profile_id;
    let body = json!({
        "locale": payload.locale,
        "key": payload.key,
        "value": payload.value,
        "context": payload.context,
    });

    let res = state.http.post(format!("{}/rest/v1/translations", state.rest_url()))
        .headers(state.supabase_headers())
        .header("Prefer", "resolution=merge-duplicates")
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to upsert translation".into()));
    }

    ok_message("Translation upserted")
}

async fn update_translation_key(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((locale, key)): Path<(String, String)>,
    Json(payload): Json<UpdateTranslationPayload>,
) -> ApiResult {
    let _uid = &auth.claims.profile_id;
    let res = state.http.patch(format!(
        "{}/rest/v1/translations?locale=eq.{locale}&key=eq.{key}",
        state.rest_url()
    ))
    .headers(state.supabase_headers())
    .json(&json!({
        "value": payload.value,
        "context": payload.context,
    }))
    .send().await
    .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update translation".into()));
    }

    ok_message("Translation updated")
}
