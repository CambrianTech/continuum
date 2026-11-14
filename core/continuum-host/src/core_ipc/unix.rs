//! Unix: a Unix domain socket, world read-write.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub(super) const PRIMARY_IS_TCP: bool = false;
pub(super) type Stream = UnixStream;
pub(super) type ClientStream = tokio::net::UnixStream;

pub(super) struct Listener {
    inner: UnixListener,
    path: PathBuf,
}

impl Listener {
    pub(super) fn bind(endpoint: &super::Endpoint<'_>) -> io::Result<Self> {
        let inner = UnixListener::bind(endpoint.socket_path)?;
        // World read-write, so a caller under another UID connects (#1008: the core as root in a
        // WSL2/Docker container, the host's client as uid 1000). The path is one the caller can
        // already see, the same reach as anything in /tmp.
        std::fs::set_permissions(endpoint.socket_path, std::fs::Permissions::from_mode(0o666))?;
        Ok(Self { inner, path: endpoint.socket_path.to_path_buf() })
    }

    pub(super) fn accept(&self) -> io::Result<Stream> {
        self.inner.accept().map(|(stream, _)| stream)
    }

    pub(super) fn describe(&self) -> String {
        format!("Unix socket {}", self.path.display())
    }
}

pub(super) fn peer(stream: &Stream) -> String {
    match stream.peer_addr() {
        Ok(addr) => format!("{addr:?}"),
        Err(e) => format!("unreadable peer ({e})"),
    }
}

pub(super) async fn connect(address: &str) -> io::Result<ClientStream> {
    tokio::net::UnixStream::connect(address).await
}
