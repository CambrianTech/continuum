//! Architecture test — proves the "module compose-by-event" doctrine
//! clause via a build-graph constraint (shape 5).
//!
//! See `docs/architecture/PROVING-THE-DOCTRINE.md` for the matrix
//! this file populates. The clause pinned here:
//!
//! > "Module compose-by-event — substrate-internal modules compose
//! > via the event substrate (MessageBus subscribe / emit), not by
//! > imperative `CommandExecutor::execute_json("uri", ...)` round-trips
//! > inside the cognition hot path. Commands are the OUTER interface
//! > (callers from outside cognition); events are the INNER fabric
//! > (cognition modules wire up via subscribe/emit)."
//!
//! ## Why a structural (build-time) check
//!
//! `CommandExecutor::execute_*` calls inside `cognition/*` are visually
//! identifiable: `executor.execute_json("ai/generate", ...)`,
//! `executor.execute_with_caller(...)`, etc. We don't need to run code
//! to enforce the rule — we walk the source. Same shape-5 build-graph
//! check as the engine-OS layering ratchet and the singleton ban.
//!
//! ## What this catches
//!
//! - `executor.execute_json("ai/generate", params).await?` from inside
//!   `cognition/vision_describe.rs` — the canonical migration target,
//!   tracked under #112-#114 (route through `InferenceHandleStore`).
//! - Any future cognition module that wires up imperative command
//!   dispatch instead of composing via the event substrate
//!   (MessageBus subscribe / emit, or LateBound<CommandExecutor>
//!   followed by direct `.execute_*` calls).
//!
//! ## What this DOES NOT catch (intentionally)
//!
//! - `CommandExecutor::execute_*` calls from OUTSIDE `cognition/*` —
//!   that's the outer command interface and is fine.
//! - Test code (under `#[cfg(test)] mod tests`) — tests can drive
//!   cognition modules however they need to validate behavior.
//! - Generic `.execute()` method calls — too broad (futures, queues,
//!   etc. all have `.execute()`). The forbidden patterns are the
//!   CommandExecutor-distinctive method names:
//!   `execute_json`, `execute_with_caller`, `execute_ts`,
//!   `execute_ts_json`. If a future refactor introduces a typed
//!   `executor.execute(typed_args)` call inside cognition, that's a
//!   ratchet-coverage gap to close in a follow-up.
//!
//! ## The fix when a violation lands
//!
//! Replace the `executor.execute_*("uri", params)` call with either:
//!
//! 1. **Event subscribe + emit** — cognition module subscribes to the
//!    relevant MessageBus topic at construction, emits the result on
//!    a sibling topic. The OUTER module that wants the result also
//!    subscribes. No imperative round-trip inside the cognition hot
//!    path.
//! 2. **Handle pre-bound at boot** — for resources where pull-via-handle
//!    makes more sense than push-via-event (e.g. inference), hold a
//!    pre-bound `InferenceHandle` (cf. tasks #107-#108) and call methods
//!    on it directly. The HANDLE is the composition primitive; the
//!    command interface is left for outer callers.
//!
//! Both patterns avoid the `execute_*("uri", json)` shape inside
//! cognition — which is the substrate's "compose via imperative
//! request-response" anti-pattern.
//!
//! ## Tag
//!
//! proves: module compose-by-event (no `CommandExecutor::execute_*`
//! calls inside `cognition/*` substrate-internal logic)

use std::fs;
use std::path::{Path, PathBuf};

/// Forbidden method-call patterns that name `CommandExecutor`'s
/// imperative dispatch API. Each must appear on a non-test source
/// line inside `core/continuum-core/src/cognition/` to trip the
/// ratchet.
///
/// Note: we intentionally don't list the bare `.execute(` suffix —
/// it would generate false positives across futures, queues, and
/// other unrelated APIs. If a typed `executor.execute(typed_args)`
/// pattern shows up inside cognition in the future, extend this list.
const FORBIDDEN_METHOD_PATTERNS: &[&str] = &[
    ".execute_json(",
    ".execute_with_caller(",
    ".execute_ts(",
    ".execute_ts_json(",
];

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    line: String,
    method: String,
}

fn scan_for_command_executor_calls(cognition_root: &Path) -> Vec<Violation> {
    let mut violations = Vec::new();
    walk(cognition_root, &mut |path| {
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return;
        }
        let Ok(content) = fs::read_to_string(path) else {
            return;
        };

        // Track `#[cfg(test)] mod NAME {...}` blocks — exempt them.
        // Same parser shape as the singleton-ban test, including
        // inline `#[cfg(test)] mod tests {` handling.
        let mut in_test_mod = false;
        let mut test_mod_depth: i32 = 0;
        let mut prev_was_cfg_test = false;

        for (idx, raw_line) in content.lines().enumerate() {
            let trimmed = raw_line.trim_start();

            // Skip line-comments — `//` lines and the body of `///`
            // doc comments often quote the very patterns we're banning
            // (as documentation OF the rule), and those should never
            // trip the scanner.
            if trimmed.starts_with("//") {
                continue;
            }

            // Test mod enter/exit tracking (same as singleton-ban).
            if trimmed.starts_with("#[cfg(test)]") {
                let rest = &trimmed["#[cfg(test)]".len()..];
                if rest.contains("mod ") && raw_line.contains('{') {
                    in_test_mod = true;
                    test_mod_depth = (raw_line.matches('{').count() as i32)
                        - (raw_line.matches('}').count() as i32);
                    if test_mod_depth <= 0 {
                        in_test_mod = false;
                        test_mod_depth = 0;
                    }
                } else {
                    prev_was_cfg_test = true;
                }
                continue;
            }
            if prev_was_cfg_test {
                prev_was_cfg_test = false;
                if trimmed.starts_with("mod ") && raw_line.contains('{') {
                    in_test_mod = true;
                    test_mod_depth = 1;
                    continue;
                }
            }
            if in_test_mod {
                test_mod_depth += raw_line.matches('{').count() as i32;
                test_mod_depth -= raw_line.matches('}').count() as i32;
                if test_mod_depth <= 0 {
                    in_test_mod = false;
                    test_mod_depth = 0;
                }
                continue;
            }

            for method in FORBIDDEN_METHOD_PATTERNS {
                if raw_line.contains(method) {
                    violations.push(Violation {
                        file: path.to_path_buf(),
                        line_num: idx + 1,
                        line: raw_line.to_string(),
                        method: (*method).to_string(),
                    });
                    break;
                }
            }
        }
    });
    violations
}

fn walk(dir: &Path, visit: &mut dyn FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit);
        } else {
            visit(&path);
        }
    }
}

fn cognition_root() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest).join("src").join("cognition")
}

/// Current count of grandfathered violations. Per the proof-discipline
/// doc § "Ratchet pattern": NEW `CommandExecutor::execute_*` calls
/// inside `cognition/*` BLOCK; existing ones are tracked here and
/// migrated by dedicated follow-up PRs. The count goes DOWN as
/// violations are migrated to handle-based or event-based composition,
/// NEVER up.
///
/// When this number drops to 0, replace it with a strict
/// `assert!(violations.is_empty(), ...)` and the clause graduates to
/// fully-enforced.
///
/// The single grandfathered violation today:
///
/// - `cognition/vision_describe.rs::describe_image_via_ai_generate`
///   calls `executor.execute_json("ai/generate", ...)`. Migration is
///   tracked under tasks #112-#114 (route persona response /
///   should_respond / validate through inference command) and #106
///   (consolidate ai/* namespace). The clean fix is to pre-bind an
///   `InferenceHandle` at boot and call it directly, like
///   `airc_chat_demo` already does.
const GRANDFATHERED_VIOLATIONS: usize = 1;

// proves: module compose-by-event (no NEW CommandExecutor::execute_*
// calls inside cognition/*; grandfathered count ratchets only down)
#[test]
fn cognition_command_executor_calls_ratchet() {
    let root = cognition_root();
    assert!(
        root.is_dir(),
        "test prerequisite: cognition directory must exist at {}",
        root.display()
    );
    let violations = scan_for_command_executor_calls(&root);
    let count = violations.len();

    if count > GRANDFATHERED_VIOLATIONS {
        let mut report = String::from(
            "Module compose-by-event violation: NEW imperative\n\
             `CommandExecutor::execute_*` call inside `cognition/*`.\n\
             Substrate-internal modules compose via the event substrate\n\
             (MessageBus subscribe/emit) or via pre-bound handles, not\n\
             by imperative request-response round-trips through the\n\
             outer command interface.\n\n\
             Fix:\n\
             - Replace the execute_* call with a pre-bound handle (e.g.\n\
               InferenceHandle, see tasks #107-#108), OR\n\
             - Replace with subscribe/emit on MessageBus — cognition\n\
               subscribes to the relevant topic at construction and\n\
               emits results on a sibling topic; outer caller also\n\
               subscribes. No imperative round-trip on hot path.\n\n\
             Violations:\n",
        );
        for v in &violations {
            report.push_str(&format!(
                "  {}:{} (method: {})\n    {}\n",
                v.file.display(),
                v.line_num,
                v.method.trim(),
                v.line.trim()
            ));
        }
        panic!(
            "{report}\n\
             Found {count} violations (grandfathered budget: {GRANDFATHERED_VIOLATIONS}). \
             New ones BLOCK; ratchet down by migrating existing ones."
        );
    }

    if count < GRANDFATHERED_VIOLATIONS {
        panic!(
            "Module compose-by-event ratchet went DOWN to {count} (was \
             {GRANDFATHERED_VIOLATIONS}). Excellent! Update \
             `GRANDFATHERED_VIOLATIONS` to {count} in this file so the \
             new lower count becomes the enforced ceiling. The ratchet \
             only moves down."
        );
    }
}

// proves: module compose-by-event (positive — outer command boundary
// IS the legitimate place to call CommandExecutor::execute_*; the rule
// only applies INSIDE cognition/*)
#[test]
fn command_executor_remains_the_outer_boundary() {
    // If CommandExecutor itself moves, both this test and the
    // engine-OS layering ratchet would notice. But pin the type's
    // location explicitly so a rename is loud, not silent.
    let manifest = env!("CARGO_MANIFEST_DIR");
    let exec_path = PathBuf::from(manifest)
        .join("src")
        .join("runtime")
        .join("command_executor.rs");
    assert!(
        exec_path.is_file(),
        "test prerequisite: runtime/command_executor.rs must exist as \
         the canonical outer command boundary — found nothing at {}",
        exec_path.display()
    );

    let content = fs::read_to_string(&exec_path).expect("read command_executor.rs");
    assert!(
        content.contains("pub struct CommandExecutor"),
        "runtime/command_executor.rs no longer declares `pub struct \
         CommandExecutor`. If renamed, update this test, the engine-OS \
         layering ratchet's allowlist, and the doctrine docs."
    );
    assert!(
        content.contains("pub async fn execute_json"),
        "runtime/command_executor.rs no longer exposes `execute_json`. \
         If the method moved or was renamed, update the FORBIDDEN \
         method-name list in `architecture_compose_by_event.rs` to \
         match the new public surface."
    );
}
