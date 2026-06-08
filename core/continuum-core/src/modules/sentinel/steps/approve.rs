//! Approve step — pauses pipeline until human or persona approval.
//!
//! When the executor hits an Approve step:
//! 1. Pipeline status → WaitingApproval, checkpoint saved
//! 2. Event emitted: sentinel:{handle}:approval-requested
//! 3. Blocks on a tokio oneshot channel stored in PENDING_APPROVALS
//! 4. sentinel/approve IPC command resolves the channel
//! 5. On timeout (if set): auto-approve

use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

use super::super::interpolation;
use super::super::types::{ExecutionContext, PipelineContext, StepResult};

/// Approval decision sent through the channel
#[derive(Debug)]
pub struct ApprovalDecision {
    pub approved: bool,
    pub reason: Option<String>,
    pub approver_id: Option<String>,
}

/// Global map of pending approvals: handle_id → oneshot sender
pub static PENDING_APPROVALS: Lazy<DashMap<String, oneshot::Sender<ApprovalDecision>>> =
    Lazy::new(DashMap::new);

/// Execute an Approve step — blocks until approval, rejection, or timeout
pub async fn execute(
    prompt: &str,
    approvers: &[String],
    timeout_secs: Option<u64>,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    let start = Instant::now();
    let handle_id = pipeline_ctx.handle_id;

    // Interpolate prompt
    let prompt = interpolation::interpolate(prompt, ctx);

    let log = crate::runtime::logger("sentinel");
    log.info(&format!(
        "[{handle_id}] Approve step {index}: waiting for approval — {prompt}"
    ));

    // Create the oneshot channel
    let (tx, rx) = oneshot::channel::<ApprovalDecision>();
    PENDING_APPROVALS.insert(handle_id.to_string(), tx);

    // Emit approval-requested event
    if let Some(bus) = pipeline_ctx.bus {
        bus.publish_async_only(
            &format!("sentinel:{handle_id}:approval-requested"),
            json!({
                "handle": handle_id,
                "stepIndex": index,
                "prompt": prompt,
                "approvers": approvers,
                "timeoutSecs": timeout_secs,
            }),
        );
    }

    // Wait for approval with optional timeout
    let decision = if let Some(secs) = timeout_secs {
        match tokio::time::timeout(Duration::from_secs(secs), rx).await {
            Ok(Ok(decision)) => decision,
            Ok(Err(_)) => {
                // Channel dropped — treat as auto-approve (step removed from pending)
                log.warn(&format!(
                    "[{handle_id}] Approve step {index}: channel dropped, auto-approving"
                ));
                ApprovalDecision {
                    approved: true,
                    reason: Some("Channel dropped — auto-approved".to_string()),
                    approver_id: None,
                }
            }
            Err(_) => {
                // Timeout — auto-approve
                PENDING_APPROVALS.remove(handle_id);
                log.info(&format!(
                    "[{handle_id}] Approve step {index}: timeout after {secs}s — auto-approving"
                ));
                ApprovalDecision {
                    approved: true,
                    reason: Some(format!("Auto-approved after {secs}s timeout")),
                    approver_id: None,
                }
            }
        }
    } else {
        // No timeout — wait indefinitely
        match rx.await {
            Ok(decision) => decision,
            Err(_) => {
                return Err(format!(
                    "[{handle_id}] Approve step {index}: channel closed without decision"
                ));
            }
        }
    };

    // Clean up pending entry (may already be removed by timeout path)
    PENDING_APPROVALS.remove(handle_id);

    let duration_ms = start.elapsed().as_millis() as u64;

    if decision.approved {
        log.info(&format!(
            "[{handle_id}] Approve step {index}: approved by {:?}",
            decision.approver_id
        ));

        // Emit approved event
        if let Some(bus) = pipeline_ctx.bus {
            bus.publish_async_only(
                &format!("sentinel:{handle_id}:approved"),
                json!({
                    "handle": handle_id,
                    "stepIndex": index,
                    "approver": decision.approver_id,
                    "reason": decision.reason,
                }),
            );
        }

        Ok(StepResult {
            step_index: index,
            step_type: "approve".to_string(),
            success: true,
            duration_ms,
            output: decision.reason.clone(),
            error: None,
            exit_code: None,
            data: json!({
                "approved": true,
                "approver": decision.approver_id,
                "reason": decision.reason,
                "waitMs": duration_ms,
            }),
        })
    } else {
        let reason = decision
            .reason
            .unwrap_or_else(|| "Rejected without reason".to_string());
        log.warn(&format!(
            "[{handle_id}] Approve step {index}: rejected — {reason}"
        ));

        // Emit rejected event
        if let Some(bus) = pipeline_ctx.bus {
            bus.publish_async_only(
                &format!("sentinel:{handle_id}:rejected"),
                json!({
                    "handle": handle_id,
                    "stepIndex": index,
                    "approver": decision.approver_id,
                    "reason": reason,
                }),
            );
        }

        Ok(StepResult {
            step_index: index,
            step_type: "approve".to_string(),
            success: false,
            duration_ms,
            output: None,
            error: Some(format!("Approval rejected: {reason}")),
            exit_code: None,
            data: json!({
                "approved": false,
                "approver": decision.approver_id,
                "reason": reason,
                "waitMs": duration_ms,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::sentinel::types::ExecutionContext;
    use crate::runtime::{message_bus::MessageBus, ModuleRegistry};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_approve_immediate_approval() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let handle_id = "test-approve-1";

        let pipeline_ctx = PipelineContext {
            handle_id,
            registry: &registry,
            bus: Some(&bus),
            steps_log_path: None,
        };
        let mut ctx = ExecutionContext::default();

        // Spawn a task that approves after a short delay
        let handle = handle_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if let Some((_, tx)) = PENDING_APPROVALS.remove(&handle) {
                let _ = tx.send(ApprovalDecision {
                    approved: true,
                    reason: Some("Looks good".to_string()),
                    approver_id: Some("human".to_string()),
                });
            }
        });

        let result = execute(
            "Review this",
            &["human".to_string()],
            Some(5),
            0,
            &mut ctx,
            &pipeline_ctx,
        )
        .await
        .unwrap();

        assert!(result.success);
        assert_eq!(result.data["approved"], true);
    }

    #[tokio::test]
    async fn test_approve_timeout_auto_approves() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let handle_id = "test-approve-timeout";

        let pipeline_ctx = PipelineContext {
            handle_id,
            registry: &registry,
            bus: Some(&bus),
            steps_log_path: None,
        };
        let mut ctx = ExecutionContext::default();

        // Don't send any approval — let it timeout (1 second)
        let result = execute(
            "Review this",
            &["human".to_string()],
            Some(1),
            0,
            &mut ctx,
            &pipeline_ctx,
        )
        .await
        .unwrap();

        assert!(result.success);
        assert!(result.data["reason"]
            .as_str()
            .unwrap()
            .contains("Auto-approved"));
    }

    #[tokio::test]
    async fn test_approve_rejection() {
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let handle_id = "test-approve-reject";

        let pipeline_ctx = PipelineContext {
            handle_id,
            registry: &registry,
            bus: Some(&bus),
            steps_log_path: None,
        };
        let mut ctx = ExecutionContext::default();

        let handle = handle_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if let Some((_, tx)) = PENDING_APPROVALS.remove(&handle) {
                let _ = tx.send(ApprovalDecision {
                    approved: false,
                    reason: Some("Not ready yet".to_string()),
                    approver_id: Some("human".to_string()),
                });
            }
        });

        let result = execute(
            "Review this",
            &["human".to_string()],
            Some(5),
            0,
            &mut ctx,
            &pipeline_ctx,
        )
        .await
        .unwrap();

        assert!(!result.success);
        assert_eq!(result.data["approved"], false);
    }
}
