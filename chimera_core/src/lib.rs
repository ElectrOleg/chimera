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

async fn handle_connection(conn: &mut dyn Connection) -> Result<()> {
    // Simple echo for now
    while let Some(data) = conn.recv().await? {
        // info!("Received {} bytes", data.len());
        conn.send(data).await?;
    }
    Ok(())
}
