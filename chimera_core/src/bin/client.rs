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
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::io::AsyncReadExt;

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

    // 2. Setup AI Router (kept for transport selection)
    let router = Arc::new(Router::new());
    router.register_path("BlockedProtocol");
    router.update_latency("BlockedProtocol", std::time::Duration::from_millis(10));
    router.register_path("TCP");
    router.update_latency("TCP", std::time::Duration::from_millis(100));

    // 3. Connect to Server (The Tunnel)
    let mut attempt = 0;
    let mut secure_conn = loop {
        attempt += 1;
        if attempt > 3 {
            error!("All connection attempts failed.");
            return Ok(());
        }

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
                let mimic = Some(Box::new(chimera_core::mimic::HttpMimic) as Box<dyn chimera_core::mimic::Mimic>);
                match EncryptedConnection::new(raw_conn, false, mimic).await {
                    Ok(conn) => {
                        info!("Tunnel established via {}!", best_path_name);
                        router.update_latency(&best_path_name, std::time::Duration::from_millis(50));
                        break conn;
                    }
                    Err(e) => {
                        error!("Handshake failed: {}", e);
                        router.report_failure(&best_path_name);
                    }
                }
            }
            Err(e) => {
                warn!("Transport failed: {}", e);
                router.report_failure(&best_path_name);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    };

    // 4. Start Client Proxy (Multiplexer)
    let (tunnel_tx, mut tunnel_rx) = mpsc::channel::<Frame>(1000);
    let proxy = Arc::new(ClientProxy::new(tunnel_tx));

    // 5. Start SOCKS5 Listener
    let socks_addr = "127.0.0.1:1080".parse()?;
    let listener = Socks5Listener::bind(socks_addr).await?;
    let proxy_clone = proxy.clone();
    
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((socket, target, port)) => {
                    proxy_clone.start_new_stream(socket, target, port).await;
                }
                Err(e) => {
                    warn!("SOCKS accept error: {}", e);
                }
            }
        }
    });

    info!("Chimera Client Running. Configure SOCKS5 proxy to 127.0.0.1:1080");

    // 6. Main Tunnel Loop (Multiplexing)
    let mut buf = BytesMut::with_capacity(8192);

    loop {
        tokio::select! {
            // Read from Tunnel -> Handle Frame
            res = secure_conn.recv() => {
                match res? {
                    Some(data) => {
                        buf.extend_from_slice(&data);
                         loop {
                            let mut cursor = std::io::Cursor::new(&buf[..]);
                            match Frame::check(&mut cursor)? {
                                Some(len) => {
                                    let mut frame_bytes = buf.split_to(len).freeze();
                                    let frame_bytes_clone = frame_bytes.clone();
                                    // Parse
                                    let mut frame_reader = std::io::Cursor::new(&frame_bytes_clone);
                                    if let Ok(frame) = Frame::parse(&mut bytes::Bytes::from(frame_bytes)) {
                                         proxy.handle_frame(frame).await?;
                                    }
                                }
                                None => break, 
                            }
                        }
                    }
                    None => {
                        error!("Tunnel closed by server.");
                        break;
                    }
                }
            }

            // Read from Proxy -> Write to Tunnel
            Some(frame) = tunnel_rx.recv() => {
                let bytes = frame.to_bytes();
                secure_conn.send(bytes).await?;
            }
        }
    }

    Ok(())
}
