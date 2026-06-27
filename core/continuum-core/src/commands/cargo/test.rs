//! `cargo/test` — run a Rust test suite and return aggregate
//! pass/fail/ignored counts plus failing test names, with compile
//! errors surfaced separately in `build_errors`.

use super::exec::run_test;
use super::types::{CargoTestParams, CargoTestResult};

crate::action_command! {
    /// Run a Rust test suite with cargo and return aggregate pass/fail/ignored
    /// counts plus the names of failing tests. Compile errors that prevent tests
    /// from running surface separately in `build_errors`. Use this to verify code
    /// you just wrote or changed. Optional params: package, filter (libtest name
    /// filter, e.g. "modules::chat::"), features, lib_only, release, working_dir,
    /// timeout_ms.
    pub struct CargoTest;
    name: "cargo/test",
    access: Privileged,
    params: CargoTestParams,
    output: CargoTestResult,
    run(_this, _ctx, p) => {
        Ok(run_test(p).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the command NAME stays mirrored to its path and the
    // model-facing DESCRIPTION fills from the doc comment — the contract the
    // persona tool surface and ACL read. Privileged keeps test (a heavy,
    // process-spawning op) off the unconditional AiSafe surface.
    #[test]
    fn name_access_and_description_are_wired() {
        assert_eq!(CargoTest::NAME, "cargo/test");
        assert!(matches!(
            CargoTest::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
        assert!(CargoTest::DESCRIPTION.contains("Run a Rust test"));
    }
}
