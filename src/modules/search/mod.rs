use axum::{
    Router,
    routing::{get, delete},
    Json,
    extract::{State, Path, Query},
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json, ok_message};
use crate::extractors::AuthUser;

// ─── Request Bodies ───────────────────────────────────────

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub r#type: Option<String>,  // people, posts, tags, all
    pub category: Option<String>,
    pub location: Option<String>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct SaveSearchPayload {
    pub query: String,
    pub filters: Option<Value>,
}

// ─── Router ───────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/", get(search))
        .route("/history", get(get_search_history).delete(clear_search_history))
        .route("/saved", get(get_saved_searches).post(save_search))
        .route("/saved/:id", delete(delete_saved_search))
        .route("/trending", get(get_trending_searches))
}

// ─── Handlers ─────────────────────────────────────────────

/// GET /search/?q=...&type=people|posts|tags|all — Universal search.
async fn search(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(params): Query<SearchQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let query = params.q.unwrap_or_default();
    let search_type = params.r#type.unwrap_or_else(|| "all".into());
    let page = params.page.unwrap_or(0);
    let limit = params.limit.unwrap_or(20).min(50);
    let offset = page * limit;

    if query.is_empty() && search_type == "all" {
        return ok_json(json!({ "people": [], "posts": [], "tags": [] }));
    }

    // Record search in history (fire and forget)
    let _ = record_search(&state, uid, &query, &search_type).await;

    match search_type.as_str() {
        "people" => {
            let mut url = format!(
                "{}/rest/v1/profiles?id=neq.{uid}&select=id,full_name,avatar_url,headline,primary_category,city,country&limit={limit}&offset={offset}",
                state.rest_url()
            );
            if !query.is_empty() {
                url = format!(
                    "{}/rest/v1/profiles?id=neq.{uid}&or=(full_name.ilike.*{query}*,headline.ilike.*{query}*,city.ilike.*{query}*)&select=id,full_name,avatar_url,headline,primary_category,city,country&limit={limit}&offset={offset}",
                    state.rest_url()
                );
            }
            if let Some(ref cat) = params.category {
                url = format!("{url}&primary_category=eq.{cat}");
            }
            if let Some(ref loc) = params.location {
                url = format!("{url}&city=ilike.*{loc}*");
            }

            let res = state.http.get(&url).headers(state.supabase_headers()).send().await
                .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
            let people: Vec<Value> = res.json().await
                .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

            ok_json(json!({ "people": people, "count": people.len() }))
        }
        "posts" => {
            let url = format!(
                "{}/rest/v1/posts?or=(content.ilike.*{query}*,tags.cs.{{{query}}})&visibility=eq.public&order=created_at.desc&limit={limit}&offset={offset}&select=*,profiles:author_id(id,full_name,avatar_url,headline)",
                state.rest_url()
            );
            let res = state.http.get(&url).headers(state.supabase_headers()).send().await
                .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
            let posts: Vec<Value> = res.json().await
                .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

            ok_json(json!({ "posts": posts, "count": posts.len() }))
        }
        "tags" => {
            // Search tags from posts
            let url = format!(
                "{}/rest/v1/posts?tags=cs.{{{query}}}&select=tags&limit=100",
                state.rest_url()
            );
            let res = state.http.get(&url).headers(state.supabase_headers()).send().await
                .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
            let rows: Vec<Value> = res.json().await
                .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

            // Flatten and count tags
            let mut tag_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
            for row in &rows {
                if let Some(tags) = row.get("tags").and_then(|v| v.as_array()) {
                    for tag in tags {
                        if let Some(tag_str) = tag.as_str() {
                            if tag_str.to_lowercase().contains(&query.to_lowercase()) {
                                *tag_counts.entry(tag_str.to_string()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }

            let mut tag_list: Vec<Value> = tag_counts.into_iter()
                .map(|(tag, count)| json!({ "tag": tag, "count": count }))
                .collect();
            tag_list.sort_by(|a, b| {
                let ca = a.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
                let cb = b.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
                cb.cmp(&ca)
            });
            tag_list.truncate(20);

            ok_json(json!({ "tags": tag_list }))
        }
        _ => {
            // "all" — search people and posts in parallel
            let people_url = format!(
                "{}/rest/v1/profiles?id=neq.{uid}&or=(full_name.ilike.*{query}*,headline.ilike.*{query}*)&select=id,full_name,avatar_url,headline,primary_category&limit=5",
                state.rest_url()
            );
            let posts_url = format!(
                "{}/rest/v1/posts?content.ilike.*{query}*&visibility=eq.public&order=created_at.desc&limit=10&select=*,profiles:author_id(id,full_name,avatar_url,headline)",
                state.rest_url()
            );

            let people_res = state.http.get(&people_url).headers(state.supabase_headers()).send().await;
            let posts_res = state.http.get(&posts_url).headers(state.supabase_headers()).send().await;

            let people: Vec<Value> = match people_res {
                Ok(r) => r.json().await.unwrap_or_default(),
                Err(_) => vec![],
            };
            let posts: Vec<Value> = match posts_res {
                Ok(r) => r.json().await.unwrap_or_default(),
                Err(_) => vec![],
            };

            ok_json(json!({
                "people": people,
                "posts": posts,
                "people_count": people.len(),
                "posts_count": posts.len()
            }))
        }
    }
}

/// GET /search/history — Recent search queries for current user.
async fn get_search_history(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/search_history?user_id=eq.{uid}&order=created_at.desc&limit=20&select=id,query,search_type,created_at",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let history: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!(history))
}

/// DELETE /search/history — Clear search history.
async fn clear_search_history(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!("{}/rest/v1/search_history?user_id=eq.{uid}", state.rest_url());
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to clear history".into()));
    }

    ok_message("Search history cleared")
}

/// GET /search/saved — Saved searches.
async fn get_saved_searches(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/saved_searches?user_id=eq.{uid}&order=created_at.desc&limit=50&select=*",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let searches: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!(searches))
}

/// POST /search/saved — Save a search query.
async fn save_search(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<SaveSearchPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let body = json!({
        "user_id": uid,
        "query": payload.query,
        "filters": payload.filters.unwrap_or(json!({}))
    });

    let url = format!("{}/rest/v1/saved_searches", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers())
        .header("Prefer", "return=representation")
        .json(&body).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        return Err(ApiError::Internal(format!("Failed to save search: {err}")));
    }

    let rows: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "id": rows.first().and_then(|r| r.get("id")),
        "message": "Search saved"
    }))
}

/// DELETE /search/saved/:id — Delete a saved search.
async fn delete_saved_search(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(search_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/saved_searches?id=eq.{search_id}&user_id=eq.{uid}",
        state.rest_url()
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete saved search".into()));
    }

    ok_message("Saved search deleted")
}

/// GET /search/trending — Trending search terms (most recent popular queries).
async fn get_trending_searches(
    State(state): State<SharedState>,
) -> ApiResult {
    // Get most-searched queries in the last 7 days
    let url = format!(
        "{}/rest/v1/search_history?created_at=gte.{}&select=query&limit=200",
        state.rest_url(),
        (chrono::Utc::now() - chrono::Duration::days(7)).format("%Y-%m-%dT%H:%M:%SZ")
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("DB error: {e}")))?;
    let rows: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Count frequency
    let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for row in &rows {
        if let Some(q) = row.get("query").and_then(|v| v.as_str()) {
            if !q.is_empty() {
                *counts.entry(q.to_string()).or_insert(0) += 1;
            }
        }
    }

    let mut trending: Vec<Value> = counts.into_iter()
        .map(|(query, count)| json!({ "query": query, "count": count }))
        .collect();
    trending.sort_by(|a, b| {
        let ca = a.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        let cb = b.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        cb.cmp(&ca)
    });
    trending.truncate(10);

    ok_json(json!(trending))
}

// ─── Helpers ──────────────────────────────────────────────

async fn record_search(
    state: &SharedState,
    user_id: &str,
    query: &str,
    search_type: &str,
) -> Result<(), ApiError> {
    if query.is_empty() {
        return Ok(());
    }

    let body = json!({
        "user_id": user_id,
        "query": query,
        "search_type": search_type
    });

    let url = format!("{}/rest/v1/search_history", state.rest_url());
    let _ = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await;
    Ok(())
}
