//! Typed params + result for the cargo module's commands.
//!
//! Every wire type carries `#[derive(TS)]` and exports to
//! `protocol/typescript/cargo/` so TS consumers get auto-generated
//! bindings — no hand-written duplicate types across the
//! Rust ↔ TS boundary.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ── cargo/build ──────────────────────────────────────────────────────

/// Params for `cargo/build`.
///
/// All fields optional. With no params, runs `cargo build` at the
/// process cwd in debug mode. Typical persona usage:
/// `{ package: "continuum-core", features: "metal,accelerate" }`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cargo/CargoBuildParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct CargoBuildParams {
    /// Workspace package to build (cargo's `--package` flag).
    /// Omit to build the whole workspace.
    #[serde(default)]
    #[ts(optional)]
    pub package: Option<String>,

    /// Cargo features, comma-separated (cargo's `--features` flag).
    /// e.g. `"metal,accelerate"`.
    #[serde(default)]
    #[ts(optional)]
    pub features: Option<String>,

    /// Build in release mode (`--release`). Default: false.
    #[serde(default)]
    pub release: bool,

    /// Working directory to run cargo in. Default: process cwd.
    /// Must be a path the substrate is allowed to invoke cargo
    /// within — typically the continuum-core workspace root or a
    /// persona-managed worktree.
    #[serde(default)]
    #[ts(optional)]
    pub working_dir: Option<String>,

    /// Max wall-clock for the entire cargo invocation in
    /// milliseconds. Default: 300_000 (5 minutes). The substrate
    /// caps this at 900_000 (15 minutes); higher values are
    /// silently clamped.
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub timeout_ms: Option<u64>,
}

/// Result of `cargo/build`. Structured errors + warnings parsed from
/// cargo's `--message-format=json` output stream.
///
/// `errors.len() == 0 && success == true` is the happy path. If
/// `success == false` but `errors.is_empty()`, something killed
/// cargo (timeout, signal, IPC error) — see `error` for details.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cargo/CargoBuildResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct CargoBuildResult {
    pub success: bool,
    pub errors: Vec<CargoMessage>,
    pub warnings: Vec<CargoMessage>,
    /// Cargo's exit code (None on timeout / signal / spawn failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub exit_code: Option<i32>,
    #[ts(type = "number")]
    pub duration_ms: u64,
    /// Substrate-level error (timeout, spawn failure, etc.). When
    /// set, the cargo run didn't complete normally — `errors` may
    /// be empty even though `success == false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// One compiler diagnostic from cargo's JSON output stream. Mirrors
/// rustc's diagnostic shape, flattened for the wire.
///
/// Per cargo's stable `--message-format=json` contract — when
/// cargo's output shape changes, this struct's parser updates with
/// it but the wire shape here stays stable for TS consumers.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cargo/CargoMessage.ts")]
#[serde(rename_all = "camelCase")]
pub struct CargoMessage {
    /// `"error"`, `"warning"`, `"note"`, `"help"`.
    pub level: String,
    pub message: String,
    /// Rust error code (e.g. `"E0382"`), when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub code: Option<String>,
    /// Primary span: the location the diagnostic anchors to. Absent
    /// for diagnostics that don't have a single anchor (e.g.
    /// linker errors).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub primary_span: Option<CargoSpan>,
    /// Help text or rendered suggestions from rustc, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rendered: Option<String>,
}

/// File location of a compiler diagnostic span. 1-indexed lines +
/// columns, matching rustc's convention.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cargo/CargoSpan.ts")]
#[serde(rename_all = "camelCase")]
pub struct CargoSpan {
    /// File path relative to the cargo invocation's working dir.
    pub file_name: String,
    pub line_start: u32,
    pub line_end: u32,
    pub column_start: u32,
    pub column_end: u32,
}

// ── cargo/test ───────────────────────────────────────────────────────

/// Params for `cargo/test`.
///
/// All fields optional. With no params, runs `cargo test` at the
/// process cwd in debug mode against the whole workspace. Typical
/// persona usage when iterating: `{ package: "continuum-core",
/// filter: "modules::chat::", features: "metal,accelerate" }`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cargo/CargoTestParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct CargoTestParams {
    /// Workspace package to test (cargo's `--package` flag).
    #[serde(default)]
    #[ts(optional)]
    pub package: Option<String>,

    /// Test name filter passed to libtest after `--` (e.g.
    /// `"modules::chat::"` to run all chat module tests).
    #[serde(default)]
    #[ts(optional)]
    pub filter: Option<String>,

    /// Cargo features (cargo's `--features` flag).
    #[serde(default)]
    #[ts(optional)]
    pub features: Option<String>,

    /// `--lib` flag — restrict to library tests, skip integration
    /// tests. Default: false (run everything).
    #[serde(default)]
    pub lib_only: bool,

    /// Build + run in release mode.
    #[serde(default)]
    pub release: bool,

    /// Working directory. Default: process cwd.
    #[serde(default)]
    #[ts(optional)]
    pub working_dir: Option<String>,

    /// Max wall-clock in milliseconds. Default: 600_000 (10
    /// minutes). Capped at 1_800_000 (30 minutes).
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub timeout_ms: Option<u64>,
}

/// Result of `cargo/test`. Aggregate counts + structured failures
/// parsed from cargo + libtest's human-readable output.
///
/// `success` reflects libtest's overall verdict (compiles + zero
/// failed tests). Build errors that prevent any tests from running
/// surface in `build_errors` (mirrors `CargoBuildResult.errors`).
/// Per-test failures surface in `failures`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cargo/CargoTestResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct CargoTestResult {
    pub success: bool,
    #[ts(type = "number")]
    pub passed: u32,
    #[ts(type = "number")]
    pub failed: u32,
    #[ts(type = "number")]
    pub ignored: u32,
    #[ts(type = "number")]
    pub measured: u32,
    /// Names of failing tests, in the order libtest reported them.
    /// Empty when all tests passed.
    pub failures: Vec<String>,
    /// Build-time errors that prevented tests from compiling. When
    /// non-empty, `passed/failed/ignored/measured` are all 0 and
    /// `success` is false.
    pub build_errors: Vec<CargoMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub exit_code: Option<i32>,
    #[ts(type = "number")]
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// Substrate clamps for timeout (build / test).
pub const BUILD_DEFAULT_TIMEOUT_MS: u64 = 300_000; // 5 min
pub const BUILD_MAX_TIMEOUT_MS: u64 = 900_000; // 15 min
pub const TEST_DEFAULT_TIMEOUT_MS: u64 = 600_000; // 10 min
pub const TEST_MAX_TIMEOUT_MS: u64 = 1_800_000; // 30 min

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_params_round_trip_camel_case() {
        let raw = json!({
            "package": "continuum-core",
            "features": "metal,accelerate",
            "release": true,
            "workingDir": "/tmp/workspace",
            "timeoutMs": 60000,
        });
        let parsed: CargoBuildParams = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(parsed.package.as_deref(), Some("continuum-core"));
        assert_eq!(parsed.features.as_deref(), Some("metal,accelerate"));
        assert!(parsed.release);
        assert_eq!(parsed.working_dir.as_deref(), Some("/tmp/workspace"));
        assert_eq!(parsed.timeout_ms, Some(60000));

        let back = serde_json::to_value(&parsed).unwrap();
        assert_eq!(back["workingDir"], raw["workingDir"]);
        assert_eq!(back["timeoutMs"], raw["timeoutMs"]);
    }

    #[test]
    fn build_params_defaults_when_omitted() {
        let parsed: CargoBuildParams = serde_json::from_value(json!({})).unwrap();
        assert!(parsed.package.is_none());
        assert!(parsed.features.is_none());
        assert!(!parsed.release, "release defaults to false");
        assert!(parsed.working_dir.is_none());
        assert!(parsed.timeout_ms.is_none());
    }

    #[test]
    fn build_result_omits_optional_fields_when_none() {
        let r = CargoBuildResult {
            success: true,
            errors: vec![],
            warnings: vec![],
            exit_code: None,
            duration_ms: 1234,
            error: None,
        };
        let val = serde_json::to_value(&r).unwrap();
        let map = val.as_object().unwrap();
        assert!(!map.contains_key("exitCode"), "missing != null on wire");
        assert!(!map.contains_key("error"));
    }

    #[test]
    fn test_params_lib_only_flag_round_trips() {
        let raw = json!({ "libOnly": true });
        let parsed: CargoTestParams = serde_json::from_value(raw).unwrap();
        assert!(parsed.lib_only);
    }

    #[test]
    fn test_result_failures_preserved_in_order() {
        let r = CargoTestResult {
            success: false,
            passed: 5,
            failed: 2,
            ignored: 0,
            measured: 0,
            failures: vec!["modules::chat::test_a".into(), "modules::chat::test_b".into()],
            build_errors: vec![],
            exit_code: Some(101),
            duration_ms: 5000,
            error: None,
        };
        let val = serde_json::to_value(&r).unwrap();
        let failures = val["failures"].as_array().unwrap();
        assert_eq!(failures[0], "modules::chat::test_a");
        assert_eq!(failures[1], "modules::chat::test_b");
    }
}
