use spark_core::TcpServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = TcpServer::new("spark.db", "127.0.0.1:8080".to_string())?;
    println!("Starting TCP server...");
    server.start().await?;
    Ok(())
}