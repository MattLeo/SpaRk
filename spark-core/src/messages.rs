use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Room,
    Private,
    Server,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub sender_id: String,
    pub message_type: MessageType,
    pub room_id: Option<String>,
    pub receiver_id: Option<String>,
    pub content: String,
    pub sent_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
    pub is_read: bool,
    pub is_edited: bool,
    pub edited_at: Option<DateTime<Utc>>,
    pub reply_to_message_id: Option<String>,
    pub reactions: Vec<ReactionSummary>,
    pub is_pinned: bool,
    pub pinned_at: Option<DateTime<Utc>>,
    pub pinned_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMember {
    pub room_id: String,
    pub user_id: String,
    pub joined_at: DateTime<Utc>,
}

// Requests

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendRoomMessageRequest {
    pub room_id: String,
    pub content: String,
    pub reply_to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendPrivateMessageRequest {
    pub receiver_username: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRoomMessagesRequest {
    pub room_id: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPrivateMessagesRequest {
    pub with_user: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub unread_only: bool,
}

// Responses

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMessageResponse {
    pub id: String,
    pub sender_username: String,
    pub message_type: MessageType,
    pub room_id: String,
    pub room_name: String,
    pub content: String,
    pub sent_at: DateTime<Utc>,
    pub is_edited: bool,
    pub edited_at: Option<DateTime<Utc>>,
    pub mentions: Vec<String>,
    pub reply_to: Option<MessageReplyContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateMessageResponse {
    pub id: String,
    pub sender_username: String,
    pub receiver_username: String,
    pub content: String,
    pub sent_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
    pub is_read: bool,
    pub is_edited: bool,
    pub edited_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReplyContext {
    pub id: String,
    pub sender_username: String,
    pub content: String,
    pub sent_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionSummary {
    pub emoji: String,
    pub count: usize,
    pub user_ids: Vec<String>,
    pub usernames: Vec<String>,
}

impl Default for GetRoomMessagesRequest {
    fn default() -> Self {
        Self {
            room_id: String::new(),
            limit: Some(100),
            offset: Some(0),
        }
    }
}

impl Default for GetPrivateMessagesRequest {
    fn default() -> Self {
        Self {
            with_user: None,
            limit: Some(100),
            offset: Some(0),
            unread_only: false,
        }
    }
}