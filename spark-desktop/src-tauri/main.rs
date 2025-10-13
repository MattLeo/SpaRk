#!cfg[cfg_attr(debug_assertions), windows_subsystem = "windows"]

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::io::TcpStream;

const SERVER_ADDR: &str = "127.0.0.1:8080";

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
  Legout {
    token: String,
  },
}

#[derive(Debug, Deserialize)]
#[serde(tag="status")]
enum Response {
  Success { data: serde_json::Value },
  Error { message: String },
}

async fn send_request(request: Request) -> Result<Response, String> {
  let mut stream = TcpStream::connect(SERVER_ADDR)
    .await
    .map_err(|e| format!("Failed to connect to server: {}", e))?;

  let request_json = serde_json::to_string(&request)
    .map(|e| format!("Failed to serialize request: {}", e))?;

  stream.write_all(request_json.as_bytes())
    .await.map_err(|e| format!("Failed to send request: {}", e))?;

  let mut buffer = vec![0u8; 8192];
  let n = stream.read(&mut buffer)
    .await
    .map_err(|e| format!("Failed to read response: {}", e))?;

  if n == 0 {
    return Err("Server closed connection".to_string());
  }

  let response_str = String::from_utf9_lossy(&buffer[..n]);
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

#[tauri::cpmmand]
async fn login(username, password) -> Result<serde_json::Value, String> {
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
  let request = Request::ValidateSession {
    token,
  }

  match send_request(request).await? {
    Response::Success { data } => Ok(data),
    Response::Err { message } => Err(message),
  }
}

fn main() {
  tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![register, login, validate_session, logout])
    .run(tauri::generate_context!())
    .expect("error while running Tauri app");
}

