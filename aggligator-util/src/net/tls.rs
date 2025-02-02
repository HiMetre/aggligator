//! TLS connection functions.

use futures::Future;
use rustls::{ClientConfig, ServerConfig, ServerName};
use std::{io::Result, net::SocketAddr, sync::Arc};

use crate::transport::{
    tcp::{TcpAcceptor, TcpConnector},
    tls::{TlsClient, TlsServer},
    Acceptor, Connector,
};
use aggligator::alc::Stream;

/// Builds a connection consisting of aggregated TCP links to the target,
/// which are encrypted and authenticated using TLS.
///
/// `target` specifies a set of IP addresses or hostnames of the target host.
/// If a hostname resolves to multiple IP addresses this is taken into account
/// automatically.
/// If an entry in target specifies no port number, `default_port` is used.
///
/// Links are established automatically from all available local network interfaces
/// to all IP addresses of the target. If a link fails, it is reconnected
/// automatically.
///
/// The identity of the server is verified using TLS against `server_name`.
/// Each outgoing link is encrypted using TLS with the configuration specified
/// in `tls_client_cfg`.
///
/// Returns the connection stream.
///
/// # Example
/// This example connects to the host `agl.server.rs` on port 5901.
///
/// Multiple links will be used if the local machine has multiple interfaces
/// that can all connect to `agl.server.rs`, or `agl.server.rs` has multiple interfaces
/// that are registered with their IP addresses in DNS.
/// ```no_run
/// use std::sync::Arc;
/// use aggligator_util::net::tls_connect;
/// use rustls::{ClientConfig, RootCertStore, ServerName};
///
/// #[tokio::main]
/// async fn main() -> std::io::Result<()> {
///     let server_name = "agl.server.rs";
///
///     let mut root_store = RootCertStore::empty();
///     // add certificates to the root_store
///
///     let tls_cfg = Arc::new(
///         ClientConfig::builder()
///             .with_safe_defaults()
///             .with_root_certificates(root_store)
///             .with_no_client_auth()
///     );
///
///     let stream = tls_connect(
///         [server_name.to_string()],
///         5901,
///         tls_cfg,
///         ServerName::try_from(server_name).unwrap(),
///     ).await?;
///
///     // use the connection
///
///     Ok(())
/// }
/// ```
pub async fn tls_connect(
    target: impl IntoIterator<Item = String>, default_port: u16, tls_client_cfg: Arc<ClientConfig>,
    server_name: ServerName, link_count: Option<u8>
) -> Result<Stream> {
    let mut connector = Connector::wrapped(TlsClient::new(tls_client_cfg, server_name));
    connector.add(TcpConnector::new(target, default_port, link_count).await?);
    let ch = connector.channel().unwrap().await?;
    Ok(ch.into_stream())
}

/// Runs a TCP server accepting connections of aggregated links,
/// which are encrypted and authenticated using TLS.
///
/// The TCP server listens on `addr` and accepts connections of aggregated TCP links.
/// For each new connection the work function `work_fn` is spawned onto a new
/// Tokio task.
///
/// Each incoming link is encrypted using TLS with the configuration specified
/// in `tls_server_cfg`.
///
/// # Example
/// This example listens on all interfaces on port 5901.
///
/// If the server has multiple interfaces, all IP addresses should be registered
/// in DNS so that clients can discover them and establish multiple links.
/// ```no_run
/// use std::net::{Ipv6Addr, SocketAddr};
/// use std::sync::Arc;
/// use aggligator_util::net::tls_server;
/// use rustls::ServerConfig;
///
/// #[tokio::main]
/// async fn main() -> std::io::Result<()> {
///     let tls_certs = todo!("load certificate tree");
///     let tls_key = todo!("load private key");
///
///     let tls_cfg = Arc::new(
///         ServerConfig::builder()
///             .with_safe_defaults()
///             .with_no_client_auth()
///             .with_single_cert(tls_certs, tls_key)
///             .unwrap()
///     );
///
///     tls_server(
///         SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 5901),
///         tls_cfg,
///         |stream| async move {
///             // use the incoming connection
///         }
///     ).await?;
///
///     Ok(())
/// }
/// ```
pub async fn tls_server<F>(
    addr: SocketAddr, tls_server_cfg: Arc<ServerConfig>, work_fn: impl Fn(Stream) -> F + Send + 'static,
) -> Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let acceptor = Acceptor::wrapped(TlsServer::new(tls_server_cfg));
    acceptor.add(TcpAcceptor::new([addr]).await?);

    loop {
        let (ch, _control) = acceptor.accept().await?;
        tokio::spawn(work_fn(ch.into_stream()));
    }
}
