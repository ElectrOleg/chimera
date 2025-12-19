use chimera_core::handshake::EncryptedConnection;
use chimera_transport::tcp::TcpTransport;
use chimera_transport::blocked::BlockedTransport;
use chimera_transport::Transport;
use chimera_ai::Router;
use chimera_core::client_proxy::ClientProxy;
use chimera_core::socks::Socks5Listener;
use chimera_core::protocol::Frame;
use anyhow::Result;
use bytes::BytesMut;
use tracing::{Level, info, error, warn};
use tracing_subscriber::FmtSubscriber;
use tokio::sync::mpsc;
use std::sync::Arc;

use chimera_core::system::MacProxyManager;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Setup Logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let addr_str = format!("{}:8080", host);
    
    // Resolve Server Address
    use std::net::ToSocketAddrs;
    let addr = addr_str.to_socket_addrs()?.next().ok_or(anyhow::anyhow!("Could not resolve hostname"))?;
    info!("Target Server: {}", addr);

    // 2. Setup AI Router
    let router = Arc::new(Router::new());
    router.register_path("BlockedProtocol");
    router.update_latency("BlockedProtocol", std::time::Duration::from_millis(10));
    router.register_path("TCP");
    router.update_latency("TCP", std::time::Duration::from_millis(100));

    // 3. Initialize Persistent Components (Proxy, SOCKS, System Config)
    let (tunnel_tx, mut tunnel_rx) = mpsc::channel::<Frame>(1000);
    let proxy = Arc::new(ClientProxy::new(tunnel_tx));

    let socks_addr = "127.0.0.1:1080".parse()?;
    let listener = Socks5Listener::bind(socks_addr).await?;
    let proxy_clone = proxy.clone();
    
    // Start SOCKS5 Listener in background (persists across reconnections)
    tokio::spawn(async move {
        loop {
            if let Ok((socket, target, port)) = listener.accept().await {
                 proxy_clone.start_new_stream(socket, target, port).await;
            }
        }
    });

    info!("Chimera Client Running. Proxy at 127.0.0.1:1080");

    // Enable Mac System Proxy
    let sys_proxy = MacProxyManager::new();
    if let Err(e) = sys_proxy.enable("127.0.0.1", 1080) {
        error!("Failed to enable System Proxy: {}", e);
    }
    
    // 4. Main Reconnection Loop
    // If the tunnel drops, we loop back here and reconnect.
    loop {
        info!("Connecting to tunnel...");
        let mut attempt = 0;
        let mut secure_conn = loop {
            attempt += 1;
            
            // AI Path Selection logic
            let best_path_name = router.get_best_path().unwrap_or("TCP".to_string());
            if attempt > 1 {
                 warn!("Attempt {}: connecting via '{}'...", attempt, best_path_name);
            }

            let transport: Box<dyn Transport> = match best_path_name.as_str() {
                "BlockedProtocol" => Box::new(BlockedTransport),
                "TCP" => Box::new(TcpTransport),
                _ => Box::new(TcpTransport),
            };

            match transport.connect(addr).await {
                Ok(raw_conn) => {
                    let mimic = Some(Box::new(chimera_core::mimic::HttpMimic) as Box<dyn chimera_core::mimic::Mimic>);
                    match EncryptedConnection::new(raw_conn, false, mimic).await {
                        Ok(conn) => {
                            info!("Tunnel established via {}!", best_path_name);
                            router.update_latency(&best_path_name, std::time::Duration::from_millis(50));
                            break conn;
                        }
                        Err(e) => {
                            warn!("Handshake failed: {}", e);
                            router.report_failure(&best_path_name);
                        }
                    }
                }
                Err(e) => {
                     warn!("Transport connect failed: {}", e);
                     router.report_failure(&best_path_name);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        };

        // 5. Data Transfer Loop (The "Active" State)
        let mut buf = BytesMut::with_capacity(8192);
        
        // We use a nested select loop. If `secure_conn` fails, we break this inner loop, 
        // which returns us to the outer `loop` (Reconnection).
        // If Ctrl+C happens, we return from Main entirely.
        
        let disconnect_reason = loop {
             tokio::select! {
                // A. Handle Cleanup Signal and EXIT APP
                _ = tokio::signal::ctrl_c() => {
                    info!("Shutdown signal received.");
                    sys_proxy.disable();
                    return Ok(());
                }
                
                // B. Read from Tunnel -> Forward to Proxy 
                res = secure_conn.recv() => {
                    match res {
                        Ok(Some(data)) => {
                            buf.extend_from_slice(&data);
                            while let Ok(Some(len)) = Frame::check(&mut std::io::Cursor::new(&buf[..])) {
                                let frame_bytes = buf.split_to(len).freeze();
                                if let Ok(frame) = Frame::parse(&mut bytes::Bytes::from(frame_bytes)) {
                                    let _ = proxy.handle_frame(frame).await;
                                }
                            }
                        }
                        Ok(None) => break "Tunnel Closed (EOF)",
                        Err(e) => break "Tunnel Error (Read)", 
                    }
                }

                // C. Read from Proxy -> Forward to Tunnel
                Some(frame) = tunnel_rx.recv() => {
                    let bytes = frame.to_bytes();
                    if let Err(_) = secure_conn.send(&bytes).await {
                         break "Tunnel Error (Write)";
                    }
                }
            }
        };
        
        warn!("Disconnected: {}. Reconnecting in 1s...", disconnect_reason);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
