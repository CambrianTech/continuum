//! Classify installed engine images without reaping control clients.
use std::path::Path;

pub(crate) fn owned_engine_candidate(
    executable: &Path,
    owned_root: &Path,
    pid: u32,
    caller: u32,
) -> bool {
    // Installed release slots contain the control client as well as engines.
    // Reaping that client kills the reboot command before it can start the core.
    // Other active clients are not engine orphans either, regardless of parentage.
    pid != caller
        && executable.starts_with(owned_root)
        && !executable.file_name().is_some_and(|name| {
            let name = name.to_string_lossy();
            name.eq_ignore_ascii_case("continuum") || name.eq_ignore_ascii_case("continuum.exe")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    // Regression for #4056: an installed CLI must survive its own orphan sweep,
    // while abandoned engines remain eligible for the existing parentage check.
    #[test]
    fn installed_control_clients_are_not_engine_orphans() {
        let root = Path::new("installed/bin");
        for name in [
            "service-a/continuum.exe",
            "service-b/CONTINUUM.EXE",
            "continuum",
        ] {
            assert!(!owned_engine_candidate(&root.join(name), root, 42, 42));
            assert!(!owned_engine_candidate(&root.join(name), root, 43, 42));
        }
        let engine = root.join("engine-a/llama-server.exe");
        assert!(owned_engine_candidate(&engine, root, 43, 42));
        assert!(
            !owned_engine_candidate(&engine, root, 42, 42),
            "caller is protected even if renamed"
        );
        assert!(!owned_engine_candidate(
            Path::new("other/bin/llama-server.exe"),
            root,
            43,
            42
        ));
    }
}
