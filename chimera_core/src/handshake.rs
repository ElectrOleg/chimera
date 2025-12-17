use chimera_transport::Connection;
use chimera_crypto::{ChimeraCrypto, Cipher};
use anyhow::{Result, anyhow};
use bytes::Bytes;
use tracing::info;
use async_trait::async_trait;

use crate::mimic::Mimic;

pub struct EncryptedConnection {
    inner: Box<dyn Connection>,
    cipher_in: Cipher,
    cipher_out: Cipher,
    seq_in: u64,
    seq_out: u64,
}

impl EncryptedConnection {
    pub async fn new(mut inner: Box<dyn Connection>, is_server: bool, mimic: Option<Box<dyn Mimic>>) -> Result<Self> {
        // 1. Generate ephemeral keypair
        let (my_private, my_public) = ChimeraCrypto::generate_ephemeral_key()?;
        
        let peer_public = if is_server {
            // Server waits for client's public key (possibly masqueraded)
            let received_data = inner.recv().await?.ok_or_else(|| anyhow!("Connection closed during handshake"))?;
            
            let peer_pub = if let Some(ref m) = mimic {
                m.decapsulate(&received_data)?.ok_or_else(|| anyhow!("Mimic decapsulation failed"))?
            } else {
                received_data.to_vec()
            };

            // Send own public key
            let my_data = if let Some(ref m) = mimic {
                m.encapsulate(&my_public, is_server)?
            } else {
                Bytes::copy_from_slice(&my_public)
            };
            inner.send(my_data).await?;
            
            peer_pub
        } else {
            // Client sends public key first
            let my_data = if let Some(ref m) = mimic {
                m.encapsulate(&my_public, is_server)?
            } else {
                 Bytes::copy_from_slice(&my_public)
            };
            inner.send(my_data).await?;

            // Wait for server's public key
            let received_data = inner.recv().await?.ok_or_else(|| anyhow!("Connection closed during handshake"))?;
            
            if let Some(ref m) = mimic {
                 m.decapsulate(&received_data)?.ok_or_else(|| anyhow!("Mimic decapsulation failed"))?
            } else {
                received_data.to_vec()
            }
        };

        // 2. Derive shared secret
        let secret = ChimeraCrypto::derive_secret(my_private, &peer_public)?;
        info!("Handshake completed. Shared secret derived.");

        // 3. Initialize ciphers (symmetric for now, simpler)
        // In a real protocol, we'd mix in nonces/IDs to have different keys for Tx/Rx
        let cipher_in = Cipher::new(&secret)?;
        let cipher_out = Cipher::new(&secret)?;

        Ok(Self {
            inner,
            cipher_in,
            cipher_out,
            seq_in: 0,
            seq_out: 0,
        })
    }

    pub async fn send(&mut self, data: &[u8]) -> Result<()> {
        let mut encrypted = data.to_vec();
        self.cipher_out.encrypt(self.seq_out, &mut encrypted)?;
        self.seq_out += 1;
        
        // Protocol: [Length: u32][Encrypted Data] to handle framing
        // For simplicity reusing transport's framing if packet-based, but stream needs framing.
        // Let's assume the transport handles frames for now (like WebSocket/Quic) or we use length prefix.
        // Detailed definition: Standard TCP needs length prefix.
        // Let's rely on inner transport `send` taking raw bytes.
        self.inner.send(Bytes::from(encrypted)).await?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<Option<Bytes>> {
        let data = self.inner.recv().await?;
        if let Some(bytes) = data {
            let mut buf = bytes.to_vec();
            // Note: In real proto, need to handle framing to get exact encrypted block.
            // Simplified: assuming 1-to-1 mapping for now (TCP stream nature might break this without framing)
            // For MVP/PoC this is acceptable if we send distinct packets.
            let len = self.cipher_in.decrypt(self.seq_in, &mut buf)?;
            buf.truncate(len);
            self.seq_in += 1;
            Ok(Some(Bytes::from(buf)))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl Connection for EncryptedConnection {
    async fn send(&mut self, data: Bytes) -> Result<()> {
        self.send(&data).await
    }

    async fn recv(&mut self) -> Result<Option<Bytes>> {
        self.recv().await
    }

    async fn close(&mut self) -> Result<()> {
        self.inner.close().await
    }
}
