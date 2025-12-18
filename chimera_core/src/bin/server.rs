use chimera_core::ChimeraNode;
use chimera_transport::tcp::TcpTransport;
use anyhow::Result;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<()> {
    // Setup logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    let mut node = ChimeraNode::new();
    
    // Add transports
    node.add_transport(Box::new(TcpTransport));

    // Bind address
    let bind_addr = std::env::var("SERVER_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let addr: SocketAddr = bind_addr.parse()?;

    // Create a shutdown signal
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C signal handler");
        tracing::info!("Shutdown signal received, stopping server...");
    };

    tokio::select! {
        res = node.run_server(addr) => {
            if let Err(e) = res {
                tracing::error!("Server error: {}", e);
            }
        }
        _ = shutdown_signal => {}
    }

    Ok(())
}
