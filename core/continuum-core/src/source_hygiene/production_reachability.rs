//! **Machinery that nothing constructs is not "ready", it is unwired — and the
//! substrate keeps shipping it.**
//!
//! # The disease this guard exists for
//!
//! On 2026-08-20 the same defect was found FIVE times in one session, each time by
//! accident, each time after hours of debugging a symptom:
//!
//! | What was built | How it failed | Found via |
//! |---|---|---|
//! | `ProbeRouterLayer` | installed, handle discarded at construction | #362, a dead probe stream |
//! | CLI install path | ran every deploy, but the BUILD was gated off | #422, a stale binary |
//! | `BenchViewState` | impl complete, never bound in `supervisor.rs` | #426, a blind citizen |
//! | `GeneratorModule` | never registered with the runtime | #325, an orphan command |
//! | `AgentContext` | **zero production callers — constructed only in its own tests** | #27, an operator with no identity |
//!
//! Every one compiled. Every one had passing unit tests. Every one was invisible
//! until something downstream failed for a reason that looked unrelated — the
//! `AgentContext` gap surfaced as *"benchmark cards are authored by the wrong
//! citizen"*, four inference steps from the cause.
//!
//! Five in one day is not bad luck, it is a missing constraint. The module-wiring
//! audit (`runtime/registry.rs`, #344) already catches an unregistered `ServiceModule`
//! — but it asks about MODULES, and four of the five above were not modules. The
//! general question is the one this rule asks: **does anything in the workspace
//! actually use this?**
//!
//! # What it flags, and what it deliberately does not
//!
//! A type is reported when ALL of these hold:
//!
//! 1. it is declared `pub struct` / `pub enum` in this crate's production code;
//! 2. its declaring file gives it an inherent `impl` AND a constructor
//!    (`fn new` / `fn bootstrap` / `fn spawn` / `fn install` / `fn connect`) — i.e. it
//!    is MACHINERY someone is meant to stand up, not a plain data carrier;
//! 3. no production line anywhere in the workspace mentions it outside its own file.
//!
//! Condition 2 is what keeps this from being a dead-code lint. Wire DTOs, ts-rs
//! payloads and error enums are *supposed* to be referenced only by serde and the
//! generated TypeScript; flagging them would bury the real finding under hundreds of
//! false ones, and a guard nobody can read is a guard that gets `#[ignore]`d.
//!
//! # The two traps that would have made it fire wrongly on day one
//!
//! - **A `use` / `pub use` line is not a use.** `AgentContext` is re-exported by
//!   `context/mod.rs`; counting that line as a reference would have hidden the exact
//!   defect this rule was written for. Import and re-export lines are excluded.
//! - **A doc comment is not code.** `AgentContext` is also *named in prose* twice in
//!   `context/mod.rs`. Comments are stripped before tokenizing — the same
//!   match-by-nature-not-by-name discipline that [`super::scan`] needed for
//!   `#[cfg(test)]`, and for the same reason: prose that mentions a thing must never
//!   be mistaken for the thing.
//!
//! # Why the corpus is the whole workspace
//!
//! `continuum-core` is a library. A `pub` type with no in-crate caller may be wired
//! perfectly well from `apps/cli` or `continuum-mcp`. Violations are raised only
//! against this crate; references are searched across every workspace root. See
//! [`super::workspace_src_roots`].

use super::{split_code_and_comment, CrateRule, SourceFile, Violation};
use std::collections::HashMap;

/// Constructor shapes that mark a type as machinery someone is meant to stand up.
///
/// Deliberately a short list of *stand-up* verbs rather than "any `pub fn`": a type
/// with only accessors is a data carrier, and asking who constructs it is the wrong
/// question. `bootstrap` is here because it is what `AgentContext` uses.
const CONSTRUCTOR_SHAPES: &[&str] = &[
    "fn new(",
    "fn bootstrap(",
    "fn spawn(",
    "fn install(",
    "fn connect(",
    "fn start(",
];

/// Unwired public machinery at the time this guard landed (2026-08-20).
///
/// **This number may only ever go DOWN.** Raising it to make a red build green is
/// defeating the guard and should be refused in review — the whole point is that the
/// five defects above were each individually "not worth blocking on" and collectively
/// cost days. If your new type trips this, the fix is to WIRE it or to not land it
/// yet; a type with no caller is not finished work.
const BASELINE_UNWIRED: usize = 107;

pub struct ProductionReachability;

impl ProductionReachability {
    /// Identifier → number of times it appears in real code (comments stripped, import
    /// and re-export lines excluded).
    ///
    /// One pass over the corpus, so the per-type question is an O(1) lookup rather than
    /// a substring search — the difference between a test that runs in a second and one
    /// that scans gigabytes.
    fn identifier_counts(text: &str) -> HashMap<&str, usize> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for line in text.lines() {
            let (code, _) = split_code_and_comment(line);
            let trimmed = code.trim_start();
            // An import or re-export names a type without USING it. This exclusion is
            // load-bearing: `pub use agent::{AgentContext, …}` is precisely how the
            // motivating defect would have hidden from its own guard.
            if trimmed.starts_with("use ")
                || trimmed.starts_with("pub use ")
                || trimmed.starts_with("pub(crate) use ")
            {
                continue;
            }
            for token in code.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                if !token.is_empty() {
                    *counts.entry(token).or_insert(0) += 1;
                }
            }
        }
        counts
    }

    /// The `pub struct` / `pub enum` names declared in a file, with their line numbers.
    fn declared_types(file: &SourceFile) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        for (line_no, line) in file.production_lines() {
            let (code, _) = split_code_and_comment(line);
            let trimmed = code.trim_start();
            for kw in ["pub struct ", "pub enum "] {
                let Some(rest) = trimmed.strip_prefix(kw) else {
                    continue;
                };
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    out.push((line_no, name));
                }
            }
        }
        out
    }

    /// Is the type surfaced to the rest of the world through a public type alias?
    ///
    /// The third false-positive class, and the one that would have done real damage:
    /// `gpu/metal_monitor` declares `pub struct MetalProbe` and exports
    /// `pub type MetalMonitor = MonitoredGpu<MetalProbe>`. Every caller names the
    /// ALIAS, so the probe is referenced only inside its own file — literally true,
    /// and not a defect. It is the backend-adapter shape this crate was refactored
    /// INTO on 2026-08-20 (`gpu::device_probe`).
    ///
    /// A guard that fires on the pattern it is meant to protect gets weakened until it
    /// says nothing, so an aliased type counts as reached.
    fn surfaced_by_alias(file: &SourceFile, name: &str) -> bool {
        file.production
            .lines()
            .map(|l| split_code_and_comment(l).0)
            .any(|code| code.trim_start().starts_with("pub type ") && code.contains(name))
    }

    /// Does this file stand the type up — an inherent `impl` plus a constructor?
    fn is_machinery(file: &SourceFile, name: &str) -> bool {
        let has_impl = file
            .production
            .lines()
            .map(|l| split_code_and_comment(l).0)
            .any(|code| {
                let t = code.trim_start();
                t.starts_with(&format!("impl {name} ")) || t.starts_with(&format!("impl {name}<"))
            });
        if !has_impl {
            return false;
        }
        file.production
            .lines()
            .map(|l| split_code_and_comment(l).0)
            .any(|code| CONSTRUCTOR_SHAPES.iter().any(|s| code.contains(s)))
    }
}

impl CrateRule for ProductionReachability {
    fn name(&self) -> &'static str {
        "production-reachability"
    }

    fn check_crate(&self, files: &[SourceFile], corpus: &str) -> Vec<Violation> {
        let corpus_counts = Self::identifier_counts(corpus);
        let mut out = Vec::new();
        for file in files {
            // Own-mentions are counted over the SAME text shape as the corpus — raw,
            // test mods included. Counting production-only here against a raw corpus
            // was a real bug in this rule's first cut: a type constructed in its OWN
            // `#[cfg(test)]` mod scored corpus>own and read as externally referenced,
            // so `AgentContext` — the defect this guard exists for — went unreported.
            // Symmetry between the two sides IS the arithmetic.
            let own_counts = Self::identifier_counts(&file.raw);
            for (line, name) in Self::declared_types(file) {
                if !Self::is_machinery(file, &name) || Self::surfaced_by_alias(file, &name) {
                    continue;
                }
                let total = corpus_counts.get(name.as_str()).copied().unwrap_or(0); // absent from the corpus means zero references — the answer, not a stand-in
                let own = own_counts.get(name.as_str()).copied().unwrap_or(0); // same: a name absent from its own file cannot be double-counted
                // The corpus CONTAINS this file, so external references are whatever is
                // left after its own mentions. `<=` not `<`: equal means every mention
                // is its own.
                if total <= own {
                    out.push(Violation {
                        rule: "production-reachability",
                        file: file.rel.clone(),
                        line,
                        source: format!(
                            "pub {name} — constructed nowhere in the workspace outside its own file"
                        ),
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
    use crate::source_hygiene::scan_crate;

    /// What this catches: new public machinery landing with nothing that constructs it
    /// — the defect class that hit five times on 2026-08-20 (see the module header),
    /// every time surfacing as an unrelated-looking downstream failure.
    ///
    /// A RATCHET, not a wall: thousands of lines predate the rule. If this fails
    /// because you added a type, wire it or don't land it. If it fails because you
    /// wired one, LOWER the baseline and take the win.
    #[test]
    fn unwired_public_machinery_never_increases() {
        let violations = scan_crate(&[&ProductionReachability]);
        let count = violations.len();
        assert!(
            count <= BASELINE_UNWIRED,
            "unwired public machinery rose to {count} (baseline {BASELINE_UNWIRED}).\n\
             A pub type with a constructor and no caller anywhere in the workspace is \
             not finished work — it is the `AgentContext` defect (#27), which sat \
             complete-and-tested while the operator had no airc identity and benchmark \
             cards were authored by the wrong citizen.\n\
             First few:\n{}",
            violations
                .iter()
                .take(20)
                .map(|v| format!("  {}:{}  {}", v.file, v.line, v.source))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// What this catches: the two exclusions that decide whether this rule is useful
    /// or useless. If a `pub use` re-export or a doc-comment mention counted as a
    /// reference, the rule would have reported ZERO on the very defect it was written
    /// for — silence that looks like health, the failure mode of every guard here.
    #[test]
    fn a_reexport_or_a_doc_mention_is_not_a_reference() {
        let counts = ProductionReachability::identifier_counts(
            "pub use agent::{AgentContext, AgentMetadata};\n\
             /// `AgentContext` (Slice 4, non-persona kind)\n\
             let ctx = AgentContext::bootstrap(root).await?;\n",
        );
        assert_eq!(
            counts.get("AgentContext").copied(),
            Some(1),
            "only the real construction counts: not the re-export, not the prose"
        );
    }

    /// What this catches: the rule failing on the very shape it was written for.
    ///
    /// This is `AgentContext` (#27) reduced to its essentials, hermetically: declared
    /// with a `bootstrap` constructor, CONSTRUCTED ONLY IN ITS OWN TEST MOD, re-exported
    /// by a parent module, and named in that parent's prose. Every one of those is a
    /// near-miss that made an earlier cut of this rule report nothing — the re-export
    /// and the doc mention inflated the corpus count, and (the subtle one) counting the
    /// file's own mentions over production-only while the corpus was raw made its test
    /// constructions look like external references.
    ///
    /// If this test ever passes vacuously, the guard is decoration.
    #[test]
    fn a_type_constructed_only_in_its_own_tests_is_still_reported() {
        let own = "pub struct AgentContext {}\n\
                   impl AgentContext {\n    pub async fn bootstrap() -> Self { Self {} }\n}\n\
                   #[cfg(test)]\nmod tests {\n    fn t() { let _ = AgentContext::bootstrap(); }\n}\n";
        let production = own.split("#[cfg(test)]").next().unwrap_or(own); // the split marker is present by construction above
        let file = SourceFile {
            rel: "context/agent.rs".into(),
            production: production.into(),
            raw: own.into(),
        };
        let corpus = format!(
            "{own}\npub use agent::{{AgentContext, AgentMetadata}};\n/// `AgentContext` (Slice 4)\n"
        );

        let found = ProductionReachability.check_crate(std::slice::from_ref(&file), &corpus);
        assert_eq!(
            found.len(),
            1,
            "a type whose only mentions are its own file, a re-export and a doc comment \
             must be reported — that is exactly #27"
        );
        assert!(found[0].source.contains("AgentContext"));
    }

    /// What this catches: the machinery predicate widening into a dead-code lint. A
    /// plain data carrier has no constructor to call, so asking who constructs it is
    /// the wrong question — and flagging every wire DTO would bury the real findings.
    #[test]
    fn a_plain_data_carrier_is_not_machinery() {
        let dto = SourceFile {
            rel: "wire.rs".into(),
            production: "pub struct Payload {\n    pub id: String,\n}\n".into(),
            raw: "pub struct Payload {\n    pub id: String,\n}\n".into(),
        };
        assert!(!ProductionReachability::is_machinery(&dto, "Payload"));

        let machine = SourceFile {
            rel: "thing.rs".into(),
            production: "pub struct Runner {}\nimpl Runner {\n    pub fn new() -> Self { Self {} }\n}\n"
                .into(),
            raw: "pub struct Runner {}\nimpl Runner {\n    pub fn new() -> Self { Self {} }\n}\n".into(),
        };
        assert!(ProductionReachability::is_machinery(&machine, "Runner"));
    }
}
