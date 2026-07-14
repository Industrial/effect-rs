//! [`TcpTransport`] — length-delimited frames over Tokio TCP.
//!
//! Frame layout: 4-byte big-endian length prefix + payload. Max frame
//! size is enforced on read so a corrupt/hostile peer cannot force an
//! unbounded allocation.

use std::sync::Arc;

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener as TokioTcpListener, TcpStream};
use tokio::sync::Mutex;

use super::{Connection, Listener, Transport, TransportError};

/// Hard cap on a single frame (16 MiB). Envelopes larger than this are a
/// protocol violation — bulk data belongs in streams/blob stores.
pub const MAX_FRAME_BYTES: u32 = 16 * 1024 * 1024;

struct TcpConnection {
  read: Arc<Mutex<OwnedReadHalf>>,
  write: Arc<Mutex<OwnedWriteHalf>>,
  peer: String,
}

impl TcpConnection {
  fn new(stream: TcpStream) -> Self {
    let peer = stream
      .peer_addr()
      .map(|a| a.to_string())
      .unwrap_or_else(|_| "unknown".to_string());
    let (read, write) = stream.into_split();
    Self {
      read: Arc::new(Mutex::new(read)),
      write: Arc::new(Mutex::new(write)),
      peer,
    }
  }
}

#[async_trait::async_trait]
impl Connection for TcpConnection {
  async fn send(&self, frame: Bytes) -> Result<(), TransportError> {
    if frame.len() as u64 > MAX_FRAME_BYTES as u64 {
      return Err(TransportError::Io(
        "frame exceeds MAX_FRAME_BYTES".to_string(),
      ));
    }
    let mut write = self.write.lock().await;
    write.write_all(&(frame.len() as u32).to_be_bytes()).await?;
    write.write_all(&frame).await?;
    write.flush().await?;
    Ok(())
  }

  async fn recv(&self) -> Result<Bytes, TransportError> {
    let mut read = self.read.lock().await;
    let mut len_buf = [0u8; 4];
    read
      .read_exact(&mut len_buf)
      .await
      .map_err(|_| TransportError::Closed)?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_BYTES {
      return Err(TransportError::Io(
        "peer frame exceeds MAX_FRAME_BYTES".to_string(),
      ));
    }
    let mut payload = vec![0u8; len as usize];
    read
      .read_exact(&mut payload)
      .await
      .map_err(|_| TransportError::Closed)?;
    Ok(Bytes::from(payload))
  }

  async fn close(&self) {
    let mut write = self.write.lock().await;
    let _ = write.shutdown().await;
  }

  fn peer_addr(&self) -> String {
    self.peer.clone()
  }
}

struct TcpListenerWrap {
  listener: TokioTcpListener,
  addr: String,
}

#[async_trait::async_trait]
impl Listener for TcpListenerWrap {
  async fn accept(&self) -> Result<Box<dyn Connection>, TransportError> {
    let (stream, _) = self.listener.accept().await?;
    stream.set_nodelay(true).ok();
    Ok(Box::new(TcpConnection::new(stream)))
  }

  fn local_addr(&self) -> String {
    self.addr.clone()
  }
}

/// Plain-TCP transport (length-delimited frames). The fallback path when
/// QUIC is unavailable; combine with the cookie handshake for auth.
#[derive(Clone, Copy, Debug, Default)]
pub struct TcpTransport;

#[async_trait::async_trait]
impl Transport for TcpTransport {
  async fn connect(&self, addr: &str) -> Result<Box<dyn Connection>, TransportError> {
    let stream = TcpStream::connect(addr)
      .await
      .map_err(|e| TransportError::Connect(e.to_string()))?;
    stream.set_nodelay(true).ok();
    Ok(Box::new(TcpConnection::new(stream)))
  }

  async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
    let listener = TokioTcpListener::bind(addr)
      .await
      .map_err(|e| TransportError::Bind(e.to_string()))?;
    let local = listener
      .local_addr()
      .map(|a| a.to_string())
      .unwrap_or_else(|_| addr.to_string());
    Ok(Box::new(TcpListenerWrap {
      listener,
      addr: local,
    }))
  }
}
