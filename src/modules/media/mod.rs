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
// Media Engine
// Upload, manage, transform media files
// via Supabase Storage
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Upload
        .route("/upload", post(presign_upload))
        .route("/upload/confirm", post(confirm_upload))
        // Library
        .route("/library", get(get_media_library))
        .route("/library/:id", get(get_media_detail).delete(delete_media))
        // Transform
        .route("/transform/:id", post(transform_media))
        // Albums
        .route("/albums", get(get_albums).post(create_album))
        .route("/albums/:id", get(get_album_detail).put(update_album).delete(delete_album))
        .route("/albums/:id/items", post(add_to_album).delete(remove_from_album))
}

#[derive(Deserialize)]
pub struct PresignPayload {
    pub filename: String,
    pub content_type: String,
    pub folder: Option<String>,
    #[allow(dead_code)]
    pub size_bytes: Option<i64>,
}

#[derive(Deserialize)]
pub struct ConfirmUploadPayload {
    pub file_path: String,
    pub file_url: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: Option<i64>,
    pub folder: Option<String>,
}

#[derive(Deserialize)]
pub struct TransformPayload {
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub format: Option<String>,
    pub quality: Option<i32>,
}

#[derive(Deserialize)]
pub struct CreateAlbumPayload {
    pub name: String,
    pub description: Option<String>,
    pub visibility: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateAlbumPayload {
    pub name: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<String>,
}

#[derive(Deserialize)]
pub struct AddToAlbumPayload {
    pub media_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct MediaQuery {
    pub folder: Option<String>,
    pub content_type: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ─── Upload ────────────────────────────────────────────

async fn presign_upload(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<PresignPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let folder = payload.folder.unwrap_or_else(|| "uploads".to_string());
    let ext = payload.filename.split('.').last().unwrap_or("bin");
    let file_id = uuid::Uuid::new_v4().to_string();
    let path = format!("{}/{}/{}.{}", uid, folder, file_id, ext);

    // Build Supabase storage presigned URL
    let bucket = "media";
    let presign_url = format!(
        "{}/storage/v1/object/{bucket}/{path}?upsert=true",
        state.config.supabase_url
    );

    ok_json(json!({
        "upload_url": presign_url,
        "file_path": path,
        "file_id": file_id,
        "method": "PUT",
        "headers": {
            "Content-Type": payload.content_type,
            "x-upsert": "true"
        }
    }))
}

async fn confirm_upload(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<ConfirmUploadPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let body = json!({
        "user_id": uid,
        "file_path": payload.file_path,
        "file_url": payload.file_url,
        "filename": payload.filename,
        "content_type": payload.content_type,
        "size_bytes": payload.size_bytes,
        "folder": payload.folder,
    });

    let res = state.http.post(format!("{}/rest/v1/media", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "media_id": data.get("id").and_then(|v| v.as_str()),
        "message": "Upload confirmed"
    }))
}

// ─── Library ───────────────────────────────────────────

async fn get_media_library(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<MediaQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let mut url = format!(
        "{}/rest/v1/media?user_id=eq.{uid}&order=created_at.desc",
        state.rest_url()
    );
    if let Some(ref folder) = query.folder {
        url.push_str(&format!("&folder=eq.{folder}"));
    }
    if let Some(ref ct) = query.content_type {
        url.push_str(&format!("&content_type=like.{ct}%"));
    }
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    url.push_str(&format!("&limit={limit}&offset={offset}"));

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let media: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "media": media, "total": media.len() }))
}

async fn get_media_detail(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(media_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!("{}/rest/v1/media?id=eq.{media_id}&user_id=eq.{uid}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let items: Vec<Value> = res.json().await.unwrap_or_default();
    let item = items.first().ok_or(ApiError::NotFound("Media not found".into()))?;
    ok_json(item.clone())
}

async fn delete_media(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(media_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    // Verify ownership
    let check_url = format!("{}/rest/v1/media?id=eq.{media_id}&user_id=eq.{uid}", state.rest_url());
    let check = state.http.get(&check_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let rows: Vec<Value> = check.json().await.unwrap_or_default();
    if rows.is_empty() { return Err(ApiError::Forbidden("Not your media".into())); }

    // Get file_path to delete from storage
    let file_path = rows.first()
        .and_then(|r| r.get("file_path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Delete from storage
    let _ = state.http.delete(format!(
        "{}/storage/v1/object/media/{file_path}",
        state.config.supabase_url
    )).headers(state.supabase_headers()).send().await;

    // Delete from DB
    let res = state.http.delete(format!("{}/rest/v1/media?id=eq.{media_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to delete media".into())); }

    ok_message("Media deleted")
}

// ─── Transform ─────────────────────────────────────────

async fn transform_media(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(media_id): Path<String>,
    Json(payload): Json<TransformPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    // Get original — verify ownership
    let url = format!("{}/rest/v1/media?id=eq.{media_id}&user_id=eq.{uid}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let items: Vec<Value> = res.json().await.unwrap_or_default();
    let _item = items.first().ok_or(ApiError::NotFound("Media not found".into()))?;

    // Get file_path from the media record for correct storage URL
    let file_path = _item.get("file_path").and_then(|v| v.as_str()).unwrap_or("");

    // Build transformed URL with query params
    let mut transform_params = Vec::new();
    if let Some(w) = payload.width { transform_params.push(format!("width={w}")); }
    if let Some(h) = payload.height { transform_params.push(format!("height={h}")); }
    if let Some(ref f) = payload.format { transform_params.push(format!("format={f}")); }
    if let Some(q) = payload.quality { transform_params.push(format!("quality={q}")); }

    let transformed_url = format!(
        "{}/storage/v1/render/image/public/media/{}?{}",
        state.config.supabase_url,
        file_path,
        transform_params.join("&")
    );

    ok_json(json!({
        "original_id": media_id,
        "transformed_url": transformed_url,
        "params": {
            "width": payload.width,
            "height": payload.height,
            "format": payload.format,
            "quality": payload.quality
        }
    }))
}

// ─── Albums ────────────────────────────────────────────

async fn get_albums(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!(
        "{}/rest/v1/media_albums?user_id=eq.{uid}&order=created_at.desc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let albums: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "albums": albums }))
}

async fn create_album(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<CreateAlbumPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let body = json!({
        "user_id": uid,
        "name": payload.name,
        "description": payload.description,
        "visibility": payload.visibility.unwrap_or_else(|| "private".to_string())
    });
    let res = state.http.post(format!("{}/rest/v1/media_albums", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    ok_json(json!({ "album_id": data.get("id").and_then(|v| v.as_str()), "message": "Album created" }))
}

async fn get_album_detail(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(album_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let url = format!("{}/rest/v1/media_albums?id=eq.{album_id}&user_id=eq.{uid}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let albums: Vec<Value> = res.json().await.unwrap_or_default();
    let album = albums.first().ok_or(ApiError::NotFound("Album not found".into()))?;
    ok_json(album.clone())
}

async fn update_album(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(album_id): Path<String>,
    Json(payload): Json<UpdateAlbumPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    // Verify ownership
    let check_url = format!("{}/rest/v1/media_albums?id=eq.{album_id}&user_id=eq.{uid}", state.rest_url());
    let check = state.http.get(&check_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let rows: Vec<Value> = check.json().await.unwrap_or_default();
    if rows.is_empty() { return Err(ApiError::Forbidden("Not your album".into())); }

    let mut body = serde_json::Map::new();
    if let Some(name) = payload.name { body.insert("name".into(), json!(name)); }
    if let Some(desc) = payload.description { body.insert("description".into(), json!(desc)); }
    if let Some(vis) = payload.visibility { body.insert("visibility".into(), json!(vis)); }
    if body.is_empty() { return Err(ApiError::BadRequest("No fields to update".into())); }

    let res = state.http.patch(format!("{}/rest/v1/media_albums?id=eq.{album_id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!(body))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to update album".into())); }
    ok_message("Album updated")
}

async fn delete_album(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(album_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    // Verify ownership
    let check_url = format!("{}/rest/v1/media_albums?id=eq.{album_id}&user_id=eq.{uid}", state.rest_url());
    let check = state.http.get(&check_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let rows: Vec<Value> = check.json().await.unwrap_or_default();
    if rows.is_empty() { return Err(ApiError::Forbidden("Not your album".into())); }

    // Remove all items first
    let _ = state.http.delete(format!("{}/rest/v1/media_album_items?album_id=eq.{album_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let res = state.http.delete(format!("{}/rest/v1/media_albums?id=eq.{album_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    if !res.status().is_success() { return Err(ApiError::Internal("Failed to delete album".into())); }
    ok_message("Album deleted")
}

async fn add_to_album(
    State(state): State<SharedState>,
    Path(album_id): Path<String>,
    Json(payload): Json<AddToAlbumPayload>,
) -> ApiResult {
    for media_id in &payload.media_ids {
        let _ = state.http.post(format!("{}/rest/v1/media_album_items", state.rest_url()))
            .headers(state.supabase_headers())
            .json(&json!({ "album_id": album_id, "media_id": media_id }))
            .send().await;
    }
    ok_message("Media added to album")
}

async fn remove_from_album(
    State(state): State<SharedState>,
    Path(album_id): Path<String>,
    Json(payload): Json<AddToAlbumPayload>,
) -> ApiResult {
    for media_id in &payload.media_ids {
        let _ = state.http.delete(format!(
            "{}/rest/v1/media_album_items?album_id=eq.{album_id}&media_id=eq.{media_id}",
            state.rest_url()
        )).headers(state.supabase_headers()).send().await;
    }
    ok_message("Media removed from album")
}
