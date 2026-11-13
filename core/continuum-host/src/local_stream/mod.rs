//! A local stream endpoint named by a path: one process serves it, others on the same machine
//! connect. What differs by OS is only the endpoint: a Unix domain socket file (mode 0600, so
//! only this user connects) or a Windows named pipe derived from the path. The relay of an
//! engine's stderr serves on one (card 1d7d6d4c).

use std::io;
use std::path::Path;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// The serving side of one endpoint.
pub struct Listener(os::Listener);

/// What a listener left on disk, removable only while it is still the listener's own: a
/// successor that took over the path is never removed by its predecessor's cleanup.
pub struct Owned(os::Owned);

impl Owned {
    pub fn remove(self) {
        self.0.remove()
    }
}

/// A connection accepted by a [`Listener`].
pub type ServerStream = os::ServerStream;

/// A connection opened by [`connect`].
pub type ClientStream = os::ClientStream;

impl Listener {
    /// Serve the endpoint named by `path`. A live owner is refused (`AddrInUse`); what a server
    /// that is gone left behind is reclaimed.
    pub async fn bind(path: &Path) -> io::Result<(Self, Owned)> {
        os::bind(path).await.map(|(listener, owned)| (Self(listener), Owned(owned)))
    }

    /// The next connection.
    pub async fn accept(&mut self) -> io::Result<ServerStream> {
        self.0.accept().await
    }
}

/// Connect to the endpoint named by `path`.
pub async fn connect(path: &Path) -> io::Result<ClientStream> {
    os::connect(path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // what this catches: an endpoint a client cannot reach.
    #[tokio::test]
    async fn a_client_reaches_the_server() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("endpoint.sock");
        let (mut listener, owned) = Listener::bind(&path).await.expect("bind");
        let served = tokio::spawn(async move {
            let mut stream = listener.accept().await.expect("accept");
            stream.write_all(b"hello\n").await.expect("write");
        });
        let mut client = connect(&path).await.expect("connect");
        let mut got = String::new();
        client.read_to_string(&mut got).await.expect("read");
        served.await.expect("served");
        assert_eq!(got, "hello\n");
        owned.remove();
    }

    // what this catches (Codex on #5533): two binders that both judged one stale endpoint dead
    // and removed each other's. Under custody exactly one binds; the rest find a live owner, and
    // the winner still serves.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_binders_of_a_stale_endpoint_leave_exactly_one_owner() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("endpoint.sock");
        let (gone, _gone_owned) = Listener::bind(&path).await.expect("bind");
        drop(gone); // stale: the file stays, nobody serves it
        let binders: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                tokio::spawn(async move { Listener::bind(&path).await })
            })
            .collect();
        let mut winners = Vec::new();
        for binder in binders {
            match binder.await.expect("binder task") {
                Ok(bound) => winners.push(bound),
                Err(e) => assert_eq!(e.kind(), io::ErrorKind::AddrInUse, "{e}"),
            }
        }
        assert_eq!(winners.len(), 1, "exactly one binder owns the endpoint");
        assert!(connect(&path).await.is_ok(), "the winner still serves");
    }

    // what this catches (Codex on #5533): reclaiming an endpoint on an answer that does not
    // prove its server gone. A socket that refuses with permission denied is left in place.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_endpoint_that_does_not_refuse_is_never_reclaimed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("endpoint.sock");
        let (gone, _gone_owned) = Listener::bind(&path).await.expect("bind");
        drop(gone);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let probe = std::os::unix::net::UnixStream::connect(&path).err().map(|e| e.kind());
        if probe != Some(io::ErrorKind::PermissionDenied) {
            return; // a superuser connects anyway: this host cannot produce the answer under test
        }
        let refused = Listener::bind(&path).await.err().map(|e| e.kind());
        assert_eq!(refused, Some(io::ErrorKind::PermissionDenied), "an unproven owner is not reclaimed");
        assert!(std::fs::symlink_metadata(&path).is_ok(), "the endpoint is left as it was");
    }

    // what this catches (Codex on #5533): a bind that removed a live incumbent's endpoint, and a
    // predecessor's cleanup that removed its replacement's. A live owner is refused; what a gone
    // server left is reclaimed; the old owner's cleanup leaves the successor's endpoint alone.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_live_owner_is_refused_and_an_old_cleanup_spares_its_successor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("endpoint.sock");
        let (first, first_owned) = Listener::bind(&path).await.expect("bind");
        let refused = Listener::bind(&path).await.err().map(|e| e.kind());
        assert_eq!(refused, Some(io::ErrorKind::AddrInUse), "a live owner keeps its endpoint");
        drop(first); // the server is gone; its file stays behind
        let (successor, _successor_owned) = Listener::bind(&path).await.expect("a stale endpoint is reclaimed");
        first_owned.remove();
        assert!(path.exists(), "the predecessor's cleanup spares its successor");
        assert!(connect(&path).await.is_ok(), "the successor still serves");
        drop(successor);
    }

    // what this catches (Codex on #5544): a cleanup that trusts only the custody token removing
    // whatever replaced its socket outside custody. A regular file or a symlink now at the path is
    // not the socket this bind created, so the old owner's cleanup leaves it.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_old_cleanup_never_removes_what_replaced_its_socket() {
        let dir = tempfile::tempdir().expect("tempdir");
        for replacement in ["a regular file", "a symlink"] {
            let path = dir.path().join("endpoint.sock");
            let (listener, owned) = Listener::bind(&path).await.expect("bind");
            drop(listener);
            std::fs::remove_file(&path).expect("the socket goes");
            if replacement == "a regular file" {
                std::fs::write(&path, b"not a socket").expect("write the replacement");
            } else {
                let target = dir.path().join("elsewhere");
                std::fs::write(&target, b"elsewhere").expect("the link's target");
                std::os::unix::fs::symlink(&target, &path).expect("link the replacement");
            }
            owned.remove();
            assert!(std::fs::symlink_metadata(&path).is_ok(), "{replacement} at the path survives the old cleanup");
            std::fs::remove_file(&path).expect("tidy");
        }
    }
}
