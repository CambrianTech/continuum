//! Grid connection handling — accept loop and incoming request processing.
//!
//! Handles incoming connections from remote Grid nodes:
//! - Request frames: execute command locally, send response
//! - Response frames: resolve pending outbound correlations
//! - Event frames: forward to local message bus
//! - Stream frames: (future) large data transfer

use super::acl::is_command_authorized;
use super::audit::{AuditDirection, AuditEntry, AuditOutcome};
use super::frame::{self, FrameType, GridFrame, GridPayload};
use super::node::TrustLevel;
use super::transport::{GridTransport, TransportError};
use super::GridState;
use crate::runtime::CommandResult;
use std::sync::Arc;
use std::time::Duration;

/// Accept loop — runs forever, accepting incoming connections on a transport.
/// Each connection is handled in its own spawned task.
pub async fn accept_loop(transport: Arc<dyn GridTransport>, state: Arc<GridState>) {
    loop {
        match transport.accept().await {
            Ok(conn) => {
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(conn, state).await {
                        eprintln!("[grid] Incoming connection error: {e}");
                    }
                });
            }
            Err(TransportError::NotReady(_)) => {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Err(e) => {
                eprintln!("[grid] Accept error: {e}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

/// Handle a single incoming connection — read frames in a loop.
async fn handle_connection(
    conn: Box<dyn super::transport::GridConnection>,
    state: Arc<GridState>,
) -> Result<(), String> {
    loop {
        let frame = match conn.recv_frame().await {
            Ok(f) => f,
            Err(TransportError::IoError(e)) if e.contains("eof") || e.contains("closed") => {
                return Ok(());
            }
            Err(e) => return Err(format!("Recv error: {e}")),
        };

        match frame.frame_type {
            FrameType::Request => {
                let response = execute_incoming_request(&frame, &state).await;
                if let Err(e) = conn.send_frame(&response).await {
                    eprintln!("[grid] Failed to send response: {e}");
                }
            }
            FrameType::Response => {
                if let Some((_, sender)) = state.pending.remove(&frame.correlation_id) {
                    let _ = sender.send(frame);
                }
            }
            FrameType::Event => {
                if let GridPayload::Event {
                    ref event,
                    ref data,
                } = frame.payload
                {
                    if let Some(bus) = state.bus.lock().await.as_ref() {
                        bus.publish_async_only(event, data.clone());
                    }
                }
            }
            FrameType::Stream => {
                // TODO: Stream chunk handling for large data transfers
            }
        }
    }
}

/// Execute an incoming command request from a remote node.
/// Performs ACL check, executes via module registry, logs audit entry.
async fn execute_incoming_request(request: &GridFrame, state: &Arc<GridState>) -> GridFrame {
    let (command, params) = match &request.payload {
        GridPayload::Command { command, params } => (command.as_str(), params.clone()),
        _ => {
            return GridFrame::error_response(request, "Expected command payload".into());
        }
    };

    // Look up the requesting node's trust level.
    // source_node may include port (e.g., "100.1.2.3:7117") — strip it for registry lookup.
    let source_ip = request
        .source_node
        .split(':')
        .next()
        .unwrap_or(&request.source_node);
    let trust = state
        .registry
        .get(source_ip)
        .map(|n| n.trust_level)
        .unwrap_or(TrustLevel::Blocked);

    // ACL check
    if !is_command_authorized(command, trust) {
        let _ = state
            .audit
            .log(&AuditEntry {
                timestamp: frame::now_millis(),
                direction: AuditDirection::Inbound,
                remote_node: request.source_node.clone(),
                command: command.to_string(),
                correlation_id: request.correlation_id.clone(),
                outcome: AuditOutcome::Denied,
                duration_ms: 0,
            })
            .await;

        return GridFrame::error_response(
            request,
            format!("Command '{command}' denied for trust level '{trust:?}'"),
        );
    }

    let start = std::time::Instant::now();

    // Execute the command locally, in the Rust module registry. There is no
    // second tier: an unclaimed command is an error, not a hop to another
    // runtime. The `executor` lock that used to be taken here — solely to hold
    // the TS bridge handle — is gone with it; acquiring a lock for a branch
    // that no longer exists is both a dead cost and a standing invitation to
    // re-add the fallback.
    let result = if let Some(registry) = state.runtime_registry.lock().await.as_ref() {
        if let Some(result) = registry.route_command(command) {
            // Command matched a Rust module prefix — try Rust handler first
            let (module, full_cmd) = result;
            match module.handle_command(&full_cmd, params.clone()).await {
                Ok(cmd_result) => match cmd_result.to_json_value() {
                    Ok(value) => GridFrame::success_response(request, value),
                    Err(e) => GridFrame::error_response(request, e),
                },
                Err(e) if e.starts_with("Unknown") => {
                    // NO TS FALLBACK. This used to fall through to
                    // `execute_ts_json`, justified by "grid/node-status,
                    // grid/job-submit live in TS, not Rust". That justification
                    // is stale — all four commands this branch named are now
                    // Rust (`ai/generate` alone has 62 references), so the
                    // fallthrough served nothing except a standing Node
                    // dependency in the p2p path.
                    //
                    // It was also a FALLBACK, which is banned outright
                    // ([[no-fallbacks-ever]], "fallbacks are cancer"): the
                    // executor refused exactly this implicit fallthrough in
                    // PR #1585, and grid re-introduced it as an explicit one.
                    // A remote peer asking for a command we do not have must
                    // get a typed refusal that NAMES it, not a silent hop into
                    // a runtime that may not be installed — which on Windows
                    // could not even fail informatively, because the socket
                    // path was hardcoded to /tmp.
                    GridFrame::error_response(
                        request,
                        format!(
                            "unknown command `{command}` — no Rust module handles it \
                             (underlying: {e})"
                        ),
                    )
                }
                Err(e) => GridFrame::error_response(request, e),
            }
        } else {
            // NO TS FALLBACK — see the sibling branch above. This one claimed
            // to handle "genome/train, ai/generate, and other TS-only
            // commands"; genome/train has 11 Rust references and ai/generate
            // has 62. Nothing TS-only remains behind it.
            //
            // The engine is Rust. A grid peer's request that no Rust module
            // claims is an unknown command, and saying so is the honest
            // answer — quietly shipping it to a Node process is how the
            // boundary eroded the second time.
            GridFrame::error_response(
                request,
                format!("unknown command `{command}` — no Rust module claims this prefix"),
            )
        }
    } else {
        GridFrame::error_response(request, "Module registry not available".into())
    };

    let duration_ms = start.elapsed().as_millis() as u64;
    let outcome = match &result.payload {
        GridPayload::CommandResult { success: true, .. } => AuditOutcome::Success,
        _ => AuditOutcome::Error,
    };

    let _ = state
        .audit
        .log(&AuditEntry {
            timestamp: frame::now_millis(),
            direction: AuditDirection::Inbound,
            remote_node: request.source_node.clone(),
            command: command.to_string(),
            correlation_id: request.correlation_id.clone(),
            outcome,
            duration_ms,
        })
        .await;

    result
}
