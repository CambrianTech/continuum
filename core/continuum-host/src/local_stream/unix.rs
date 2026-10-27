//! Unix: a Unix domain socket file, mode 0600. Who may serve the path is decided under an
//! exclusive lock on a sidecar file: probing an existing endpoint, reclaiming one whose server
//! is gone, binding and recording what was bound happen as one step, and so does cleanup, so
//! two binders can never both judge an endpoint stale and remove each other's.

use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
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

/// The sidecar whose exclusive lock decides who serves `path`. It is never removed: removing a
/// lock file another process may be holding would let two holders exist. Binders are serialized
/// on it, so of several that judge one stale endpoint dead exactly one reclaims it; but the wait
/// is bounded (Codex on #5536): past [`CUSTODY_WAIT`] the binder is refused `AddrInUse`.
fn custody(path: &Path) -> io::Result<std::fs::File> {
    let mut name = path.as_os_str().to_owned();
    name.push(".lock");
    let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(PathBuf::from(name))?;
    let deadline = std::time::Instant::now() + CUSTODY_WAIT;
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(lock),
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() >= deadline => {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, format!("another binder held custody of {} past {CUSTODY_WAIT:?}", path.display())));
            }
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(CUSTODY_POLL),
        }
    }
}

// derived-or-floor: a CEILING on waiting for another binder; custody is held only across one bind's probe, unlink and bind syscalls (microseconds to milliseconds), so a holder past this has failed and the binder is refused. It bounds no serving or token budget
const CUSTODY_WAIT: std::time::Duration = std::time::Duration::from_secs(2);
// derived-or-floor: the POLL cadence of that wait; it bounds how late a released custody is noticed, never a budget
const CUSTODY_POLL: std::time::Duration = std::time::Duration::from_millis(2);

impl Owned {
    pub(super) fn remove(self) {
        let Ok(_custody) = custody(&self.path) else { return }; // without custody nothing is removed
        let mine = std::fs::symlink_metadata(&self.path).is_ok_and(|m| (m.dev(), m.ino()) == self.identity);
        if mine {
            let _ = std::fs::remove_file(&self.path); // gone already is the outcome wanted
        }
    }
}

/// Under custody: refuse a live owner, reclaim only an endpoint whose server refuses the
/// connection (proof it is gone), bind, and record the identity of what was bound.
fn bind_in_custody(path: &Path) -> io::Result<(std::os::unix::net::UnixListener, Owned)> {
    let _custody = custody(path)?;
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
        Ok(found) if !found.file_type().is_socket() => {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} exists and is not a socket", path.display())));
        }
        Ok(_) => match std::os::unix::net::UnixStream::connect(path) {
            Ok(_) => {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, format!("{} is served by a live owner", path.display())));
            }
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => std::fs::remove_file(path)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            // Any other answer does not prove the owner gone: the endpoint is left as it is.
            Err(e) => return Err(e),
        },
    }
    let listener = std::os::unix::net::UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let meta = std::fs::symlink_metadata(path)?;
    Ok((listener, Owned { path: path.to_path_buf(), identity: (meta.dev(), meta.ino()) }))
}

pub(super) async fn bind(path: &Path) -> io::Result<(Listener, Owned)> {
    let path = path.to_path_buf();
    let (listener, owned) = tokio::task::spawn_blocking(move || bind_in_custody(&path))
        .await
        .map_err(io::Error::other)??;
    Ok((Listener(UnixListener::from_std(listener)?), owned))
}

pub(super) async fn connect(path: &Path) -> io::Result<ClientStream> {
    UnixStream::connect(path).await
}
