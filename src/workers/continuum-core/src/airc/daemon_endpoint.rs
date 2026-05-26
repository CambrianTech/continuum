//! Local AIRC daemon endpoint derivation.

use std::path::{Path, PathBuf};

/// Default daemon IPC endpoint for an AIRC home.
///
/// The path is versioned by `airc_ipc::IPC_PROTOCOL_VERSION` so a client
/// cannot accidentally talk to a daemon speaking an older ABI.
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
