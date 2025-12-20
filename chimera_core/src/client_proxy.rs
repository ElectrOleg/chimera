use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use anyhow::Result;
use bytes::Bytes;
use tracing::{info, warn};
use crate::protocol::{Frame, FrameType};

/// Manages SOCKS connections on the client side
pub struct ClientProxy {
    streams: Arc<Mutex<HashMap<u32, mpsc::Sender<Bytes>>>>,
    tunnel_tx: mpsc::Sender<Frame>,
    next_id: Arc<Mutex<u32>>,
}

impl ClientProxy {
    pub fn new(tunnel_tx: mpsc::Sender<Frame>) -> Self {
        Self {
            streams: Arc::new(Mutex::new(HashMap::new())),
            tunnel_tx,
            next_id: Arc::new(Mutex::new(1)),
        }
    }

    /// Called when we receive a Frame from the Server
    pub async fn handle_frame(&self, frame: Frame) -> Result<()> {
        match frame.frame_type {
            FrameType::Data => {
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
                map.remove(&frame.stream_id);
            }
             _ => {} // Client shouldn't receive Connect frames
        }
        Ok(())
    }

    /// Registers a new SOCKS connection and starts the bridge
    pub async fn start_new_stream(&self, mut socket: TcpStream, target: String, port: u16) {
        let stream_id;
        {
            let mut id_lock = self.next_id.lock().await;
            stream_id = *id_lock;
            *id_lock += 1;
        }

        let tunnel_tx = self.tunnel_tx.clone();
        let payload = format!("{}:{}", target, port).into_bytes();
        
        // 1. Send CONNECT Frame
        let connect_frame = Frame::new(FrameType::Connect, stream_id, Bytes::copy_from_slice(&payload));
        if tunnel_tx.send(connect_frame).await.is_err() {
            return;
        }

        // 2. Setup Bridge
        // Increased buffer to 10000 to prevent HOL blocking of the main tunnel loop
        let (tx, mut rx) = mpsc::channel::<Bytes>(10000);
        {
            let mut map = self.streams.lock().await;
            map.insert(stream_id, tx);
        }
        
        // Remove stream on drop
        let streams = self.streams.clone();
        tokio::spawn(async move {
            let (mut rd, mut wr) = socket.split();

            // Socket -> Tunnel
            let to_tunnel = async {
                // Reduced buffer size to 1400 to avoid MTU fragmentation with headers
                let mut buf = [0u8; 1400];
                loop {
                    match rd.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            let data = Bytes::copy_from_slice(&buf[0..n]);
                            
                            // Traffic Obfuscation:
                            // DISABLED for performance testing
                            // if n < 500 && rng.gen_bool(0.5) ... 
                            // ...
                            
                            let frame = Frame::new(FrameType::Data, stream_id, data);
                            if tunnel_tx.send(frame).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                 let _ = tunnel_tx.send(Frame::new(FrameType::Disconnect, stream_id, Bytes::new())).await;
            };

            // Tunnel -> Socket
            let from_tunnel = async {
                while let Some(data) = rx.recv().await {
                    if wr.write_all(&data).await.is_err() {
                        break;
                    }
                }
            };

            tokio::join!(to_tunnel, from_tunnel);
            
            let mut map = streams.lock().await;
            map.remove(&stream_id);
            info!("Closed stream {}", stream_id);
        });
    }
}
