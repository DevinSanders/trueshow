//! Native MIT-licensed Open Media Transport (OMT) LAN video + alpha channel transport.
//!
//! Provides zero-royalty bidirectional LAN streaming with alpha channel support and
//! `_omt._tcp.local.` mDNS discovery metadata.

use crate::alpha::{BroadcastFrame, premul_rgba_to_straight_bgra};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const OMT_MAGIC: [u8; 4] = *b"OMT1";
const MAX_OMT_FRAME_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum OmtError {
    #[error("OMT network I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid OMT frame header magic")]
    InvalidMagic,
    #[error("OMT payload size {0} exceeds maximum allowed limit")]
    FrameTooLarge(usize),
    #[error("invalid frame buffer dimensions")]
    InvalidDimensions,
}

/// mDNS discovery descriptor (`_omt._tcp.local.`) advertised on the local subnet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmtServiceDescriptor {
    pub service_type: String,
    pub instance_name: String,
    pub host: String,
    pub port: u16,
    pub supports_alpha: bool,
}

impl OmtServiceDescriptor {
    pub fn new(instance_name: impl Into<String>, port: u16, supports_alpha: bool) -> Self {
        Self {
            service_type: "_omt._tcp.local.".to_string(),
            instance_name: instance_name.into(),
            host: "127.0.0.1".to_string(),
            port,
            supports_alpha,
        }
    }
}

/// Wire header preceding every OMT frame payload (28 bytes little-endian).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OmtFrameHeader {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub timestamp_us: u64,
    pub payload_len: u32,
}

impl OmtFrameHeader {
    pub const BYTE_LEN: usize = 28;

    pub fn encode(&self) -> [u8; Self::BYTE_LEN] {
        let mut buf = [0u8; Self::BYTE_LEN];
        buf[0..4].copy_from_slice(&OMT_MAGIC);
        buf[4..8].copy_from_slice(&self.width.to_le_bytes());
        buf[8..12].copy_from_slice(&self.height.to_le_bytes());
        buf[12..16].copy_from_slice(&(u32::from(self.has_alpha)).to_le_bytes());
        buf[16..24].copy_from_slice(&self.timestamp_us.to_le_bytes());
        buf[24..28].copy_from_slice(&self.payload_len.to_le_bytes());
        buf
    }

    pub fn decode(buf: &[u8; Self::BYTE_LEN]) -> Result<Self, OmtError> {
        if buf[0..4] != OMT_MAGIC {
            return Err(OmtError::InvalidMagic);
        }
        let width = u32::from_le_bytes(buf[4..8].try_into().unwrap_or([0; 4]));
        let height = u32::from_le_bytes(buf[8..12].try_into().unwrap_or([0; 4]));
        let has_alpha = u32::from_le_bytes(buf[12..16].try_into().unwrap_or([0; 4])) != 0;
        let timestamp_us = u64::from_le_bytes(buf[16..24].try_into().unwrap_or([0; 8]));
        let payload_len = u32::from_le_bytes(buf[24..28].try_into().unwrap_or([0; 4]));
        if (payload_len as usize) > MAX_OMT_FRAME_BYTES {
            return Err(OmtError::FrameTooLarge(payload_len as usize));
        }
        Ok(Self {
            width,
            height,
            has_alpha,
            timestamp_us,
            payload_len,
        })
    }
}

/// Encodes and writes an OMT straight-BGRA + Alpha frame to any async transport stream.
pub async fn write_omt_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &BroadcastFrame,
) -> Result<usize, OmtError> {
    let expected = (frame.width as usize) * (frame.height as usize) * 4;
    if expected == 0 || frame.rgba_premul.len() != expected {
        return Err(OmtError::InvalidDimensions);
    }
    let mut bgra = Vec::new();
    premul_rgba_to_straight_bgra(&frame.rgba_premul, &mut bgra);

    let hdr = OmtFrameHeader {
        width: frame.width,
        height: frame.height,
        has_alpha: true,
        timestamp_us: frame.timestamp_us,
        payload_len: bgra.len() as u32,
    };
    writer.write_all(&hdr.encode()).await?;
    writer.write_all(&bgra).await?;
    writer.flush().await?;
    Ok(bgra.len())
}

/// Reads and validates an OMT frame header and BGRA8 payload from any async transport stream.
pub async fn read_omt_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<(OmtFrameHeader, Vec<u8>), OmtError> {
    let mut hdr_buf = [0u8; OmtFrameHeader::BYTE_LEN];
    reader.read_exact(&mut hdr_buf).await?;
    let hdr = OmtFrameHeader::decode(&hdr_buf)?;
    let mut payload = vec![0u8; hdr.payload_len as usize];
    reader.read_exact(&mut payload).await?;
    Ok((hdr, payload))
}

/// Async LAN OMT sender serving straight-BGRA + Alpha frames over TCP.
pub struct OmtSender {
    listener: TcpListener,
    descriptor: OmtServiceDescriptor,
}

impl OmtSender {
    pub async fn bind(instance_name: impl Into<String>, addr: &str) -> Result<Self, OmtError> {
        let listener = TcpListener::bind(addr).await?;
        let local_port = listener.local_addr()?.port();
        Ok(Self {
            listener,
            descriptor: OmtServiceDescriptor::new(instance_name, local_port, true),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, OmtError> {
        Ok(self.listener.local_addr()?)
    }

    pub fn descriptor(&self) -> &OmtServiceDescriptor {
        &self.descriptor
    }

    pub async fn send_single_frame(&self, frame: &BroadcastFrame) -> Result<usize, OmtError> {
        let (mut stream, _) = self.listener.accept().await?;
        write_omt_frame(&mut stream, frame).await
    }
}

/// Async LAN OMT receiver reading straight-BGRA + Alpha frames over TCP.
pub struct OmtReceiver {
    stream: TcpStream,
}

impl OmtReceiver {
    pub async fn connect(addr: SocketAddr) -> Result<Self, OmtError> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Self { stream })
    }

    pub async fn recv_frame(&mut self) -> Result<(OmtFrameHeader, Vec<u8>), OmtError> {
        read_omt_frame(&mut self.stream).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn streams_alpha_frame_and_rejects_invalid_dimensions_over_async_transport() {
        let desc = OmtServiceDescriptor::new("OpenPresenter Lower Third", 6400, true);
        assert_eq!(desc.service_type, "_omt._tcp.local.");
        assert!(desc.supports_alpha);

        let (mut tx, mut rx) = tokio::io::duplex(4096);
        let frame = BroadcastFrame {
            width: 2,
            height: 2,
            timestamp_us: 99_000,
            sequence: 7,
            rgba_premul: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0,
            ],
        };

        let sent_bytes = write_omt_frame(&mut tx, &frame).await.expect("write omt");
        let (hdr, bgra) = read_omt_frame(&mut rx).await.expect("read omt");

        assert_eq!(sent_bytes, 16);
        assert_eq!(hdr.width, 2);
        assert_eq!(hdr.height, 2);
        assert!(hdr.has_alpha);
        assert_eq!(hdr.timestamp_us, 99_000);
        assert_eq!(&bgra[0..4], &[0, 0, 255, 255]); // Red in BGRA
        assert_eq!(&bgra[12..16], &[0, 0, 0, 0]); // Transparent alpha preserved

        // Failure path: bad dimensions rejected before touching wire
        let bad = BroadcastFrame {
            width: 4,
            height: 4,
            timestamp_us: 0,
            sequence: 8,
            rgba_premul: vec![0; 4],
        };
        assert!(matches!(
            write_omt_frame(&mut tx, &bad).await,
            Err(OmtError::InvalidDimensions)
        ));
    }

    #[test]
    fn rejects_invalid_magic_and_oversized_header() {
        let bad_magic = [0u8; OmtFrameHeader::BYTE_LEN];
        assert!(matches!(
            OmtFrameHeader::decode(&bad_magic),
            Err(OmtError::InvalidMagic)
        ));

        let huge = OmtFrameHeader {
            width: 8192,
            height: 8192,
            has_alpha: true,
            timestamp_us: 0,
            payload_len: (MAX_OMT_FRAME_BYTES as u32) + 1,
        }
        .encode();
        assert!(matches!(
            OmtFrameHeader::decode(&huge),
            Err(OmtError::FrameTooLarge(_))
        ));
    }
}
