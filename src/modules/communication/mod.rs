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
// Communication Engine  (Screens 69–78)
// Full Supabase-backed messaging, notifications,
// conversations, and channel implementation
// ──────────────────────────────────────────────

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Screen 69 – Notifications List
        .route("/notifications", get(get_notifications))
        // Screen 70 – Mark Notification Read
        .route("/notifications/:id/read", put(mark_notification_read))
        // Screen 71 – Mark All Notifications Read
        .route("/notifications/read-all", put(mark_all_notifications_read))
        // Screen 72 – Conversations List
        .route("/conversations", get(get_conversations).post(create_conversation))
        // Screen 73 – Conversation Detail / Messages
        .route("/conversations/:id", get(get_conversation_detail).delete(delete_conversation))
        .route("/conversations/:id/messages", get(get_messages).post(send_message))
        // Screen 74 – Message Detail / Edit / Delete
        .route("/messages/:id", get(get_message_detail).put(update_message).delete(delete_message))
        // Screen 75 – Message Reactions
        .route("/messages/:id/reactions", get(get_message_reactions).post(add_reaction).delete(remove_reaction))
        // Screen 76 – Typing Indicator
        .route("/conversations/:id/typing", post(send_typing_indicator))
        // Screen 77 – Read Receipts
        .route("/conversations/:id/read", put(mark_conversation_read))
        // Screen 78 – Channels / Group Conversations
        .route("/channels", get(get_channels).post(create_channel))
        .route("/channels/:id", get(get_channel_detail).put(update_channel).delete(delete_channel))
        .route("/channels/:id/members", get(get_channel_members).post(add_channel_member).delete(remove_channel_member))
        .route("/channels/:id/messages", get(get_channel_messages).post(send_channel_message))
}

// ─── Request Bodies ───────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateConversationPayload {
    pub participant_id: String,
    pub initial_message: Option<String>,
}

#[derive(Deserialize)]
pub struct SendMessagePayload {
    pub content: String,
    pub message_type: Option<String>,       // text, image, file
    pub reply_to_id: Option<String>,
}

#[derive(Deserialize)]
pub struct NotificationQuery {
    pub unread_only: Option<bool>,
    pub notif_type: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct CreateChannelPayload {
    pub name: String,
    pub description: Option<String>,
    pub member_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateChannelPayload {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct AddChannelMemberPayload {
    pub user_id: String,
    pub role: Option<String>,  // admin, moderator, member
}

// ─── Screen 69: Notifications List ─────────────────────

async fn get_notifications(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<NotificationQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let mut url = format!(
        "{}/rest/v1/notifications?recipient_id=eq.{uid}&order=created_at.desc",
        state.rest_url()
    );

    if let Some(true) = query.unread_only {
        url.push_str("&is_read=eq.false");
    }
    if let Some(ref notif_type) = query.notif_type {
        url.push_str(&format!("&type=eq.{notif_type}"));
    }
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    url.push_str(&format!("&limit={limit}&offset={offset}"));

    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let notifications: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Get unread count
    let count_url = format!(
        "{}/rest/v1/notifications?recipient_id=eq.{uid}&is_read=eq.false&select=id",
        state.rest_url()
    );
    let count_res = state.http.get(&count_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let count_rows: Vec<Value> = count_res.json().await.unwrap_or_default();
    let unread_count = count_rows.len() as i64;

    ok_json(json!({
        "notifications": notifications,
        "unread_count": unread_count,
        "total": notifications.len()
    }))
}

// ─── Screen 70: Mark Notification Read ─────────────────

async fn mark_notification_read(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/notifications?id=eq.{id}&recipient_id=eq.{uid}",
        state.rest_url()
    );
    let body = json!({ "is_read": true });

    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&body).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to mark notification as read".into()));
    }

    ok_message("Notification marked as read")
}

// ─── Screen 71: Mark All Notifications Read ────────────

async fn mark_all_notifications_read(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    let url = format!(
        "{}/rest/v1/notifications?recipient_id=eq.{uid}&is_read=eq.false",
        state.rest_url()
    );
    let body = json!({ "is_read": true });

    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&body).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to mark all notifications as read".into()));
    }

    ok_message("All notifications marked as read")
}

// ─── Screen 72: Conversations List & Create ────────────

async fn get_conversations(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Get conversations where user is a participant
    let url = format!(
        "{}/rest/v1/conversation_members?profile_id=eq.{uid}&select=conversation_id",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let memberships: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let conv_ids: Vec<String> = memberships.iter()
        .filter_map(|m| m.get("conversation_id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();

    if conv_ids.is_empty() {
        return ok_json(json!({ "conversations": [], "total": 0 }));
    }

    let or_filter = conv_ids.iter().map(|id| format!("id.eq.{id}")).collect::<Vec<_>>().join(",");
    let conv_url = format!(
        "{}/rest/v1/conversations?or=({or_filter})&order=last_message_at.desc",
        state.rest_url()
    );
    let conv_res = state.http.get(&conv_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let conversations: Vec<Value> = conv_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Enrich each conversation with participant info and last message
    let mut enriched = Vec::new();
    for conv in &conversations {
        let conv_id = conv.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let mut enriched_conv = conv.clone();

        // Get participants
        let members_url = format!(
            "{}/rest/v1/conversation_members?conversation_id=eq.{conv_id}&select=profile_id",
            state.rest_url()
        );
        if let Ok(members_res) = state.http.get(&members_url).headers(state.supabase_headers()).send().await {
            if let Ok(members) = members_res.json::<Vec<Value>>().await {
                let member_ids: Vec<String> = members.iter()
                    .filter_map(|m| m.get("profile_id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect();
                enriched_conv["participants"] = json!(member_ids);
            }
        }

        // Get last message
        let msg_url = format!(
            "{}/rest/v1/messages?conversation_id=eq.{conv_id}&order=created_at.desc&limit=1",
            state.rest_url()
        );
        if let Ok(msg_res) = state.http.get(&msg_url).headers(state.supabase_headers()).send().await {
            if let Ok(msgs) = msg_res.json::<Vec<Value>>().await {
                if let Some(last_msg) = msgs.first() {
                    enriched_conv["last_message"] = last_msg.clone();
                }
            }
        }

        enriched.push(enriched_conv);
    }

    ok_json(json!({
        "conversations": enriched,
        "total": enriched.len()
    }))
}

async fn create_conversation(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<CreateConversationPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if *uid == payload.participant_id {
        return Err(ApiError::BadRequest("Cannot create conversation with yourself".into()));
    }

    // Check if conversation already exists between these two users
    let check_url = format!(
        "{}/rest/v1/rpc/check_existing_conversation?user1={uid}&user2={}",
        state.rest_url(), payload.participant_id
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await;
    if let Ok(res) = check_res {
        if let Ok(existing) = res.json::<Value>().await {
            if let Some(conv_id) = existing.get("conversation_id").and_then(|v| v.as_str()) {
                return ok_json(json!({
                    "conversation_id": conv_id,
                    "message": "Existing conversation found"
                }));
            }
        }
    }

    // Create conversation
    let conv_body = json!({
        "created_by": uid,
        "type": "direct"
    });
    let conv_res = state.http.post(format!("{}/rest/v1/conversations", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&conv_body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let conv_data: Value = conv_res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    let conv_id = conv_data.get("id").and_then(|v| v.as_str()).unwrap_or("");

    // Add both members
    let members = json!([
        { "conversation_id": conv_id, "profile_id": uid },
        { "conversation_id": conv_id, "profile_id": payload.participant_id }
    ]);
    let _ = state.http.post(format!("{}/rest/v1/conversation_members", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&members)
        .send().await;

    // Send initial message if provided
    if let Some(content) = payload.initial_message {
        if !content.trim().is_empty() {
            let msg_body = json!({
                "conversation_id": conv_id,
                "sender_id": uid,
                "content": content,
                "message_type": "text"
            });
            let _ = state.http.post(format!("{}/rest/v1/messages", state.rest_url()))
                .headers(state.supabase_headers())
                .json(&msg_body)
                .send().await;
        }
    }

    ok_json(json!({
        "conversation_id": conv_id,
        "message": "Conversation created"
    }))
}

// ─── Screen 73: Conversation Detail & Delete ───────────

async fn get_conversation_detail(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify membership
    let member_url = format!(
        "{}/rest/v1/conversation_members?conversation_id=eq.{id}&profile_id=eq.{uid}&select=id",
        state.rest_url()
    );
    let member_res = state.http.get(&member_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let member_rows: Vec<Value> = member_res.json().await.unwrap_or_default();
    if member_rows.is_empty() {
        return Err(ApiError::NotFound("Conversation not found".into()));
    }

    // Get conversation details
    let conv_url = format!("{}/rest/v1/conversations?id=eq.{id}", state.rest_url());
    let conv_res = state.http.get(&conv_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let conv_data: Vec<Value> = conv_res.json().await.unwrap_or_default();
    let conversation = conv_data.first().ok_or(ApiError::NotFound("Conversation not found".into()))?;

    // Get participants with profile info
    let members_url = format!(
        "{}/rest/v1/conversation_members?conversation_id=eq.{id}&select=profile_id",
        state.rest_url()
    );
    let members_res = state.http.get(&members_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let members: Vec<Value> = members_res.json().await.unwrap_or_default();

    let participant_ids: Vec<String> = members.iter()
        .filter_map(|m| m.get("profile_id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();

    let mut participants = Vec::new();
    for pid in &participant_ids {
        let p_url = format!("{}/rest/v1/profiles?id=eq.{pid}&select=id,full_name,avatar_url,headline", state.rest_url());
        if let Ok(p_res) = state.http.get(&p_url).headers(state.supabase_headers()).send().await {
            if let Ok(p_data) = p_res.json::<Vec<Value>>().await {
                if let Some(p) = p_data.first() {
                    participants.push(p.clone());
                }
            }
        }
    }

    // Get unread count for this user
    let unread_url = format!(
        "{}/rest/v1/messages?conversation_id=eq.{id}&sender_id=neq.{uid}&is_read=eq.false&select=id",
        state.rest_url()
    );
    let unread_count = if let Ok(u_res) = state.http.get(&unread_url).headers(state.supabase_headers()).send().await {
        if let Ok(u_rows) = u_res.json::<Vec<Value>>().await { u_rows.len() as i64 } else { 0 }
    } else { 0 };

    ok_json(json!({
        "conversation": conversation,
        "participants": participants,
        "unread_count": unread_count
    }))
}

async fn delete_conversation(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify membership
    let member_url = format!(
        "{}/rest/v1/conversation_members?conversation_id=eq.{id}&profile_id=eq.{uid}&select=id",
        state.rest_url()
    );
    let member_res = state.http.get(&member_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let member_rows: Vec<Value> = member_res.json().await.unwrap_or_default();
    if member_rows.is_empty() {
        return Err(ApiError::NotFound("Conversation not found".into()));
    }

    // Delete members first, then messages, then conversation
    let _ = state.http.delete(format!("{}/rest/v1/conversation_members?conversation_id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let _ = state.http.delete(format!("{}/rest/v1/messages?conversation_id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let res = state.http.delete(format!("{}/rest/v1/conversations?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete conversation".into()));
    }

    ok_message("Conversation deleted")
}

// ─── Screen 74: Messages – Get, Send, Edit, Delete ─────

async fn get_messages(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<NotificationQuery>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify membership
    let member_url = format!(
        "{}/rest/v1/conversation_members?conversation_id=eq.{id}&profile_id=eq.{uid}&select=id",
        state.rest_url()
    );
    let member_res = state.http.get(&member_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let member_rows: Vec<Value> = member_res.json().await.unwrap_or_default();
    if member_rows.is_empty() {
        return Err(ApiError::NotFound("Conversation not found".into()));
    }

    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);

    let url = format!(
        "{}/rest/v1/messages?conversation_id=eq.{id}&order=created_at.desc&limit={limit}&offset={offset}",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let messages: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Enrich messages with sender profile
    let mut enriched = Vec::new();
    for msg in &messages {
        let mut enriched_msg = msg.clone();
        if let Some(sender_id) = msg.get("sender_id").and_then(|v| v.as_str()) {
            let p_url = format!(
                "{}/rest/v1/profiles?id=eq.{sender_id}&select=id,full_name,avatar_url",
                state.rest_url()
            );
            if let Ok(p_res) = state.http.get(&p_url).headers(state.supabase_headers()).send().await {
                if let Ok(p_data) = p_res.json::<Vec<Value>>().await {
                    if let Some(p) = p_data.first() {
                        enriched_msg["sender"] = p.clone();
                    }
                }
            }
        }
        enriched.push(enriched_msg);
    }

    ok_json(json!({
        "messages": enriched,
        "total": enriched.len()
    }))
}

async fn send_message(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<SendMessagePayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if payload.content.trim().is_empty() {
        return Err(ApiError::BadRequest("Message content cannot be empty".into()));
    }

    // Verify membership
    let member_url = format!(
        "{}/rest/v1/conversation_members?conversation_id=eq.{id}&profile_id=eq.{uid}&select=id",
        state.rest_url()
    );
    let member_res = state.http.get(&member_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let member_rows: Vec<Value> = member_res.json().await.unwrap_or_default();
    if member_rows.is_empty() {
        return Err(ApiError::Forbidden("You are not a member of this conversation".into()));
    }

    let msg_type = payload.message_type.unwrap_or_else(|| "text".to_string());
    let body = json!({
        "conversation_id": id,
        "sender_id": uid,
        "content": payload.content,
        "message_type": msg_type,
        "reply_to_id": payload.reply_to_id,
        "is_read": false
    });

    let res = state.http.post(format!("{}/rest/v1/messages", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let msg_data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Update conversation last_message_at
    let update_body = json!({ "last_message_at": chrono::Utc::now().to_rfc3339() });
    let _ = state.http.patch(format!("{}/rest/v1/conversations?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&update_body)
        .send().await;

    ok_json(json!({
        "message": msg_data,
        "message_text": "Message sent"
    }))
}

async fn get_message_detail(
    State(state): State<SharedState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let url = format!("{}/rest/v1/messages?id=eq.{id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let messages: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let message = messages.first().ok_or(ApiError::NotFound("Message not found".into()))?;
    ok_json(message.clone())
}

async fn update_message(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<SendMessagePayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify ownership
    let msg_url = format!("{}/rest/v1/messages?id=eq.{id}&sender_id=eq.{uid}", state.rest_url());
    let msg_res = state.http.get(&msg_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let msgs: Vec<Value> = msg_res.json().await.unwrap_or_default();
    if msgs.is_empty() {
        return Err(ApiError::NotFound("Message not found or not owned by you".into()));
    }

    let body = json!({
        "content": payload.content,
        "is_edited": true,
        "edited_at": chrono::Utc::now().to_rfc3339()
    });

    let res = state.http.patch(format!("{}/rest/v1/messages?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update message".into()));
    }

    ok_message("Message updated")
}

async fn delete_message(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify ownership
    let msg_url = format!("{}/rest/v1/messages?id=eq.{id}&sender_id=eq.{uid}", state.rest_url());
    let msg_res = state.http.get(&msg_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let msgs: Vec<Value> = msg_res.json().await.unwrap_or_default();
    if msgs.is_empty() {
        return Err(ApiError::NotFound("Message not found or not owned by you".into()));
    }

    // Delete reactions first, then message
    let _ = state.http.delete(format!("{}/rest/v1/message_reactions?message_id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let res = state.http.delete(format!("{}/rest/v1/messages?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete message".into()));
    }

    ok_message("Message deleted")
}

// ─── Screen 75: Message Reactions ──────────────────────

async fn get_message_reactions(
    State(state): State<SharedState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let url = format!(
        "{}/rest/v1/message_reactions?message_id=eq.{id}&order=created_at.asc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let reactions: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "reactions": reactions }))
}

async fn add_reaction(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let emoji = payload.get("emoji").and_then(|v| v.as_str())
        .ok_or(ApiError::BadRequest("emoji field required".into()))?;

    // Check for existing reaction
    let check_url = format!(
        "{}/rest/v1/message_reactions?message_id=eq.{id}&profile_id=eq.{uid}&emoji=eq.{emoji}",
        state.rest_url()
    );
    let check_res = state.http.get(&check_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let existing: Vec<Value> = check_res.json().await.unwrap_or_default();
    if !existing.is_empty() {
        return Err(ApiError::BadRequest("Reaction already exists".into()));
    }

    let body = json!({
        "message_id": id,
        "profile_id": uid,
        "emoji": emoji
    });

    let res = state.http.post(format!("{}/rest/v1/message_reactions", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let reaction_data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({ "reaction": reaction_data }))
}

async fn remove_reaction(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;
    let emoji = payload.get("emoji").and_then(|v| v.as_str())
        .ok_or(ApiError::BadRequest("emoji field required".into()))?;

    let url = format!(
        "{}/rest/v1/message_reactions?message_id=eq.{id}&profile_id=eq.{uid}&emoji=eq.{emoji}",
        state.rest_url()
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to remove reaction".into()));
    }

    ok_message("Reaction removed")
}

// ─── Screen 76: Typing Indicator ───────────────────────

async fn send_typing_indicator(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify membership
    let member_url = format!(
        "{}/rest/v1/conversation_members?conversation_id=eq.{id}&profile_id=eq.{uid}&select=id",
        state.rest_url()
    );
    let member_res = state.http.get(&member_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let member_rows: Vec<Value> = member_res.json().await.unwrap_or_default();
    if member_rows.is_empty() {
        return Err(ApiError::Forbidden("Not a member of this conversation".into()));
    }

    // Store typing indicator (expires quickly - DB-level TTL or client-side)
    let body = json!({
        "conversation_id": id,
        "profile_id": uid,
        "typing_at": chrono::Utc::now().to_rfc3339()
    });

    let _ = state.http.post(format!("{}/rest/v1/typing_indicators", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await;

    ok_message("Typing indicator sent")
}

// ─── Screen 77: Read Receipts ──────────────────────────

async fn mark_conversation_read(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Mark all unread messages in this conversation as read (sent by others)
    let url = format!(
        "{}/rest/v1/messages?conversation_id=eq.{id}&sender_id=neq.{uid}&is_read=eq.false",
        state.rest_url()
    );
    let body = json!({ "is_read": true });

    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&body).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to mark messages as read".into()));
    }

    // Update last_read_at in conversation_members
    let member_url = format!(
        "{}/rest/v1/conversation_members?conversation_id=eq.{id}&profile_id=eq.{uid}",
        state.rest_url()
    );
    let member_body = json!({ "last_read_at": chrono::Utc::now().to_rfc3339() });
    let _ = state.http.patch(&member_url).headers(state.supabase_headers()).json(&member_body).send().await;

    ok_message("Conversation marked as read")
}

// ─── Screen 78: Channels / Group Conversations ─────────

async fn get_channels(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Get channels user is a member of
    let member_url = format!(
        "{}/rest/v1/channel_members?profile_id=eq.{uid}&select=channel_id,role",
        state.rest_url()
    );
    let member_res = state.http.get(&member_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let memberships: Vec<Value> = member_res.json().await.unwrap_or_default();

    let channel_ids: Vec<String> = memberships.iter()
        .filter_map(|m| m.get("channel_id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();

    if channel_ids.is_empty() {
        return ok_json(json!({ "channels": [], "total": 0 }));
    }

    let or_filter = channel_ids.iter().map(|id| format!("id.eq.{id}")).collect::<Vec<_>>().join(",");
    let url = format!(
        "{}/rest/v1/channels?or=({or_filter})&order=created_at.desc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let channels: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Enrich with member count
    let mut enriched = Vec::new();
    for ch in &channels {
        let ch_id = ch.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let mut enriched_ch = ch.clone();

        let count_url = format!(
            "{}/rest/v1/channel_members?channel_id=eq.{ch_id}&select=id",
            state.rest_url()
        );
        if let Ok(count_res) = state.http.get(&count_url).headers(state.supabase_headers()).send().await {
            if let Ok(rows) = count_res.json::<Vec<Value>>().await {
                enriched_ch["member_count"] = json!(rows.len());
            }
        }

        enriched.push(enriched_ch);
    }

    ok_json(json!({
        "channels": enriched,
        "total": enriched.len()
    }))
}

async fn create_channel(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<CreateChannelPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest("Channel name cannot be empty".into()));
    }

    let body = json!({
        "name": payload.name,
        "description": payload.description,
        "created_by": uid,
        "type": "group"
    });

    let res = state.http.post(format!("{}/rest/v1/channels", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let channel_data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;
    let channel_id = channel_data.get("id").and_then(|v| v.as_str()).unwrap_or("");

    // Add creator as admin
    let mut members = vec![json!({
        "channel_id": channel_id,
        "profile_id": uid,
        "role": "admin"
    })];

    // Add other members
    for mid in &payload.member_ids {
        members.push(json!({
            "channel_id": channel_id,
            "profile_id": mid,
            "role": "member"
        }));
    }

    let _ = state.http.post(format!("{}/rest/v1/channel_members", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&members)
        .send().await;

    ok_json(json!({
        "channel_id": channel_id,
        "message": "Channel created"
    }))
}

async fn get_channel_detail(
    State(state): State<SharedState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let url = format!("{}/rest/v1/channels?id=eq.{id}", state.rest_url());
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let channels: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    let channel = channels.first().ok_or(ApiError::NotFound("Channel not found".into()))?;

    // Get member count
    let count_url = format!("{}/rest/v1/channel_members?channel_id=eq.{id}&select=id", state.rest_url());
    let member_count = if let Ok(c_res) = state.http.get(&count_url).headers(state.supabase_headers()).send().await {
        if let Ok(rows) = c_res.json::<Vec<Value>>().await { rows.len() as i64 } else { 0 }
    } else { 0 };

    let mut result = channel.clone();
    result["member_count"] = json!(member_count);

    ok_json(result)
}

async fn update_channel(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<UpdateChannelPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify admin role
    let member_url = format!(
        "{}/rest/v1/channel_members?channel_id=eq.{id}&profile_id=eq.{uid}&role=eq.admin",
        state.rest_url()
    );
    let member_res = state.http.get(&member_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let member_rows: Vec<Value> = member_res.json().await.unwrap_or_default();
    if member_rows.is_empty() {
        return Err(ApiError::Forbidden("Only channel admins can update the channel".into()));
    }

    let mut body = serde_json::Map::new();
    if let Some(name) = payload.name { body.insert("name".into(), json!(name)); }
    if let Some(desc) = payload.description { body.insert("description".into(), json!(desc)); }

    if body.is_empty() {
        return Err(ApiError::BadRequest("No fields to update".into()));
    }

    let res = state.http.patch(format!("{}/rest/v1/channels?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&json!(body))
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update channel".into()));
    }

    ok_message("Channel updated")
}

async fn delete_channel(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify creator or admin
    let member_url = format!(
        "{}/rest/v1/channel_members?channel_id=eq.{id}&profile_id=eq.{uid}&role=eq.admin",
        state.rest_url()
    );
    let member_res = state.http.get(&member_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let member_rows: Vec<Value> = member_res.json().await.unwrap_or_default();
    if member_rows.is_empty() {
        return Err(ApiError::Forbidden("Only channel admins can delete the channel".into()));
    }

    // Clean up
    let _ = state.http.delete(format!("{}/rest/v1/channel_members?channel_id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let _ = state.http.delete(format!("{}/rest/v1/channel_messages?channel_id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await;
    let res = state.http.delete(format!("{}/rest/v1/channels?id=eq.{id}", state.rest_url()))
        .headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete channel".into()));
    }

    ok_message("Channel deleted")
}

async fn get_channel_members(
    State(state): State<SharedState>,
    _auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let url = format!(
        "{}/rest/v1/channel_members?channel_id=eq.{id}&order=joined_at.asc",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let members: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Enrich with profile info
    let mut enriched = Vec::new();
    for m in &members {
        let mut member = m.clone();
        if let Some(pid) = m.get("profile_id").and_then(|v| v.as_str()) {
            let p_url = format!(
                "{}/rest/v1/profiles?id=eq.{pid}&select=id,full_name,avatar_url,headline",
                state.rest_url()
            );
            if let Ok(p_res) = state.http.get(&p_url).headers(state.supabase_headers()).send().await {
                if let Ok(p_data) = p_res.json::<Vec<Value>>().await {
                    if let Some(p) = p_data.first() {
                        member["profile"] = p.clone();
                    }
                }
            }
        }
        enriched.push(member);
    }

    ok_json(json!({ "members": enriched, "total": enriched.len() }))
}

async fn add_channel_member(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<AddChannelMemberPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify admin
    let admin_url = format!(
        "{}/rest/v1/channel_members?channel_id=eq.{id}&profile_id=eq.{uid}&role=eq.admin",
        state.rest_url()
    );
    let admin_res = state.http.get(&admin_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let admin_rows: Vec<Value> = admin_res.json().await.unwrap_or_default();
    if admin_rows.is_empty() {
        return Err(ApiError::Forbidden("Only admins can add members".into()));
    }

    let role = payload.role.unwrap_or_else(|| "member".to_string());
    let body = json!({
        "channel_id": id,
        "profile_id": payload.user_id,
        "role": role
    });

    let res = state.http.post(format!("{}/rest/v1/channel_members", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to add member".into()));
    }

    ok_message("Member added to channel")
}

async fn remove_channel_member(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<AddChannelMemberPayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    // Verify admin OR removing self
    if *uid != payload.user_id {
        let admin_url = format!(
            "{}/rest/v1/channel_members?channel_id=eq.{id}&profile_id=eq.{uid}&role=eq.admin",
            state.rest_url()
        );
        let admin_res = state.http.get(&admin_url).headers(state.supabase_headers()).send().await
            .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
        let admin_rows: Vec<Value> = admin_res.json().await.unwrap_or_default();
        if admin_rows.is_empty() {
            return Err(ApiError::Forbidden("Only admins can remove other members".into()));
        }
    }

    let url = format!(
        "{}/rest/v1/channel_members?channel_id=eq.{id}&profile_id=eq.{}",
        state.rest_url(), payload.user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to remove member".into()));
    }

    ok_message("Member removed from channel")
}

async fn get_channel_messages(
    State(state): State<SharedState>,
    _auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<NotificationQuery>,
) -> ApiResult {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);

    let url = format!(
        "{}/rest/v1/channel_messages?channel_id=eq.{id}&order=created_at.desc&limit={limit}&offset={offset}",
        state.rest_url()
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let messages: Vec<Value> = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    // Enrich with sender profile
    let mut enriched = Vec::new();
    for msg in &messages {
        let mut enriched_msg = msg.clone();
        if let Some(sender_id) = msg.get("sender_id").and_then(|v| v.as_str()) {
            let p_url = format!(
                "{}/rest/v1/profiles?id=eq.{sender_id}&select=id,full_name,avatar_url",
                state.rest_url()
            );
            if let Ok(p_res) = state.http.get(&p_url).headers(state.supabase_headers()).send().await {
                if let Ok(p_data) = p_res.json::<Vec<Value>>().await {
                    if let Some(p) = p_data.first() {
                        enriched_msg["sender"] = p.clone();
                    }
                }
            }
        }
        enriched.push(enriched_msg);
    }

    ok_json(json!({
        "messages": enriched,
        "total": enriched.len()
    }))
}

async fn send_channel_message(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<SendMessagePayload>,
) -> ApiResult {
    let uid = &auth.claims.profile_id;

    if payload.content.trim().is_empty() {
        return Err(ApiError::BadRequest("Message content cannot be empty".into()));
    }

    // Verify membership
    let member_url = format!(
        "{}/rest/v1/channel_members?channel_id=eq.{id}&profile_id=eq.{uid}&select=id",
        state.rest_url()
    );
    let member_res = state.http.get(&member_url).headers(state.supabase_headers()).send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;
    let member_rows: Vec<Value> = member_res.json().await.unwrap_or_default();
    if member_rows.is_empty() {
        return Err(ApiError::Forbidden("You are not a member of this channel".into()));
    }

    let msg_type = payload.message_type.unwrap_or_else(|| "text".to_string());
    let body = json!({
        "channel_id": id,
        "sender_id": uid,
        "content": payload.content,
        "message_type": msg_type,
        "reply_to_id": payload.reply_to_id
    });

    let res = state.http.post(format!("{}/rest/v1/channel_messages", state.rest_url()))
        .headers(state.supabase_headers())
        .json(&body)
        .send().await
        .map_err(|e| ApiError::Internal(format!("Network error: {e}")))?;

    let msg_data: Value = res.json().await
        .map_err(|e| ApiError::Internal(format!("Parse error: {e}")))?;

    ok_json(json!({
        "message": msg_data,
        "message_text": "Message sent to channel"
    }))
}
