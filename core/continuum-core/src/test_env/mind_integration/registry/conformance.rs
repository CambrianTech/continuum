#![cfg(test)]
//! From `cognition::tool_executor::command_executor` (its `behavioral_conformance` tests),
//! moved where core's commands link: the stateless surface these exams sweep is the
//! registry core's test binary sees, never the fraction a mind-only binary links.
//!
//! Behavioral conformance exams (#163 Slice 2) — the DISPATCH half of the tool
//! AI-usability harness. The static audit (`sdk_codegen::conformance`) proves a
//! tool is discoverable + learnable; these prove it BEHAVES when a model fumbles
//! the call: a mangled/unknown name fails LOUD (never a silent no-op that forges
//! a receipt, #159), and a real command given the wrong args fails LOUD with the
//! fix inline. Run over the REAL self-contained command surface: `ModuleRegistry
//! ::new()` auto-seeds every stateless command object (no module wiring, no live
//! state, no daemon), so this exercises production commands through the
//! production persona-hands path, safely and deterministically.

use serde_json::json;

use crate::ai::types::ToolCall as NativeToolCall;
use crate::cognition::tool_executor::command_executor::test_support::*;
use crate::cognition::tool_executor::ToolExecutor;

use crate::sdk_codegen::{command_registry, stateless_command_objects, AccessLevel};
use std::collections::HashSet;

// ─────── the executor→receipt path for a projected outcome ───────
//
// Astra on #4352: "Projector-only tests cannot catch the current Running
// regression at the consumer." Constructing an `Observation` by hand proves
// the MAPPING; it proves nothing about the WIRING. These dispatch a real
// command through the real `execute_native_batch` and read what comes back.
//
// The command is declared HERE rather than reusing `code/run`, because
// `code/run` spawns a real interpreter and a CI box that lacks one would
// make this test a coin toss.

#[derive(Debug, Default, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
pub struct VerdictProbeParams {
    /// "ok" | "fail" | "running" | "garbage"
    pub mode: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct VerdictProbeResult {
    pub ok: bool,
    pub running: bool,
    /// Preserved verbatim through the fold — the feedback the model needs.
    pub stderr: String,
    pub handle: Option<String>,
}

#[derive(Default)]
pub struct VerdictProbe;

#[async_trait::async_trait]
impl crate::sdk_codegen::ActionCommand for VerdictProbe {
    const NAME: &'static str = "test/verdict-probe";
    const ALIASES: &'static [&'static str] = &["probe_alias"];
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str = "test-only: returns a chosen outcome shape.";
    type Params = VerdictProbeParams;
    type Output = VerdictProbeResult;

    async fn run(
        &self,
        _ctx: &crate::sdk_codegen::Ctx,
        p: VerdictProbeParams,
    ) -> Result<VerdictProbeResult, crate::sdk_codegen::CommandError> {
        Ok(VerdictProbeResult {
            ok: p.mode == "ok",
            running: p.mode == "running",
            stderr: if p.mode == "fail" {
                "error[E0425]: cannot find value `x`".into()
            } else {
                String::new()
            },
            handle: (p.mode == "running")
                .then(|| "6f1a0f5e-0000-4000-8000-000000000001".to_string()),
        })
    }
}

impl crate::sdk_codegen::ProjectsOutcome for VerdictProbe {
    fn outcome(o: &VerdictProbeResult) -> crate::sdk_codegen::ToolVerdict {
        if o.running {
            crate::sdk_codegen::ToolVerdict::Running
        } else if o.ok {
            crate::sdk_codegen::ToolVerdict::Succeeded
        } else {
            crate::sdk_codegen::ToolVerdict::Failed
        }
    }
    fn dispatch_handle(o: &VerdictProbeResult) -> Option<uuid::Uuid> {
        o.handle.as_deref().and_then(|h| uuid::Uuid::parse_str(h).ok())
    }
}

crate::register_stateless_command!(VerdictProbe);
crate::register_outcome!(VerdictProbe);

/// Dispatch through the REAL batch path and hand back the receipt plus the
/// typed carrier, correlated the way the act seam correlates them.
async fn dispatch_verdict(
    mode: &str,
    name: &str,
) -> (
    Option<bool>,
    String,
    crate::sdk_codegen::ActVerdict,
    Option<uuid::Uuid>,
) {
    let exec = stateless_surface_hands();
    let calls = vec![NativeToolCall {
        id: "vp".to_string(),
        name: name.to_string(),
        input: json!({ "mode": mode }),
    }];
    let out = exec
        .execute_native_batch(&calls, &ctx())
        .await
        .expect("batch itself succeeds");
    let r = out
        .results
        .iter()
        .find(|r| r.tool_use_id == "vp")
        .expect("result correlates by tool_use_id");
    let v = out.verdicts.iter().find(|v| v.tool_use_id == "vp");
    (
        r.is_error,
        r.content.clone(),
        v.map(|v| v.verdict).unwrap_or_default(), // JUSTIFIED: absence means unprojected, which is the correct default
        v.and_then(|v| v.dispatch_handle),
    )
}

/// what this catches: THE defect, end to end. A command that reports its own
/// failure AS DATA takes the executor's `Ok` arm — before #4352 that arm set
/// `is_error: None` unconditionally and the room rendered a tick.
#[tokio::test]
async fn a_failure_returned_as_data_comes_back_flagged_and_keeps_its_stderr() {
    let (is_error, content, verdict, handle) =
        dispatch_verdict("fail", "test/verdict-probe").await;
    assert_eq!(is_error, Some(true), "the command said it failed: {content}");
    assert_eq!(
        verdict,
        crate::sdk_codegen::ActVerdict::Declared(crate::sdk_codegen::ToolVerdict::Failed)
    );
    assert!(
        content.contains("E0425"),
        "the stderr the model self-corrects from survives the fold: {content}"
    );
    assert_eq!(handle, None);
}

/// what this catches: Running flattened at the executor. `is_error` stays
/// None (not-finished is not failed) but the TYPED carrier must say running,
/// and the command-declared handle must ride along — that handle is what
/// registers the dispatch so the completion folds back into working memory.
#[tokio::test]
async fn a_running_call_carries_running_and_its_handle_not_an_error() {
    let (is_error, _c, verdict, handle) =
        dispatch_verdict("running", "test/verdict-probe").await;
    assert_eq!(is_error, None, "accepted is not failed");
    assert_eq!(
        verdict,
        crate::sdk_codegen::ActVerdict::Declared(crate::sdk_codegen::ToolVerdict::Running)
    );
    assert_eq!(
        handle,
        Some(uuid::Uuid::parse_str("6f1a0f5e-0000-4000-8000-000000000001").unwrap()), // JUSTIFIED: a literal the test itself authored
        "the command declares its handle; the seam no longer re-parses folded text"
    );
}

/// what this catches: the alias miss. The projector is keyed on the CANONICAL
/// name, and a model reaching for a declared alias must still get its own
/// command's verdict — otherwise the fix is dead for exactly the calls that
/// hit it most (`bash` for `code/shell`).
#[tokio::test]
async fn a_call_made_through_a_declared_alias_still_gets_its_verdict() {
    let (is_error, _c, verdict, _h) = dispatch_verdict("fail", "probe_alias").await;
    assert_eq!(is_error, Some(true), "the alias must reach the projector");
    assert_eq!(
        verdict,
        crate::sdk_codegen::ActVerdict::Declared(crate::sdk_codegen::ToolVerdict::Failed)
    );
}

// what this catches (#159): an unknown/mangled tool NAME never silently
// no-ops — it comes back as an ERROR result naming what she tried and
// pointing at discovery, so she recovers instead of narrating a fake
// receipt. The exact "write_file / list_files" snake-case vocabulary gap.
#[tokio::test]
async fn unknown_tool_name_fails_loud_never_silent() {
    let exec = stateless_surface_hands();
    for bogus in ["write_file", "list_files", "claim_task", "totally/made/up"] {
        let (is_error, content) = dispatch(&exec, bogus, json!({})).await;
        assert!(
            is_error,
            "'{bogus}' must fail LOUD (is_error), never a silent no-op: {content}"
        );
        // The recovery affordance rides back in the same observation.
        assert!(
            content.contains("commands/help") || content.contains("not a tool"),
            "'{bogus}' feedback must point at discovery so she recovers: {content}"
        );
    }
}

// what this catches (#163/#159): a REAL command given the WRONG args (empty
// when it requires some) fails LOUD, never runs a degenerate no-op. Swept
// over every stateless AiSafe command that DECLARES required params — a
// living exam that grows with the surface, not a hand-picked list. Dispatch
// is safe: a missing required field fails at the params boundary BEFORE the
// command body runs, so nothing mutates.
#[tokio::test]
async fn required_args_missing_fails_loud_across_the_stateless_surface() {
    let exec = stateless_surface_hands();
    let stateless: HashSet<&'static str> = stateless_command_objects()
        .iter()
        .map(|c| c.name())
        .collect();

    let mut examined = 0usize;
    for d in command_registry()
        .into_iter()
        .filter(|d| d.access_level == AccessLevel::AiSafe && stateless.contains(d.name))
    {
        // Only commands that DECLARE required params — those MUST reject `{}`.
        let requires = d
            .params_schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        if !requires {
            continue;
        }
        examined += 1;
        let (is_error, content) = dispatch(&exec, d.name, json!({})).await;
        assert!(
            is_error,
            "`{}` requires params but ran on empty input `{{}}` without erroring — \
             a silent no-op a persona would mistake for success: {content}",
            d.name
        );
    }
    // Non-vacuity: the sweep must have actually exercised commands.
    assert!(
        examined >= 1,
        "no stateless AiSafe command with required params was examined — the filter \
         is broken; the exam checked nothing"
    );
}

/// Every command name core's own production source declares: the `name:` of an
/// `action_command!` block, or an `ActionCommand`'s `const NAME`. Read from core's files
/// only (`continuum-core/src`), never the cut crates', so it names exactly the surface that
/// links nowhere but core's binary.
fn names_core_declares() -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    for file in crate::test_env::source_tree::crate_sources() {
        if !file.path.to_string_lossy().replace('\\', "/").contains("/continuum-core/src/") {
            continue;
        }
        let production = file.production();
        let mut in_action_command = false;
        for line in production.lines() {
            let t = line.trim();
            if t.contains("action_command!") {
                in_action_command = true;
            }
            let quoted = |prefix: &str| {
                t.strip_prefix(prefix)
                    .and_then(|rest| rest.strip_prefix('"'))
                    .and_then(|rest| rest.split_once('"'))
                    .map(|(name, _)| name.to_string())
            };
            if in_action_command {
                if let Some(name) = quoted("name: ") {
                    names.insert(name);
                    in_action_command = false;
                }
            }
            if let Some(name) = quoted("const NAME: &'static str = ") {
                names.insert(name);
            }
        }
    }
    names
}

// what this catches: a surface exam that reads green over a fraction of the surface. These
// exams sweep `command_registry()` and `stateless_command_objects()`, which hold only what
// is linked into the running test binary; run from a crate that does not link core (the mind
// cut first ran them from continuum-mind's binary), they examine the kernel's and the
// mind's commands and pass while every core command goes unexamined. This fails the moment
// the registry they sweep lacks any command core declares, or core declares none.
#[test]
fn the_registry_these_exams_sweep_carries_every_command_core_declares() {
    let declared = names_core_declares();
    assert!(
        declared.len() >= 100,
        "the source scan found only {} command names in continuum-core/src: the scan, not the \
         surface, is broken",
        declared.len()
    );
    let registered: HashSet<&'static str> = command_registry().into_iter().map(|d| d.name).collect();
    let missing: Vec<&String> = declared.iter().filter(|n| !registered.contains(n.as_str())).collect();
    assert!(
        missing.is_empty(),
        "{} of the {} commands core declares are absent from the registry this binary sees, so \
         every surface exam here runs against a partial surface: {missing:?}",
        missing.len(),
        declared.len()
    );
}
