use anyhow::Result;
use bytes::Bytes;

/// Trait for disguising handshake data as other protocols.
pub trait Mimic: Send + Sync {
    /// Wrap the initial handshake payload (e.g. public key) into a cover protocol.
    fn encapsulate(&self, payload: &[u8], is_server: bool) -> Result<Bytes>;

    /// Extract the handshake payload from a cover protocol packet.
    /// Returns None if the packet doesn't match the expected format.
    fn decapsulate(&self, packet: &[u8]) -> Result<Option<Vec<u8>>>;
    
    /// Name of the cover protocol (e.g. "HTTP", "TLS")
    fn protocol_name(&self) -> &str;
}

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

pub struct HttpMimic;

impl Mimic for HttpMimic {
    fn encapsulate(&self, payload: &[u8], is_server: bool) -> Result<Bytes> {
        // Encode payload as Base64 URL Safe
        let encoded = URL_SAFE_NO_PAD.encode(payload);
        
        if is_server {
            // Server responds with 200 OK and payload in header
             let response = format!(
                "HTTP/1.1 200 OK\r\nServer: Chuck/1.0\r\nContent-Type: text/html\r\nContent-Length: 0\r\nX-Data: {}\r\n\r\n",
                encoded
            );
            Ok(Bytes::from(response))
        } else {
             // Client sends GET
             let request = format!(
                "GET /api/v1/resource/{} HTTP/1.1\r\nHost: cdn.example.com\r\nUser-Agent: Chimera/1.0\r\nConnection: keep-alive\r\n\r\n",
                encoded
            );
            Ok(Bytes::from(request))
        }
    }

    fn decapsulate(&self, packet: &[u8]) -> Result<Option<Vec<u8>>> {
        let text = String::from_utf8_lossy(packet);
        // Simple parser: look for GET /api/v1/resource/
        if let Some(start) = text.find("GET /api/v1/resource/") {
            if let Some(end) = text[start..].find(" HTTP/1.1") {
                let encoded_part = &text[start + 21..start + end];
                let decoded = URL_SAFE_NO_PAD.decode(encoded_part)?;
                return Ok(Some(decoded));
            }
        }
        // Also handle Server response: HTTP/1.1 200 OK\r\n...X-Data: <payload>
        // For simplicity, let's make the server response also put data in a header
        if let Some(start) = text.find("X-Data: ") {
             if let Some(end) = text[start..].find("\r\n") {
                 let encoded_part = &text[start + 8..start + end];
                 let decoded = URL_SAFE_NO_PAD.decode(encoded_part)?;
                 return Ok(Some(decoded));
             }
        }
        
        Ok(None)
    }

    fn protocol_name(&self) -> &str {
        "HTTP"
    }
}
