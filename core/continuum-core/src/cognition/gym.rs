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

/// A fetched gym is a DERIVED artifact of (dataset rows × adapter conversion
/// code), but its cache file is keyed only by dataset name — so an adapter fix
/// silently kept serving stale conversions until 2026-08-22, when the DS-1000
/// oracle fix (#2366) shipped while all 1,000 cached tasks still carried the
/// outlawed splicing runner baked into their `setup_shell`. The cure is a
/// fingerprint COMPUTED FROM THE CONVERSION ITSELF (never hand-bumped): hash the
/// adapter's output for one canonical probe row, plus any program it stages
/// out-of-band (AlgoTune's on-disk harness). Any adapter change moves the
/// fingerprint automatically; materialize writes it as a sidecar; resolve
/// refuses a mismatch loudly, naming the one re-fetch command.
pub fn fingerprint_parts(parts: &[&str]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0u8]); // part separator so ("ab","c") != ("a","bc")
    }
    format!("{:x}", h.finalize())
}

/// The one writer for fetched gyms: atomic jsonl write + the fingerprint
/// sidecar. Adapters MUST come through here — the sidecar is what lets
/// [`resolve_gym`] refuse a cache the current adapter didn't produce, and a
/// hand-rolled write that skips it would re-open the stale-oracle hole this
/// seam exists to close. Sidecar lands AFTER the jsonl, so a crash between the
/// two leaves a mismatch that refuses (the safe direction), never a lie.
pub fn write_fetched_gym(
    basename: &str,
    lines: &[String],
    fingerprint: &str,
) -> Result<(std::path::PathBuf, usize), String> {
    let dir = gym_cache_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join(basename);
    let tmp = path.with_extension("jsonl.tmp");
    std::fs::write(&tmp, lines.join("\n") + "\n").map_err(|e| format!("write: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))?;
    std::fs::write(path.with_extension("jsonl.fingerprint"), fingerprint)
        .map_err(|e| format!("write fingerprint sidecar: {e}"))?;
    Ok((path, lines.len()))
}

/// Every fetched-gym basename under the staleness contract — the iteration order
/// of [`fetched_gym_statuses`] and the key set of [`fetched_fingerprint_for`].
const FETCHED_GYM_BASENAMES: &[&str] = &[
    "ds-1000.jsonl",
    "algotune.jsonl",
    "super-masked.jsonl",
    "terminal-bench.jsonl",
];

/// Current adapter fingerprint per fetched-gym basename. A basename NOT listed
/// here has no staleness contract (an operator's hand-placed cache file resolves
/// as before); every `benchmark/fetch`-materialized suite must be. Same
/// one-table-in-one-file shape as [`EMBEDDED_GYMS`].
fn fetched_fingerprint_for(basename: &str) -> Option<String> {
    match basename {
        "ds-1000.jsonl" => Some(crate::cognition::benchmark_ds1000::adapter_fingerprint()),
        "algotune.jsonl" => Some(crate::cognition::benchmark_algotune::adapter_fingerprint()),
        "super-masked.jsonl" => Some(crate::cognition::benchmark_super::adapter_fingerprint()),
        "terminal-bench.jsonl" => {
            Some(crate::cognition::benchmark_terminalbench::adapter_fingerprint())
        }
        _ => None,
    }
}

/// One fetched gym's cache health, as `benchmark/verify` reports it.
pub struct FetchedGymStatus {
    /// Cache file basename (`ds-1000.jsonl`).
    pub basename: String,
    /// `fresh` | `stale` | `not-fetched`.
    pub state: &'static str,
    /// The one command that fixes a non-fresh state, `None` when fresh.
    /// (`not-fetched` is only a problem if you intend to dispatch that suite.)
    pub action: Option<String>,
}

/// Cache health for every contracted fetched gym — the check `resolve_gym`
/// performs per-reference, surfaced for all suites at once so an operator (or a
/// weaker driver) never has to reconstruct tonight's stale-oracle audit by hand.
pub fn fetched_gym_statuses() -> Vec<FetchedGymStatus> {
    FETCHED_GYM_BASENAMES
        .iter()
        .map(|basename| {
            let bench = basename.strip_suffix(".jsonl").unwrap_or(basename); // diagnostic text only, same as the freshness refusal
            let refetch = format!("continuum benchmark/fetch --benchmark {bench}");
            let cached = gym_cache_dir().join(basename);
            if !cached.is_file() {
                return FetchedGymStatus {
                    basename: basename.to_string(),
                    state: "not-fetched",
                    action: Some(refetch),
                };
            }
            let sidecar = std::fs::read_to_string(cached.with_extension("jsonl.fingerprint")).ok();
            let fresh = fetched_gym_freshness(
                basename,
                sidecar.as_deref(),
                fetched_fingerprint_for(basename).as_deref(),
            )
            .is_ok();
            FetchedGymStatus {
                basename: basename.to_string(),
                state: if fresh { "fresh" } else { "stale" },
                action: (!fresh).then_some(refetch),
            }
        })
        .collect()
}

/// Pure freshness verdict for a cached fetched gym, split out so the refusal
/// logic is testable without $HOME games. `sidecar` is the sidecar's contents
/// (None = file absent — a pre-fingerprint or hand-rolled write, equally
/// unprovable, equally refused when a contract exists).
fn fetched_gym_freshness(
    basename: &str,
    sidecar: Option<&str>,
    current: Option<&str>,
) -> Result<(), String> {
    let Some(current) = current else {
        return Ok(()); // no contract for this basename — operator artifact, resolves as-is
    };
    match sidecar {
        Some(s) if s.trim() == current => Ok(()),
        _ => {
            let bench = basename.strip_suffix(".jsonl").unwrap_or(basename); // diagnostic text only: a suffixless basename names itself in the re-fetch hint, nothing is budgeted on it
            Err(format!(
                "fetched gym '{basename}' is STALE: its cache was not produced by the \
                 current adapter (sidecar {found}, adapter {current}). Grading tasks \
                 staged from it would use an outdated oracle — the exact defect that \
                 shipped 1,000 splicing DS-1000 runners on 2026-08-22. \
                 Re-materialize: `continuum benchmark/fetch --benchmark {bench}`",
                found = sidecar.map_or("missing".to_string(), |s| s.trim().to_string()),
            ))
        }
    }
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
        // Freshness gate: refuse a cache the CURRENT adapter didn't produce.
        // Serving it would stage tasks under an outdated oracle (#2366's stale-
        // cache shadow) — refusing names the one command that re-materializes.
        let base = Path::new(reference)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(reference); // non-UTF8 reference: fall through as no-contract, same as embedded_for
        let sidecar = std::fs::read_to_string(cached.with_extension("jsonl.fingerprint")).ok();
        fetched_gym_freshness(base, sidecar.as_deref(), fetched_fingerprint_for(base).as_deref())?;
        let text = std::fs::read_to_string(&cached)
            .map_err(|e| format!("fetched gym '{}' could not be read: {e}", cached.display()))?;
        return Ok((cached.display().to_string(), text));
    }
    // (2) A committed gym baked into the binary — CWD-/deployment-independent.
    if let Some((name, bytes)) = embedded_for(reference) {
        return Ok((format!("embedded:{name}"), bytes.to_string()));
    }
    // (2.5) GENERATED gyms — deterministic in-binary generators (no JSONL to
    // commit, no blobs in git; byte-stable by seeded construction). vision-qa:
    // the input-side vision benchmark (see cognition::vision_gym).
    if reference == "vision-qa" || reference == "vision-qa.jsonl" {
        return Ok((
            "generated:vision-qa".to_string(),
            crate::cognition::vision_gym::vision_qa_jsonl().to_string(),
        ));
    }
    // (3a) SIGNPOST, not a dead-end: a SWE-class benchmark reached the GYM resolver means
    // the caller used the wrong verb — SWE does NOT run through `benchmark/round`/the gym
    // eval path, it runs through the kanban adapter `benchmark/dispatch`. The old error
    // ("not a committed gym. Committed gyms: <rust list>") sent every driver — human and
    // Opus — into days of archaeology looking for a SWE gym that does not exist. Point at
    // the right verb instead (measured 2026-08-25: this refusal, verbatim, cost a session).
    if reference.contains("swe-bench") || reference.contains("swe-rebench") {
        return Err(format!(
            "'{reference}' is a SWE-bench benchmark — it does NOT run through the gym path \
             (benchmark/round resolves GYMS only). SWE runs through the kanban adapter:\n  \
             continuum benchmark/dispatch --name {reference} \
             --instances '[\"<instance_id>\", ...]' --assignees '[\"<persona>\"]' \
             --drive detached_solve --force\n\
             Step-by-step + failure modes: benchmarks/swe/RUNBOOK.md"
        ));
    }
    // (3b) Neither file, cache, committed gym, nor SWE — fail loud with everything tried.
    Err(format!(
        "eval_set '{reference}' could not be resolved: no such file on disk \
         (cwd={cwd}), and it is not a committed gym. Committed gyms: {names}. \
         (A SWE-bench collection? Use `benchmark/dispatch`, not the gym path — \
         see benchmarks/swe/RUNBOOK.md.)",
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

    // what this catches: the stale-fetched-gym class (regression for #2366's
    // shadow — the DS-1000 oracle fix shipped while all 1,000 cached tasks still
    // staged the outlawed splicing runner). A cache whose sidecar doesn't match
    // the CURRENT adapter fingerprint (or has no sidecar at all) must refuse to
    // resolve, naming the one re-fetch command; a matching sidecar passes; a
    // basename with no contract (operator artifact) passes untouched.
    #[test]
    fn a_stale_fetched_gym_refuses_to_resolve_and_names_the_refetch_command() {
        let current = crate::cognition::benchmark_ds1000::adapter_fingerprint();
        // deterministic: same code → same fingerprint, every call
        assert_eq!(current, crate::cognition::benchmark_ds1000::adapter_fingerprint());

        // fresh cache: sidecar matches → resolves
        assert!(fetched_gym_freshness("ds-1000.jsonl", Some(&current), Some(&current)).is_ok());
        // stale cache: sidecar from an older adapter → refuse, name the command
        let err = fetched_gym_freshness("ds-1000.jsonl", Some("deadbeef"), Some(&current))
            .expect_err("a mismatched fingerprint must refuse");
        assert!(err.contains("benchmark/fetch --benchmark ds-1000"), "{err}");
        assert!(err.contains("STALE"), "{err}");
        // pre-fingerprint cache (no sidecar): equally unprovable, equally refused
        assert!(fetched_gym_freshness("ds-1000.jsonl", None, Some(&current)).is_err());
        // no contract for this basename: an operator's hand-placed file resolves as before
        assert!(fetched_gym_freshness("my-custom.jsonl", None, None).is_ok());

        // every registered fetched gym computes a real fingerprint (a panic or
        // empty string here means an adapter broke its probe conversion)
        for name in ["ds-1000.jsonl", "algotune.jsonl", "super-masked.jsonl"] {
            let fp = fetched_fingerprint_for(name).expect("registered");
            assert_eq!(fp.len(), 64, "{name}: sha256 hex expected, got '{fp}'");
        }
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

    // what this catches: a SWE-bench benchmark reaching the GYM resolver (the wrong verb —
    // benchmark/round instead of benchmark/dispatch) is a SIGNPOST, not a dead-end. The old
    // "not a committed gym. Committed gyms: <rust list>" sent every driver into days of
    // archaeology (measured 2026-08-25). The error must name benchmark/dispatch + the runbook.
    #[test]
    fn swe_benchmark_name_points_at_dispatch_not_a_dead_end() {
        for name in ["swe-bench-verified", "swe-bench-lite", "swe-rebench"] {
            let err = resolve_gym(name).expect_err("SWE is not a gym — must fail loud");
            assert!(err.contains("benchmark/dispatch"), "{name}: points at the right verb: {err}");
            assert!(err.contains("RUNBOOK"), "{name}: points at the runbook: {err}");
            assert!(!err.contains("Committed gyms:"), "{name}: NOT the generic dead-end: {err}");
        }
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
