use axum::{
    Router,
    routing::{get, post},
    Json,
    extract::{State, Query},
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json};
use crate::extractors::AuthUser;

// ──────────────────────────────────────────────
// Recommendation Engine
// People you may know, content suggestions,
// trending topics, similar professionals
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/people", get(recommend_people))
        .route("/content", get(recommend_content))
        .route("/trending", get(trending_topics))
        .route("/similar", get(similar_professionals))
        .route("/discovery", get(discovery_feed))
        .route("/feedback", post(submit_feedback))
}

#[derive(Deserialize)]
pub struct RecommendQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct FeedbackPayload {
    pub target_type: String,
    pub target_id: String,
    pub feedback: String,  // helpful, not_relevant, already_know
}

// ─── People You May Know ─────────────────────────────

async fn recommend_people(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<RecommendQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let limit = query.limit.unwrap_or(20);

    // Get my connections
    let my_conns_url = format!(
        "{}/rest/v1/connections?or=(requester_id=eq.{uid},addressee_id=eq.{uid})&status=eq.accepted&select=requester_id,addressee_id",
        state.rest_url()
    );
    let my_conns: Vec<Value> = if let Ok(r) = state.http.get(&my_conns_url).headers(state.supabase_headers()).send().await {
        r.json().await.unwrap_or_default()
    } else { vec![] };

    // Collect connected IDs
    let mut connected = std::collections::HashSet::new();
    connected.insert(uid.to_string());
    for c in &my_conns {
        if let Some(rid) = c.get("requester_id").and_then(|v| v.as_str()) {
            if rid != uid { connected.insert(rid.to_string()); }
        }
        if let Some(aid) = c.get("addressee_id").and_then(|v| v.as_str()) {
            if aid != uid { connected.insert(aid.to_string()); }
        }
    }

    // Get my follows
    let my_follows_url = format!(
        "{}/rest/v1/follows?follower_id=eq.{uid}&select=following_id",
        state.rest_url()
    );
    let my_follows: Vec<Value> = if let Ok(r) = state.http.get(&my_follows_url).headers(state.supabase_headers()).send().await {
        r.json().await.unwrap_or_default()
    } else { vec![] };
    for f in &my_follows {
        if let Some(fid) = f.get("following_id").and_then(|v| v.as_str()) {
            connected.insert(fid.to_string());
        }
    }

    // Get my profile for category matching
    let profile_url = format!(
        "{}/rest/v1/profiles?id=eq.{uid}&select=primary_category,specialization",
        state.rest_url()
    );
    let profile: Vec<Value> = if let Ok(r) = state.http.get(&profile_url).headers(state.supabase_headers()).send().await {
        r.json().await.unwrap_or_default()
    } else { vec![] };
    let my_category = profile.first()
        .and_then(|p| p.get("primary_category").and_then(|v| v.as_str()))
        .unwrap_or("");

    // Find people with same category (exclude connected)
    let mut candidates_url = format!(
        "{}/rest/v1/profiles?id=neq.{uid}&account_status=eq.active&select=id,full_name,avatar_url,headline,primary_category",
        state.rest_url()
    );
    if !my_category.is_empty() {
        candidates_url.push_str(&format!("&primary_category=eq.{my_category}"));
    }
    candidates_url.push_str(&format!("&limit={}", limit * 2));

    let candidates: Vec<Value> = if let Ok(r) = state.http.get(&candidates_url).headers(state.supabase_headers()).send().await {
        r.json().await.unwrap_or_default()
    } else { vec![] };

    // Filter out already connected
    let recommendations: Vec<Value> = candidates.into_iter()
        .filter(|c| {
            let id = c.get("id").and_then(|v| v.as_str()).unwrap_or("");
            !connected.contains(id)
        })
        .take(limit as usize)
        .collect();

    ok_json(json!({
        "people": recommendations,
        "total": recommendations.len()
    }))
}

// ─── Content Recommendations ─────────────────────────

async fn recommend_content(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<RecommendQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let limit = query.limit.unwrap_or(20);

    // Get posts from connections (most engaging)
    let conns_url = format!(
        "{}/rest/v1/connections?or=(requester_id=eq.{uid},addressee_id=eq.{uid})&status=eq.accepted&select=requester_id,addressee_id",
        state.rest_url()
    );
    let conns: Vec<Value> = if let Ok(r) = state.http.get(&conns_url).headers(state.supabase_headers()).send().await {
        r.json().await.unwrap_or_default()
    } else { vec![] };

    let mut conn_ids = Vec::new();
    for c in &conns {
        if let Some(rid) = c.get("requester_id").and_then(|v| v.as_str()) {
            if rid != uid { conn_ids.push(rid.to_string()); }
        }
        if let Some(aid) = c.get("addressee_id").and_then(|v| v.as_str()) {
            if aid != uid { conn_ids.push(aid.to_string()); }
        }
    }

    if conn_ids.is_empty() {
        // Fallback: trending public posts
        let url = format!(
            "{}/rest/v1/posts?visibility=eq.public&order=created_at.desc&limit={limit}",
            state.rest_url()
        );
        let res = state.http.get(&url).headers(state.supabase_headers()).send().await
            .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
        let posts: Vec<Value> = res.json().await
            .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
        return ok_json(json!({ "posts": posts, "source": "trending" }));
    }

    // Get recent posts from connections
    let author_filter = conn_ids.iter().map(|id| format!("author_id=eq.{id}")).collect::<Vec<_>>().join(",");
    let url = format!(
        "{}/rest/v1/posts?or=({author_filter})&order=created_at.desc&limit={limit}",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let posts: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "posts": posts, "source": "connections" }))
}

// ─── Trending Topics ─────────────────────────────────

async fn trending_topics(
    State(state): State<SharedState>,
    Query(query): Query<RecommendQuery>,
) -> ApiResult {
    let limit = query.limit.unwrap_or(10);
    let since = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();

    // Get recent searches to find trending terms
    let url = format!(
        "{}/rest/v1/search_history?created_at=gx.{since}&select=query",
        state.rest_url()
    );
    let searches: Vec<Value> = if let Ok(r) = state.http.get(&url).headers(state.supabase_headers()).send().await {
        r.json().await.unwrap_or_default()
    } else { vec![] };

    let mut query_counts = std::collections::HashMap::new();
    for s in &searches {
        if let Some(q) = s.get("query").and_then(|v| v.as_str()) {
            *query_counts.entry(q.to_string()).or_insert(0) += 1;
        }
    }

    let mut trending: Vec<(String, i64)> = query_counts.into_iter().collect();
    trending.sort_by(|a, b| b.1.cmp(&a.1));
    trending.truncate(limit as usize);

    // Also get recent post topics
    let posts_url = format!(
        "{}/rest/v1/posts?created_at=gx.{since}&order=created_at.desc&limit=100",
        state.rest_url()
    );
    let posts: Vec<Value> = if let Ok(r) = state.http.get(&posts_url).headers(state.supabase_headers()).send().await {
        r.json().await.unwrap_or_default()
    } else { vec![] };

    let post_count = posts.len() as i64;

    ok_json(json!({
        "trending_queries": trending.into_iter().map(|(q, c)| json!({"query": q, "count": c})).collect::<Vec<_>>(),
        "recent_posts": post_count,
    }))
}

// ─── Similar Professionals ───────────────────────────

async fn similar_professionals(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<RecommendQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let limit = query.limit.unwrap_or(10);

    // Get my profile details
    let profile_url = format!(
        "{}/rest/v1/profiles?id=eq.{uid}&select=primary_category,specialization,location",
        state.rest_url()
    );
    let my_profile: Vec<Value> = if let Ok(r) = state.http.get(&profile_url).headers(state.supabase_headers()).send().await {
        r.json().await.unwrap_or_default()
    } else { vec![] };
    let me = my_profile.first().ok_or(ApiError::Internal("Could not fetch profile".into()))?;

    let my_category = me.get("primary_category").and_then(|v| v.as_str()).unwrap_or("");
    let my_location = me.get("location").and_then(|v| v.as_str()).unwrap_or("");

    // Find similar professionals
    let mut url = format!(
        "{}/rest/v1/profiles?id=neq.{uid}&account_status=eq.active&select=id,full_name,avatar_url,headline,primary_category,location,specialization",
        state.rest_url()
    );
    if !my_category.is_empty() {
        url.push_str(&format!("&primary_category=eq.{my_category}"));
    }
    url.push_str(&format!("&limit={}", limit * 3));

    let candidates: Vec<Value> = if let Ok(r) = state.http.get(&url).headers(state.supabase_headers()).send().await {
        r.json().await.unwrap_or_default()
    } else { vec![] };

    // Score by similarity (same category + location = high, same category = medium)
    let mut scored: Vec<(Value, i32)> = candidates.into_iter().map(|c| {
        let mut score = 0;
        if c.get("primary_category").and_then(|v| v.as_str()) == Some(my_category) { score += 10; }
        if !my_location.is_empty() && c.get("location").and_then(|v| v.as_str()) == Some(my_location) { score += 5; }
        if c.get("specialization").is_some() { score += 1; }
        (c, score)
    }).collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.truncate(limit as usize);

    let results: Vec<Value> = scored.into_iter().map(|(c, _)| c).collect();

    ok_json(json!({ "professionals": results, "total": results.len() }))
}

// ─── Discovery Feed ──────────────────────────────────

async fn discovery_feed(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<RecommendQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let limit = query.limit.unwrap_or(20);
    let offset = query.offset.unwrap_or(0);

    // Get recent public posts with engagement, excluding own posts
    let url = format!(
        "{}/rest/v1/posts?visibility=eq.public&author_id=neq.{uid}&order=created_at.desc&limit={limit}&offset={offset}",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let posts: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "posts": posts,
        "total": posts.len(),
        "offset": offset,
        "has_more": posts.len() as i64 >= limit,
    }))
}

// ─── Recommendation Feedback ─────────────────────────

async fn submit_feedback(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<FeedbackPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let body = json!({
        "user_id": uid,
        "target_type": payload.target_type,
        "target_id": payload.target_id,
        "feedback": payload.feedback
    });
    let _ = state.http.post(format!("{}/rest/v1/recommendation_feedback", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await;

    ok_json(json!({ "message": "Feedback recorded" }))
}
