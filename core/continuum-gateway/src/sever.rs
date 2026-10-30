//! Connections the gateway's module can cut.
//!
//! Codex on #5541 (f2a3600fe): an agent that stays connected but stops reading fills its
//! socket, hyper stops polling the response body, and a stop signal the body would read is
//! never read, so the generation it owns (the engine's `TurnAdmission`, the capture lease)
//! outlives the stop. Cancellation has to reach the connection without the agent reading.
//!
//! Every accepted socket is wrapped in [`Severable`]. When the module raises `cut`, the
//! connection's task is woken and every read and write it makes fails; hyper ends the
//! connection and drops the body with the generation inside it, the engine's one
//! cancellation path. axum's graceful serve returns only after every connection task has
//! ended (`close_tx.closed()` in `axum::serve`), so the listener's exit is the proof that no
//! connection, body or lease survives the stop.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};

use crate::stop::Signal;

/// The gateway's socket, whose every connection is cut when `cut` is raised.
pub(crate) struct Severing {
    pub(crate) socket: TcpListener,
    pub(crate) cut: Signal,
}

impl axum::serve::Listener for Severing {
    type Io = Severable;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        let (stream, addr) = axum::serve::Listener::accept(&mut self.socket).await;
        let cut = Box::pin(self.cut.raised());
        (
            Severable {
                stream,
                cut,
                severed: false,
            },
            addr,
        )
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.socket.local_addr()
    }
}

/// One agent's connection. Its `cut` is polled on every read and write, so the task that
/// owns the connection is woken when it is raised, however the agent behaves.
pub(crate) struct Severable {
    stream: TcpStream,
    /// Polled until it resolves, never after (`severed`).
    cut: Pin<Box<dyn Future<Output = ()> + Send>>,
    severed: bool,
}

impl Severable {
    /// Whether the module cut this connection; registers the task to be woken when it does.
    fn severed(&mut self, cx: &mut Context<'_>) -> bool {
        if !self.severed && self.cut.as_mut().poll(cx).is_ready() {
            self.severed = true;
        }
        self.severed
    }

    fn cut_error() -> io::Error {
        io::Error::new(io::ErrorKind::ConnectionAborted, crate::stop::STOPPING)
    }
}

impl AsyncRead for Severable {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.severed(cx) {
            return Poll::Ready(Err(Self::cut_error()));
        }
        Pin::new(&mut this.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for Severable {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.severed(cx) {
            return Poll::Ready(Err(Self::cut_error()));
        }
        Pin::new(&mut this.stream).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.severed(cx) {
            return Poll::Ready(Err(Self::cut_error()));
        }
        Pin::new(&mut this.stream).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.severed(cx) {
            return Poll::Ready(Err(Self::cut_error()));
        }
        Pin::new(&mut this.stream).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.severed(cx) {
            return Poll::Ready(Err(Self::cut_error()));
        }
        Pin::new(&mut this.stream).poll_shutdown(cx)
    }
}
