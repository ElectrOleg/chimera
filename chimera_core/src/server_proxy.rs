use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use anyhow::{Result, anyhow};
use bytes::Bytes;
use tracing::{info, error, warn};
use crate::protocol::{Frame, FrameType};
use crate::Connection;

/// Manages multiple outgoing TCP connections multiplexed over a single transport
pub struct ServerProxy {
    streams: Arc<Mutex<HashMap<u32, mpsc::Sender<Bytes>>>>,
    tunnel_tx: mpsc::Sender<Frame>,
}

impl ServerProxy {
    pub fn new(tunnel_tx: mpsc::Sender<Frame>) -> Self {
        Self {
            streams: Arc::new(Mutex::new(HashMap::new())),
            tunnel_tx,
        }
    }

    pub async fn handle_frame(&self, frame: Frame) -> Result<()> {
        match frame.frame_type {
            FrameType::Connect => {
                let target = String::from_utf8(frame.payload.to_vec())?;
                info!("Proxy Request: Connect to {}", target);
                
                let stream_id = frame.stream_id;
                let tunnel_tx = self.tunnel_tx.clone();
                let streams = self.streams.clone();

                tokio::spawn(async move {
                    match TcpStream::connect(&target).await {
                        Ok(mut socket) => {
                            info!("Connected to {}", target);
                            
                            // Split stream handling
                            // Increased buffer to 10000 to prevent HOL blocking
                            let (tx, mut rx) = mpsc::channel::<Bytes>(10000);
                            
                            {
                                let mut map = streams.lock().await;
                                map.insert(stream_id, tx);
                            }

                            let (mut rd, mut wr) = socket.split();
                            
                            // Remote -> Tunnel Loop
                            let to_tunnel = async {
                                // Reduced buffer to 1400 to fit in MTU
                                let mut buf = [0u8; 1400];
                                loop {
                                    match rd.read(&mut buf).await {
                                        Ok(0) => break, // EOF
                                        Ok(n) => {
                                            let data = Bytes::copy_from_slice(&buf[0..n]);
                                            let frame = Frame::new(FrameType::Data, stream_id, data);
                                            if tunnel_tx.send(frame).await.is_err() {
                                                break;
                                            }
                                        }
                                        Err(_) => break,
                                    }
                                }
                                // Send disconnect
                                let _ = tunnel_tx.send(Frame::new(FrameType::Disconnect, stream_id, Bytes::new())).await;
                            };

                            // Tunnel -> Remote Loop
                            let from_tunnel = async {
                                while let Some(data) = rx.recv().await {
                                    if wr.write_all(&data).await.is_err() {
                                        break;
                                    }
                                }
                            };

                            tokio::join!(to_tunnel, from_tunnel);
                            
                            // Cleanup
                            let mut map = streams.lock().await;
                            map.remove(&stream_id);
                            info!("Closed connection {} ({})", stream_id, target);
                        }
                        Err(e) => {
                            warn!("Failed to connect to {}: {}", target, e);
                            // Send disconnect immediately
                             let _ = tunnel_tx.send(Frame::new(FrameType::Disconnect, stream_id, Bytes::new())).await;
                        }
                    }
                });
            }
            FrameType::Data => {
                // Clone sender and release lock before awaiting to prevent deadlock
                let tx = {
                    let map = self.streams.lock().await;
                    map.get(&frame.stream_id).cloned()
                };
                
                if let Some(tx) = tx {
                    let _ = tx.send(frame.payload).await;
                }
            }
            FrameType::Disconnect => {
                let mut map = self.streams.lock().await;
                // Removing the sender drops it, causing the `rx.recv()` in the spawn to return None, closing the write half
                map.remove(&frame.stream_id);
            }
            FrameType::Padding => {
                // Ignore
            }
        }
        Ok(())
    }
}
