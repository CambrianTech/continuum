//! The gateway's listener as a resource its module owns (CONCURRENCY-STYLE-GUIDE.md: owned
//! tasks, nothing detached). `initialize` binds it or fails; `drain` closes admission and lets
//! the calls in flight finish; `shutdown` abandons whatever is still in flight, cutting every
//! connection (`crate::sever`) so one whose agent stopped reading ends too, and joins the
//! serve task. Codex on #5541 (856ee2b69): a detached `axum::serve` with its port kept in a
//! `OnceLock` reported `listening` after the listener died, could not be stopped, and left a
//! reinitialize colliding with the old socket.

use std::future::Future;

use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::stop::Signal;

/// A bound listener and the task serving it.
pub(crate) struct Listener {
    port: u16,
    /// Raised: axum stops accepting connections and lets the open ones finish.
    close: watch::Sender<bool>,
    /// Raised: every call still in flight is stopped (`crate::answer`, `crate::server`) and
    /// every connection is cut (`crate::sever`), whether or not its agent is reading.
    abandon: watch::Sender<bool>,
    /// Raised by the serve task as it returns, for whoever waits on the drain.
    exited: Signal,
    serving: JoinHandle<()>,
}

impl Listener {
    /// Bind `port` on loopback (0: any free port) and serve `routes` on it (the gateway's are
    /// `crate::server::router`), each given the signal that abandons its calls. A port that
    /// cannot be bound is the error; there is no listener that only says it is one.
    pub(crate) async fn bind(
        port: u16,
        routes: impl FnOnce(Signal) -> axum::Router,
    ) -> Result<Self, String> {
        let socket = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|error| format!("the gateway cannot bind 127.0.0.1:{port}: {error}"))?;
        let port = socket
            .local_addr()
            .map_err(|error| {
                format!("the gateway's socket on port {port} has no address: {error}")
            })?
            .port();
        let (close, closed) = Signal::pair();
        let (abandon, abandoned) = Signal::pair();
        let (exit, exited) = Signal::pair();
        let connections = crate::sever::Severing {
            socket,
            cut: abandoned.clone(),
        };
        let router = routes(abandoned);
        let serving = tokio::spawn(async move {
            let outcome = axum::serve(connections, router)
                .with_graceful_shutdown(closed.raised())
                .await;
            match outcome {
                Ok(()) => continuum_base::probe!(
                    class = "gateway.stopped",
                    port = port as u64,
                    "the gateway's listener closed"
                ),
                Err(error) => {
                    continuum_base::probe!(class = "gateway.stopped", port = port as u64, error = %error, "the gateway's listener failed")
                }
            }
            exit.send_replace(true);
        });
        Ok(Self {
            port,
            close,
            abandon,
            exited,
            serving,
        })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Whether it still serves: false once the serve task returned, however it ended.
    pub(crate) fn is_live(&self) -> bool {
        !self.serving.is_finished()
    }

    /// Stop accepting connections; the calls in flight run on. Idempotent.
    pub(crate) fn close(&self) {
        self.close.send_replace(true);
    }

    /// Stop every call still in flight and cut every connection. An answer being read is
    /// told the node is stopping as its connection closes; one whose agent stopped reading is
    /// cut all the same, dropping its generation, admission and capture lease.
    pub(crate) fn abandon(&self) {
        self.abandon.send_replace(true);
    }

    /// Resolves once the serve task has returned (every connection closed).
    pub(crate) fn exited(&self) -> impl Future<Output = ()> + Send + 'static {
        self.exited.raised()
    }

    /// Join the serve task. Called after it exited it returns at once; otherwise the task is
    /// aborted first (the bound of a stop ran out), which the probe says.
    pub(crate) async fn join(mut self) -> Result<(), String> {
        if self.is_live() {
            continuum_base::probe!(
                class = "gateway.aborted",
                port = self.port as u64,
                "the gateway's listener did not finish within its stop bound; aborted"
            );
            self.serving.abort();
        }
        match (&mut self.serving).await {
            Ok(()) => Ok(()),
            Err(error) if error.is_cancelled() => Ok(()),
            Err(error) => Err(format!(
                "the gateway's listener on port {} panicked: {error}",
                self.port
            )),
        }
    }
}

impl Drop for Listener {
    /// A listener its module let go of never serves on detached.
    fn drop(&mut self) {
        self.serving.abort();
    }
}
