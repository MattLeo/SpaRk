use crate::network::{AuthService, MessageService};
use crate::messages::{RoomMessageResponse, SendRoomMessageRequest};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum WsClientMessage {
    Authenticate { token: String },
    CreateRoom {name: String, desc: String},
    GetAllRooms,
    JoinRoom { room_id: String },
    LeaveRoom { room_id: String },
    SendMessage { room_id: String, content: String },
    GetRoomHistory { room_id: String, limit: Option<usize>, offset: Option<usize> },
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
}

#[derive(Debug, Serialize, Clone)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    pub desc: String,
}

struct Client {
    user_id: String,
    username: String,
    sender: mpsc::UnboundedSender<WsServerMessage>,
    rooms: HashSet<String>,
}

pub struct ConnectionManager {
    clients: HashMap<String, Client>,
    rooms: HashMap<String, HashSet<String>>,
}

impl ConnectionManager {
    fn new() -> Self {
        Self {
            clients: HashMap::new(),
            rooms: HashMap::new(),
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
        if let Some(client) = self.clients.remove(user_id) {
            for room_id in &client.rooms {
                if let Some(room) = self.rooms.get_mut(room_id) {
                    room.remove(user_id);
                }
            }
        }
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
                                user_id: user.id, 
                                username: user.username 
                            });
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
                        WsClientMessage::SendMessage { room_id, content } => {
                            let msg_service = message_service.lock().await;
                            let request = SendRoomMessageRequest {
                                room_id: room_id.clone(),
                                content,
                            };

                            match msg_service.send_room_message(user_id, request) {
                                Ok(message) => {
                                    let _ = tx.send(WsServerMessage::MessageSent { message_id: message.id.clone() });

                                    connections.read().await.broadcast_to_room(
                                        &room_id,
                                        WsServerMessage::NewMessage { 
                                            room_id: room_id.clone(), 
                                            message 
                                        } 
                                    );
                                }
                                Err(e) => {
                                    let _ = tx.send(WsServerMessage::Error { 
                                        message: format!("Failed to send message: {}", e) 
                                    });
                                }
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
                    }
                }
            }
        }
    }

    if let Some(user_id) = authenticated_user_id {
        connections.write().await.remove_client(&user_id);
    }

    send_task.abort();
    Ok(())
}

