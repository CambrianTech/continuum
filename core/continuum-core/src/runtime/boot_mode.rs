//! `BootMode` — explicit operator-intent flag for substrate boot.
//!
//! ## Why
//!
//! Slice A (PR #1527) used a heuristic: "if persona seeds exist on
//! disk, assume the operator wants persona hosting; if not, assume
//! they want inference-only." Reviewer R2 BLOCKed on
//! [[no-fallbacks-ever]]: the operator NEVER stated they wanted
//! inference-only mode; the substrate inferred it from a directory
//! listing. That's silent substitution of a degraded capability set.
//!
//! A.2 replaces the heuristic with an explicit flag. The operator
//! states intent via `--mode=<full-citizen|inference-only|fail-fast>`
//! (default `full-citizen`). The substrate never guesses.
//!
//! ## The three modes
//!
//! - **FullCitizen** (default) — the substrate is expected to host
//!   personas + run inference + everything else. AIRC must be
//!   `Healthy` AND ≥1 persona seed must exist. Either missing →
//!   refuse boot with the actionable repair message. This is the
//!   common case for an operator running `continuum-core-server`
//!   on their machine.
//!
//! - **InferenceOnly** — the operator explicitly wants the substrate
//!   to come up WITHOUT persona hosting (e.g. a forge worker that
//!   only runs `inference/`, `embedding/`, `forge/`, `cargo/`,
//!   `code/`; a sandboxed CI runner; a model evaluation harness).
//!   AIRC may be degraded; persona seeds may be absent. No
//!   `PersonaInstanceManagerModule` is registered (or required).
//!
//! - **FailFast** — strictest mode. Refuses to boot if ANY
//!   substrate capability is unavailable: AIRC unhealthy, ORT
//!   missing, model files missing. Used by CI smoke tests and
//!   production rollouts that want loud failure over silent
//!   degradation.
//!
//! ## Threaded through `Context`
//!
//! Like `AircDiscovery`, `BootMode` is added to the `Context` trait
//! (`fn boot_mode(&self) -> BootMode`) so every actor created
//! during this boot carries the mode that was true at creation.
//! Future B' code paths can dispatch on mode without re-reading
//! argv.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    /// Default. Substrate is expected to host personas + run
    /// inference + everything else. Requires AIRC `Healthy` AND
    /// ≥1 persona seed; refuses boot otherwise.
    FullCitizen,
    /// Inference / forge / code only. No persona hosting, no AIRC
    /// requirement, no seed requirement. Operator opted in
    /// explicitly.
    InferenceOnly,
    /// Strictest. Refuses any degradation, including missing
    /// libonnxruntime or model files. Used for CI smoke + prod.
    FailFast,
}

impl BootMode {
    /// Human-readable label for log lines + boot banner.
    pub fn label(&self) -> &'static str {
        match self {
            BootMode::FullCitizen => "full-citizen",
            BootMode::InferenceOnly => "inference-only",
            BootMode::FailFast => "fail-fast",
        }
    }

    /// `true` iff persona hosting MUST be available in this mode.
    /// Used by `verify_registration` to compute the required-module
    /// set and by `start_server` to decide whether seeds-absent
    /// is a hard error.
    pub fn requires_persona_hosting(&self) -> bool {
        matches!(self, BootMode::FullCitizen | BootMode::FailFast)
    }

    /// `true` iff voice subsystem MUST be available. Only `FailFast`
    /// raises this bar — `FullCitizen` is OK to boot without ORT
    /// (operator gets a `🔇 Voice subsystem: unavailable` line at
    /// the boot banner, can install libonnxruntime later).
    pub fn requires_voice(&self) -> bool {
        matches!(self, BootMode::FailFast)
    }
}

impl Default for BootMode {
    fn default() -> Self {
        BootMode::FullCitizen
    }
}

impl FromStr for BootMode {
    type Err = BootModeParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "full-citizen" | "full" | "citizen" => Ok(BootMode::FullCitizen),
            "inference-only" | "inference" => Ok(BootMode::InferenceOnly),
            "fail-fast" | "strict" => Ok(BootMode::FailFast),
            other => Err(BootModeParseError(other.to_string())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown --mode={0:?} — valid: full-citizen (default), inference-only, fail-fast")]
pub struct BootModeParseError(pub String);

/// Parse `--mode=<value>` out of an argv vector. Removes the
/// matched argument(s) so the caller's positional parsing (e.g.
/// the socket path) sees a clean slice. `--mode VALUE` (space form)
/// and `--mode=VALUE` (equals form) both supported.
///
/// Returns the parsed `BootMode` (default if absent) + a vector
/// of remaining args.
pub fn extract_boot_mode(args: Vec<String>) -> Result<(BootMode, Vec<String>), BootModeParseError> {
    let mut mode = BootMode::default();
    let mut rest = Vec::with_capacity(args.len());
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--mode=") {
            mode = value.parse()?;
            continue;
        }
        if arg == "--mode" {
            let value = iter.next().ok_or_else(|| {
                BootModeParseError("--mode requires a value (e.g. --mode=full-citizen)".into())
            })?;
            mode = value.parse()?;
            continue;
        }
        rest.push(arg);
    }
    Ok((mode, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_full_citizen() {
        assert_eq!(BootMode::default(), BootMode::FullCitizen);
    }

    #[test]
    fn full_citizen_requires_persona_hosting() {
        assert!(BootMode::FullCitizen.requires_persona_hosting());
        assert!(!BootMode::FullCitizen.requires_voice());
    }

    #[test]
    fn inference_only_does_not_require_persona_hosting() {
        assert!(!BootMode::InferenceOnly.requires_persona_hosting());
        assert!(!BootMode::InferenceOnly.requires_voice());
    }

    #[test]
    fn fail_fast_requires_everything() {
        assert!(BootMode::FailFast.requires_persona_hosting());
        assert!(BootMode::FailFast.requires_voice());
    }

    #[test]
    fn parse_canonical_forms() {
        assert_eq!(
            "full-citizen".parse::<BootMode>().unwrap(),
            BootMode::FullCitizen
        );
        assert_eq!(
            "inference-only".parse::<BootMode>().unwrap(),
            BootMode::InferenceOnly
        );
        assert_eq!("fail-fast".parse::<BootMode>().unwrap(), BootMode::FailFast);
    }

    #[test]
    fn parse_aliases() {
        assert_eq!("full".parse::<BootMode>().unwrap(), BootMode::FullCitizen);
        assert_eq!(
            "citizen".parse::<BootMode>().unwrap(),
            BootMode::FullCitizen
        );
        assert_eq!(
            "inference".parse::<BootMode>().unwrap(),
            BootMode::InferenceOnly
        );
        assert_eq!("strict".parse::<BootMode>().unwrap(), BootMode::FailFast);
    }

    #[test]
    fn parse_is_case_insensitive_and_trims() {
        assert_eq!(
            "  Full-Citizen  ".parse::<BootMode>().unwrap(),
            BootMode::FullCitizen
        );
        assert_eq!(
            "INFERENCE-ONLY".parse::<BootMode>().unwrap(),
            BootMode::InferenceOnly
        );
    }

    #[test]
    fn parse_unknown_errors_with_actionable_message() {
        let err = "persona-host".parse::<BootMode>().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("persona-host"));
        assert!(msg.contains("full-citizen"));
        assert!(msg.contains("inference-only"));
        assert!(msg.contains("fail-fast"));
    }

    #[test]
    fn extract_boot_mode_equals_form() {
        let args = vec![
            "continuum-core-server".into(),
            "--mode=inference-only".into(),
            "/tmp/x.sock".into(),
        ];
        let (mode, rest) = extract_boot_mode(args).unwrap();
        assert_eq!(mode, BootMode::InferenceOnly);
        assert_eq!(rest, vec!["continuum-core-server", "/tmp/x.sock"]);
    }

    #[test]
    fn extract_boot_mode_space_form() {
        let args = vec![
            "continuum-core-server".into(),
            "--mode".into(),
            "fail-fast".into(),
            "/tmp/x.sock".into(),
        ];
        let (mode, rest) = extract_boot_mode(args).unwrap();
        assert_eq!(mode, BootMode::FailFast);
        assert_eq!(rest, vec!["continuum-core-server", "/tmp/x.sock"]);
    }

    #[test]
    fn extract_boot_mode_absent_returns_default() {
        let args = vec!["continuum-core-server".into(), "/tmp/x.sock".into()];
        let (mode, rest) = extract_boot_mode(args).unwrap();
        assert_eq!(mode, BootMode::FullCitizen);
        assert_eq!(rest, vec!["continuum-core-server", "/tmp/x.sock"]);
    }

    #[test]
    fn extract_boot_mode_dangling_space_form_errors() {
        let args = vec!["continuum-core-server".into(), "--mode".into()];
        let err = extract_boot_mode(args).unwrap_err();
        assert!(format!("{err}").contains("requires a value"));
    }
}
