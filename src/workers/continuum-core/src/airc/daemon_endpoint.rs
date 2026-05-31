//! Local AIRC daemon endpoint derivation (DEPRECATED).
//!
//! **Use [`crate::airc::discover_airc_socket`] instead.** This module's
//! resolver is a stale parallel copy of airc's own scheme — it derives
//! `/tmp/airc-ipc-v<N>-<sha12>.sock` from a hash of the home dir, but
//! the airc daemon binds `~/.airc/runtime/airc-machine-<account-hash>
//! -v<N>.sock` under its actual resolution rules. The two never match,
//! which broke headless continuum-core boot (`AIRC daemon attach
//! stream stopped: daemon not reachable: ENOENT`).
//!
//! Fixed by asking airc directly (`airc ipc-endpoint`, landed in
//! airc#1095) rather than re-deriving — see [`crate::airc::discovery`]
//! module docs for the decoupling rationale. This file is kept only so
//! existing callers compile while their imports migrate to
//! `discover_airc_socket`; delete once all call sites are switched.

use std::path::{Path, PathBuf};

/// Default daemon IPC endpoint for an AIRC home (DEPRECATED).
///
/// **DO NOT USE for runtime attach** — this derivation does not match
/// what the airc daemon actually binds (see module-level doc). Use
/// [`crate::airc::discover_airc_socket`] for live attach paths.
#[deprecated(
    since = "0.1.0",
    note = "Derivation drifts from airc's own resolver — use `crate::airc::discover_airc_socket` which asks airc via `airc ipc-endpoint` (airc#1095). Delete this function once `AircModule::with_daemon_home` and `src/workers/continuum-core/src/modules/airc_runtime_e2e_tests.rs` migrate off it (only two remaining callers as of this PR)."
)]
pub fn default_socket_path_in(home: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        use sha2::{Digest, Sha256};

        let canonical = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
        let mut hasher = Sha256::new();
        hasher.update(airc_ipc::IPC_PROTOCOL_VERSION.to_be_bytes());
        hasher.update(canonical.to_string_lossy().as_bytes());
        let digest = hasher.finalize();
        let hex = digest
            .iter()
            .take(12)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();

        std::env::temp_dir().join(format!(
            "airc-ipc-v{}-{hex}.sock",
            airc_ipc::IPC_PROTOCOL_VERSION
        ))
    }

    #[cfg(not(unix))]
    {
        home.join(format!("daemon-v{}.sock", airc_ipc::IPC_PROTOCOL_VERSION))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_is_protocol_versioned() {
        let path = default_socket_path_in(Path::new("/tmp/continuum-airc-home"));
        let rendered = path.to_string_lossy();
        assert!(
            rendered.contains(&format!("v{}", airc_ipc::IPC_PROTOCOL_VERSION)),
            "socket path must carry IPC protocol version: {rendered}"
        );
    }
}
