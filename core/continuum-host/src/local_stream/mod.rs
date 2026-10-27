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

/// A connection accepted by a [`Listener`].
pub type ServerStream = os::ServerStream;

/// A connection opened by [`connect`].
pub type ClientStream = os::ClientStream;

impl Listener {
    /// Serve the endpoint named by `path`, replacing one a predecessor left behind.
    pub fn bind(path: &Path) -> io::Result<Self> {
        os::bind(path).map(Self)
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

/// Remove what the endpoint left on disk, if anything.
pub fn remove(path: &Path) {
    os::remove(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // what this catches: an endpoint a client cannot reach, or one a second server cannot take
    // over from a predecessor that left it behind.
    #[tokio::test]
    async fn a_client_reaches_the_server_and_a_successor_takes_over_the_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("endpoint.sock");
        let first = Listener::bind(&path).expect("bind");
        drop(first);
        let mut listener = Listener::bind(&path).expect("a successor binds the same path");
        let served = tokio::spawn(async move {
            let mut stream = listener.accept().await.expect("accept");
            stream.write_all(b"hello\n").await.expect("write");
        });
        let mut client = connect(&path).await.expect("connect");
        let mut got = String::new();
        client.read_to_string(&mut got).await.expect("read");
        served.await.expect("served");
        assert_eq!(got, "hello\n");
        remove(&path);
    }
}
