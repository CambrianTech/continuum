//! The core's primary IPC endpoint, and the client side of a local command endpoint. What differs
//! by OS is only the transport: a Unix domain socket at the core's path (mode 0666, so a caller
//! under another UID connects, #1008), or, on Windows, which has no Unix sockets, a TCP listener on
//! the loopback address the core names. Callers are local either way (owner-by-locality). This is
//! exactly what core/ipc held inline before (moved for #5842's os_branches): the Windows endpoint
//! stays TCP, never a named pipe, so no Windows client changes.

use std::io::{self, Read, Write};
use std::path::Path;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// Whether the primary endpoint is already TCP: then the core runs no second TCP ingress beside it.
pub const PRIMARY_IS_TCP: bool = os::PRIMARY_IS_TCP;

/// Where the endpoint binds: the socket path on Unix, the TCP address on Windows. Both are given,
/// so a caller never branches on the OS to build one.
pub struct Endpoint<'a> {
    pub socket_path: &'a Path,
    pub tcp_addr: &'a str,
}

/// The bound endpoint.
pub struct Listener(os::Listener);

impl Listener {
    pub fn bind(endpoint: &Endpoint<'_>) -> io::Result<Self> {
        os::Listener::bind(endpoint).map(Self)
    }

    /// The next connection, blocking until one arrives.
    pub fn accept(&self) -> io::Result<Stream> {
        self.0.accept().map(Stream)
    }

    /// What was bound, for the ready log line.
    pub fn describe(&self) -> String {
        self.0.describe()
    }
}

/// One accepted connection.
pub struct Stream(os::Stream);

impl Stream {
    /// A second handle on the same connection (a reader beside a writer).
    pub fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }

    /// The peer, for logs: its address, or why it could not be read.
    pub fn peer(&self) -> String {
        os::peer(&self.0)
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// The client side of [`connect`] (`tokio::net::UnixStream` | `tokio::net::TcpStream`).
pub type ClientStream = os::ClientStream;

/// Connect to a local command endpoint by its address: a socket path on Unix, a TCP address on
/// Windows.
pub async fn connect(address: &str) -> io::Result<ClientStream> {
    os::connect(address).await
}
