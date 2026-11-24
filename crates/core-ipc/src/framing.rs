//! Length-prefixed `postcard` binary framing for local control sockets.

use serde::{Serialize, de::DeserializeOwned};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Upper bound on any single control-plane frame (16 MiB).
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Serializes `msg` with `postcard` and writes a 4-byte little-endian length prefix + payload.
pub async fn write_frame<W, T>(writer: &mut W, msg: &T) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let payload = postcard::to_allocvec(msg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    if payload.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("IPC frame exceeds limit ({} bytes)", payload.len()),
        ));
    }

    let len = payload.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

/// Reads a 4-byte little-endian length prefix + `postcard` payload and deserializes `T`.
pub async fn read_frame<R, T>(reader: &mut R) -> io::Result<T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("incoming IPC frame exceeds limit ({len} bytes)"),
        ));
    }

    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;

    postcard::from_bytes(&payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{EngineCommand, EngineEvent};
    use core_scene::{LayerKind, SceneDiff};

    #[tokio::test]
    async fn round_trips_commands_and_events() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        let cmd = EngineCommand::ApplyDiff(Box::new(SceneDiff::ClearLayer(LayerKind::Slide)));
        write_frame(&mut client, &cmd).await.expect("write command");

        let received: EngineCommand = read_frame(&mut server).await.expect("read command");
        assert_eq!(received, cmd);

        let evt = EngineEvent::Pong(42);
        write_frame(&mut server, &evt).await.expect("write event");

        let received_evt: EngineEvent = read_frame(&mut client).await.expect("read event");
        assert_eq!(received_evt, evt);
    }

    #[tokio::test]
    async fn rejects_oversized_frame_header() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let bad_len = (MAX_FRAME_BYTES as u32 + 1).to_le_bytes();
        client.write_all(&bad_len).await.expect("write bad len");

        let err = read_frame::<_, EngineCommand>(&mut server)
            .await
            .expect_err("must reject oversized frame");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn rejects_corrupt_postcard_payload() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let garbage = [0xFFu8; 5];
        client
            .write_all(&(garbage.len() as u32).to_le_bytes())
            .await
            .expect("write len");
        client.write_all(&garbage).await.expect("write garbage");

        let err = read_frame::<_, EngineCommand>(&mut server)
            .await
            .expect_err("must fail on corrupt postcard");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
