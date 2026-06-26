use axum::{
    Router,
    routing::{get, post},
    Json,
    extract::{State, Path},
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json, ok_message};
use crate::extractors::AuthUser;

// ─── Request Bodies ───────────────────────────────────────

#[derive(Deserialize)]
pub struct CreatePostPayload {
    pub content: String,
    pub post_type: Option<String>,       // text, article, case_study, question
    pub visibility: Option<String>,      // public, connections, private
    pub tags: Option<Vec<String>>,
    pub media_urls: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct UpdatePostPayload {
    pub content: Option<String>,
    pub visibility: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct ReactPayload {
    pub reaction_type: String, // like, insightful, celebrate, support
}

#[derive(Deserialize)]
pub struct AddCommentPayload {
    pub content: String,
    pub parent_comment_id: Option<String>, // for threaded replies
}

// ─── Router ───────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/feed", get(get_feed))
        .route("/posts", post(create_post))
        .route("/posts/:id", get(get_post).put(update_post).delete(delete_post))
        .route("/posts/:id/react", post(react_post).delete(unreact_post))
        .route("/posts/:id/comments", get(get_comments).post(add_comment))
        .route("/posts/:id/bookmark", post(bookmark_post).delete(remove_bookmark))
        .route("/bookmarks", get(get_bookmarks))
}

// ─── Handlers ─────────────────────────────────────────────

/// GET /content/feed — Paginated feed of posts from connections + self.
async fn get_feed(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Get connection IDs
    let conn_url = format!(
        "{}/rest/v1/connections?status=eq.accepted&or=(requester_id.eq.{uid},addressee_id.eq.{uid})&select=requester_id,addressee_id",
        state.rest_url()
    );
    let conn_res = state
        .http
        .get(&conn_url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let conn_rows: Vec<Value> = conn_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let mut author_ids = vec![uid.to_string()];
    for row in &conn_rows {
        if let Some(req) = row.get("requester_id").and_then(|v| v.as_str()) {
            if req != uid { author_ids.push(req.to_string()); }
        }
        if let Some(add) = row.get("addressee_id").and_then(|v| v.as_str()) {
            if add != uid { author_ids.push(add.to_string()); }
        }
    }

    // Fetch posts from these authors (public + connections visibility)
    let or_filter: Vec<String> = author_ids.iter()
        .map(|id| format!("author_id.eq.{id}"))
        .collect();
    let or_str = or_filter.join(",");

    let url = format!(
        "{}/rest/v1/posts?or=({or_str})&visibility=in.(public,connections)&order=created_at.desc&limit=20&select=*,profiles:author_id(id,full_name,avatar_url,headline,primary_category)",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let posts: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Enrich each post with reaction counts
    let mut enriched_posts = Vec::new();
    for post in &posts {
        let post_id = post.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let reactions = count_rows(&state, "post_reactions", &format!("post_id.eq.{post_id}")).await?;
        let comments = count_rows(&state, "post_comments", &format!("post_id.eq.{post_id}")).await?;
        let bookmarks = count_rows(&state, "post_bookmarks", &format!("post_id.eq.{post_id}")).await?;

        // Check if current user reacted
        let my_react_url = format!(
            "{}/rest/v1/post_reactions?post_id=eq.{post_id}&user_id=eq.{uid}&select=reaction_type&limit=1",
            state.rest_url()
        );
        let my_react_res = state.http.get(&my_react_url).headers(state.supabase_headers()).send().await;
        let my_reaction: Option<String> = if let Ok(r) = my_react_res {
            if let Ok(rows) = r.json::<Vec<Value>>().await {
                rows.first().and_then(|r| r.get("reaction_type")?.as_str()).map(|s| s.to_string())
            } else { None }
        } else { None };

        let mut enriched = post.clone();
        if let Some(obj) = enriched.as_object_mut() {
            obj.insert("reaction_count".into(), json!(reactions));
            obj.insert("comment_count".into(), json!(comments));
            obj.insert("bookmark_count".into(), json!(bookmarks));
            obj.insert("my_reaction".into(), json!(my_reaction));
        }
        enriched_posts.push(enriched);
    }

    ok_json(json!(enriched_posts))
}

/// POST /content/posts — Create a post.
async fn create_post(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<CreatePostPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if payload.content.trim().is_empty() {
        return Err(ApiError::BadRequest("Content cannot be empty".into()));
    }

    let body = json!({
        "author_id": uid,
        "content": payload.content,
        "post_type": payload.post_type.unwrap_or_else(|| "text".into()),
        "visibility": payload.visibility.unwrap_or_else(|| "public".into()),
        "tags": payload.tags.unwrap_or_default(),
        "media_urls": payload.media_urls.unwrap_or_default()
    });

    let url = format!("{}/rest/v1/posts", state.rest_url());
    let res = state
        .http
        .post(&url)
        .headers(state.supabase_headers())
        .header("Prefer", "return=representation")
        .json(&body)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!("Failed to create post: {err}")));
    }

    let rows: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "id": rows.first().and_then(|r| r.get("id")),
        "message": "Post created"
    }))
}

/// GET /content/posts/:id — Get a single post with comments and reactions.
async fn get_post(
    State(state): State<SharedState>,
    Path(post_id): Path<String>,
) -> ApiResult {
    let url = format!(
        "{}/rest/v1/posts?id=eq.{post_id}&select=*,profiles:author_id(id,full_name,avatar_url,headline,primary_category)",
        state.rest_url()
    );
    let res = state
        .http
        .get(&url)
        .headers(state.supabase_headers())
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let rows: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let post = rows.first().ok_or_else(|| ApiError::NotFound("Post not found".into()))?;

    let reactions = count_rows(&state, "post_reactions", &format!("post_id.eq.{post_id}")).await?;
    let comment_count = count_rows(&state, "post_comments", &format!("post_id.eq.{post_id}")).await?;

    let mut result = post.clone();
    if let Some(obj) = result.as_object_mut() {
        obj.insert("reaction_count".into(), json!(reactions));
        obj.insert("comment_count".into(), json!(comment_count));
    }

    ok_json(result)
}

/// PUT /content/posts/:id — Update a post (owner only).
async fn update_post(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(post_id): Path<String>,
    Json(payload): Json<UpdatePostPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify ownership
    let check_url = format!(
        "{}/rest/v1/posts?id=eq.{post_id}&select=author_id",
        state.rest_url()
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let existing: Vec<Value> = check_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let owner = existing.first()
        .and_then(|r| r.get("author_id")?.as_str())
        .ok_or_else(|| ApiError::NotFound("Post not found".into()))?;

    if owner != uid {
        return Err(ApiError::Forbidden("Not your post".into()));
    }

    let mut body = serde_json::Map::new();
    if let Some(content) = payload.content { body.insert("content".into(), json!(content)); }
    if let Some(visibility) = payload.visibility { body.insert("visibility".into(), json!(visibility)); }
    if let Some(tags) = payload.tags { body.insert("tags".into(), json!(tags)); }

    if body.is_empty() {
        return Err(ApiError::BadRequest("Nothing to update".into()));
    }

    let patch_url = format!("{}/rest/v1/posts?id=eq.{post_id}", state.rest_url());
    let patch_res = state.http.patch(&patch_url).headers(state.supabase_headers())
        .json(&Value::Object(body)).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !patch_res.status().is_success() {
        return Err(ApiError::Internal("Failed to update post".into()));
    }

    ok_message("Post updated")
}

/// DELETE /content/posts/:id — Delete a post (owner only).
async fn delete_post(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(post_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify ownership
    let check_url = format!(
        "{}/rest/v1/posts?id=eq.{post_id}&select=author_id",
        state.rest_url()
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let existing: Vec<Value> = check_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let owner = existing.first()
        .and_then(|r| r.get("author_id")?.as_str())
        .ok_or_else(|| ApiError::NotFound("Post not found".into()))?;

    if owner != uid {
        return Err(ApiError::Forbidden("Not your post".into()));
    }

    let del_url = format!("{}/rest/v1/posts?id=eq.{post_id}", state.rest_url());
    let del_res = state.http.delete(&del_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !del_res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete post".into()));
    }

    // Cascade: delete reactions, comments, bookmarks
    let _ = state.http.delete(format!("{}/rest/v1/post_reactions?post_id=eq.{post_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let _ = state.http.delete(format!("{}/rest/v1/post_comments?post_id=eq.{post_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let _ = state.http.delete(format!("{}/rest/v1/post_bookmarks?post_id=eq.{post_id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;

    ok_message("Post deleted")
}

/// POST /content/posts/:id/react — Add a reaction to a post.
async fn react_post(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(post_id): Path<String>,
    Json(payload): Json<ReactPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let valid_reactions = ["like", "insightful", "celebrate", "support"];
    if !valid_reactions.contains(&payload.reaction_type.as_str()) {
        return Err(ApiError::BadRequest("Invalid reaction type".into()));
    }

    // Check if already reacted
    let check_url = format!(
        "{}/rest/v1/post_reactions?post_id=eq.{post_id}&user_id=eq.{uid}&select=id",
        state.rest_url()
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let existing: Vec<Value> = check_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    if !existing.is_empty() {
        // Update existing reaction
        let react_id = existing.first().and_then(|r| r.get("id")?.as_str()).unwrap_or("");
        let patch_url = format!("{}/rest/v1/post_reactions?id=eq.{react_id}", state.rest_url());
        state.http.patch(&patch_url).headers(state.supabase_headers())
            .json(&json!({ "reaction_type": payload.reaction_type })).send().await
            .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
        return ok_message("Reaction updated");
    }

    let body = json!({
        "post_id": post_id,
        "user_id": uid,
        "reaction_type": payload.reaction_type
    });

    let url = format!("{}/rest/v1/post_reactions", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to react".into()));
    }

    ok_message("Reaction added")
}

/// DELETE /content/posts/:id/react — Remove reaction from a post.
async fn unreact_post(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(post_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/post_reactions?post_id=eq.{post_id}&user_id=eq.{uid}",
        state.rest_url()
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to remove reaction".into()));
    }

    ok_message("Reaction removed")
}

/// GET /content/posts/:id/comments — Get comments for a post (top-level + threaded).
async fn get_comments(
    State(state): State<SharedState>,
    Path(post_id): Path<String>,
) -> ApiResult {
    let url = format!(
        "{}/rest/v1/post_comments?post_id=eq.{post_id}&order=created_at.asc&select=*,profiles:user_id(id,full_name,avatar_url,headline)",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let comments: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!(comments))
}

/// POST /content/posts/:id/comments — Add a comment to a post.
async fn add_comment(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(post_id): Path<String>,
    Json(payload): Json<AddCommentPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if payload.content.trim().is_empty() {
        return Err(ApiError::BadRequest("Comment content cannot be empty".into()));
    }

    let body = json!({
        "post_id": post_id,
        "user_id": uid,
        "content": payload.content,
        "parent_comment_id": payload.parent_comment_id
    });

    let url = format!("{}/rest/v1/post_comments", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers())
        .header("Prefer", "return=representation")
        .json(&body).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!("Failed to add comment: {err}")));
    }

    let rows: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "id": rows.first().and_then(|r| r.get("id")),
        "message": "Comment added"
    }))
}

/// POST /content/posts/:id/bookmark — Bookmark a post.
async fn bookmark_post(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(post_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Check if already bookmarked
    let check_url = format!(
        "{}/rest/v1/post_bookmarks?post_id=eq.{post_id}&user_id=eq.{uid}&select=id",
        state.rest_url()
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let existing: Vec<Value> = check_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    if !existing.is_empty() {
        return Err(ApiError::Conflict("Already bookmarked".into()));
    }

    let body = json!({
        "post_id": post_id,
        "user_id": uid
    });

    let url = format!("{}/rest/v1/post_bookmarks", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to bookmark".into()));
    }

    ok_message("Post bookmarked")
}

/// DELETE /content/posts/:id/bookmark — Remove bookmark.
async fn remove_bookmark(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(post_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/post_bookmarks?post_id=eq.{post_id}&user_id=eq.{uid}",
        state.rest_url()
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to remove bookmark".into()));
    }

    ok_message("Bookmark removed")
}

/// GET /content/bookmarks — All bookmarked posts for current user.
async fn get_bookmarks(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/post_bookmarks?user_id=eq.{uid}&select=*,posts:post_id(*,profiles:author_id(id,full_name,avatar_url,headline))&order=created_at.desc&limit=20",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let bookmarks: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!(bookmarks))
}

// ─── Helpers ──────────────────────────────────────────────

async fn count_rows(state: &SharedState, table: &str, filter: &str) -> Result<i64, ApiError> {
    let url = format!("{}/rest/v1/{table}?{filter}&select=id", state.rest_url());
    let res = state.http.head(&url).headers(state.supabase_headers())
        .header("Prefer", "count=exact,head=true").send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if let Some(range) = res.headers().get("content-range") {
        let range_str = range.to_str().unwrap_or("");
        if let Some(count_str) = range_str.split('/').last() {
            return count_str.parse::<i64>().map_err(|_| ApiError::Internal("Invalid count".into()));
        }
    }
    Ok(0)
}
