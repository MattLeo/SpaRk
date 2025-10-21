#!cfg[cfg_attr(debug_assertions), windows_subsystem = "windows"]

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use std::sync::Arc;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tauri::{Manager, State, Emitter};

const SERVER_ADDR: &str = "127.0.0.1:8080";
const WS_SERVER_ADDR: &str = "ws://127.0.0.1:8081";

#[derive(Debug, Serialize)]
#[serde(tag="type")]
enum Request {
  Register {
    username: String,
    email: String,
    password: String,
  },
  Login {
    username: String,
    password: String,
  },
  ValidateSession {
    token: String,
  },
  Logout {
    token: String,
  },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "status")]
enum Response {
  Success { data: serde_json::Value },
  Error { message: String },
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type")]
enum WsClientMessage {
  Authenticate { token: String },
  CreateRoom { name: String, desc: String },
  GetAllRooms,
  JoinRoom { room_id: String },
  LeaveRoom { room_id: String },
  SendMessage { room_id: String, content: String },
  GetRoomHistory { room_id: String, limit: Option<usize>, offset: Option<usize> },
  EditMessage {room_id: String, message_id: String, new_content: String },
  DeleteMessage { room_id: String, message_id: String },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
enum WsServerMessage {
  Authenticated { user_id: String, username: String },
  Error { message: String },
  RoomCreated { room_id: String, room_name: String },
  RoomList { rooms: Vec<serde_json::Value> },
  RoomJoined { room_id: String, room_name: String },
  RoomLeft { room_id: String },
  NewMessage { message: serde_json::Value },
  MessageSent { message_id: String },
  RoomHistory { room_id: String, messages: Vec<serde_json::Value> },
  UserJoined { room_id: String, user_id: String, username: String },
  UserLeft { room_id: String, user_id: String, username: String },
  MessageEdited { room_id: String, message_id: String, new_content: String, edited_at: String },
  MessageDeleted { room_id: String, message_id: String },
}

type WsSender = 
  Arc<Mutex<Option<futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>, Message>>>>;

struct AppState {
  ws_sender: WsSender,
}

async fn send_request(request: Request) -> Result<Response, String> {
  let mut stream = TcpStream::connect(SERVER_ADDR)
    .await
    .map_err(|e| format!("Failed to connect to server: {}", e))?;

  let request_json = serde_json::to_string(&request)
    .map_err(|e| format!("Failed to serialize request: {}", e))?;

  stream.write_all(request_json.as_bytes())
    .await.map_err(|e| format!("Failed to send request: {}", e))?;

  let mut buffer = vec![0u8; 8192];
  let n = stream.read(&mut buffer)
    .await
    .map_err(|e| format!("Failed to read response: {}", e))?;

  if n == 0 {
    return Err("Server closed connection".to_string());
  }

  let response_str = String::from_utf8_lossy(&buffer[..n]);
  let response: Response = serde_json::from_str(&response_str)
    .map_err(|e| format!("Failed to parse response: {}", e))?;

  Ok(response)
}

#[tauri::command]
async fn register(username: String, email: String, password: String) -> Result<serde_json::Value, String> {
  let request = Request::Register {
    username,
    email,
    password,
  };

  match send_request(request).await? {
    Response::Success { data } => Ok(data),
    Response::Error { message } => Err(message),
  }
}

#[tauri::command]
async fn login(username: String, password: String) -> Result<serde_json::Value, String> {
  let request = Request::Login {
    username,
    password,
  };

  match send_request(request).await? {
    Response::Success { data } => Ok(data),
    Response::Error { message } => Err(message),
  }
}

#[tauri::command]
async fn validate_session(token: String) -> Result<serde_json::Value, String> {
  let request = Request::ValidateSession { token };

  match send_request(request).await? {
    Response::Success { data } => Ok(data),
    Response::Error { message } => Err(message),
  }
}

#[tauri::command]
async fn logout(token: String) -> Result<serde_json::Value, String> {
  let request = Request::Logout { token };

  match send_request(request).await? {
    Response::Success { data } => Ok(data),
    Response::Error { message } => Err(message),
  }
}

#[tauri::command]
async fn connect_websocket(
  token: String,
  state: State<'_, AppState>,
  app_handle: tauri::AppHandle
) -> Result<(), String> {
  let (ws_stream, _) = connect_async(WS_SERVER_ADDR)
    .await
    .map_err(|e| format!("Failed to connect to WebSocket: {}", e))?;

  let (mut write, mut read) = ws_stream.split();

  let auth_msg = WsClientMessage::Authenticate{ token };
  let auth_json = serde_json::to_string(&auth_msg)
    .map_err(|e| format!("Failed to serialize auth request: {}", e))?;

  write.send(Message::Text(auth_json.into()))
    .await
    .map_err(|e| format!("Failed to send auth request: {}", e))?;

  *state.ws_sender.lock().await = Some(write);

  tokio::spawn(async move {
    while let Some(msg) = read.next().await {
      match msg {
        Ok(Message::Text(text)) => {
          if let Ok(server_msg) = serde_json::from_str::<WsServerMessage>(&text) {
            app_handle.emit("ws-message", server_msg).ok();
          }
        }
        Ok(Message::Close(_)) => {
          app_handle.emit("ws_closed", ()).ok();
          break;
        }
        Err(e) => {
          eprintln!("WebSocket error: {}", e);
          app_handle.emit("ws_error", format!("{}", e)).ok();
          break;
        }
        _ => {}
      }
    }
  });

  Ok(())
}

#[tauri::command]
async fn ws_get_all_rooms(state: State<'_, AppState>) -> Result<(), String> {
  let msg = WsClientMessage::GetAllRooms;
  let json = serde_json::to_string(&msg)
    .map_err(|e| format!("Failed to serialize room list request: {}", e))?;

  if let Some(sender) = state.ws_sender.lock().await.as_mut() {
    sender.send(Message::Text(json.into()))
      .await
      .map_err(|e| format!("Failed to send room list request: {}", e))?;
    Ok(())
  } else {
    Err("WebSocket not connected".to_string())
  }
}

#[tauri::command]
async fn ws_create_room(name: String, desc: String, state: State<'_, AppState>) -> Result<(), String> {
  let msg = WsClientMessage::CreateRoom { name, desc };
  let json = serde_json::to_string(&msg)
    .map_err(|e| format!("Failed to serialize room creation request: {}", e))?;

  if let Some(sender) = state.ws_sender.lock().await.as_mut() {
    sender.send(Message::Text(json.into()))
      .await
      .map_err(|e| format!("Failed to send room creation request: {}", e))?;
    Ok(())
  } else {
    Err("WebSocket not connected".to_string())
  }
}

#[tauri::command]
async fn ws_join_room(room_id: String, state: State<'_, AppState>) -> Result<(), String> {
  let msg = WsClientMessage::JoinRoom { room_id };
  let json = serde_json::to_string(&msg)
    .map_err(|e| format!("Failed to serialize message: {}", e))?;

  if let Some(sender) = state.ws_sender.lock().await.as_mut() {
    sender.send(Message::Text(json.into()))
      .await
      .map_err(|e| format!("Failed to send message: {}", e))?;
    Ok(())
  } else {
    Err("WebSocket not connected".to_string())
  }
}

#[tauri::command]
async fn ws_leave_room(room_id: String, state: State<'_, AppState>) -> Result<(), String> {
  let msg = WsClientMessage::LeaveRoom { room_id };
  let json = serde_json::to_string(&msg)
    .map_err(|e| format!("Failed to serialize message: {}", e))?;

  if let Some(sender) = state.ws_sender.lock().await.as_mut() {
    sender.send(Message::Text(json.into()))
      .await
      .map_err(|e| format!("Failed to send message: {}", e))?;
    Ok(())
  } else {
    Err("WebSocket not connected".to_string())
  } 
}

#[tauri::command]
async fn ws_send_message(room_id: String, content: String, state: State<'_, AppState>) -> Result<(), String> {
  let msg = WsClientMessage::SendMessage { room_id, content };
  let json = serde_json::to_string(&msg)
    .map_err(|e| format!("Failed to serialize message: {}", e))?;

  if let Some(sender) = state.ws_sender.lock().await.as_mut() {
    sender.send(Message::Text(json.into()))
      .await
      .map_err(|e| format!("Failed to send message: {}", e))?;
    Ok(())
  } else {
    Err("WebSocket not connected".to_string())
  }
}

#[tauri::command]
async fn ws_get_room_history(
  room_id: String, 
  limit: Option<usize>, 
  offset: Option<usize>, 
  state: State<'_, AppState>
) -> Result<(), String> {
  let msg = WsClientMessage::GetRoomHistory { room_id, limit, offset };
  let json = serde_json::to_string(&msg)
    .map_err(|e| format!("Failed to serialize message: {}", e))?;

  if let Some(sender) = state.ws_sender.lock().await.as_mut() {
    sender.send(Message::Text(json.into()))
      .await
      .map_err(|e| format!("Failed to send message: {}", e))?;
    Ok(())
  } else {
    Err("WebSocket not connected".to_string())
  }
}

#[tauri::command]
async fn ws_edit_message(
  room_id: String,
  message_id: String,
  new_content: String,
  state: State<'_, AppState>,
) -> Result<(), String> {
  let msg = WsClientMessage::EditMessage { room_id, message_id, new_content };
  let json = serde_json::to_string(&msg).map_err(|e| format!("Failed to serialize message: {}", e))?;

  if let Some(sender) = state.ws_sender.lock().await.as_mut() {
    sender.send(Message::Text(json.into()))
    .await.map_err(|e| format!("Failed to edit message: {}", e))?;
  Ok(())
  } else {
    Err("WebSocket not connected".to_string())
  }
}

#[tauri::command]
async fn ws_delete_message(
  room_id: String,
  message_id: String,
  state: State<'_, AppState>,
) -> Result<(), String> {
  let msg = WsClientMessage::DeleteMessage { room_id, message_id };
  let json = serde_json::to_string(&msg).map_err(|e| format!("Failed to serialize message: {}", e))?;

  if let Some(sender) = state.ws_sender.lock().await.as_mut() {
    sender.send(Message::Text(json.into()))
    .await.map_err(|e| format!("Failed to delete message: {}", e))?;
  Ok(())
  } else {
    Err("WebSocket not connected".to_string())
  }
}

fn main() {
  let app_state = AppState {
    ws_sender: Arc::new(Mutex::new(None)),
  };

  tauri::Builder::default()
    .manage(app_state)
    .invoke_handler(tauri::generate_handler![
      register, 
      login, 
      validate_session, 
      logout,
      connect_websocket,
      ws_join_room,
      ws_leave_room,
      ws_send_message,
      ws_get_room_history,
      ws_create_room,
      ws_get_all_rooms,
      ws_edit_message,
      ws_delete_message,
    ])
    .run(tauri::generate_context!())
    .expect("error while running Tauri app");
}

