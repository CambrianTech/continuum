//! `log/<verb>` — the logger command surface as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! Three dep-holding verbs, all sharing the [`LoggerCommandState`] the
//! [`LoggerModule`](crate::modules::logger::LoggerModule) owns (the queue sender +
//! open-file cache + lifetime counters):
//! - **`log/write`** (Internal) — queue one entry.
//! - **`log/write-batch`** (Internal) — queue many entries in one call.
//! - **`log/ping`** (Privileged) — health snapshot (uptime, counts, pending).
//!
//! Contributed via [`command_objects`] from the module's `commands()`.

use std::sync::Arc;

use crate::modules::logger::LoggerCommandState;
use crate::sdk_codegen::DynCommand;

pub mod ping;
pub mod write;
pub mod write_batch;

/// The dep-holding `log/*` family, each command sharing the logger's state.
pub fn command_objects(state: Arc<LoggerCommandState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(write::LogWrite {
            state: state.clone(),
        }),
        Arc::new(write_batch::LogWriteBatch {
            state: state.clone(),
        }),
        Arc::new(ping::LogPing { state }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the family exposes exactly the three log verbs, each name
    // mirroring its file path under commands/log/ (the path==name invariant), and a
    // guard that command_objects() stays in sync with the files.
    #[test]
    fn family_exposes_all_three_log_verbs() {
        let (state, _rx) = LoggerCommandState::new_for_test();
        let names: Vec<&str> = command_objects(state).iter().map(|o| o.name()).collect();
        assert_eq!(names, vec!["log/write", "log/write-batch", "log/ping"]);
    }
}
