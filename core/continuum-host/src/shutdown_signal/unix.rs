//! Unix: SIGTERM and SIGINT, each installed on its own.

use super::Edge;
use tokio::signal::unix::{signal, Signal, SignalKind};

pub(super) async fn first_edge() -> std::io::Result<Edge> {
    let terminate = signal(SignalKind::terminate());
    let interrupt = signal(SignalKind::interrupt());
    let (mut terminate, mut interrupt) = match (terminate, interrupt) {
        (Err(e), Err(_)) => return Err(e),
        (terminate, interrupt) => (terminate, interrupt),
    };
    tokio::select! {
        _ = arrival(&mut terminate) => Ok(Edge::Terminate),
        _ = arrival(&mut interrupt) => Ok(Edge::Interrupt),
    }
}

/// The edge's next arrival; one that could not be installed never arrives.
async fn arrival(installed: &mut std::io::Result<Signal>) {
    match installed {
        Ok(signal) => {
            signal.recv().await;
        }
        Err(_) => std::future::pending::<()>().await,
    }
}
