//! [`QuicTransport`] — quinn + rustls (feature `quic`).
//!
//! Each logical connection is one QUIC connection with a single
//! bidirectional control stream carrying length-delimited frames (the
//! same layout as the TCP transport). Multiplexed per-process-pair
//! streams are a wire-compatible upgrade reserved for a later phase.
//!
//! ## Trust model
//!
//! v1 uses **cluster-internal TLS**: each node generates (or is given) a
//! certificate; peers accept any certificate and authentication is done
//! by the cookie handshake on top ([`crate::handshake`]). This gives
//! confidentiality + integrity from TLS and node authentication from the
//! cookie — equivalent to BEAM's TLS distribution with cookie auth. Full
//! mTLS pinning (provide a CA, verify peers against it) plugs into
//! [`QuicConfig::with_client_verification`].

use std::sync::Arc;

use bytes::Bytes;
use quinn::{ClientConfig, Endpoint, RecvStream, SendStream, ServerConfig};
use tokio::sync::Mutex;

use super::{Connection, Listener, Transport, TransportError};
use crate::transport::tcp::MAX_FRAME_BYTES;

/// Certificate material for one node.
pub struct QuicConfig {
  cert_chain: Vec<rustls::pki_types::CertificateDer<'static>>,
  key: rustls::pki_types::PrivateKeyDer<'static>,
  /// Roots used to verify peers; `None` = accept any cert (cookie auth
  /// on top provides node authentication).
  roots: Option<rustls::RootCertStore>,
}

impl QuicConfig {
  /// Self-signed certificate for `host` — the zero-config cluster mode.
  pub fn self_signed(host: &str) -> Result<Self, TransportError> {
    let certified = rcgen::generate_simple_self_signed(vec![host.to_string()])
      .map_err(|e| TransportError::Io(format!("rcgen: {e}")))?;
    let cert = certified.cert.der().clone();
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
      certified.key_pair.serialize_der(),
    ));
    Ok(Self {
      cert_chain: vec![cert],
      key,
      roots: None,
    })
  }

  /// Verify peers against the given roots (full mTLS pinning).
  pub fn with_client_verification(mut self, roots: rustls::RootCertStore) -> Self {
    self.roots = Some(roots);
    self
  }
}

/// Accepts any server certificate; used when node authentication is done
/// by the cookie handshake instead of PKI.
#[derive(Debug)]
struct AcceptAnyCert(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for AcceptAnyCert {
  fn verify_server_cert(
    &self,
    _end_entity: &rustls::pki_types::CertificateDer<'_>,
    _intermediates: &[rustls::pki_types::CertificateDer<'_>],
    _server_name: &rustls::pki_types::ServerName<'_>,
    _ocsp: &[u8],
    _now: rustls::pki_types::UnixTime,
  ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
    Ok(rustls::client::danger::ServerCertVerified::assertion())
  }

  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &rustls::pki_types::CertificateDer<'_>,
    dss: &rustls::DigitallySignedStruct,
  ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls12_signature(
      message,
      cert,
      dss,
      &self.0.signature_verification_algorithms,
    )
  }

  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &rustls::pki_types::CertificateDer<'_>,
    dss: &rustls::DigitallySignedStruct,
  ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls13_signature(
      message,
      cert,
      dss,
      &self.0.signature_verification_algorithms,
    )
  }

  fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
    self.0.signature_verification_algorithms.supported_schemes()
  }
}

struct QuicConnection {
  send: Mutex<SendStream>,
  recv: Mutex<RecvStream>,
  conn: quinn::Connection,
}

#[async_trait::async_trait]
impl Connection for QuicConnection {
  async fn send(&self, frame: Bytes) -> Result<(), TransportError> {
    if frame.len() as u64 > MAX_FRAME_BYTES as u64 {
      return Err(TransportError::Io(
        "frame exceeds MAX_FRAME_BYTES".to_string(),
      ));
    }
    let mut send = self.send.lock().await;
    send
      .write_all(&(frame.len() as u32).to_be_bytes())
      .await
      .map_err(|_| TransportError::Closed)?;
    send
      .write_all(&frame)
      .await
      .map_err(|_| TransportError::Closed)?;
    Ok(())
  }

  async fn recv(&self) -> Result<Bytes, TransportError> {
    let mut recv = self.recv.lock().await;
    let mut len_buf = [0u8; 4];
    recv
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
    recv
      .read_exact(&mut payload)
      .await
      .map_err(|_| TransportError::Closed)?;
    Ok(Bytes::from(payload))
  }

  async fn close(&self) {
    self.conn.close(0u32.into(), b"close");
  }

  fn peer_addr(&self) -> String {
    self.conn.remote_address().to_string()
  }
}

struct QuicListener {
  endpoint: Endpoint,
  addr: String,
}

#[async_trait::async_trait]
impl Listener for QuicListener {
  async fn accept(&self) -> Result<Box<dyn Connection>, TransportError> {
    let incoming = self.endpoint.accept().await.ok_or(TransportError::Closed)?;
    let conn = incoming
      .await
      .map_err(|e| TransportError::Connect(e.to_string()))?;
    let (send, recv) = conn
      .accept_bi()
      .await
      .map_err(|e| TransportError::Connect(e.to_string()))?;
    Ok(Box::new(QuicConnection {
      send: Mutex::new(send),
      recv: Mutex::new(recv),
      conn,
    }))
  }

  fn local_addr(&self) -> String {
    self.addr.clone()
  }
}

/// QUIC transport (quinn + rustls). See the module docs for the trust
/// model.
pub struct QuicTransport {
  config: Arc<QuicConfig>,
}

impl QuicTransport {
  /// Transport from certificate material.
  pub fn new(config: QuicConfig) -> Self {
    Self {
      config: Arc::new(config),
    }
  }

  fn client_config(&self) -> Result<ClientConfig, TransportError> {
    let provider = rustls::crypto::CryptoProvider::get_default()
      .cloned()
      .unwrap_or_else(|| Arc::new(rustls::crypto::ring::default_provider()));
    let tls = match &self.config.roots {
      Some(roots) => rustls::ClientConfig::builder()
        .with_root_certificates(roots.clone())
        .with_no_client_auth(),
      None => rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCert(provider)))
        .with_no_client_auth(),
    };
    let quic_tls = quinn::crypto::rustls::QuicClientConfig::try_from(tls)
      .map_err(|e| TransportError::Io(format!("tls: {e}")))?;
    Ok(ClientConfig::new(Arc::new(quic_tls)))
  }

  fn server_config(&self) -> Result<ServerConfig, TransportError> {
    let tls = rustls::ServerConfig::builder()
      .with_no_client_auth()
      .with_single_cert(self.config.cert_chain.clone(), self.config.key.clone_key())
      .map_err(|e| TransportError::Io(format!("tls: {e}")))?;
    let quic_tls = quinn::crypto::rustls::QuicServerConfig::try_from(tls)
      .map_err(|e| TransportError::Io(format!("tls: {e}")))?;
    Ok(ServerConfig::with_crypto(Arc::new(quic_tls)))
  }
}

#[async_trait::async_trait]
impl Transport for QuicTransport {
  async fn connect(&self, addr: &str) -> Result<Box<dyn Connection>, TransportError> {
    let remote: std::net::SocketAddr = addr
      .parse()
      .map_err(|e| TransportError::Connect(format!("bad addr {addr}: {e}")))?;
    let bind: std::net::SocketAddr = if remote.is_ipv6() {
      "[::]:0".parse().expect("valid")
    } else {
      "0.0.0.0:0".parse().expect("valid")
    };
    let mut endpoint =
      Endpoint::client(bind).map_err(|e| TransportError::Connect(e.to_string()))?;
    endpoint.set_default_client_config(self.client_config()?);
    let conn = endpoint
      .connect(remote, "cluster")
      .map_err(|e| TransportError::Connect(e.to_string()))?
      .await
      .map_err(|e| TransportError::Connect(e.to_string()))?;
    let (send, recv) = conn
      .open_bi()
      .await
      .map_err(|e| TransportError::Connect(e.to_string()))?;
    Ok(Box::new(QuicConnection {
      send: Mutex::new(send),
      recv: Mutex::new(recv),
      conn,
    }))
  }

  async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
    let bind: std::net::SocketAddr = addr
      .parse()
      .map_err(|e| TransportError::Bind(format!("bad addr {addr}: {e}")))?;
    let endpoint = Endpoint::server(self.server_config()?, bind)
      .map_err(|e| TransportError::Bind(e.to_string()))?;
    let local = endpoint
      .local_addr()
      .map(|a| a.to_string())
      .unwrap_or_else(|_| addr.to_string());
    Ok(Box::new(QuicListener {
      endpoint,
      addr: local,
    }))
  }
}
