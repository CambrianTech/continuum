//! `cargo/build` — compile a Rust workspace/package and return
//! structured diagnostics parsed from cargo's `--message-format=json`
//! stream.

use super::exec::run_build;
use super::types::{CargoBuildParams, CargoBuildResult};

crate::action_command! {
    /// Build a Rust workspace or package with cargo and return structured
    /// errors + warnings (each with file/line span, error code, and rendered
    /// text). Use this to compile code you just wrote and get back machine-
    /// readable diagnostics — the same feedback density a human gets from
    /// `cargo build`. Optional params: package, features (comma-separated),
    /// release, working_dir, timeout_ms.
    pub struct CargoBuild;
    name: "cargo/build",
    access: Privileged,
    params: CargoBuildParams,
    output: CargoBuildResult,
    run(_this, _ctx, p) => {
        Ok(run_build(p).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the command NAME stays mirrored to its path and the
    // model-facing DESCRIPTION fills from the doc comment — the contract the
    // persona tool surface and ACL read. Privileged keeps build (a heavy,
    // process-spawning op) off the unconditional AiSafe surface.
    #[test]
    fn name_access_and_description_are_wired() {
        assert_eq!(CargoBuild::NAME, "cargo/build");
        assert!(matches!(
            CargoBuild::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
        assert!(CargoBuild::DESCRIPTION.contains("Build a Rust"));
    }
}
