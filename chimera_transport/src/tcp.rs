use async_trait::async_trait;
use bytes::Bytes;
use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::{TcpStream, TcpListener};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct TcpTransport;

#[async_trait]
impl super::Transport for TcpTransport {
    async fn connect(&self, addr: SocketAddr) -> Result<Box<dyn super::Connection>> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Box::new(TcpConnection { stream }))
    }

    async fn listen(&self, addr: SocketAddr) -> Result<Box<dyn super::Listener>> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Box::new(TcpListenerWrapper { listener }))
    }

    fn name(&self) -> &str {
        "TCP"
    }
}

struct TcpConnection {
    stream: TcpStream,
}

#[async_trait]
impl super::Connection for TcpConnection {
    async fn send(&mut self, data: Bytes) -> Result<()> {
        self.stream.write_all(&data).await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<Option<Bytes>> {
        let mut buf = vec![0u8; 1024];
        let n = self.stream.read(&mut buf).await?;
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(Bytes::copy_from_slice(&buf[..n])))
    }

    async fn close(&mut self) -> Result<()> {
        self.stream.shutdown().await?;
        Ok(())
    }
}

struct TcpListenerWrapper {
    listener: TcpListener,
}

#[async_trait]
impl super::Listener for TcpListenerWrapper {
    async fn accept(&mut self) -> Result<(Box<dyn super::Connection>, SocketAddr)> {
        let (stream, addr) = self.listener.accept().await?;
        Ok((Box::new(TcpConnection { stream }), addr))
    }
}
