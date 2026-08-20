//! Architecture test — proves the "localized state per citizen"
//! doctrine clause via a build-graph constraint (shape 5).
//!
//! See `docs/architecture/PROVING-THE-DOCTRINE.md` for the matrix
//! this file populates. The clause pinned here:
//!
//! > "Localized state per citizen — no singleton substrate state.
//! > Each peer / persona / citizen has its own runtime; install-once
//! > dependency injection goes through the `runtime::LateBound<T>`
//! > primitive on a per-instance struct field. A module-scope
//! > `static OnceLock<Arc<T>>` (or `OnceCell<Arc<T>>`) is the
//! > doctrine violation because it pretends substrate state is
//! > process-global when in practice each citizen needs its own."
//!
//! ## Why a structural (build-time) check
//!
//! Static singletons are visually identifiable: `static FOO: OnceLock<Arc<T>>`
//! or `static FOO: OnceCell<Arc<T>>` at module scope or inside a
//! `fn instance()` accessor. We don't need to run code to enforce the
//! ban — we walk the source. Same shape-5 build-graph check as the
//! engine-OS layering ratchet, applied to a different doctrine clause.
//!
//! ## What this catches
//!
//! - `static GLOBAL_X: OnceLock<Arc<X>> = OnceLock::new();` — process-global
//!   singleton that violates the per-citizen scope contract.
//! - `static INSTANCE: OnceLock<Arc<X>> = OnceLock::new();` inside a
//!   `pub fn instance()` accessor (the canonical global-singleton
//!   anti-pattern PR #1583 closed for `CommandExecutor` via `LateBound`).
//! - Same shapes spelled `std::sync::OnceLock<...>` or
//!   `once_cell::sync::OnceCell<...>` (qualified paths).
//!
//! ## What this DOES NOT catch (intentionally)
//!
//! - Struct fields like `bus: OnceLock<Arc<MessageBus>>` — those are
//!   per-instance install-once, which is the legitimate pattern. The
//!   companion doctrine push is to migrate those to `LateBound<T>` for
//!   ergonomics, but they don't violate the per-citizen rule.
//! - `OnceLock<T>` (no `Arc`) — that's a value primitive, not a
//!   shared substrate-resource pointer.
//! - Test scaffolding under `#[cfg(test)] mod tests` — tests can have
//!   whatever fixture statics they need (today: none qualify, but the
//!   scanner exempts test mods to keep the rule future-proof).
//!
//! ## The fix when a violation lands
//!
//! Two paths:
//! 1. **Per-instance install-once** — move the dependency to a struct
//!    field using `LateBound<T>`, install it at construction or via
//!    `ModuleContext`. See PR #1583 for the worked example.
//! 2. **Genuinely process-global immutable config** — if the value is
//!    truly never-per-citizen and never mutates (a compile-time
//!    constant, a build hash, etc.), prefer `const` or an inline
//!    function. `OnceLock<Arc<T>>` is the wrong primitive.
//!
//! ## Tag
//!
//! proves: localized state per citizen (no static `OnceLock<Arc<T>>` /
//! `OnceCell<Arc<T>>` singleton state in continuum-core)

use std::fs;
use std::path::{Path, PathBuf};

/// Forbidden type shapes that name a SHARED-ACROSS-CITIZENS substrate
/// resource via `Arc<T>` behind a process-global install-once cell.
/// These are the patterns the scanner looks for; each must appear on
/// a `static FOO: <pattern>` line to trip the ratchet.
const FORBIDDEN_TYPE_PATTERNS: &[&str] = &[
    ": OnceLock<Arc<",
    ": OnceCell<Arc<",
    ": std::sync::OnceLock<Arc<",
    ": once_cell::sync::OnceCell<Arc<",
    ": tokio::sync::OnceCell<Arc<",
];

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_num: usize,
    line: String,
    pattern: String,
}

fn scan_for_singletons(src_root: &Path) -> Vec<Violation> {
    let mut violations = Vec::new();
    walk(src_root, &mut |path| {
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return;
        }
        let Ok(content) = fs::read_to_string(path) else {
            return;
        };

        // Track whether we're inside a `#[cfg(test)] mod tests { ... }`
        // block — exempt those. The scanner is line-based with a simple
        // brace-depth counter from a `#[cfg(test)] mod` opener, which
        // covers the codebase's single-tests-mod-per-file convention.
        let mut in_test_mod = false;
        let mut test_mod_depth: i32 = 0;
        let mut prev_was_cfg_test = false;

        for (idx, raw_line) in content.lines().enumerate() {
            let trimmed = raw_line.trim_start();

            // `#[cfg(test)]` on its own line OR collapsed inline with
            // the `mod NAME {` opener. The inline form
            // `#[cfg(test)] mod tests { ... }` is rare but legal — handle
            // it on the same line so the test mod still gets exempted.
            if trimmed.starts_with("#[cfg(test)]") {
                let rest_of_line = &trimmed["#[cfg(test)]".len()..];
                if rest_of_line.contains("mod ") && raw_line.contains('{') {
                    in_test_mod = true;
                    test_mod_depth = (raw_line.matches('{').count() as i32)
                        - (raw_line.matches('}').count() as i32);
                    if test_mod_depth <= 0 {
                        // Single-line mod block (`#[cfg(test)] mod x {}`),
                        // already balanced — nothing to exempt.
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

            // Match `static NAME: <forbidden-pattern>` — covers module
            // scope and function-local `static`s (both are process-global
            // singletons; function-local just narrows visibility). Strip
            // any leading visibility modifier so `pub static`,
            // `pub(crate) static`, and `pub(super) static` are caught
            // too. Order matters: longer prefixes first.
            let after_vis = trimmed
                .trim_start_matches("pub(crate) ")
                .trim_start_matches("pub(super) ")
                .trim_start_matches("pub ");
            if !after_vis.starts_with("static ") {
                continue;
            }
            let line_for_matching = after_vis;
            for pat in FORBIDDEN_TYPE_PATTERNS {
                if line_for_matching.contains(pat) {
                    violations.push(Violation {
                        file: path.to_path_buf(),
                        line_num: idx + 1,
                        line: raw_line.to_string(),
                        pattern: (*pat).to_string(),
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

fn src_root() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest).join("src")
}

/// Current count of grandfathered violations. Per the proof-discipline
/// doc § "Ratchet pattern": NEW static singletons BLOCK, existing ones
/// are tracked here and cleaned up by dedicated follow-up PRs. The
/// count goes DOWN as violations are migrated to `LateBound<T>` or
/// `ModuleContext`-threaded dependencies, NEVER up.
///
/// When this number drops to 0, replace it with a strict
/// `assert!(violations.is_empty(), ...)` and the clause graduates to
/// fully-enforced.
///
/// Follow-up cleanup targets (the existing 12):
///
/// - `live/video/capture.rs::instance()` — VideoFrameCapture singleton
/// - `live/video/bevy_renderer/api.rs::RENDERER_GPU_MANAGER`
/// - `live/audio/tts/mod.rs::TTS_GPU_MANAGER` + `TTS_REGISTRY`
/// - `live/audio/vad/silero.rs::SILERO_SESSION` (+ raw variant)
/// - `live/audio/stt/mod.rs::STT_REGISTRY`
/// - `live/avatar/registry.rs::AVATAR_REGISTRY`
/// - `modules/embedding.rs::{MODEL_CACHE, EMBEDDING_GPU_MANAGER, EMBEDDING_POOL}`
/// - `modules/sentinel/mod.rs::GLOBAL_SENTINEL` (read by signal handlers
///   — the trickiest one; needs a different shutdown plumbing)
const GRANDFATHERED_VIOLATIONS: usize = 12;

// proves: localized state per citizen (no NEW static singleton substrate
// state; grandfathered count ratchets only down)
#[test]
fn static_singleton_state_ratchet() {
    let root = src_root();
    let violations = scan_for_singletons(&root);
    let count = violations.len();

    if count > GRANDFATHERED_VIOLATIONS {
        let mut report = String::from(
            "Localized-state-per-citizen violation: NEW static singleton\n\
             substrate state. Per the substrate doctrine, each peer /\n\
             persona / citizen has its own runtime; install-once goes\n\
             through `runtime::LateBound<T>` on a per-instance struct\n\
             field, not a process-global `static`.\n\n\
             Fix:\n\
             - If the dependency is per-citizen (most cases): move it to\n\
               a struct field of type `LateBound<T>`, install at\n\
               construction or via `ModuleContext`. See PR #1583 for\n\
               the canonical migration pattern.\n\
             - If it's genuinely never-per-citizen and never mutates:\n\
               use `const` or an inline function. `OnceLock<Arc<T>>` is\n\
               the wrong primitive.\n\n\
             Violations:\n",
        );
        for v in &violations {
            report.push_str(&format!(
                "  {}:{} (pattern: {})\n    {}\n",
                v.file.display(),
                v.line_num,
                v.pattern.trim(),
                v.line.trim()
            ));
        }
        panic!(
            "{report}\n\
             Found {count} static singletons (grandfathered budget: {GRANDFATHERED_VIOLATIONS}). \
             New ones BLOCK; ratchet down by migrating existing ones to LateBound<T>."
        );
    }

    if count < GRANDFATHERED_VIOLATIONS {
        panic!(
            "Localized-state-per-citizen ratchet went DOWN to {count} \
             (was {GRANDFATHERED_VIOLATIONS}). Excellent! Update \
             `GRANDFATHERED_VIOLATIONS` to {count} in this file so the \
             new lower count becomes the enforced ceiling. The ratchet \
             only moves down."
        );
    }
}

// proves: localized state per citizen (positive — LateBound<T> remains
// the canonical install-once primitive at its known location)
#[test]
fn late_bound_remains_the_canonical_primitive() {
    // If `LateBound<T>` ever moves, the engine-OS layering ratchet would
    // catch consumers reaching past the runtime root. But the primitive
    // ITSELF must exist somewhere — pin its location so a rename or
    // accidental deletion is loud, not silent.
    let root = src_root();
    let late_bound = root.join("runtime").join("late_bound.rs");
    assert!(
        late_bound.is_file(),
        "test prerequisite: `runtime/late_bound.rs` must exist as the \
         canonical install-once primitive's home — found nothing at {}",
        late_bound.display()
    );

    let content = fs::read_to_string(&late_bound).expect("read late_bound.rs");
    assert!(
        content.contains("pub struct LateBound"),
        "runtime/late_bound.rs no longer declares `pub struct LateBound`. \
         If it was renamed, update this test and the doctrine references; \
         if it was deleted, the entire install-once primitive needs to be \
         restored before this ratchet can keep enforcing migration target."
    );
    assert!(
        content.contains("OnceLock<Arc<"),
        "runtime/late_bound.rs no longer wraps `OnceLock<Arc<T>>`. The \
         primitive's whole job is to make that pattern uniform — if the \
         underlying type changed, audit the migrations and update both \
         doctrine docs and this test."
    );
}
