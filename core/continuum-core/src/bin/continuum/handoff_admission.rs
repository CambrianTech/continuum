//! A core handoff's close of in-engine training admission (card 91921d0f): closed before the
//! hold read, so the read is final and no run is admitted between it and the stop; reopened when
//! the handoff does not stop the core. The close itself lives in the running core
//! (`serving/handoff-close`, the engine's lifecycle gate); this is the CLI's half.

use super::*;
use continuum_cli_lifecycle::deploy_hold::AdmissionClose;

const CLOSE: &str = "serving/handoff-close";

/// The close this handoff holds, if any.
#[derive(Default)]
pub(super) struct HandoffAdmission {
    token: std::sync::Mutex<Option<String>>,
}

impl HandoffAdmission {
    /// Close training admission on the running core, owned by this process (its death releases
    /// it). A core that predates the command is `NotSupported`, asked by name through its own
    /// `commands/list`, never inferred from an error's wording.
    pub(super) async fn close(&self) -> AdmissionClose {
        if !running_core_pids().into_iter().any(pid_alive) {
            return AdmissionClose::NoServing;
        }
        let listed = match running_core_call("commands/list", serde_json::json!({ "filter": CLOSE }), "handoff admission close").await {
            Ok(listed) => listed,
            Err(why) => return AdmissionClose::Failed(why),
        };
        // only a well-formed listing that lacks the command is an older core (Codex on #5553)
        let Some(commands) = listed.get("commands").and_then(Value::as_array) else {
            return AdmissionClose::Failed(format!("commands/list answered without a commands array: {listed}"));
        };
        if !commands.iter().any(|c| c.get("name").and_then(Value::as_str) == Some(CLOSE)) {
            return AdmissionClose::NotSupported;
        }
        let reply = match running_core_call(CLOSE, serde_json::json!({ "ownerPid": std::process::id() }), "handoff admission close").await {
            Ok(reply) => reply,
            Err(why) => return AdmissionClose::Failed(why),
        };
        match reply.get("kind").and_then(Value::as_str) {
            Some("closed") => match reply.get("token").and_then(Value::as_str) {
                Some(token) => {
                    if let Ok(mut held) = self.token.lock() {
                        *held = Some(token.to_string());
                    }
                    AdmissionClose::Closed
                }
                None => AdmissionClose::Failed(format!("{CLOSE} answered closed with no token: {reply}")),
            },
            Some("busy") => AdmissionClose::Busy,
            Some("taken") => AdmissionClose::Failed(format!(
                "another handoff (process {}) holds training admission closed",
                reply.get("ownerPid").and_then(Value::as_u64).map_or_else(|| "unknown".to_string(), |pid| pid.to_string())
            )),
            Some("no_serving") => AdmissionClose::NoServing,
            _ => AdmissionClose::Failed(format!("{CLOSE} answered something unknown: {reply}")),
        }
    }

    /// Reopen what this handoff closed, when it closed anything. Said either way; a core that never
    /// held the close (a restarted or restored one boots open) answers that nothing was reopened.
    pub(super) async fn reopen(&self) {
        let token = self.token.lock().ok().and_then(|mut held| held.take());
        let Some(token) = token else { return };
        match running_core_call("serving/handoff-reopen", serde_json::json!({ "token": token }), "handoff admission reopen").await {
            Ok(reply) if reply.get("reopened").and_then(Value::as_bool) == Some(true) => {
                println!("training admission reopened: this handoff did not stop the core");
            }
            Ok(_) => println!("training admission: the core holds no close of this handoff (it was restarted or restored, and boots open)"),
            Err(why) => println!(
                "⚠ training admission could not be reopened ({why}); it reopens when this process exits, since the close belongs to it"
            ),
        }
    }
}
