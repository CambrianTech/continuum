//! Windows: Ctrl+C is required; console close and system shutdown are best-effort.

use super::Edge;
use tokio::signal::windows::{ctrl_c, ctrl_close, ctrl_shutdown};

pub(super) async fn first_edge() -> std::io::Result<Edge> {
    let mut interrupt = ctrl_c()?;
    let mut close = ctrl_close();
    let mut shutdown = ctrl_shutdown();
    let closed = async {
        match close.as_mut() {
            Ok(edge) => {
                edge.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    let shut_down = async {
        match shutdown.as_mut() {
            Ok(edge) => {
                edge.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        _ = interrupt.recv() => Ok(Edge::Interrupt),
        _ = closed => Ok(Edge::ConsoleClose),
        _ = shut_down => Ok(Edge::SystemShutdown),
    }
}
