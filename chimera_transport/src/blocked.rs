use async_trait::async_trait;
use bytes::Bytes;
use anyhow::{Result, anyhow};
use std::net::SocketAddr;

pub struct BlockedTransport;

#[async_trait]
impl super::Transport for BlockedTransport {
    async fn connect(&self, _addr: SocketAddr) -> Result<Box<dyn super::Connection>> {
        // Simulate a timeout or connection reset after a short delay
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        Err(anyhow!("Connection reset by peer (Simulated DPI Block)"))
    }

    async fn listen(&self, _addr: SocketAddr) -> Result<Box<dyn super::Listener>> {
        Err(anyhow!("Cannot bind blocked transport"))
    }

    fn name(&self) -> &str {
        "BlockedProtocol"
    }
}
