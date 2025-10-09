use crate::{network::AuthService, Database};
use serde::{Deserialize, Serialize};
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