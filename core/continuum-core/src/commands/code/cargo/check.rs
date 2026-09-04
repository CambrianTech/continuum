//! `code/cargo/check` — type-check the caller's Rust workspace and hand back
//! structured compiler diagnostics. This is the persona's primary self-correction
//! hand: write Rust, check, read the errors with their file+line, fix, repeat.

use std::sync::Arc;
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    parse_diagnostics, run_cargo, CargoDiagnostic, DEFAULT_TIMEOUT_SECS, MAX_TIMEOUT_SECS,
};
use crate::commands::code::git::workspace_root_for;
use crate::modules::code::CodeState;

/// Inputs to `code/cargo/check`. All optional — the bare call checks the whole
/// workspace with default features.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/CargoCheckParams.ts"
)]
pub struct CargoCheckParams {
    /// Scope the check to one workspace package (`cargo check -p <package>`), e.g.
    /// `"continuum-core"`. Omit to check the whole workspace (slower).
    #[ts(optional)]
    pub package: Option<String>,
    /// Comma-separated cargo features to enable (`--features <features>`), e.g.
    /// `"metal,accelerate"`. Omit for the crate's default features.
    #[ts(optional)]
    pub features: Option<String>,
    /// Wall-clock budget in seconds (default 180, hard-capped at 1800). A run that
    /// exceeds it is killed and returned with `timed_out: true`.
    #[ts(optional)]
    #[ts(type = "number")]
    pub timeout_secs: Option<u64>,
}

/// Result of a `cargo check` run: the at-a-glance verdict plus every error/warning
/// the compiler emitted, each with its location.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/code/CargoCheckResult.ts"
)]
pub struct CargoCheckResult {
    /// `true` iff cargo exited 0 AND no error diagnostics — "does it compile?".
    pub ok: bool,
    /// Count of error-level diagnostics.
    #[ts(type = "number")]
    pub errors: u32,
    /// Count of warning-level diagnostics.
    #[ts(type = "number")]
    pub warnings: u32,
    /// Every error/warning, each carrying file + line + the rendered caret block.
    pub diagnostics: Vec<CargoDiagnostic>,
    /// `true` if the run was killed by the safety timeout rather than finishing.
    pub timed_out: bool,
    /// Wall-clock duration of the run in milliseconds.
    #[ts(type = "number")]
    pub duration_ms: u64,
}

crate::action_command! {
    /// Type-check the Rust workspace ON DISK you are editing with `cargo check` and
    /// get back structured compiler diagnostics — each error/warning with its file,
    /// line, and rendered message. Use this when you have edited files in a real cargo
    /// project and want to verify the project still compiles. It checks the project in
    /// your working directory — it CANNOT check a standalone function or snippet you
    /// have only written in chat (there is no file for it to compile); for that, use
    /// `code/run`, which compiles and runs the exact code you pass it. Optionally scope
    /// to one package (`package`) and enable features (`features`).
    pub struct CargoCheck { state: Arc<CodeState> }
    name: "code/cargo/check",
    access: AiSafe,
    params: CargoCheckParams,
    output: CargoCheckResult,
    run(this, ctx, p) => {
        let root = workspace_root_for(&this.state, ctx)?;
        let timeout = Duration::from_secs(
            p.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).clamp(1, MAX_TIMEOUT_SECS),
        );

        let mut args = vec!["check".to_string(), "--message-format=json".to_string()];
        if let Some(pkg) = &p.package {
            args.push("-p".to_string());
            args.push(pkg.clone());
        }
        if let Some(features) = &p.features {
            args.push("--features".to_string());
            args.push(features.clone());
        }

        let run = run_cargo(&root, &args, timeout).await?;
        let diagnostics = parse_diagnostics(&run.stdout);
        let errors = diagnostics.iter().filter(|d| d.level == "error").count() as u32;
        let warnings = diagnostics.iter().filter(|d| d.level == "warning").count() as u32;

        Ok(CargoCheckResult {
            ok: run.exit_code == Some(0) && errors == 0 && !run.timed_out,
            errors,
            warnings,
            diagnostics,
            timed_out: run.timed_out,
            duration_ms: run.duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the wire name must mirror the file path so the persona
    // reaches the tool by the name it would guess (`code/cargo/check`).
    #[test]
    fn name_mirrors_path() {
        assert_eq!(CargoCheck::NAME, "code/cargo/check");
    }
}
