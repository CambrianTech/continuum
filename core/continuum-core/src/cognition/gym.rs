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
pub fn resolve_gym(reference: &str) -> Result<(String, String), String> {
    // (1) An existing on-disk file wins — a custom gym the operator points at.
    if Path::new(reference).is_file() {
        let text = std::fs::read_to_string(reference)
            .map_err(|e| format!("eval_set '{reference}' exists but could not be read: {e}"))?;
        return Ok((reference.to_string(), text));
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
        assert!(err.contains("coder-eval.jsonl"), "lists embedded candidates");
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
