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
    buffer: bytes::BytesMut,
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

        let cipher_in = Cipher::new(&secret)?;
        let cipher_out = Cipher::new(&secret)?;

        Ok(Self {
            inner,
            cipher_in,
            cipher_out,
            seq_in: 0,
            seq_out: 0,
            buffer: bytes::BytesMut::with_capacity(4096),
        })
    }

    pub async fn send(&mut self, data: &[u8]) -> Result<()> {
        let mut encrypted = data.to_vec();
        self.cipher_out.encrypt(self.seq_out, &mut encrypted)?;
        self.seq_out += 1;
        
        // Framing: [Length: u32][Encrypted Data]
        let len = encrypted.len() as u32;
        let mut framed = bytes::BytesMut::with_capacity(4 + encrypted.len());
        use bytes::BufMut;
        framed.put_u32(len);
        framed.put_slice(&encrypted);
        
        self.inner.send(framed.freeze()).await?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<Option<Bytes>> {
        use bytes::Buf;
        loop {
            // 1. Try to parse a frame from current buffer
            if self.buffer.len() >= 4 {
                let mut cursor = std::io::Cursor::new(&self.buffer[..]);
                let len = cursor.get_u32() as usize;
                
                if self.buffer.len() >= 4 + len {
                    // Full packet available
                    self.buffer.advance(4); // Consume len
                    let mut encrypted_chunk = self.buffer.split_to(len).to_vec();
                    
                    let decrypted_len = self.cipher_in.decrypt(self.seq_in, &mut encrypted_chunk)?;
                    encrypted_chunk.truncate(decrypted_len);
                    self.seq_in += 1;
                    
                    return Ok(Some(Bytes::from(encrypted_chunk)));
                }
            }
            
            // 2. Need more data
            let data = self.inner.recv().await?;
            match data {
                Some(chunk) => {
                    self.buffer.extend_from_slice(&chunk);
                }
                None => {
                    if self.buffer.is_empty() {
                        return Ok(None); // Clean EOF
                    } else {
                        return Err(anyhow!("Connection closed with partial data"));
                    }
                }
            }
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
