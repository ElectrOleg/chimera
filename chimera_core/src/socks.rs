use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::{Result, anyhow};
use std::net::{SocketAddr, Ipv4Addr};
use tracing::{info, debug, error};

pub struct Socks5Listener {
    listener: TcpListener,
}

impl Socks5Listener {
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        info!("SOCKS5 Listener bound to {}", addr);
        Ok(Self { listener })
    }

    pub async fn accept(&self) -> Result<(TcpStream, String, u16)> {
        let (mut socket, peer) = self.listener.accept().await?;
        debug!("SOCKS5: New connection from {}", peer);

        // 1. Handshake
        // Client sends: [VER, NMETHODS, METHODS...]
        let mut buf = [0u8; 258];
        socket.read_exact(&mut buf[0..2]).await?;
        let ver = buf[0];
        let nmethods = buf[1] as usize;
        socket.read_exact(&mut buf[0..nmethods]).await?;

        if ver != 0x05 {
            return Err(anyhow!("Unsupported SOCKS version: {}", ver));
        }

        // We only support No Auth (0x00)
        // Check if 0x00 is in methods
        if !buf[0..nmethods].contains(&0x00) {
            // Send FF (No acceptable methods)
            socket.write_all(&[0x05, 0xFF]).await?;
            return Err(anyhow!("No acceptable authentication methods"));
        }

        // Send Server Choice: [VER, METHOD] = [0x05, 0x00]
        socket.write_all(&[0x05, 0x00]).await?;

        // 2. Request
        // Client sends: [VER, CMD, RSV, ATYP, DST.ADDR, DST.PORT]
        socket.read_exact(&mut buf[0..4]).await?;
        let _ver = buf[0];
        let cmd = buf[1];
        let _rsv = buf[2];
        let atyp = buf[3];

        if cmd != 0x01 { // CONNECT
            // Reply Command Not Supported
             socket.write_all(&[0x05, 0x07, 0x00, 0x01, 0,0,0,0, 0,0]).await?;
            return Err(anyhow!("Unsupported command: {}", cmd));
        }

        let target_host;
        let target_port;

        match atyp {
            0x01 => { // IPv4
                let mut ip_buf = [0u8; 4];
                socket.read_exact(&mut ip_buf).await?;
                let ip = Ipv4Addr::from(ip_buf);
                target_host = ip.to_string();
            }
            0x03 => { // Domain Name
                let len = socket.read_u8().await? as usize;
                let mut name_buf = vec![0u8; len];
                socket.read_exact(&mut name_buf).await?;
                target_host = String::from_utf8(name_buf)?;
            }
            _ => {
                 socket.write_all(&[0x05, 0x08, 0x00, 0x01, 0,0,0,0, 0,0]).await?;
                 return Err(anyhow!("Unsupported address type: {}", atyp));
            }
        }

        target_port = socket.read_u16().await?;

        // Send Success Reply immediately (we lie and say we connected)
        // [VER, REP, RSV, ATYP, BND.ADDR(0.0.0.0), BND.PORT(0)]
        socket.write_all(&[0x05, 0x00, 0x00, 0x01, 0,0,0,0, 0,0]).await?;

        info!("SOCKS5 Request: Connect to {}:{}", target_host, target_port);

        Ok((socket, target_host, target_port))
    }
}
