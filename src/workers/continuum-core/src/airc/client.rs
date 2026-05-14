use crate::airc::process::{AircCommandOutput, AircCommandRunner, AircInvocation};
use crate::airc::types::{
    command_vector, queue_failure_result, unique_card_field, AircQueueListEnvelope,
    AircQueueListRequest, AircQueueScanErrorKind, AircQueueScanResult,
};
use async_trait::async_trait;

#[async_trait]
pub trait AircQueueClient: Send + Sync {
    async fn list_queue(&self, request: AircQueueListRequest) -> AircQueueScanResult;
}

#[derive(Debug, Clone)]
pub struct CliAircQueueClient<R> {
    runner: R,
}

impl<R> CliAircQueueClient<R>
where
    R: AircCommandRunner,
{
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
}

#[async_trait]
impl<R> AircQueueClient for CliAircQueueClient<R>
where
    R: AircCommandRunner,
{
    async fn list_queue(&self, request: AircQueueListRequest) -> AircQueueScanResult {
        let args = request.args();
        let invocation = AircInvocation {
            program: request.airc_bin.clone(),
            args: args.clone(),
            timeout_ms: request.timeout_ms,
        };

        let output = match self.runner.run(invocation).await {
            Ok(output) => output,
            Err(error) => {
                return queue_failure_result(
                    &request,
                    &args,
                    error.kind,
                    error.message,
                    None,
                    String::new(),
                    0,
                );
            }
        };

        decode_queue_output(&request, &args, output)
    }
}

fn decode_queue_output(
    request: &AircQueueListRequest,
    args: &[String],
    output: AircCommandOutput,
) -> AircQueueScanResult {
    if !output.success {
        return queue_failure_result(
            request,
            args,
            AircQueueScanErrorKind::CommandFailed,
            "airc queue list exited non-zero".to_string(),
            output.exit_code,
            output.stderr,
            output.stdout.len(),
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let queue: AircQueueListEnvelope = match serde_json::from_str(&stdout) {
        Ok(queue) => queue,
        Err(e) => {
            return queue_failure_result(
                request,
                args,
                AircQueueScanErrorKind::InvalidJson,
                format!("invalid airc JSON: {e}"),
                output.exit_code,
                output.stderr,
                output.stdout.len(),
            );
        }
    };

    if queue.repo != request.repo {
        return queue_failure_result(
            request,
            args,
            AircQueueScanErrorKind::InvalidEnvelope,
            format!(
                "airc queue repo mismatch: requested {}, got {}",
                request.repo, queue.repo
            ),
            output.exit_code,
            output.stderr,
            output.stdout.len(),
        );
    }

    let statuses = unique_card_field(&queue.cards, |card| Some(card.card.status.as_str()));
    let owners = unique_card_field(&queue.cards, |card| card.card.owner.as_deref());
    let card_count = queue.cards.len();

    AircQueueScanResult {
        ok: true,
        repo: queue.repo.clone(),
        card_count,
        statuses,
        owners,
        command: command_vector(&request.airc_bin, args),
        stdout_bytes: output.stdout.len(),
        stderr: output.stderr,
        queue: Some(queue),
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::process::AircCommandError;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakeRunner {
        output: Result<AircCommandOutput, AircCommandError>,
        invocations: Arc<Mutex<Vec<AircInvocation>>>,
    }

    impl FakeRunner {
        fn new(output: Result<AircCommandOutput, AircCommandError>) -> Self {
            Self {
                output,
                invocations: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl AircCommandRunner for FakeRunner {
        async fn run(
            &self,
            invocation: AircInvocation,
        ) -> Result<AircCommandOutput, AircCommandError> {
            self.invocations.lock().unwrap().push(invocation);
            self.output.clone()
        }
    }

    fn request() -> AircQueueListRequest {
        AircQueueListRequest {
            repo: "CambrianTech/continuum".to_string(),
            limit: 2,
            owner: None,
            status: None,
            airc_bin: "airc".to_string(),
            timeout_ms: 1000,
        }
    }

    fn success(stdout: &str) -> Result<AircCommandOutput, AircCommandError> {
        Ok(AircCommandOutput {
            success: true,
            exit_code: Some(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: String::new(),
        })
    }

    #[tokio::test]
    async fn queue_scan_parses_typed_cards_without_node() {
        let runner = FakeRunner::new(success(
            r#"{"now_utc":"2026-05-14T15:18:09Z","repo":"CambrianTech/continuum","cards":[{"number":1167,"title":"alpha-gap","url":"https://github.com/CambrianTech/continuum/issues/1167","createdAt":"2026-05-14T13:54:08Z","updatedAt":"2026-05-14T13:59:35Z","card":{"kind":"airc-queue-card-v1","status":"in-progress","owner":"codex-main","branch":"feat/airc-rust-agent-flywheel"}},{"number":1166,"title":"probe","url":"https://github.com/CambrianTech/continuum/issues/1166","createdAt":"2026-05-14T13:10:48Z","updatedAt":"2026-05-14T13:10:48Z","card":{"kind":"airc-queue-card-v1","status":"blocked","owner":"claude-tab-1"}}]}"#,
        ));
        let client = CliAircQueueClient::new(runner.clone());
        let result = client.list_queue(request()).await;

        assert!(result.ok);
        assert_eq!(result.repo, "CambrianTech/continuum");
        assert_eq!(result.card_count, 2);
        assert_eq!(result.statuses, ["in-progress", "blocked"]);
        assert_eq!(result.owners, ["codex-main", "claude-tab-1"]);
        assert_eq!(result.queue.unwrap().cards[0].number, 1167);

        let invocations = runner.invocations.lock().unwrap();
        assert_eq!(invocations[0].args[0], "queue");
        assert_eq!(invocations[0].args[1], "list");
    }

    #[tokio::test]
    async fn queue_scan_returns_structured_failure_for_bad_json() {
        let runner = FakeRunner::new(Ok(AircCommandOutput {
            success: true,
            exit_code: Some(0),
            stdout: b"not json".to_vec(),
            stderr: "bad output".to_string(),
        }));
        let result = CliAircQueueClient::new(runner).list_queue(request()).await;

        assert!(!result.ok);
        assert_eq!(result.card_count, 0);
        assert!(matches!(
            result.error.as_ref().unwrap().kind,
            AircQueueScanErrorKind::InvalidJson
        ));
        assert!(result
            .error
            .as_ref()
            .unwrap()
            .message
            .contains("invalid airc JSON"));
        assert!(result.stderr.contains("bad output"));
    }

    #[tokio::test]
    async fn queue_scan_rejects_repo_mismatch() {
        let runner = FakeRunner::new(success(
            r#"{"now_utc":"2026-05-14T15:18:09Z","repo":"Other/repo","cards":[]}"#,
        ));
        let result = CliAircQueueClient::new(runner).list_queue(request()).await;

        assert!(!result.ok);
        assert!(matches!(
            result.error.as_ref().unwrap().kind,
            AircQueueScanErrorKind::InvalidEnvelope
        ));
    }
}
