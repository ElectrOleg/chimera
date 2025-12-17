use async_trait::async_trait;
use bytes::Bytes;
use anyhow::Result;
use std::net::SocketAddr;

/// The core trait that all transport mechanisms must implement.
/// This allows the protocol to switch between TCP, UDP, Websockets, etc.
/// without changing the upper layers.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Connect to a remote endpoint.
    async fn connect(&self, addr: SocketAddr) -> Result<Box<dyn Connection>>;

    /// Listen for incoming connections.
    async fn listen(&self, addr: SocketAddr) -> Result<Box<dyn Listener>>;

    /// valid traffic mimicry type (e.g. "TLS", "HTTP", "Random")
    fn name(&self) -> &str;
}

#[async_trait]
pub trait Connection: Send + Sync {
    async fn send(&mut self, data: Bytes) -> Result<()>;
    async fn recv(&mut self) -> Result<Option<Bytes>>;
    async fn close(&mut self) -> Result<()>;
}

#[async_trait]
pub trait Listener: Send + Sync {
    async fn accept(&mut self) -> Result<(Box<dyn Connection>, SocketAddr)>;
}

pub mod tcp;
pub mod blocked;
