//! `airc/<verb>` — the Rust-native AIRC command surface as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! Three dep-holding verbs, sharing the two seams the
//! [`AircModule`](crate::modules::airc::AircModule) owns (the queue client + the
//! realtime event transport):
//! - **`airc/queue-scan`** (Privileged) — poll the GitHub work queue (no-Node flywheel).
//! - **`airc/realtime-publish`** (Privileged) — publish a typed realtime envelope.
//! - **`airc/realtime-replay`** (AiSafe) — replay bounded room realtime state.
//!
//! Contributed via [`command_objects`] from the module's `commands()`.

use std::sync::Arc;

use crate::airc::{AircEventTransport, AircQueueClient};
use crate::sdk_codegen::DynCommand;

pub mod queue_scan;
pub mod realtime_publish;
pub mod realtime_replay;

/// The dep-holding `airc/*` family. Each command captures only the seam it uses:
/// `queue-scan` the queue client; `realtime-publish`/`realtime-replay` the event
/// transport.
pub fn command_objects(
    queue_client: Arc<dyn AircQueueClient>,
    event_transport: Arc<dyn AircEventTransport>,
) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(queue_scan::AircQueueScan { queue_client }),
        Arc::new(realtime_publish::AircRealtimePublish {
            event_transport: event_transport.clone(),
        }),
        Arc::new(realtime_replay::AircRealtimeReplay { event_transport }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::{
        AircQueueListRequest, AircQueueScanResult, InMemoryAircRealtimeStore,
        StoreAircEventTransport,
    };
    use async_trait::async_trait;

    struct NoopQueueClient;

    #[async_trait]
    impl AircQueueClient for NoopQueueClient {
        async fn list_queue(&self, request: AircQueueListRequest) -> AircQueueScanResult {
            let command = request.args();
            AircQueueScanResult {
                ok: true,
                repo: request.repo,
                card_count: 0,
                statuses: Vec::new(),
                owners: Vec::new(),
                command,
                stdout_bytes: 0,
                stderr: String::new(),
                queue: None,
                error: None,
            }
        }
    }

    // what this catches: the family exposes exactly the three airc verbs, each name
    // mirroring its file path under commands/airc/ (the path==name invariant), and a
    // guard that command_objects() stays in sync with the files.
    #[test]
    fn family_exposes_all_three_airc_verbs() {
        let queue_client: Arc<dyn AircQueueClient> = Arc::new(NoopQueueClient);
        let event_transport: Arc<dyn AircEventTransport> = Arc::new(StoreAircEventTransport::new(
            Arc::new(InMemoryAircRealtimeStore::default()),
        ));
        let names: Vec<&str> = command_objects(queue_client, event_transport)
            .iter()
            .map(|o| o.name())
            .collect();
        assert_eq!(
            names,
            vec![
                "airc/queue-scan",
                "airc/realtime-publish",
                "airc/realtime-replay"
            ]
        );
    }
}
