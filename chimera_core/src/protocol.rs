use bytes::{Buf, BufMut, Bytes, BytesMut};
use anyhow::{Result, anyhow};
use std::io::Cursor;

/// Packet Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    Connect = 0x01,
    Data = 0x02,
    Disconnect = 0x03,
    Padding = 0x04,
}

impl  TryFrom<u8> for FrameType {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0x01 => Ok(FrameType::Connect),
            0x02 => Ok(FrameType::Data),
            0x03 => Ok(FrameType::Disconnect),
            0x04 => Ok(FrameType::Padding),
            _ => Err(anyhow!("Invalid FrameType: {}", value)),
        }
    }
}

/// The Chimera Multiplexing Frame
/// Format: [Type: 1] [StreamID: 4] [Length: 2] [Payload: Var]
#[derive(Debug, Clone)]
pub struct Frame {
    pub frame_type: FrameType,
    pub stream_id: u32,
    pub payload: Bytes,
}

impl Frame {
    pub fn new(frame_type: FrameType, stream_id: u32, payload: Bytes) -> Self {
        Self {
            frame_type,
            stream_id,
            payload,
        }
    }

    /// Serializes the frame into bytes
    pub fn to_bytes(&self) -> Bytes {
        let len = self.payload.len() as u16;
        let mut buf = BytesMut::with_capacity(1 + 4 + 2 + self.payload.len());
        
        buf.put_u8(self.frame_type as u8);
        buf.put_u32(self.stream_id);
        buf.put_u16(len);
        buf.put(self.payload.clone());
        
        buf.freeze()
    }

    /// Tries to parse a frame from a buffer.
    /// Returns headers length + payload length if successful, or None if incomplete.
    /// This allows the caller to extract exactly that many bytes.
    pub fn check(src: &mut Cursor<&[u8]>) -> Result<Option<usize>> {
        if src.remaining() < 7 {
            return Ok(None);
        }

        // Peek length
        let pos = src.position();
        src.advance(5); // Skip Type (1) + StreamID (4)
        let len = src.get_u16();
        src.set_position(pos); // Reset

        let total_len = 7 + len as usize;
        if src.remaining() < total_len {
            return Ok(None); 
        }

        Ok(Some(total_len))
    }

    /// Parses a complete frame from bytes
    pub fn parse(src: &mut Bytes) -> Result<Frame> {
        if src.len() < 7 {
            return Err(anyhow!("Incomplete frame header"));
        }

        let type_byte = src.get_u8();
        let frame_type = FrameType::try_from(type_byte)?;
        let stream_id = src.get_u32();
        let len = src.get_u16() as usize;

        if src.len() < len {
            return Err(anyhow!("Incomplete frame payload"));
        }

        let payload = src.split_to(len);

        Ok(Frame {
            frame_type,
            stream_id,
            payload,
        })
    }
}
