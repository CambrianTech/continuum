//! Windows: no Unix domain sockets, so the primary endpoint is a TCP listener on the loopback
//! address the core names (CONTINUUM_CORE_BIND, default 127.0.0.1), and local clients connect there.

use std::io;
use std::net::{TcpListener, TcpStream};

pub(super) const PRIMARY_IS_TCP: bool = true;
pub(super) type Stream = TcpStream;
pub(super) type ClientStream = tokio::net::TcpStream;

pub(super) struct Listener {
    inner: TcpListener,
    addr: String,
}

impl Listener {
    pub(super) fn bind(endpoint: &super::Endpoint<'_>) -> io::Result<Self> {
        Ok(Self { inner: TcpListener::bind(endpoint.tcp_addr)?, addr: endpoint.tcp_addr.to_string() })
    }

    pub(super) fn accept(&self) -> io::Result<Stream> {
        self.inner.accept().map(|(stream, _)| stream)
    }

    pub(super) fn describe(&self) -> String {
        format!("TCP {} (Windows primary transport, no Unix-domain sockets)", self.addr)
    }
}

pub(super) fn peer(stream: &Stream) -> String {
    match stream.peer_addr() {
        Ok(addr) => addr.to_string(),
        Err(e) => format!("unreadable peer ({e})"),
    }
}

pub(super) async fn connect(address: &str) -> io::Result<ClientStream> {
    tokio::net::TcpStream::connect(address).await
}
