use crate::network::{AuthService, MessageService};
use crate::messages::{ReactionSummary, RoomMessageResponse, SendRoomMessageRequest};
use crate::users::{Presence, User};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use chrono::Utc;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum WsClientMessage {
    Authenticate { token: String },
    CreateRoom {name: String, desc: String},
    GetAllRooms,
    JoinRoom { room_id: String },
    LeaveRoom { room_id: String },
    SendMessage { 
        room_id: String, 
        content: String , 
        reply_to_message_id: Option<String>, 
        content_format: Option<String> 
    },
    GetRoomHistory { room_id: String, limit: Option<usize>, offset: Option<usize> },
    EditMessage {
        room_id: String, 
        message_id: String, 
        new_content: String,
        content_format: Option<String>,
    },
    DeleteMessage {room_id: String, message_id: String},
    GetUserRooms { user_id: String},
    GetRoomMembers { room_id: String },
    UpdatePresence { user_id: String, presence: Presence },
    UpdateStatus { user_id: String, status: String },
    UpdateTyping { room_id: String, is_typing: bool },
    GetUnreadMentionsCount { user_id: String },
    MarkMentionsRead { message_id: String },
    MarkRoomMentionsRead { room_id: String },
    GetUserMentions { limit: Option<usize>, offset: Option<usize> },
    AddReaction { room_id: String, message_id: String, emoji: String },
    RemoveReaction { room_id: String, message_id: String, emoji: String },
    PinMessage { room_id: String, message_id: String },
    UnpinMessage { room_id: String, message_id: String },
    GetPinnedMessages { room_id: String },
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type")]
pub enum WsServerMessage {
    Authenticated { user_id: String, username: String },
    Error { message: String },
    RoomCreated { room_id: String, room_name: String },
    RoomList { rooms: Vec<RoomInfo> },
    RoomJoined { room_id: String, room_name: String },
    RoomLeft { room_id: String },
    NewMessage { room_id: String, message: RoomMessageResponse },
    MessageSent { message_id: String },
    RoomHistory { room_id: String, messages: Vec<RoomMessageResponse> },
    UserJoined { room_id: String, user_id: String, username: String },
    UserLeft { room_id: String, user_id: String, username: String },
    MessageEdited {
        room_id: String, 
        message_id: String, 
        new_content: String, 
        edited_at: String, 
        content_format: Option<String>,
    },
    MessageDeleted {room_id: String, message_id: String},
    UserRoomList { rooms: Vec<RoomInfo> },
    RoomMembers { room_id: String, members: Vec<User> },
    PresenceChanged { user_id: String, username: String, presence: Presence },
    StatusChanged { user_id: String, username: String, status: String },
    TypingStatusChanged { room_id: String, typing_users: Vec<TypingUser> },
    MentionNotification {
        message_id: String,
        room_id: String,
        room_name: String,
        sender_username: String,
        content: String,
        content_format: String,
        sent_at: String,
    },
    UnreadMentionsCount { count: i64 },
    ReactionAdded {
        room_id: String,
        message_id: String,
        emoji: String,
        user_id: String,
        username: String,
        reactions: Vec<ReactionSummary>,
    },
    ReactionRemoved {
        room_id: String,
        message_id: String,
        emoji: String,
        user_id: String,
        reactions: Vec<ReactionSummary>,
    },
    MessagePinned {
        room_id: String,
        message_id: String,
        pinned_by: String,
        pinned_at: String,
    },
    MessageUnpinned { room_id: String, message_id: String },
    PinnedMessages { room_id: String, messages: Vec<RoomMessageResponse> },
}

#[derive(Debug, Serialize, Clone)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    pub desc: String,
}

#[allow(dead_code)]
struct Client {
    user_id: String,
    username: String,
    sender: mpsc::UnboundedSender<WsServerMessage>,
    rooms: HashSet<String>,
}

pub struct ConnectionManager {
    clients: HashMap<String, Client>,
    rooms: HashMap<String, HashSet<String>>,
    typing_users: HashMap<String, HashSet<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TypingUser {
    pub user_id: String,
    pub username: String, 
}

impl ConnectionManager {
    fn new() -> Self {
        Self {
            clients: HashMap::new(),
            rooms: HashMap::new(),
            typing_users: HashMap::new(),
        }
    }

    fn add_client(&mut self, user_id: String, username: String, sender: mpsc::UnboundedSender<WsServerMessage>) {
        self.clients.insert(user_id.clone(), Client {
            user_id,
            username,
            sender,
            rooms: HashSet::new(),
        });
    }

    fn remove_client(&mut self, user_id: &str) {
        if let Some(client) = self.clients.get(user_id) {
            for room_id in &client.rooms {
                if let Some(typing_set) = self.typing_users.get_mut(room_id) {
                    typing_set.remove(user_id);
                }
            }
        }

        self.clients.remove(user_id);
    }

    fn join_room(&mut self, user_id: &str, room_id: String) -> Result<(), String> {
        let client = self.clients.get_mut(user_id).ok_or("Client not found")?;

        client.rooms.insert(room_id.clone());
        self.rooms.entry(room_id).or_insert_with(HashSet::new).insert(user_id.to_string());

        Ok(())
    }

    fn leave_room(&mut self, user_id: &str, room_id: String) -> Result<(), String> {
        let client = self.clients.get_mut(user_id).ok_or("Client not found")?;

        client.rooms.remove(&room_id);
        if let Some(room) = self.rooms.get_mut(&room_id) {
            room.remove(user_id);
        }

        if let Some(typing_set) = self.typing_users.get_mut(&room_id) {
            typing_set.remove(user_id);
        }

        Ok(())
    }

    fn broadcast_to_room(&self, room_id: &str, message: WsServerMessage) {
        if let Some(user_ids) = self.rooms.get(room_id) {
            for user_id in user_ids {
                if let Some(client) = self.clients.get(user_id) {
                    let _ = client.sender.send(message.clone());
                }
            }
        }
    }

    fn restore_user_rooms(&mut self, user_id: &str, room_ids: Vec<String>) {
        if let Some(client) = self.clients.get_mut(user_id) {
            for room_id in room_ids {
                client.rooms.insert(room_id.clone());
                self.rooms.entry(room_id).or_insert_with(HashSet::new).insert(user_id.to_string());
            }
        }
    }

    fn set_typing(&mut self, user_id: &str, room_id: &str, is_typing: bool) -> Result<(), String> {
        if let Some(client) = self.clients.get(user_id) {
            if !client.rooms.contains(room_id) {
                return Err("User not in room".to_string());
            }
        } else {
            return Err("User not found".to_string());
        }

        let typing_set = self.typing_users.entry(room_id.to_string()).or_insert_with(HashSet::new);

        if is_typing {
            typing_set.insert(user_id.to_string());
        } else {
            typing_set.remove(user_id);
        }

        Ok(())
    }

    fn get_typing_users(&mut self, room_id: &str) -> Vec<(String, String)> {
        if let Some(typing_set) = self.typing_users.get(room_id) {
            typing_set.iter()
                .filter_map(|user_id| {
                    self.clients.get(user_id).map(|client| {
                        (user_id.clone(), client.username.clone())
                    })
                }).collect()
        } else {
            Vec::new()
        }
    }

    /*
    fn send_to_user(&self, user_id: &str, message: WsServerMessage) -> Result<(), String> {
        let client = self.clients.get(user_id).ok_or("Client not found")?;
        client.sender.send(message).map_err(|_| "Failed to send message".to_string())
    }

    fn get_username(&self, user_id: &str) -> Option<String> {
        self.clients.get(user_id).map(|c| c.username.clone())
    }
    */
}

pub struct WebSocketServer {
    auth: Arc<Mutex<AuthService>>,
    message_service: Arc<Mutex<MessageService>>,
    connections: Arc<RwLock<ConnectionManager>>,
    addr: String,
}

impl WebSocketServer {
    pub fn new(auth: Arc<Mutex<AuthService>>, message_service: Arc<Mutex<MessageService>>, addr: String) -> Self{
        Self {
            auth,
            message_service,
            connections: Arc::new(RwLock::new(ConnectionManager::new())),
            addr,
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(&self.addr).await?;
        println!("WebSocket server listening on {}", self.addr);

        loop {
            let (stream, addr) = listener.accept().await?;
            println!("New WebSocket connection from: {}", addr);

            let auth = Arc::clone(&self.auth);
            let message_service = Arc::clone(&self.message_service);
            let connections = Arc::clone(&self.connections);

            tokio::spawn(async move {
                if let Err(e) = handle_websocket_connections(stream, auth, message_service, connections).await {
                    eprintln!(" {} - WebSocket error: {}", addr, e);
                }
            });
        }
    }
}

async fn handle_websocket_connections(
    stream: TcpStream,
    auth: Arc<Mutex<AuthService>>,
    message_service: Arc<Mutex<MessageService>>,
    connections: Arc<RwLock<ConnectionManager>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let ws_stream = accept_async(stream).await?;
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<WsServerMessage>();

    let mut authenticated_user_id: Option<String> = None;
    let mut authenticated_username: Option<String> = None;

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await{
            let json = serde_json::to_string(&msg).unwrap();
            if ws_sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(msg) = ws_receiver.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("WebSocket received error: {}", e);
                break;
            }
        };

        if let Message::Text(text) = msg {
            let client_msg: WsClientMessage = match serde_json::from_str(&text) {
                Ok(msg) => msg,
                Err(e) => {
                    let _ = tx.send(WsServerMessage::Error { 
                        message: format!("Invalid message format: {}", e) 
                    });
                    continue;
                }
            };

            match client_msg {
                WsClientMessage::Authenticate { token } => {
                    let auth = auth.lock().await;
                    match auth.validate_session(&token) {
                        Ok(user) => {
                            authenticated_user_id = Some(user.id.clone());
                            authenticated_username = Some(user.username.clone());

                            connections.write().await.add_client(
                                user.id.clone(),
                                user.username.clone(),
                                tx.clone()
                            );

                            let _ = tx.send(WsServerMessage::Authenticated { 
                                user_id: user.id.clone(), 
                                username: user.username.clone() 
                            });

                            let msg_service = message_service.lock().await;
                            let _ = msg_service.update_user_presence(&user.id, Presence::Online);


                            if let Ok(user_rooms) = msg_service.get_user_rooms(&user.id) {
                                let room_ids: Vec<String> = user_rooms.iter().map(|r| r.id.clone()).collect();

                                drop(msg_service);
                                connections.write().await.restore_user_rooms(&user.id, room_ids);
                                let conns = connections.read().await;

                                for room in user_rooms {
                                    let _ = tx.send(WsServerMessage::RoomJoined { room_id: room.id.clone(), room_name: room.name });
                                    
                                    conns.broadcast_to_room(&room.id, WsServerMessage::PresenceChanged { 
                                        user_id: user.id.clone(), 
                                        username: user.username.clone(), 
                                        presence: Presence::Online 
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(WsServerMessage::Error { 
                                message: format!("Authentication failed: {}", e) 
                            });
                            break;
                        }
                    }
                }
                _ => {
                    let user_id = match &authenticated_user_id {
                        Some(id) => id,
                        None => {
                            let _ = tx.send(WsServerMessage::Error { 
                                message: "User not authenticated".to_string(),
                            });
                            continue;
                        }
                    };

                    match client_msg {
                        WsClientMessage::JoinRoom { room_id } => {
                            let msg_service = message_service.lock().await;
                            match msg_service.get_room(&room_id) {
                                Ok(Some(room)) => {
                                    if let Err(e) = msg_service.join_room(user_id, &room_id) {
                                        let _ = tx.send(WsServerMessage::Error { 
                                            message: format!("Failed to join room: {}", e) 
                                        });
                                        continue;
                                    }

                                    if let Err(e) = connections.write().await.join_room(user_id, room_id.clone()) {
                                        let _ = tx.send(WsServerMessage::Error { 
                                            message: format!("Failed to join room: {}", e) 
                                        });
                                        continue;
                                    }

                                    let _ = tx.send(WsServerMessage::RoomJoined { 
                                        room_id: room_id.clone(), 
                                        room_name: room.name.clone() 
                                    });

                                    if let Some(username) = &authenticated_username {
                                        connections.read().await.broadcast_to_room(
                                            &room_id, 
                                            WsServerMessage::UserJoined { 
                                                room_id: room_id.clone(), 
                                                user_id: user_id.clone(), 
                                                username: username.clone() 
                                            },
                                        );

                                        let announcement_content = format!("{} has joined the room", username);
                                        let announcment_request = SendRoomMessageRequest {
                                            room_id: room_id.clone(),
                                            content: announcement_content,
                                            content_format: None,
                                            reply_to_message_id: None,
                                        };

                                        if let Ok(announcement_response) = msg_service.send_room_announcement(user_id, announcment_request) {
                                            connections.read().await.broadcast_to_room(
                                                &room_id, 
                                                WsServerMessage::NewMessage {
                                                    room_id: room_id.clone(),
                                                    message: announcement_response
                                                }
                                            );
                                        }

                                        match msg_service.get_room_members(&room_id) {
                                            Ok(members) => {
                                                let _ = tx.send(WsServerMessage::RoomMembers { room_id: room_id.clone(), members });
                                            }
                                            Err(e) => {
                                                let _ = tx.send(WsServerMessage::Error { message: format!("Failed to get room members: {}", e) });
                                            }
                                        }
                                    }
                                }
                                Ok(None) => {
                                    let _ = tx.send(WsServerMessage::Error { 
                                        message: "Room not found".to_string() 
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(WsServerMessage::Error { 
                                        message: format!("Failed to get room: {}", e), 
                                    });
                                }
                            }
                        }
                        WsClientMessage::LeaveRoom { room_id } => {
                            let msg_service = message_service.lock().await;
                            if let Err(e) = connections.write().await.leave_room(user_id, room_id.clone()) {
                                let _ = tx.send(WsServerMessage::Error { message: format!("Failed to leave room: {}", e) });
                                continue;
                            }

                            if let Err(e) = msg_service.leave_room(user_id, &room_id) {
                                let _ = tx.send(WsServerMessage::Error { 
                                    message: format!("Failed to leave room: {}", e) 
                                });
                                continue;
                            }

                            let _ = tx.send(WsServerMessage::RoomLeft { room_id: room_id.clone() });

                            if let Some(username) = &authenticated_username {
                                connections.read().await.broadcast_to_room(&room_id, WsServerMessage::UserLeft { 
                                    room_id: room_id.clone(), 
                                    user_id: user_id.clone(), 
                                    username: username.clone() 
                                });

                                let announcement_content = format!("{} has left the room", username);
                                let announcement_request = SendRoomMessageRequest {
                                    room_id: room_id.clone(),
                                    content: announcement_content,
                                    content_format: None,
                                    reply_to_message_id: None,
                                };

                                if let Ok(announcement_response) = msg_service.send_room_announcement(user_id, announcement_request) {
                                    connections.read().await.broadcast_to_room(
                                        &room_id,
                                        WsServerMessage::NewMessage { room_id: room_id.clone(), 
                                            message: announcement_response } 
                                    );
                                }
                            }
                        }
                        WsClientMessage::SendMessage { room_id, content , reply_to_message_id, content_format} => {
                            if let (Some(user_id), Some(_username)) = (&authenticated_user_id, &authenticated_username) {
                                let msg_service = message_service.lock().await;
                                let request = SendRoomMessageRequest {
                                    room_id: room_id.clone(),
                                    content,
                                    content_format,
                                    reply_to_message_id,
                                };

                                match msg_service.send_room_message(user_id, request) {
                                    Ok((message_response, mentioned_user_ids)) => {
                                        let _ = tx.send(WsServerMessage::MessageSent { message_id: message_response.id.clone() });

                                        connections.read().await.broadcast_to_room(
                                            &room_id,
                                            WsServerMessage::NewMessage { 
                                                room_id: room_id.clone(), 
                                                message: message_response.clone()
                                            } 
                                        );

                                        if !mentioned_user_ids.is_empty() {
                                            let conns = connections.read().await;
                                            for mentioned_user_id in &mentioned_user_ids {
                                                if let Some(client) = conns.clients.get(mentioned_user_id) {
                                                    let _ = client.sender.send(WsServerMessage::MentionNotification { 
                                                        message_id: message_response.id.clone(), 
                                                        room_id: message_response.room_id.clone(), 
                                                        room_name: message_response.room_name.clone(), 
                                                        sender_username: message_response.sender_username.clone(), 
                                                        content: message_response.content.clone(), 
                                                        content_format: message_response.content_format.clone(),
                                                        sent_at: message_response.sent_at.to_rfc3339(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = tx.send(WsServerMessage::Error { 
                                            message: format!("Failed to send message: {}", e) 
                                        });
                                    }
                                }
                            } else {
                                let _ = tx.send(WsServerMessage::Error { message: "Not Authenticated".to_string() });
                            }
                        }
                        WsClientMessage::GetRoomHistory { room_id, limit, offset } => { 
                            let msg_service = message_service.lock().await;

                            match msg_service.get_room_messages(
                                &room_id,
                                limit.unwrap_or(50),
                                offset.unwrap_or(0)) {

                                Ok(messages) => {
                                    let _ = tx.send(WsServerMessage::RoomHistory { room_id, messages });
                                }
                                Err(e) => {
                                    let _ = tx.send(WsServerMessage::Error { 
                                        message: format!("Failed to get history: {}", e) 
                                    });
                                }
                            }
                        }
                        #[allow(unused_variables)]
                        WsClientMessage::Authenticate { token } => {
                            //already handled, leaving here just to satistfy the compiler
                        }
                        WsClientMessage::CreateRoom { name, desc } => {
                            let msg_service = message_service.lock().await;

                            match msg_service.create_room(user_id, &name, &desc) {
                                Ok(room) => {
                                    let _ = tx.send(WsServerMessage::RoomCreated { 
                                        room_id: room.id.clone(), 
                                        room_name: room.name.clone() 
                                    });

                                    if let Err(e) = connections.write().await.join_room(user_id, room.id.clone()) {
                                        let _ = tx.send(WsServerMessage::Error { 
                                            message: format!("Error joining created room: {}", e)  
                                        });
                                    }

                                    if let Err(e) = msg_service.join_room(user_id, &room.id) {
                                        let _ = tx.send(WsServerMessage::Error { 
                                            message: format!("Error joining created room: {}", e)  
                                        });
                                    }

                                    let _ = tx.send(WsServerMessage::RoomJoined { 
                                        room_id: room.id.clone(), 
                                        room_name: room.name.clone()
                                    });

                                    let _ = tx.send(WsServerMessage::RoomHistory { 
                                        room_id: room.id.clone(), 
                                        messages: vec![] 
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(WsServerMessage::Error { 
                                        message: format!("Error creating room: {}", e) 
                                    });
                                }
                            }
                        }
                        WsClientMessage::GetAllRooms => {
                            let msg_service = message_service.lock().await;

                            match msg_service.get_all_rooms() {
                                Ok(rooms) => {
                                    let rooms_info: Vec<RoomInfo> = rooms.into_iter()
                                        .map(|r| RoomInfo {
                                            id: r.id,
                                            name: r.name,
                                            desc: r.desc
                                        })
                                        .collect();

                                    let _ = tx.send(WsServerMessage::RoomList { rooms: rooms_info });
                                }
                                Err(e) => {
                                    let _ = tx.send(WsServerMessage::Error { 
                                        message: format!("Error getting room list: {}", e)  
                                    });
                                }
                            }
                        }
                        WsClientMessage::EditMessage { room_id, message_id, new_content , content_format}  => {
                            let msg_service = message_service.lock().await;

                            match msg_service.edit_message(&message_id, &new_content, content_format.as_deref()) {
                                Ok(()) => {
                                    let edited_at = Utc::now().to_rfc3339();
                                    connections.read().await.broadcast_to_room(
                                        &room_id, 
                                        WsServerMessage::MessageEdited { 
                                            room_id: room_id.clone(), 
                                            message_id, 
                                            new_content,
                                            content_format, 
                                            edited_at 
                                        }
                                    );
                                }
                                Err(e) => {
                                    let _ = tx.send(WsServerMessage::Error { message: format!("Failed to edit message: {}", e) });
                                }
                            }
                        }
                        WsClientMessage::DeleteMessage { room_id, message_id } => {
                            let msg_server = message_service.lock().await;

                            match msg_server.delete_message(user_id, &message_id) {
                                Ok(()) => {
                                    connections.read().await.broadcast_to_room(
                                        &room_id, 
                                        WsServerMessage::MessageDeleted { 
                                            room_id: room_id.clone(), 
                                            message_id 
                                        }
                                    );
                                }
                                Err(e) => {
                                    let _ = tx.send(WsServerMessage::Error { 
                                        message: format!("Failed to delete message: {}", e) 
                                    });
                                }
                            }
                        }
                        WsClientMessage::GetUserRooms { user_id } => {
                            let msg_service = message_service.lock().await;

                            match msg_service.get_user_rooms(&user_id) {
                                Ok(rooms) => {
                                    let rooms_info = rooms.into_iter().map(|r| {RoomInfo {
                                        id: r.id,
                                        name: r.name,
                                        desc: r.desc,
                                    }}).collect();

                                    let _ = tx.send(WsServerMessage::UserRoomList { rooms: rooms_info });
                                }
                                Err(e) => {
                                    let _ = tx.send(WsServerMessage::Error { message: format!("Failed to get user rooms: {}", e) });
                                }
                            }
                        }
                        WsClientMessage::UpdatePresence { user_id, presence } => {
                            let msg_service = message_service.lock().await;

                            if let Err(e) = msg_service.update_user_presence(&user_id, presence.clone()) {
                                let _ = tx.send(WsServerMessage::Error { message: format!("Failed to update presence: {}", e) });
                            }

                            match msg_service.get_user_rooms(&user_id) {
                                Ok(rooms) => {
                                    let conns = connections.read().await;
                                    for room in rooms {
                                        conns.broadcast_to_room(&room.id, WsServerMessage::PresenceChanged { 
                                            user_id: user_id.clone(), 
                                            username: authenticated_username.clone().unwrap_or_default(), 
                                            presence: presence.clone() 
                                        });
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(WsServerMessage::Error { message: format!("Failed to broadcase presence: {}", e) });
                                }
                            }
                        }
                        WsClientMessage::UpdateStatus { user_id, status } => {
                            let msg_service = message_service.lock().await;

                            if let Err(e) = msg_service.update_user_status(&user_id, &status) {
                                let _ = tx.send(WsServerMessage::Error { message: format!("Failed to update status: {}", e) });
                            }

                            match msg_service.get_user_rooms(&user_id) {
                                Ok(rooms) => {
                                    let conns = connections.read().await;
                                    for room in rooms {
                                        conns.broadcast_to_room(&room.id, WsServerMessage::StatusChanged { 
                                            user_id: user_id.clone(), 
                                            username: authenticated_username.clone().unwrap_or_default(), 
                                            status: status.clone() 
                                        });
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(WsServerMessage::Error { message: format!("Failed to broadcast status: {}", e) });
                                }
                            }
                        }
                        WsClientMessage::GetRoomMembers { room_id } => {
                            let msg_service = message_service.lock().await;

                            match msg_service.get_room_members(&room_id) {
                                Ok(members) => {
                                    let _ = tx.send(WsServerMessage::RoomMembers { 
                                        room_id, 
                                        members, 
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(WsServerMessage::Error { message: format!("Failed to get room members: {}", e) });
                                }
                            }
                        }
                        WsClientMessage::UpdateTyping { room_id, is_typing } => {
                            if let Err(e) = connections.write().await.set_typing(user_id, &room_id, is_typing) {
                                let _ = tx.send(WsServerMessage::Error { message: format!("Failed to update typing status : {}", e) });
                                continue;
                            }

                            let typing_users_list = connections.write().await.get_typing_users(&room_id);
                            let typing_users: Vec<TypingUser> = typing_users_list.into_iter()
                                .map(|(user_id, username)| TypingUser { user_id, username,})
                                .collect();

                            connections.read().await.broadcast_to_room(
                                &room_id, 
                                WsServerMessage::TypingStatusChanged { 
                                    room_id: room_id.clone(), 
                                    typing_users 
                                }
                            );
                        }
                        WsClientMessage::GetUnreadMentionsCount { user_id } => {
                            if let Some(auth_user_id) = &authenticated_user_id {
                                if auth_user_id == &user_id {
                                    let msg_service = message_service.lock().await;
                                    match msg_service.get_unread_mentions_count(&user_id) {
                                        Ok(count) => {
                                            let _ = tx.send(WsServerMessage::UnreadMentionsCount { count });
                                        }
                                        Err(e) => {
                                            let _ = tx.send(WsServerMessage::Error { message: format!("Error getting unread mentions count: {}", e) });
                                        }
                                    }
                                }
                            }
                        }
                        WsClientMessage::MarkMentionsRead { message_id } => {
                            if let Some(user_id) = &authenticated_user_id {
                                let msg_service = message_service.lock().await;
                                match msg_service.mark_mention_as_read(user_id, &message_id) {
                                    Ok(()) => {
                                        if let Ok(count) = msg_service.get_unread_mentions_count(user_id) {
                                            let _ = tx.send(WsServerMessage::UnreadMentionsCount { count });
                                        }
                                    }
                                    Err(e) => {
                                        let _ = tx.send(WsServerMessage::Error { message: format!("Error marking mentions as read: {}", e) });
                                    }
                                }
                            }
                        }
                        WsClientMessage::MarkRoomMentionsRead { room_id } => {
                            if let Some(user_id) = &authenticated_user_id {
                                let msg_service = message_service.lock().await;
                                match msg_service.mark_room_mentions_as_read(user_id, &room_id) {
                                    Ok(()) => {
                                        if let Ok(count) = msg_service.get_unread_mentions_count(user_id) {
                                            let _ = tx.send(WsServerMessage::UnreadMentionsCount { count });
                                        }
                                    }
                                    Err(e) => {
                                        let _ = tx.send(WsServerMessage::Error { message: format!("Error marking room mentions as read: {}", e) });
                                    }
                                }
                            }
                        }
                        WsClientMessage::GetUserMentions { limit, offset } => {
                            if let Some(user_id) = &authenticated_user_id {
                                let msg_service = message_service.lock().await;
                                match msg_service.get_user_mentions(user_id, limit.unwrap_or(100), offset.unwrap_or(0)) {
                                    Ok(mentions) => {
                                        let _ = tx.send(WsServerMessage::RoomHistory { 
                                            room_id: "mentions".to_string(),
                                            messages: mentions,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(WsServerMessage::Error { message: format!("Error fetching mentions: {}", e) });
                                    }
                                }
                            }
                        }
                        WsClientMessage::AddReaction { room_id, message_id, emoji } => {
                            if let (Some(user_id), Some(username)) = (&authenticated_user_id, &authenticated_username) {
                                let msg_service = message_service.lock().await;

                                match msg_service.add_reaction(&message_id, user_id, username, &emoji) {
                                    Ok(reactions) => {
                                        connections.read().await.broadcast_to_room(
                                            &room_id,
                                            WsServerMessage::ReactionAdded { 
                                                room_id: room_id.clone(), 
                                                message_id: message_id.clone(), 
                                                emoji: emoji.clone(), 
                                                user_id: user_id.clone(), 
                                                username: username.clone(), 
                                                reactions 
                                            }
                                        );
                                    }
                                    Err(e) => {
                                        let _ = tx.send(WsServerMessage::Error { 
                                            message: format!("Failed to add reaction: {}", e) 
                                        });
                                    }
                                }
                            }
                        }
                        WsClientMessage::RemoveReaction { room_id, message_id, emoji } => {
                            if let Some(user_id) = &authenticated_user_id {
                                let msg_service = message_service.lock().await;

                                match msg_service.remove_reaction(&message_id, &user_id.clone(), &emoji) {
                                    Ok(reactions) => {
                                        connections.read().await.broadcast_to_room(
                                            &room_id, 
                                            WsServerMessage::ReactionRemoved { 
                                                room_id: room_id.clone(), 
                                                message_id: message_id.clone(), 
                                                emoji: emoji.clone(), 
                                                user_id: user_id.clone(), 
                                                reactions, 
                                            }
                                        );
                                    }
                                    Err(e) => {
                                        let _ = tx.send(WsServerMessage::Error { 
                                            message: format!("Unable to remove reaction: {}", e) 
                                        });
                                    }
                                }
                            }
                        }
                        WsClientMessage::PinMessage { room_id, message_id } => {
                            if let Some(user_id) = &authenticated_user_id {
                                let msg_service = message_service.lock().await;

                                match msg_service.pin_message(&room_id, &message_id, user_id) {
                                    Ok(pinned_at) => {
                                        connections.read().await.broadcast_to_room(
                                            &room_id, 
                                            WsServerMessage::MessagePinned { 
                                                room_id: room_id.clone(), 
                                                message_id: message_id.clone(), 
                                                pinned_by: user_id.clone(), 
                                                pinned_at: pinned_at.to_rfc3339(), 
                                            }
                                        );
                                    },
                                    Err(e) => {
                                        let _ = tx.send(WsServerMessage::Error { 
                                            message: format!("Failed to pin message: {}", e) 
                                        });
                                    }
                                }
                            }
                        }
                        WsClientMessage::UnpinMessage { room_id, message_id } => {
                            if let Some(user_id) = &authenticated_user_id {
                                let msg_service = message_service.lock().await;

                                match msg_service.unpin_message(&room_id, &message_id, user_id) {
                                    Ok(()) => {
                                        connections.read().await.broadcast_to_room(
                                            &room_id, 
                                            WsServerMessage::MessageUnpinned { 
                                                room_id: room_id.clone(), 
                                                message_id: message_id.clone(), 
                                            }
                                        );
                                    }
                                    Err(e) => {
                                        let _ = tx.send(WsServerMessage::Error { 
                                            message: format!("Failed to unpin message: {}", e)  
                                        });
                                    } 
                                }
                            }
                        }
                        WsClientMessage::GetPinnedMessages { room_id } => {
                            if let Some(_user_id) = &authenticated_user_id {
                                let msg_service = message_service.lock().await;

                                match msg_service.get_pinned_messages(&room_id) {
                                    Ok(messages) => {
                                        let _ = tx.send(WsServerMessage::PinnedMessages { 
                                            room_id: room_id.clone(), 
                                            messages, 
                                        });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(WsServerMessage::Error { 
                                            message: format!("Failed to get pinned messages: {}", e) 
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(user_id) = authenticated_user_id.as_ref() {
        let msg_service = message_service.lock().await;

        if let Err(e) = msg_service.update_user_presence(user_id, Presence::Offline) {
            eprintln!("Failed to update presence on disconnect: {}", e);
        }

        if let Ok(rooms) = msg_service.get_user_rooms(user_id) {
            let username = authenticated_username.clone().unwrap_or_default();
            drop(msg_service);
            let conns = connections.read().await;

            for room in rooms {
                conns.broadcast_to_room(&room.id, WsServerMessage::PresenceChanged { 
                    user_id: user_id.clone(), 
                    username: username.clone(), 
                    presence: Presence::Offline 
                });
            }
        }
    }

    if let Some(user_id) = authenticated_user_id {
        connections.write().await.remove_client(&user_id);
    }

    send_task.abort();
    Ok(())
}

