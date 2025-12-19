use chimera_transport::{Transport, Listener, Connection};
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, error};

use chimera_ai::Router;

/// The main Chimera node.
/// It can listen on multiple transports simultaneously.
pub struct ChimeraNode {
    transports: Vec<Box<dyn Transport>>,
    router: Arc<Router>,
}

impl ChimeraNode {
    pub fn new() -> Self {
        Self {
            transports: Vec::new(),
            router: Arc::new(Router::new()),
        }
    }

    pub fn add_transport(&mut self, transport: Box<dyn Transport>) {
        info!("Adding transport: {}", transport.name());
        self.router.register_path(transport.name());
        self.transports.push(transport);
    }

    pub async fn run_server(&self, bind_addr: SocketAddr) -> Result<()> {
        info!("Starting Chimera Server on {}", bind_addr);
        
        let (tx, mut rx) = mpsc::channel::<Box<dyn Connection>>(100);

        // Start listeners for each transport
        for transport in &self.transports {
            let transport_name = transport.name().to_string();
            let mut listener = transport.listen(bind_addr).await?;
            let tx = tx.clone();
            
            info!("Transport {} listening on {}", transport_name, bind_addr);

            tokio::spawn(async move {
                loop {
                    match listener.accept().await {
                        Ok((connection, remote_addr)) => {
                            info!("[{}] New connection from {}", transport_name, remote_addr);
                            if let Err(e) = tx.send(connection).await {
                                error!("Failed to send connection to main loop: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("[{}] Accept error: {}", transport_name, e);
                        }
                    }
                }
            });
        }

        // Main connection handler loop
        while let Some(raw_connection) = rx.recv().await {
            let router = self.router.clone();
            tokio::spawn(async move {
                // Heuristic Check: Log the best path
                if let Some(best) = router.get_best_path() {
                     info!("AI Logic: Best path for new connection is {}", best);
                }

                // Use HttpMimic for now
                let mimic = Some(Box::new(mimic::HttpMimic) as Box<dyn mimic::Mimic>);
                
                // Add 5 second timeout for handshake
                let handshake_future = handshake::EncryptedConnection::new(raw_connection, true, mimic);
                match tokio::time::timeout(std::time::Duration::from_secs(5), handshake_future).await {
                    Ok(result) => match result {
                        Ok(mut conn) => {
                        info!("Handshake successful. Connection secured.");
                        if let Err(e) = handle_connection(&mut conn).await {
                            error!("Connection error: {}", e);
                        }
                    }
                        Err(e) => {
                            error!("Handshake failed: {}", e);
                        }
                    },
                    Err(_) => {
                        error!("Handshake timed out");
                        // raw_connection drops here, closing socket
                    }
                }
            });
        }

        Ok(())
    }
}

pub mod handshake;
pub mod mimic;
pub mod handshake;
pub mod mimic;
pub mod protocol;
pub mod socks;
pub mod server_proxy;
pub mod client_proxy;

use crate::server_proxy::ServerProxy;
use crate::protocol::Frame;
use tokio::sync::mpsc;
use bytes::BytesMut;
use tokio::io::AsyncReadExt;

async fn handle_connection(conn: &mut dyn Connection) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<Frame>(100);
    let proxy = Arc::new(ServerProxy::new(tx));
    
    let mut buf = BytesMut::with_capacity(4096);
    
    loop {
        tokio::select! {
             // 1. Read from Tunnel
            res = conn.recv() => {
                match res? {
                    Some(data) => {
                        buf.extend_from_slice(&data);
                        // Parse Frames
                        loop {
                            let mut cursor = std::io::Cursor::new(&buf[..]);
                            match Frame::check(&mut cursor)? {
                                Some(len) => {
                                    let mut frame_bytes = buf.split_to(len).freeze();
                                    let mut frame_bytes_cursor = frame_bytes.clone(); // convert to bytes for parsing
                                    let frame = Frame::parse(&mut frame_bytes)?;
                                    proxy.handle_frame(frame).await?;
                                }
                                None => break, // Need more data
                            }
                        }
                    }
                    None => break, // EOF
                }
            }
            
             // 2. Write to Tunnel (from Proxy)
            Some(frame) = rx.recv() => {
                let bytes = frame.to_bytes();
                conn.send(bytes).await?;
            }
        }
    }
    
    Ok(())
}
