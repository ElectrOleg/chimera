use chimera_core::handshake::EncryptedConnection;
use chimera_transport::tcp::TcpTransport;
use chimera_transport::blocked::BlockedTransport;
use chimera_transport::Transport;
use chimera_ai::Router;
use anyhow::Result;
use bytes::Bytes;
use tracing::{Level, info, error, warn};
use tracing_subscriber::FmtSubscriber;
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let addr_str = format!("{}:8080", host);
    let addr: SocketAddr = addr_str.parse()
         .or_else(|_| format!("{}:8080", "127.0.0.1").parse())?; // fallback if lookup fails (e.g. docker DNS)
    
    // Note: To resolve "server" hostname in Docker, we strictly probably need getaddrinfo 
    // or std::net::ToSocketAddrs. 
    // SocketAddr::parse() only accepts "IP:PORT".
    // Let's implement a quick resolver helper or just assume IP for local, 
    // but proper Docker support needs hostname resolution.
    // Simplifying: we will use `to_socket_addrs` which does DNS.
    use std::net::ToSocketAddrs;
    let addr = addr_str.to_socket_addrs()?.next().ok_or(anyhow::anyhow!("Could not resolve hostname"))?;

    info!("Connecting to server at {}", addr);
    
    // Initialize Router and Register Transports
    let router = Arc::new(Router::new());
    
    // In a real app, these would be separate modules. 
    // Here we register them manually.
    // 1. "BlockedProtocol" - Simulates a fast but blocked path
    router.register_path("BlockedProtocol");
    router.update_latency("BlockedProtocol", std::time::Duration::from_millis(10)); // AI thinks this is FAST

    // 2. "TCP" - Simulates a working fallback
    router.register_path("TCP");
    router.update_latency("TCP", std::time::Duration::from_millis(100)); // Standard latency

    // Attempt connection loop
    let mut attempt = 0;
    loop {
        attempt += 1;
        if attempt > 3 {
             error!("All connection attempts failed.");
             break;
        }

        // AI Step 1: Ask for best path
        let best_path_name = router.get_best_path().unwrap_or("TCP".to_string());
        info!("Attempt {}: AI chose path '{}'", attempt, best_path_name);
        
        let transport: Box<dyn Transport> = match best_path_name.as_str() {
            "BlockedProtocol" => Box::new(BlockedTransport),
            "TCP" => Box::new(TcpTransport),
            _ => Box::new(TcpTransport),
        };

        match transport.connect(addr).await {
            Ok(raw_conn) => {
                info!("Transport connected. Starting handshake...");
                
                // Use HttpMimic
                let mimic = Some(Box::new(chimera_core::mimic::HttpMimic) as Box<dyn chimera_core::mimic::Mimic>);
                match EncryptedConnection::new(raw_conn, false, mimic).await {
                    Ok(mut secure_conn) => {
                        println!("Client handshake success via {}!", best_path_name);
                        
                        // Send data
                        let msg = "Hello Secure World";
                        if let Err(e) = secure_conn.send(msg.as_bytes()).await {
                            error!("Send failed: {}", e);
                            router.report_failure(&best_path_name);
                            continue;
                        }
                        println!("Sent: {}", msg);

                        if let Some(response) = secure_conn.recv().await? {
                            println!("Received: {}", String::from_utf8_lossy(&response));
                            // AI Step 2: Update success stats (simplified latency)
                            router.update_latency(&best_path_name, std::time::Duration::from_millis(50));
                            break; // Success
                        }
                    }
                    Err(e) => {
                        error!("Handshake failed on {}: {}", best_path_name, e);
                         router.report_failure(&best_path_name);
                    }
                }
            }
            Err(e) => {
                warn!("Transport '{}' failed: {}", best_path_name, e);
                // AI Step 3: Report failure to punish this path
                router.report_failure(&best_path_name);
                // Loop will try again, and Router should pick the NEXT best path
            }
        }
        
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    Ok(())
}
