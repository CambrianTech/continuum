//! **continuum-core has an order. An import may point down it, never up.**
//!
//! # The class
//!
//! Joel, 2026-10-11: "The architecture is why it's so slow to build… the organization is
//! nonexistent… You can't think of code as something to patch. It's always refine and
//! consolidate into better and more diverse centralized abstractions."
//!
//! Measured the same night on canary c19533f77: continuum-core is ~670k lines in ONE crate,
//! a PR's core test job ran 19.9 min, and canary's core-binaries ran 87–94 min per push. Rust
//! compiles a crate as a unit, so splitting it is the build-time lever, and it was impossible:
//! 81 pairs of top-level modules imported each other, and 722 `crate::` references in code
//! pointed UP the order below (the substrate importing persona, inference importing
//! cognition, cognition importing persona 249 times). Every patch reached sideways for what
//! it needed instead of moving the shared piece down to where both sides could depend on it.
//!
//! # The order
//!
//! [`LAYERS`], lowest first. A module may reference its own layer and any layer below it.
//! A reference upward is counted. The fix is never a re-export or a wrapper: move the shared
//! type or contract DOWN (dependency inversion: the lower layer owns a trait, the upper one
//! implements it), migrate the callers, delete the old path. A layer whose count reaches
//! zero can leave this crate as its own, and the build parallelizes there.
//!
//! The order is a design decision, held in exactly one place. Amend it here, in review, with
//! the counts beside it; never per-file.
//!
//! # A ratchet, not a wall
//!
//! **The count may never rise.** Every untangling lowers the baseline.

use super::{split_code_and_comment, SourceFile, SourceRule, Violation};

/// continuum-core's top-level modules, lowest layer first. A module absent from every layer
/// (a small leaf, `bin/`) is neither checked nor counted against; add it here when it grows
/// dependencies of its own.
pub const LAYERS: &[&[&str]] = &[
    // 0: primitives with no domain of their own
    &["tool_parsing", "sdk_codegen", "orm"],
    // 1: the RTOS substrate: scheduling, resources, pressure, paging
    &["runtime", "system_resources", "paging", "governor", "gpu", "resources", "capacity"],
    // 2: inference, the adapter every mind calls through
    &["inference_capability", "model_registry", "inference", "ai"],
    // 3: cognition, the per-turn mind
    &["cognition", "memory", "perception", "experience"],
    // 4: the citizen: persona, genome, live senses
    &["persona", "genome", "live", "media"],
    // 5: surfaces: service modules, commands, IPC, routing, airc, code, forge
    &["modules", "commands", "ipc", "routing", "airc", "code", "forge", "provisioning"],
];

fn layer_of(module: &str) -> Option<usize> {
    LAYERS.iter().position(|layer| layer.contains(&module))
}

pub struct Layering;

impl Layering {
    /// Every `crate::<module>` on a code line, as module names.
    fn referenced_modules(code: &str) -> impl Iterator<Item = &str> {
        code.match_indices("crate::").filter_map(move |(at, _)| {
            // `super::crate::` cannot occur; an identifier ending in `crate` (`subcrate::`) can.
            if at > 0 && code.as_bytes()[at - 1].is_ascii_alphanumeric() {
                return None;
            }
            let rest = &code[at + "crate::".len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            (end > 0).then(|| &rest[..end])
        })
    }
}

impl SourceRule for Layering {
    fn name(&self) -> &'static str {
        "layering"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        let Some(own) = file.rel.split('/').next().and_then(layer_of) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        // The whole file: a test mod that imports up the order still welds the crates.
        for (i, line) in file.raw.lines().enumerate() {
            let code = split_code_and_comment(line).0;
            for module in Self::referenced_modules(code) {
                if layer_of(module).is_some_and(|theirs| theirs > own) {
                    out.push(Violation {
                        rule: "layering",
                        file: file.rel.clone(),
                        line: i + 1,
                        source: format!("L{own} -> L{} crate::{module}: {}", layer_of(module).unwrap_or(own), line.trim()),
                    });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    /// Up-references when this guard landed, on canary c19533f77 (2026-10-11).
    ///
    /// **This number may only ever go DOWN.** Raising it to green a build is defeating the
    /// guard, and should be refused in review.
    const BASELINE_UP_REFERENCES: usize = 722;

    /// What this catches: a new import up the order, the weld that keeps continuum-core one
    /// crate and every build a full rebuild.
    ///
    /// If this fails because you added one: move what you need DOWN (the lower layer owns the
    /// type or trait; the upper one implements it) and migrate the callers. If it fails the
    /// other way, you untangled something: LOWER the baseline and take the win.
    #[test]
    fn imports_up_the_layer_order_never_increase() {
        let violations = scan(&[&Layering]);
        let count = violations.len();
        eprintln!("up-references: {count} (baseline {BASELINE_UP_REFERENCES})");
        let mut by_edge: std::collections::BTreeMap<String, usize> = Default::default();
        for v in &violations {
            let edge = v.source.split(": ").next().unwrap_or_default().to_string();
            let from = v.file.split('/').next().unwrap_or_default();
            *by_edge.entry(format!("{from} {edge}")).or_default() += 1;
        }
        let mut edges: Vec<_> = by_edge.into_iter().collect();
        edges.sort_by(|a, b| b.1.cmp(&a.1));
        assert!(
            count <= BASELINE_UP_REFERENCES,
            "imports up the layer order rose to {count} (baseline {BASELINE_UP_REFERENCES}).\n\
             Move the shared type or contract DOWN to where both sides can depend on it, \
             migrate the callers, delete the old path. Never a re-export or a wrapper.\n\
             Largest edges:\n{}",
            edges
                .iter()
                .take(15)
                .map(|(e, n)| format!("  {n:5}  {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// What this catches: the predicate missing an up-reference, or counting a sibling, a
    /// downward import, a comment, or an identifier that merely ends in `crate`.
    #[test]
    fn only_references_up_the_order_count() {
        let file = SourceFile::for_test(
            "runtime/x.rs",
            "use crate::persona::Thing;\nuse crate::paging::Pool;\nuse crate::bin_only::X;\n// crate::cognition::Y\nlet p = subcrate::cognition::Z;\nfn f() { crate::modules::run(); }\n",
        );
        let found = Layering.check(&file);
        let lines: Vec<usize> = found.iter().map(|v| v.line).collect();
        assert_eq!(lines, vec![1, 6], "persona and modules are up from runtime; paging is beside it: {found:?}");
        assert!(Layering.check(&SourceFile::for_test("modules/x.rs", "use crate::runtime::R;")).is_empty());
        assert!(Layering.check(&SourceFile::for_test("bin/x.rs", "use crate::modules::M;")).is_empty());
    }
}
