use crate::users::AuthResponse;
use crate::{network::AuthService, Database};
use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::fmt::format;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
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

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
enum Response {
    Success { data: serde_json::Value },
    Error { message: String },
}

pub struct TcpServer {
    auth: Arc<Mutex<AuthService>>,
    addr: String,
}

impl TcpServer {
    pub fn new(db_path: &str, addr: String) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Database::new(db_path)?;
        let auth = AuthService::new(db);

        Ok(Self {
            auth: Arc::new(Mutex::new(auth)),
            addr,
        })
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(&self.addr).await?;
        println!("Server listening on {}", self.addr);

        loop {
            let (socket, addr) = listener.accept().await?;
            println!("New connection from: {}", addr);

            let auth = Arc::clone(&self.auth);
            tokio::spawn(async move {
                if let Err(e) = handle_client(socket, auth).await {
                    eprintln!("Error handling clinet {}: {}", addr, e);
                }
            });
        }
    }
}

async fn handle_client(
    mut socket: TcpStream, auth: Arc<Mutex<AuthService>>
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![0u8; 4096];

    loop {
        let n = socket.read(&mut buffer).await?;

        if n == 0 { return Ok(()); }

        let request_str = String::from_utf8_lossy(&buffer[..n]);
        let response = match serde_json::from_str::<Request>(&request_str) {
            Ok(request) => process_request(request, &auth).await,
            Err(e) => Response::Error { message: format!("Invalid request format: {}", e) }
        };

        let response_json = serde_json::to_string(&response)?;
        socket.write_all(response_json.as_bytes()).await?;
        socket.write_all(b"\n").await?;
    }
}

async fn process_request(request: Request, auth: &Arc<Mutex<AuthService>>) -> Response {
    let auth = auth.lock().await;

    match request {
        Request::Register { username, email, password } => {
            let req = crate::users::CreateUserRequest {
                username,
                email,
                password
            };

            match auth.register(req) {
                Ok(auth_response) => {
                    match serde_json::to_value(auth_response) {
                        Ok(data) => Response::Success { data },
                        Err(e) => Response::Error { message: format!("Serialization error: {}", e) }
                    }
                }
                Err(e) => Response::Error { message: e.to_string() }
            }
        }
        Request::Login { username, password } => {
            let req = crate::users::LoginRequest {
                username,
                password
            };

            match auth.login(req) {
                Ok(user) => {
                    match serde_json::to_value(user) {
                        Ok(data) => Response::Success { data },
                        Err(e) => Response::Error { message: format!("Serialization error: {}", e) }
                    }
                }
                Err(e) => Response::Error { message: e.to_string() }
            }
        }
        Request::Logout { token } => {
            match auth.logout(&token) {
                Ok(_) => Response::Success { data: serde_json::json!({"message": "Logged out successfully"}) },
                Err(e) => Response::Error { message: e.to_string() }
            }
        }
        Request::ValidateSession { token } => {
            match auth.validate_session(&token) {
                Ok( user) => {
                    match::serde_json::to_value(user) {
                        Ok(data) => Response::Success { data },
                        Err(e) => Response::Error { message: format!("Serialization error: {}", e) }
                    }
                }
                Err(e) => Response::Error { message: e.to_string() }
            }
        }
    }
}