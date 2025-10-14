use spark_core::{Database, TcpServer, WebSocketServer};
use spark_core::network::{AuthService, MessageService};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::new("spark.db")?;
    let auth_service = Arc::new(Mutex::new(AuthService::new(db.clone())));
    let message_service = Arc::new(Mutex(new(MessageService::new(db))));
    let tcp_server = TcpServer::new("spark.db", "127.0.0.1:8080".to_string())?;
    let ws_server = WebSocketServer::new(
        Arc::clone(&auth_service), 
        Arc::clone(&message_service), 
        "127.0.0.1:8081"
    );

    println!("Starting SpaRk Server..");
    println!("TCP Server (Auth): 127.0.0.1:8080");
    println!("WebSocket Server (Chat): 127.0.0.1:8081");

    tokio::select! {
        result = tcp_server.start() => {
            eprintln!("TCP server stopped: {:?}", result);
        }
        result = ws_server.start() => {
            eprintln!("WebSocket server stopped {:?}", result);
        }
    }
    Ok(())
}