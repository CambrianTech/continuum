//! Architecture test — proves the engine-OS layering doctrine clause
//! via a build-graph constraint (shape 5).
//!
//! See `docs/architecture/PROVING-THE-DOCTRINE.md` for the matrix
//! this file populates. The clause pinned here:
//!
//! > "Engine-OS layering — code outside `runtime/*` must consume the
//! > runtime's PUBLIC re-exports only, not reach into specific
//! > submodule paths." Per the substrate doctrine, `runtime::*` is
//! > the engine block; consumers (cognition, persona, modules,
//! > inference, etc.) compose against the documented public surface
//! > so the engine's internals can be refactored without breaking
//! > every callsite.
//!
//! ## Why a structural (build-time) check, not a runtime test
//!
//! The constraint is about the SHAPE of `use crate::runtime::...`
//! statements. We don't need to run code to enforce it — we just
//! need to walk the source and assert the structure. This is the
//! canonical shape-5 build-graph check per the proof-discipline doc
//! § "Shape 5 — Build-graph constraint."
//!
//! ## What this catches
//!
//! - `use crate::runtime::message_bus::MessageBus` — reaches past
//!   the public re-export `crate::runtime::MessageBus`. Refactor of
//!   the inner submodule (renaming, splitting) breaks this caller
//!   even though the public surface didn't change.
//! - `use crate::runtime::service_module::{CommandResult, ...}` —
//!   same shape, same problem.
//!
//! The fix is mechanical: replace the submodule-path import with the
//! root-level re-export. All the items called out by this test are
//! re-exported at `runtime/mod.rs` top level.
//!
//! ## Tag
//!
//! proves: engine-OS layering (cognition / persona / modules / etc.
//! don't reach into runtime submodule internals)

use std::fs;
use std::path::{Path, PathBuf};

/// The submodule names under `runtime/*` that are forbidden to
/// appear in `use crate::runtime::<submodule>::*` statements from
/// code OUTSIDE the runtime directory. Every item exposed by these
/// submodules is re-exported at `runtime/mod.rs` root.
const FORBIDDEN_RUNTIME_SUBMODULES: &[&str] = &[
    "airc_interceptor",
    "artifact_handle",
    "boot_mode",
    "brain_region",
    "cell_shapes",
    "command_envelope",
    "command_events",
    "command_executor",
    "command_interceptor",
    "control",
    "grid_interceptor",
    "late_bound",
    "message_bus",
    "module_context",
    "module_logger",
    "module_metrics",
    "per_key_gate",
    "ready_buffer",
    "region_telemetry",
    "registry",
    "runtime",
    "service_module",
    "shared_compute",
];

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    line: String,
    submodule: String,
}

fn scan_for_violations(src_root: &Path) -> Vec<Violation> {
    let mut violations = Vec::new();
    walk(src_root, &mut |path| {
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return;
        }
        // Skip the runtime module's own files — internal use of
        // sibling submodules is fine.
        let rel = path.strip_prefix(src_root).unwrap_or(path);
        if rel.starts_with("runtime") {
            return;
        }
        let Ok(content) = fs::read_to_string(path) else {
            return;
        };
        for (idx, raw_line) in content.lines().enumerate() {
            let trimmed = raw_line.trim_start();
            // Match both `use crate::runtime::<sub>::*` and
            // `use super::runtime::<sub>::*` (the latter is unusual
            // but possible from intermediate modules).
            for sub in FORBIDDEN_RUNTIME_SUBMODULES {
                let crate_pat = format!("use crate::runtime::{sub}::");
                let super_pat = format!("use super::runtime::{sub}::");
                if trimmed.starts_with(&crate_pat) || trimmed.starts_with(&super_pat) {
                    violations.push(Violation {
                        file: path.to_path_buf(),
                        line_num: idx + 1,
                        line: raw_line.to_string(),
                        submodule: (*sub).to_string(),
                    });
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

fn src_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at `core/continuum-core`
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest).join("src")
}

/// Current count of grandfathered violations. Per the
/// proof-discipline doc § "Ratchet pattern": new violations BLOCK,
/// existing ones are tracked here and cleaned up in dedicated
/// follow-up PRs. The count goes DOWN as violations are fixed,
/// NEVER up.
///
/// When this number drops to 0, replace it with a strict
/// `assert!(violations.is_empty(), ...)` and the clause graduates to
/// a fully-enforced build-graph constraint.
///
/// Follow-up cards to drive this number down:
///
/// - Promote `BusEvent` to a `runtime/*` root re-export AND fix the
///   `airc/realtime_wire.rs` caller — or surface why it shouldn't be
///   public and refactor the consumer.
/// - Audit the 29 grandfathered violations file-by-file; each one
///   is either a missing root re-export (add it) or a legitimate
///   internal-API use that the consumer shouldn't be doing (refactor).
const GRANDFATHERED_VIOLATIONS: usize = 29;

// proves: engine-OS layering (no NEW reaches past runtime's public re-exports;
// grandfathered count ratchets only down)
#[test]
fn runtime_submodule_paths_outside_runtime_ratchet() {
    let root = src_root();
    let violations = scan_for_violations(&root);
    let count = violations.len();

    if count > GRANDFATHERED_VIOLATIONS {
        let mut report = String::from(
            "Engine-OS layering violation: NEW reaches into runtime\n\
             submodule paths from outside `runtime/*`. Per the substrate\n\
             doctrine, consumers must use the root-level re-exports at\n\
             `runtime/mod.rs` so engine internals can be refactored\n\
             without breaking every callsite.\n\n\
             Fix:\n\
             - If the item IS re-exported at runtime root, change the\n\
               import to `use crate::runtime::X`.\n\
             - If it's NOT re-exported, EITHER promote it to the root\n\
               (add `pub use <submodule>::X;` to runtime/mod.rs) AND use\n\
               the root-level import, OR refactor the caller to not\n\
               depend on the engine internal.\n\n\
             Violations:\n",
        );
        for v in &violations {
            report.push_str(&format!(
                "  {}:{} (submodule: {})\n    {}\n",
                v.file.display(),
                v.line_num,
                v.submodule,
                v.line.trim()
            ));
        }
        panic!(
            "{report}\n\
             Found {count} violations (grandfathered budget: {GRANDFATHERED_VIOLATIONS}). \
             New ones BLOCK; ratchet down by fixing existing ones."
        );
    }

    if count < GRANDFATHERED_VIOLATIONS {
        panic!(
            "Engine-OS layering ratchet went DOWN to {count} (was {GRANDFATHERED_VIOLATIONS}). \
             Excellent! Update `GRANDFATHERED_VIOLATIONS` to {count} in this file so the new \
             lower count becomes the enforced ceiling. The ratchet only moves down."
        );
    }
}

// proves: engine-OS layering (positive — runtime/* CAN reach into its
// own submodules; the rule is about consumers, not internals)
#[test]
fn runtime_internal_submodule_use_is_allowed() {
    // The scan deliberately skips `runtime/*` itself. This test
    // documents that intent so a future contributor doesn't
    // "tighten" the scan to also check runtime's own files (which
    // would break legitimate sibling-submodule composition).
    let root = src_root();
    let runtime_dir = root.join("runtime");
    assert!(
        runtime_dir.is_dir(),
        "test prerequisite: runtime directory must exist at {}",
        runtime_dir.display()
    );

    // Sanity: the runtime dir contains the submodules we listed.
    let mod_rs = runtime_dir.join("mod.rs");
    assert!(mod_rs.is_file(), "runtime/mod.rs must exist");

    let mod_content = fs::read_to_string(&mod_rs).expect("read runtime/mod.rs");
    for sub in FORBIDDEN_RUNTIME_SUBMODULES {
        // Skip "runtime" — that's the inner re-export of runtime::runtime
        // (the lifecycle module), declared via `pub mod runtime;` not
        // `pub mod <sub>;` at top-level mod.rs for all subs.
        if *sub == "runtime" {
            continue;
        }
        assert!(
            mod_content.contains(&format!("pub mod {sub};")),
            "FORBIDDEN_RUNTIME_SUBMODULES is stale: '{sub}' not declared in runtime/mod.rs"
        );
    }
}
