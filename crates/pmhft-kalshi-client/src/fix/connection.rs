use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tracing::{debug, error, info};

/// A TLS-encrypted TCP connection for FIX 4.4 protocol.
pub struct FixTcpConnection {
    stream: tokio_rustls::client::TlsStream<TcpStream>,
    read_buf: Vec<u8>,
}

impl FixTcpConnection {
    /// Connect to a FIX gateway over TLS.
    pub async fn connect(host: &str, port: u16) -> anyhow::Result<Self> {
        info!(host = host, port = port, "Connecting to FIX gateway");

        let tcp_stream = TcpStream::connect((host, port)).await?;
        tcp_stream.set_nodelay(true)?;

        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(tls_config));
        let server_name = rustls::pki_types::ServerName::try_from(host.to_string())?;
        let tls_stream = connector.connect(server_name, tcp_stream).await?;

        info!("FIX TLS connection established");

        Ok(Self {
            stream: tls_stream,
            read_buf: Vec::with_capacity(65536),
        })
    }

    /// Send raw bytes over the connection.
    pub async fn send(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.stream.write_all(data).await?;
        self.stream.flush().await?;
        debug!(len = data.len(), "Sent FIX message");
        Ok(())
    }

    /// Read available bytes into the internal buffer.
    /// Returns the number of bytes read (0 = connection closed).
    pub async fn read(&mut self) -> anyhow::Result<usize> {
        let mut tmp = [0u8; 8192];
        let n = self.stream.read(&mut tmp).await?;
        if n > 0 {
            self.read_buf.extend_from_slice(&tmp[..n]);
        }
        Ok(n)
    }

    /// Take the accumulated read buffer (drains it).
    pub fn take_buffer(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.read_buf)
    }

    /// Set the remaining bytes back into the buffer.
    pub fn put_back(&mut self, data: Vec<u8>) {
        self.read_buf = data;
    }

    /// Shutdown the connection.
    pub async fn shutdown(&mut self) {
        if let Err(e) = self.stream.shutdown().await {
            error!(error = %e, "Error shutting down FIX connection");
        }
    }
}
