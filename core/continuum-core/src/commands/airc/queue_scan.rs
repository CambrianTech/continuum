//! `airc/queue-scan` — Rust-native AIRC work-queue scan for the no-Node agent
//! flywheel poll.
//!
//! Dep-holding: captures the module's `Arc<dyn AircQueueClient>` (the seam that
//! shells out to the `airc` CLI's `queue list`). Read-only over the queue, but
//! gated `Privileged` — it spawns a subprocess whose binary path the caller can
//! influence (`airc_bin`), which is an escalation surface untrusted callers must
//! not reach.

use std::sync::Arc;

use crate::airc::{
    AircQueueClient, AircQueueListRequest, AircQueueScanParams, AircQueueScanResult,
};

crate::action_command! {
    /// Scan the AIRC work queue for a GitHub repo (the no-Node agent flywheel
    /// poll). Returns the open cards with their statuses and owners, the card
    /// count, and the exact `airc` command that produced them. Read-only over
    /// the queue.
    pub struct AircQueueScan { queue_client: Arc<dyn AircQueueClient> }
    name: "airc/queue-scan",
    access: Privileged,
    params: AircQueueScanParams,
    output: AircQueueScanResult,
    run(this, _ctx, p) => {
        // `try_from` clamps limit/timeout to the typed bounds and resolves the
        // binary path; it fails loud with an actionable String on bad input.
        let request = AircQueueListRequest::try_from(p)?;
        Ok(this.queue_client.list_queue(request).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};
    use async_trait::async_trait;

    // A queue client that echoes the request back as a result, so the test
    // asserts the command builds the request and surfaces the client's output
    // without shelling out to a real `airc` binary.
    struct EchoQueueClient;

    #[async_trait]
    impl AircQueueClient for EchoQueueClient {
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

    // what this catches: name/access wiring — queue-scan spawns a subprocess with
    // a caller-influenced binary path, so it is Privileged, not AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AircQueueScan::NAME, "airc/queue-scan");
        assert!(matches!(
            AircQueueScan::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: the command converts its typed params into a queue list
    // request and returns the client's result verbatim (repo preserved, command
    // is the canonical `queue list` invocation).
    #[tokio::test]
    async fn queue_scan_builds_request_and_returns_result() {
        let cmd = AircQueueScan {
            queue_client: Arc::new(EchoQueueClient),
        };
        let result = cmd
            .run(
                &Ctx::default(),
                AircQueueScanParams {
                    repo: "CambrianTech/continuum".into(),
                    limit: Some(2),
                    owner: None,
                    status: None,
                    airc_bin: None,
                    timeout_ms: None,
                },
            )
            .await
            .expect("queue-scan must succeed");
        assert!(result.ok);
        assert_eq!(result.repo, "CambrianTech/continuum");
        assert_eq!(result.command[0], "queue");
        assert_eq!(result.command[1], "list");
    }
}
