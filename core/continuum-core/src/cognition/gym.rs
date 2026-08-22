//! `cognition::gym` — CWD- and deployment-independent resolution of a gym
//! (`eval_set`) reference to its JSONL text.
//!
//! ## Why this exists
//!
//! A gym is a committed artifact under `docs/genome/*.jsonl`. The recipe that
//! rides a gene to the L3 sentinel carries its gym as an `eval_set` string —
//! authored the natural way, repo-relative (`docs/genome/coder-eval.jsonl`).
//! But `cognition/eval` runs inside the headless core, whose CWD is the *crate*
//! dir (`core/continuum-core/`) under `cargo run`, the repo root under a
//! manual launch, and — for a public deployed binary — a machine with **no repo
//! checkout at all**. A bare `std::fs::read_to_string(eval_set)` therefore
//! resolves a committed gym DIFFERENTLY depending on where the core was started,
//! and not at all in production. That is exactly the bug the embedded
//! `DEFAULT_EVAL_SET_BYTES` in [`super::eval`] was created to kill — but only
//! for the *defaulted* gym; an explicitly-NAMED committed gym still hit the
//! fragile from-disk branch. The live L3 verification of slice 1 caught this:
//! a recipe that explicitly declared `docs/genome/coder-eval.jsonl` failed loud
//! with `No such file or directory` because the core ran from the crate dir.
//!
//! ## The resolution contract (deterministic, fail-loud — no silent degrade)
//!
//! For a reference `r`, in order:
//!   1. If `r` names an **existing file on disk** → read it. An operator
//!      iterating on a *custom* gym (absolute path, or a path that resolves
//!      from their CWD) keeps full control and overrides the embedded copy.
//!   2. Else if `r`'s basename is a **committed gym** baked into the binary →
//!      use the embedded bytes. CWD- and deployment-independent: the committed
//!      gym resolves identically from the crate dir, the repo root, or a
//!      deployed binary with no checkout.
//!   3. Else → **fail loud**, naming the reference AND listing every embedded
//!      gym. A typo'd or vanished gym never silently degrades to a default or a
//!      smaller set ([[fallbacks-are-illegal-fail-loud]]).
//!
//! Step 1 before step 2 means a genuine on-disk custom gym always wins; for a
//! committed gym the on-disk copy (when present) and the embedded copy are the
//! same bytes rebuilt together, so the result is identical either way — the
//! reliability win is that the embedded copy makes the committed gym resolvable
//! when the file is NOT reachable.

use std::path::Path;

/// Every committed gym, baked into the binary: `(basename, bytes)`. Adding a
/// committed gym under `docs/genome/` is one `include_str!` line here — that is
/// the single edit that makes it referenceable as an `eval_set` from any CWD or
/// a deployed binary. Keyed by basename so a recipe may reference a gym by its
/// repo path (`docs/genome/coder-eval.jsonl`) or bare name (`coder-eval.jsonl`)
/// and resolve to the same bytes.
const EMBEDDED_GYMS: &[(&str, &str)] = &[
    (
        "coder-eval.jsonl",
        include_str!("../../../../docs/genome/coder-eval.jsonl"),
    ),
    (
        "coder-write-eval.jsonl",
        include_str!("../../../../docs/genome/coder-write-eval.jsonl"),
    ),
    (
        "humaneval-rs.jsonl",
        include_str!("../../../../docs/genome/humaneval-rs.jsonl"),
    ),
    (
        "hard-rs.jsonl",
        include_str!("../../../../docs/genome/hard-rs.jsonl"),
    ),
    (
        // hard-rs-big: the 44-task widening of hard-rs — the 8 originals plus 36 new hard
        // algorithmic tasks (DP, two-pointer, sliding-window, matrix, string, graph-flood),
        // every one reference-verified through rustc (a wrong test can't reach the exam) and
        // zero fn-name overlap with the training pools. ±1/44 = ±0.023 resolution vs hard-rs's
        // ±0.125, so a genome forge's small lift is finally legible. Embedded here so it
        // resolves from any CWD / a deployed binary — the relative-path refusal (#194 sibling)
        // that cost this session hours can't recur.
        "hard-rs-big.jsonl",
        include_str!("../../../../docs/genome/hard-rs-big.jsonl"),
    ),
    (
        // frontier-rs: the "strive-toward" tier — real algorithms (Levenshtein,
        // Dijkstra, O(n log n) LIS, topological sort, arbitrary-precision add, a
        // precedence-climbing calculator, `.`/`*` regex matching). Problems a small
        // local model rarely nails one-shot; the system's write→compile→test→read-
        // error→fix loop is where it earns them. Every task's assertions are
        // reference-verified (a wrong test poisons the benchmark).
        "frontier-rs.jsonl",
        include_str!("../../../../docs/genome/frontier-rs.jsonl"),
    ),
    (
        // games-rs: OUR games benchmark — the tier public benchmarks lack (they grade
        // an agent PLAYING, not BUILDING). Auto-verifiable game LOGIC: a Conway step,
        // a tic-tac-toe/connect-4 win-checker, a 2048 merge, knight moves, minesweeper
        // counts. Every task reference-verified. The runnable complement to the Conway/
        // Snake whole-game project cards.
        "games-rs.jsonl",
        include_str!("../../../../docs/genome/games-rs.jsonl"),
    ),
    (
        // webdev-rs: OUR functional web-dev benchmark — the tier public UI benchmarks lack (they
        // grade an agent NAVIGATING a site, or a screenshot's pixel similarity, not whether the
        // agent BUILT a UI that structurally WORKS). Each task asks the persona to write a
        // complete `index.html`, then grades what it ACTUALLY RENDERED by OBSERVING it through the
        // eye-node (`perception/observe`) and scoring the element tree against a UiCheck spec. The
        // structure tree is text every model reads, so a lesser local model competes on the same
        // rendered-UI facts as Claude. Proves images (perception) + code-dev in one benchmark.
        "webdev-rs.jsonl",
        include_str!("../../../../docs/genome/webdev-rs.jsonl"),
    ),
    (
        // tool-bugfix-rs: the first TOOL-USING gym. Every other gym is spoken-graded
        // codegen (no tools offered — needs_tools is false). Each task here seeds a
        // BUGGY source file into the workspace (`setup_shell`) and grades the persona's
        // EDITED file (`dod_shell` — a fresh cheat-proof harness that include!()s her
        // file, asserts, compiles, runs), so she MUST read → edit → compile → run with
        // her hands. That makes `needs_tools` true → the native tool surface is offered
        // → this is the ONLY benchmark whose score depends on tool USE, the honest
        // instrument for the offer-name A/B (#204) [[tool-naming-meet-their-training-alias-or-redirect]].
        "tool-bugfix-rs.jsonl",
        include_str!("../../../../docs/genome/tool-bugfix-rs.jsonl"),
    ),
];

/// Look up a committed gym's embedded bytes by the basename of `reference`.
fn embedded_for(reference: &str) -> Option<(&'static str, &'static str)> {
    let base = Path::new(reference)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(reference);
    EMBEDDED_GYMS
        .iter()
        .find(|(name, _)| *name == base)
        .copied()
}

/// Comma-joined list of every embedded gym basename, for fail-loud diagnostics.
fn embedded_names() -> String {
    EMBEDDED_GYMS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolve a gym (`eval_set`) reference to `(origin_label, jsonl_text)`.
///
/// `origin_label` is for reporting only — either the on-disk path read, or
/// `embedded:<basename>` for a baked committed gym. See the module docs for the
/// resolution order. Returns a human-readable error string on failure (caller
/// wraps it in its own `CommandError`), naming the reference and the candidates
/// tried — never silently degrades.
/// Where FETCHED gym suites materialize (`benchmark/fetch` writes converted
/// external collections here — e.g. `ds-1000.jsonl`). Sibling of the SWE cache,
/// same eviction story owner (`~/.continuum/benchmarks` is a governed cache class).
pub fn gym_cache_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string()); // no $HOME (systemd/minimal env): cwd-relative cache beats a panic — same fallback the sibling cache dirs use
    std::path::PathBuf::from(home).join(".continuum/benchmarks/gym")
}

pub fn resolve_gym(reference: &str) -> Result<(String, String), String> {
    // (1) An existing on-disk file wins — a custom gym the operator points at.
    if Path::new(reference).is_file() {
        let text = std::fs::read_to_string(reference)
            .map_err(|e| format!("eval_set '{reference}' exists but could not be read: {e}"))?;
        return Ok((reference.to_string(), text));
    }
    // (1.5) A FETCHED gym in the benchmark cache — external collections
    // (ds-1000, …) that `benchmark/fetch` converted onto the gym rails. After
    // the on-disk check (an operator's explicit file still wins) and before the
    // embedded registry (a fetched suite must not be shadowed by a stale
    // committed copy of the same name).
    let cached = gym_cache_dir().join(reference);
    if cached.is_file() {
        let text = std::fs::read_to_string(&cached)
            .map_err(|e| format!("fetched gym '{}' could not be read: {e}", cached.display()))?;
        return Ok((cached.display().to_string(), text));
    }
    // (2) A committed gym baked into the binary — CWD-/deployment-independent.
    if let Some((name, bytes)) = embedded_for(reference) {
        return Ok((format!("embedded:{name}"), bytes.to_string()));
    }
    // (3) Neither — fail loud with everything tried.
    Err(format!(
        "eval_set '{reference}' could not be resolved: no such file on disk \
         (cwd={cwd}), and it is not a committed gym. Committed gyms: {names}.",
        cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string()),
        names = embedded_names(),
    ))
}

/// Maps a trait (`DomainClassifier` domain — the `trait_kind` of a gene's
/// `(persona_id, trait_kind, base_model)` bucket) → the committed gym that
/// MEASURES it. This is the recipe's `{trait → gym}` edge: it lets the
/// AUTOMATIC producer path stamp an `eval_set` onto a gene it dispatches, so the
/// L3 sentinel can A/B and adopt it. Without this map the producer omitted
/// `eval_set` (→ `None`), and the sentinel correctly REFUSED to adopt every
/// auto-produced gene as unmeasurable — so only a hand-dispatched job (with a
/// hand-declared gym) could ever close the loop.
///
/// A trait with NO entry returns `None` — and that is the honest answer, NOT a
/// gap to paper over: we have no gym that measures (say) `conversation`
/// improvement, so the producer declares no gym and the sentinel refuses to
/// adopt rather than grading a conversation gene against a coder set
/// ([[fallbacks-are-illegal-fail-loud]]). Adding a measured trait is one line
/// here, paired with its committed gym in [`EMBEDDED_GYMS`] (the unit test below
/// asserts every mapped gym resolves, so a typo fails at test time, not in prod).
const TRAIT_GYMS: &[(&str, &str)] = &[("code", "docs/genome/coder-eval.jsonl")];

/// The committed gym that measures `trait_kind`, or `None` when no gym measures
/// that trait yet. See [`TRAIT_GYMS`]. The returned reference is resolvable via
/// [`resolve_gym`] (CWD-/deployment-independent).
pub fn gym_for_trait(trait_kind: &str) -> Option<&'static str> {
    TRAIT_GYMS
        .iter()
        .find(|(trait_name, _)| *trait_name == trait_kind)
        .map(|(_, gym)| *gym)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a committed gym referenced by its repo-relative path
    // resolves to the EMBEDDED bytes regardless of CWD — the exact reliability
    // bug found in live L3 verification (core ran from the crate dir, the
    // repo-relative path did not exist there, eval failed loud). Embedded
    // resolution makes the committed gym CWD- and deployment-independent.
    #[test]
    fn committed_gym_resolves_from_embedded_regardless_of_cwd() {
        let (origin, text) = resolve_gym("docs/genome/coder-eval.jsonl")
            .expect("committed gym must resolve from the embedded registry");
        assert_eq!(origin, "embedded:coder-eval.jsonl");
        assert!(!text.trim().is_empty(), "embedded gym must carry tasks");
        // Bare basename resolves to the same embedded bytes.
        let (_, text2) = resolve_gym("coder-eval.jsonl").unwrap();
        assert_eq!(text, text2);
    }

    // what this catches: a typo'd / nonexistent gym FAILS LOUD naming the
    // reference and the embedded candidates — never silently degrades to a
    // default or a smaller set ([[fallbacks-are-illegal-fail-loud]]).
    #[test]
    fn unknown_gym_fails_loud_listing_candidates() {
        let err = resolve_gym("docs/genome/does-not-exist.jsonl")
            .expect_err("unknown gym must fail loud");
        assert!(err.contains("does-not-exist.jsonl"), "names the reference");
        assert!(
            err.contains("coder-eval.jsonl"),
            "lists embedded candidates"
        );
    }

    // what this catches: the `code` trait resolves to its measuring gym, an
    // unmapped trait honestly returns None (→ producer omits eval_set → sentinel
    // refuses to adopt, never grades against the wrong gym), and EVERY mapped gym
    // actually resolves — a typo in TRAIT_GYMS fails here, not in production when
    // the automatic loop tries to eval an auto-produced gene.
    #[test]
    fn trait_gym_map_is_honest_and_every_target_resolves() {
        assert_eq!(
            gym_for_trait("code"),
            Some("docs/genome/coder-eval.jsonl"),
            "the code trait must map to its committed gym"
        );
        assert_eq!(
            gym_for_trait("conversation"),
            None,
            "an unmeasured trait must return None — not a wrong-gym fallback"
        );
        for (trait_name, gym) in super::TRAIT_GYMS {
            resolve_gym(gym).unwrap_or_else(|e| {
                panic!("trait '{trait_name}' maps to gym '{gym}' which does not resolve: {e}")
            });
        }
    }

    // what this catches: an existing on-disk custom gym is read from disk (step
    // 1 wins), so an operator iterating on a custom set keeps full control.
    #[test]
    fn existing_on_disk_file_is_read_directly() {
        let dir = std::env::temp_dir();
        let path = dir.join("continuum-gym-resolve-test.jsonl");
        std::fs::write(&path, "{\"prompt\":\"x\",\"expect\":\"y\"}\n").unwrap();
        let path_str = path.to_str().unwrap();
        let (origin, text) = resolve_gym(path_str).unwrap();
        assert_eq!(origin, path_str);
        assert!(text.contains("\"prompt\""));
        let _ = std::fs::remove_file(&path);
    }
}
