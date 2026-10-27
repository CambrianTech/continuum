//! Unix: a Unix domain socket file, mode 0600.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use tokio::net::{UnixListener, UnixStream};

pub(super) type ServerStream = UnixStream;
pub(super) type ClientStream = UnixStream;

pub(super) struct Listener(UnixListener);

impl Listener {
    pub(super) async fn accept(&mut self) -> io::Result<ServerStream> {
        self.0.accept().await.map(|(stream, _)| stream)
    }
}

pub(super) fn bind(path: &Path) -> io::Result<Listener> {
    remove(path);
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(Listener(listener))
}

pub(super) async fn connect(path: &Path) -> io::Result<ClientStream> {
    UnixStream::connect(path).await
}

pub(super) fn remove(path: &Path) {
    let _ = std::fs::remove_file(path); // absent already is the outcome wanted
}
