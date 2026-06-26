use axum::{
    Router,
    routing::{get, post},
    Json,
    extract::{State, Path, Query},
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json};
use crate::extractors::AuthUser;

// ──────────────────────────────────────────────
// Analytics Engine  (User-facing)
// Personal analytics: post views, engagement,
// follower growth, search insights
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Personal dashboard
        .route("/dashboard", get(personal_dashboard))
        // Post analytics
        .route("/posts", get(post_analytics))
        .route("/posts/:id", get(single_post_analytics))
        // Profile analytics
        .route("/profile/views", get(profile_views))
        .route("/profile/followers", get(follower_analytics))
        // Search analytics
        .route("/search", get(search_analytics))
        // Engagement
        .route("/engagement", get(engagement_analytics))
        // Export
        .route("/export", post(export_analytics))
}

#[derive(Deserialize)]
pub struct AnalyticsPeriodQuery {
    pub period: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct ExportAnalyticsPayload {
    pub period: Option<String>,
    pub sections: Option<Vec<String>>,
    pub format: Option<String>,
}

fn period_days(period: &str) -> i64 {
    match period {
        "24h" => 1,
        "7d" => 7,
        "30d" => 30,
        "90d" => 90,
        "1y" => 365,
        _ => 30,
    }
}

// ─── Personal Dashboard ───────────────────────────────

async fn personal_dashboard(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsPeriodQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let period = query.period.unwrap_or_else(|| "30d".to_string());
    let since = (chrono::Utc::now() - chrono::Duration::days(period_days(&period))).to_rfc3339();

    // My posts — get IDs for downstream queries
    let my_posts_url = format!(
        "{}/rest/v1/posts?author_id=eq.{uid}&created_at=gx.{since}&select=id",
        state.rest_url()
    );
    let my_post_ids: Vec<String> = if let Ok(r) = state.http.get(&my_posts_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default()
            .iter()
            .filter_map(|p| p.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect()
    } else { vec![] };
    let my_posts = my_post_ids.len() as i64;

    // My post reactions received (reactions on my posts)
    let reactions_received = if my_post_ids.is_empty() { 0i64 } else {
        let id_list = my_post_ids.iter().map(|id| format!("{id}")).collect::<Vec<_>>().join(",");
        let my_reactions_url = format!(
            "{}/rest/v1/post_reactions?post_id=in.({id_list})&select=id",
            state.rest_url()
        );
        if let Ok(r) = state.http.get(&my_reactions_url).headers(state.supabase_headers()).send().await {
            r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
        } else { 0 }
    };

    // My comments received (comments on my posts)
    let comments_received = if my_post_ids.is_empty() { 0i64 } else {
        let id_list = my_post_ids.iter().map(|id| format!("{id}")).collect::<Vec<_>>().join(",");
        let my_comments_url = format!(
            "{}/rest/v1/post_comments?post_id=in.({id_list})&select=id",
            state.rest_url()
        );
        if let Ok(r) = state.http.get(&my_comments_url).headers(state.supabase_headers()).send().await {
            r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
        } else { 0 }
    };

    // Follower count
    let followers_url = format!(
        "{}/rest/v1/follows?following_id=eq.{uid}&select=id",
        state.rest_url()
    );
    let total_followers = if let Ok(r) = state.http.get(&followers_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    // New followers in period
    let new_followers_url = format!(
        "{}/rest/v1/follows?following_id=eq.{uid}&created_at=gx.{since}&select=id",
        state.rest_url()
    );
    let new_followers = if let Ok(r) = state.http.get(&new_followers_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    // Connection count
    let conns_url = format!(
        "{}/rest/v1/connections?or=(requester_id=eq.{uid},addressee_id=eq.{uid})&status=eq.accepted&select=id",
        state.rest_url()
    );
    let total_connections = if let Ok(r) = state.http.get(&conns_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    ok_json(json!({
        "period": period,
        "posts_created": my_posts,
        "reactions_received": reactions_received,
        "comments_received": comments_received,
        "total_followers": total_followers,
        "new_followers": new_followers,
        "total_connections": total_connections,
        "engagement_score": reactions_received + comments_received,
    }))
}

// ─── Post Analytics ───────────────────────────────────

async fn post_analytics(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsPeriodQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let limit = query.limit.unwrap_or(20);

    let url = format!(
        "{}/rest/v1/posts?author_id=eq.{uid}&order=created_at.desc&limit={limit}",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let posts: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let mut analytics = Vec::new();
    for post in &posts {
        let post_id = post.get("id").and_then(|v| v.as_str()).unwrap_or("");

        let reactions_url = format!("{}/rest/v1/post_reactions?post_id=eq.{post_id}&select=id", state.rest_url());
        let reactions = if let Ok(r) = state.http.get(&reactions_url).headers(state.supabase_headers()).send().await {
            r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
        } else { 0 };

        let comments_url = format!("{}/rest/v1/post_comments?post_id=eq.{post_id}&select=id", state.rest_url());
        let comments = if let Ok(r) = state.http.get(&comments_url).headers(state.supabase_headers()).send().await {
            r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
        } else { 0 };

        analytics.push(json!({
            "post_id": post_id,
            "content_preview": post.get("content").and_then(|v| v.as_str()).unwrap_or("").chars().take(100).collect::<String>(),
            "created_at": post.get("created_at"),
            "reactions": reactions,
            "comments": comments,
            "engagement": reactions + comments,
        }));
    }

    ok_json(json!({ "posts": analytics, "total": analytics.len() }))
}

async fn single_post_analytics(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(post_id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    // Verify ownership — only get analytics for your own posts
    let post_url = format!("{}/rest/v1/posts?id=eq.{post_id}&author_id=eq.{uid}", state.rest_url());
    let post_res = state.http.get(&post_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let posts: Vec<Value> = post_res.json().await.unwrap_or_default();
    let post = posts.first().ok_or(ApiError::NotFound("Post not found".into()))?;

    // Reactions breakdown
    let reactions_url = format!("{}/rest/v1/post_reactions?post_id=eq.{post_id}&select=reaction_type", state.rest_url());
    let reactions: Vec<Value> = if let Ok(r) = state.http.get(&reactions_url).headers(state.supabase_headers()).send().await {
        r.json().await.unwrap_or_default()
    } else { vec![] };

    let mut reaction_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in &reactions {
        let rtype = r.get("reaction_type").and_then(|v| v.as_str()).unwrap_or("unknown");
        *reaction_counts.entry(rtype.to_string()).or_insert(0) += 1;
    }

    // Comments
    let comments_url = format!("{}/rest/v1/post_comments?post_id=eq.{post_id}&select=id", state.rest_url());
    let comment_count = if let Ok(r) = state.http.get(&comments_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    // Bookmarks
    let bookmarks_url = format!("{}/rest/v1/post_bookmarks?post_id=eq.{post_id}&select=id", state.rest_url());
    let bookmark_count = if let Ok(r) = state.http.get(&bookmarks_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    ok_json(json!({
        "post_id": post_id,
        "content_preview": post.get("content").and_then(|v| v.as_str()).unwrap_or("").chars().take(200).collect::<String>(),
        "created_at": post.get("created_at"),
        "total_reactions": reactions.len() as i64,
        "reaction_breakdown": reaction_counts,
        "comments": comment_count,
        "bookmarks": bookmark_count,
    }))
}

// ─── Profile Views ────────────────────────────────────

async fn profile_views(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsPeriodQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let period = query.period.unwrap_or_else(|| "30d".to_string());
    let since = (chrono::Utc::now() - chrono::Duration::days(period_days(&period))).to_rfc3339();

    let url = format!(
        "{}/rest/v1/profile_views?viewed_id=eq.{uid}&created_at=gx.{since}&select=id,viewer_id,created_at",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let views: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Unique viewers
    let mut unique_viewers = std::collections::HashSet::new();
    for v in &views {
        if let Some(viewer) = v.get("viewer_id").and_then(|v| v.as_str()) {
            unique_viewers.insert(viewer.to_string());
        }
    }

    ok_json(json!({
        "period": period,
        "total_views": views.len(),
        "unique_viewers": unique_viewers.len(),
    }))
}

// ─── Follower Analytics ───────────────────────────────

async fn follower_analytics(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsPeriodQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let period = query.period.unwrap_or_else(|| "30d".to_string());
    let since = (chrono::Utc::now() - chrono::Duration::days(period_days(&period))).to_rfc3339();

    // Total followers
    let total_url = format!("{}/rest/v1/follows?following_id=eq.{uid}&select=id", state.rest_url());
    let total = if let Ok(r) = state.http.get(&total_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    // New followers in period
    let new_url = format!(
        "{}/rest/v1/follows?following_id=eq.{uid}&created_at=gx.{since}&select=id",
        state.rest_url()
    );
    let new_followers = if let Ok(r) = state.http.get(&new_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    // Unfollows (approximate: followers who no longer follow)
    let following_url = format!("{}/rest/v1/follows?follower_id=eq.{uid}&select=following_id", state.rest_url());
    let i_follow = if let Ok(r) = state.http.get(&following_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    ok_json(json!({
        "period": period,
        "total_followers": total,
        "new_followers": new_followers,
        "i_follow_count": i_follow,
        "net_growth": new_followers,
    }))
}

// ─── Search Analytics ─────────────────────────────────

async fn search_analytics(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsPeriodQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let period = query.period.unwrap_or_else(|| "30d".to_string());
    let since = (chrono::Utc::now() - chrono::Duration::days(period_days(&period))).to_rfc3339();

    let url = format!(
        "{}/rest/v1/search_history?user_id=eq.{uid}&created_at=gx.{since}&select=query,created_at",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let searches: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Top searches
    let mut query_counts = std::collections::HashMap::new();
    for s in &searches {
        if let Some(q) = s.get("query").and_then(|v| v.as_str()) {
            *query_counts.entry(q.to_string()).or_insert(0) += 1;
        }
    }
    let mut top_searches: Vec<(String, i64)> = query_counts.into_iter().collect();
    top_searches.sort_by(|a, b| b.1.cmp(&a.1));
    top_searches.truncate(10);

    ok_json(json!({
        "period": period,
        "total_searches": searches.len(),
        "top_searches": top_searches.into_iter().map(|(q, c)| json!({"query": q, "count": c})).collect::<Vec<_>>(),
    }))
}

// ─── Engagement Analytics ─────────────────────────────

async fn engagement_analytics(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsPeriodQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let period = query.period.unwrap_or_else(|| "30d".to_string());
    let since = (chrono::Utc::now() - chrono::Duration::days(period_days(&period))).to_rfc3339();

    // Reactions given
    let given_url = format!(
        "{}/rest/v1/post_reactions?user_id=eq.{uid}&created_at=gx.{since}&select=id",
        state.rest_url()
    );
    let reactions_given = if let Ok(r) = state.http.get(&given_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    // Comments made
    let comments_made_url = format!(
        "{}/rest/v1/post_comments?author_id=eq.{uid}&created_at=gx.{since}&select=id",
        state.rest_url()
    );
    let comments_made = if let Ok(r) = state.http.get(&comments_made_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    // Posts bookmarked
    let bookmarks_url = format!(
        "{}/rest/v1/post_bookmarks?user_id=eq.{uid}&created_at=gx.{since}&select=id",
        state.rest_url()
    );
    let posts_bookmarked = if let Ok(r) = state.http.get(&bookmarks_url).headers(state.supabase_headers()).send().await {
        r.json::<Vec<Value>>().await.unwrap_or_default().len() as i64
    } else { 0 };

    ok_json(json!({
        "period": period,
        "reactions_given": reactions_given,
        "comments_made": comments_made,
        "posts_bookmarked": posts_bookmarked,
        "total_activity": reactions_given + comments_made + posts_bookmarked,
    }))
}

// ─── Export ────────────────────────────────────────────

async fn export_analytics(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<ExportAnalyticsPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let period = payload.period.unwrap_or_else(|| "30d".to_string());
    let sections = payload.sections.unwrap_or_else(|| vec![
        "posts".to_string(), "followers".to_string(), "engagement".to_string()
    ]);

    let mut data = serde_json::Map::new();
    for section in &sections {
        match section.as_str() {
            "posts" => {
                let url = format!(
                    "{}/rest/v1/posts?author_id=eq.{uid}&select=id,content,created_at",
                    state.rest_url()
                );
                let posts = if let Ok(r) = state.http.get(&url).headers(state.supabase_headers()).send().await {
                    r.json::<Vec<Value>>().await.unwrap_or_default()
                } else { vec![] };
                data.insert("posts".to_string(), json!(posts));
            }
            "followers" => {
                let url = format!(
                    "{}/rest/v1/follows?following_id=eq.{uid}&select=follower_id,created_at",
                    state.rest_url()
                );
                let followers = if let Ok(r) = state.http.get(&url).headers(state.supabase_headers()).send().await {
                    r.json::<Vec<Value>>().await.unwrap_or_default()
                } else { vec![] };
                data.insert("followers".to_string(), json!(followers));
            }
            "engagement" => {
                // Get user's post IDs first
                let posts_url = format!(
                    "{}/rest/v1/posts?author_id=eq.{uid}&select=id",
                    state.rest_url()
                );
                let post_ids: Vec<String> = if let Ok(r) = state.http.get(&posts_url).headers(state.supabase_headers()).send().await {
                    r.json::<Vec<Value>>().await.unwrap_or_default()
                        .iter()
                        .filter_map(|p| p.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                        .collect()
                } else { vec![] };
                let engagement: Vec<Value> = if post_ids.is_empty() { vec![] } else {
                    let id_list = post_ids.iter().map(|id| format!("{id}")).collect::<Vec<_>>().join(",");
                    let url = format!(
                        "{}/rest/v1/post_reactions?post_id=in.({id_list})&select=reaction_type,created_at",
                        state.rest_url()
                    );
                    if let Ok(r) = state.http.get(&url).headers(state.supabase_headers()).send().await {
                        r.json().await.unwrap_or_default()
                    } else { vec![] }
                };
                data.insert("engagement".to_string(), json!(engagement));
            }
            _ => {}
        }
    }

    ok_json(json!({
        "period": period,
        "format": payload.format.unwrap_or_else(|| "json".to_string()),
        "data": data
    }))
}
