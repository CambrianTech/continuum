//! Unix: a Unix domain socket file, mode 0600.

use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use tokio::net::{UnixListener, UnixStream};

pub(super) type ServerStream = UnixStream;
pub(super) type ClientStream = UnixStream;

pub(super) struct Listener(UnixListener);

impl Listener {
    pub(super) async fn accept(&mut self) -> io::Result<ServerStream> {
        self.0.accept().await.map(|(stream, _)| stream)
    }
}

/// The socket file this listener created: its device and inode.
pub(super) struct Owned {
    path: PathBuf,
    identity: (u64, u64),
}

impl Owned {
    pub(super) fn remove(self) {
        let mine = std::fs::symlink_metadata(&self.path).is_ok_and(|m| (m.dev(), m.ino()) == self.identity);
        if mine {
            let _ = std::fs::remove_file(&self.path); // gone already is the outcome wanted
        }
    }
}

pub(super) async fn bind(path: &Path) -> io::Result<(Listener, Owned)> {
    if std::fs::symlink_metadata(path).is_ok() {
        // A live owner answers; a stale file (its server gone) refuses the connection.
        if UnixStream::connect(path).await.is_ok() {
            return Err(io::Error::new(io::ErrorKind::AddrInUse, format!("{} is served by a live owner", path.display())));
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    let meta = std::fs::symlink_metadata(path)?;
    Ok((Listener(listener), Owned { path: path.to_path_buf(), identity: (meta.dev(), meta.ino()) }))
}

pub(super) async fn connect(path: &Path) -> io::Result<ClientStream> {
    UnixStream::connect(path).await
}
