//! Crate-wide source hygiene: **rules that make a bad shape unrepresentable, not
//! merely discouraged.**
//!
//! # Why this module exists
//!
//! CLAUDE.md already carries hygiene laws in prose — "every unwrap requires an
//! inline justification", "implement the probe, never the monitor", "no new
//! hardcoded context constant". Prose stops nobody. Every one of those laws that
//! actually holds today holds because someone wrote a TEST that scans the source,
//! and every one that gets re-broken is one that didn't.
//!
//! There were already two such scanners in tree, each hand-rolled inside the module
//! it happened to guard: the de-hardcode guard in `cognition/context_budget.rs` and
//! the module-wiring audit in `runtime/registry.rs`. Two copies of "walk src/, strip
//! comments, match a predicate, report offenders" is the same duplication this
//! codebase forbids everywhere else — so the third rule got the SEAM instead of a
//! third copy.
//!
//! - [`SourceFile`] — one file, already split into its production and test halves.
//! - [`SourceRule`] — what a rule supplies, and all it supplies: a predicate over
//!   one file. The walking, the comment handling, the reporting are the scanner's.
//! - [`scan`] — walks `src/` once and applies every rule.
//!
//! The two existing scanners should migrate onto this seam (they predate it); doing
//! that is a follow-up, not a reason to hand-roll a fourth.

pub mod boundary_serialization;
pub mod budget_discipline;
pub mod cognition_names_persona;
pub mod context_codec;
pub mod continuum_home;
pub mod crate_policy;
pub mod crate_wiring;
pub mod dev_only_deps;
pub mod grab_bag;
pub mod file_size;
pub mod identity_discipline;
pub mod layer_discipline;
pub mod lexer;
pub mod module_cycles;
pub mod os_branches;
pub mod production_reachability;
pub mod ratchet;
pub mod tenant_neutrality;
pub mod time_bounds;
pub mod test_mod_singularity;
pub mod tls_stack;
pub mod subprocess_spawn;
pub mod unwrap_justification;
pub mod vendor_pin;
pub mod wall_clock;

use std::path::{Path, PathBuf};

use lexer::{production_half, split_code_and_comment, without_signature};

/// One source file, pre-split so a rule never has to re-derive the split.
pub struct SourceFile {
    /// Path relative to `src/`, forward-slashed — the form a rule's allow-list and
    /// a failure message should both use.
    pub rel: String,
    /// The PRODUCTION half: everything before the file's first column-zero `#[cfg(test)]`.
    ///
    /// A test-only helper gated mid-file is scanned as production: this errs toward a
    /// little noise, never toward the silence an early cut produced (see [`production_half`]).
    pub production: String,
    /// The WHOLE file, test mods included.
    ///
    /// Carried alongside `production` rather than re-read on demand because a rule
    /// that compares one text shape against another must be able to get both from the
    /// same read. (The first cut of [`production_reachability`] re-read the file and
    /// fell back to `production` when the read failed — a fallback that silently
    /// swapped the shape and reintroduced the exact miscount it was written to fix.)
    pub raw: String,
    /// Path from the root of the scanned checkout (`core/continuum-cognition/src/...`):
    /// the file's identity in the repository, which `rel` is not (a module crate's `rel`
    /// carries no crate name). A PR's changed files are matched on it.
    pub repo: String,
}

impl SourceFile {
    /// An in-memory file for a rule's own tests.
    #[cfg(test)]
    pub fn fixture(rel: &str, production: &str, raw: &str) -> Self {
        Self { rel: rel.to_owned(), production: production.to_owned(), raw: raw.to_owned(), repo: format!("fixture/{rel}") }
    }

    /// The module a file's code belongs to, as the path that names it: `x` for `x.rs`,
    /// `x/mod.rs`, and for a test module moved out of it (`x/tests.rs`,
    /// `x/tests/mod.rs`, `x/tests/part_2.rs`, each marked `#![cfg(test)]`). A rule that
    /// asks "outside its own file" asks "outside its owning module": a test that moved
    /// to a sibling file to keep its owner readable is still that owner's own test.
    pub fn owning_module(&self) -> &str {
        let rel = self.rel.as_str();
        if self.is_moved_test() {
            let dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
            return dir.strip_suffix("/tests").unwrap_or(dir); // unwrap_or: `x/tests.rs` is already at `x`
        }
        rel.strip_suffix("/mod.rs")
            .or_else(|| rel.strip_suffix(".rs"))
            .unwrap_or(rel) // unwrap_or: every scanned file ends in .rs
    }

    /// A whole file of test code: a test module moved out of its owner, which declares
    /// `#![cfg(test)]` before any item.
    pub fn is_moved_test(&self) -> bool {
        for line in self.raw.lines() {
            let code = split_code_and_comment(line).0.trim();
            if code.is_empty() {
                continue;
            }
            if !code.starts_with("#![") {
                return false;
            }
            if code.starts_with("#![cfg(test)]") {
                return true;
            }
        }
        false
    }

    /// Production lines paired with their 1-indexed line number in the whole file,
    /// so a violation can point at a real editor line.
    pub fn production_lines(&self) -> impl Iterator<Item = (usize, &str)> {
        self.production.lines().enumerate().map(|(i, l)| (i + 1, l))
    }

    /// Build a `SourceFile` from an inline snippet — fixture for a rule's own
    /// unit tests, so predicate regressions are pinned without touching disk.
    #[cfg(test)]
    pub fn for_test(rel: &str, production: &str) -> Self {
        Self::fixture(rel, production, production)
    }
}

/// One violation, in the shape a failure message can print directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: &'static str,
    pub file: String,
    pub line: usize,
    /// The offending source line, trimmed — so the message shows the code, not just
    /// a coordinate.
    pub source: String,
}

/// What a hygiene rule supplies. Implement this; do not walk the tree yourself.
pub trait SourceRule {
    fn name(&self) -> &'static str;
    fn check(&self, file: &SourceFile) -> Vec<Violation>;
}

/// The production lines of `file` whose code (comments removed) `hit` names a finding for, as
/// violations of `rule`, located by repository path. A function's SIGNATURE is a definition,
/// not a use, so it is never a site; its body is code like any other, including a body on
/// the signature's own line (`fn cap(&self) -> usize { self.grant.take(8) }`, Codex on #5609).
pub fn sites(file: &SourceFile, rule: &'static str, hit: impl Fn(&str) -> Option<String>) -> Vec<Violation> {
    file.production_lines()
        .filter_map(|(line, text)| {
            let code = without_signature(split_code_and_comment(text).0);
            hit(code).map(|what| Violation { rule, file: file.repo.clone(), line, source: format!("{what}: {}", text.trim()) })
        })
        .collect()
}


/// `src/` of this crate, resolved from the manifest so the scan is independent of
/// the process's working directory (a test's cwd is not the crate root under every
/// runner).
pub fn crate_src_root() -> PathBuf {
    // THE SCANNED CRATE, NOT THIS ONE. These rules used to live inside
    // `continuum-core`, where `CARGO_MANIFEST_DIR` and "the tree under audit" were
    // the same directory. They are not the same any more: this crate's manifest dir
    // is `core/source-hygiene`, and a scan rooted there would walk 2,173 lines of
    // scanner and report a clean bill of health on a codebase it never opened —
    // every ratchet silently passing at 0 findings. The audited tree is named
    // explicitly so that failure is impossible rather than merely unlikely.
    audited_crate_root().join("src")
}

/// `core/continuum-core` — the crate these rules audit, resolved from this crate's
/// own manifest so the answer never depends on the process's working directory.
fn audited_crate_root() -> PathBuf {
    // A ratchet is judged by measuring the base tree with the head's rules
    // (`ratchet.rs`): `SOURCE_HYGIENE_CORE` names that tree's `core` directory.
    if let Some(core) = std::env::var_os("SOURCE_HYGIENE_CORE") {
        return PathBuf::from(core).join(AUDITED_CRATE);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // core/source-hygiene -> core
        .map(|core| core.join(AUDITED_CRATE))
        // unwrap_or_else: a manifest dir with no parent cannot happen under cargo,
        // and inventing a relative fallback would scan the wrong tree silently.
        .unwrap_or_else(|| PathBuf::from(AUDITED_CRATE))
}

/// The root of the checkout under audit (head, or the base tree a ratchet measures).
pub(crate) fn scanned_checkout() -> PathBuf {
    let Some(checkout) = audited_crate_root().ancestors().nth(2).map(Path::to_path_buf) else {
        panic!("{} has no checkout above core/: repository paths cannot be named", audited_crate_root().display());
    };
    checkout
}

/// The crate under audit. One name, one place — a second spelling of this is how a
/// rule ends up scanning a directory that does not exist and reporting zero.
pub(crate) const AUDITED_CRATE: &str = "continuum-core";

/// Crates split out of `continuum-core` (card a31492cb). A split must never move code out
/// of the gates' sight, so their files are audited too, with the crate name as a path
/// prefix (`continuum-base/time/mod.rs`): totals stay continuous across a cut and every
/// per-path rule stays unambiguous. Derived from the workspace ([`crate_policy`]).
pub fn split_crates() -> Result<Vec<String>, crate_policy::PolicyError> {
    let (members, policy) = crate_policy::workspace()?;
    Ok(crate_policy::split(members, policy).iter().map(|m| m.dir.clone()).collect())
}

/// Crates cut out of `continuum-core` that keep core's module names: `continuum-kernel`'s
/// `runtime/`, `routing/`, `orm/`… are re-exported by core as `crate::runtime` and so on, so
/// their files are audited UNPREFIXED (`runtime/registry.rs`), exactly as before the cut.
/// Every allow-list, ring and count stays continuous; prefixing them would drop 78k lines out
/// of the ring table and the path allow-lists, and the counts would fall for no change.
/// Derived from the workspace ([`crate_policy`]), never listed: a new cut is one `members`
/// line. continuum-core's tree-walking guards (`test_env::source_tree`) read it too.
pub fn module_crates() -> Result<Vec<String>, crate_policy::PolicyError> {
    let (members, policy) = crate_policy::workspace()?;
    Ok(crate_policy::modules(members, policy).iter().map(|m| m.dir.clone()).collect())
}

/// Every file the per-file and crate-wide rules audit: `continuum-core/src` with paths
/// relative to it, then each split crate's `src` with its name prefixed. A listed crate
/// whose `src` is missing fails loudly rather than auditing nothing.
pub fn audited_files() -> Result<Vec<SourceFile>, crate_policy::PolicyError> {
    let checkout = scanned_checkout();
    let mut files = collect_files(&checkout, &format!("core/{AUDITED_CRATE}/src"));
    for name in &split_crates()? {
        let dir = format!("core/{name}/src");
        assert!(checkout.join(&dir).is_dir(), "split crate {name} has no src at {dir}: the gates would audit nothing");
        files.extend(collect_files(&checkout, &dir).into_iter().map(|f| SourceFile { rel: format!("{name}/{}", f.rel), ..f }));
    }
    for name in &module_crates()? {
        // A tree from before the cut has no such crate: its modules are still in
        // continuum-core/src and were collected above, so nothing goes unaudited.
        let dir = format!("core/{name}/src");
        if checkout.join(&dir).is_dir() {
            files.extend(collect_files(&checkout, &dir).into_iter().filter(|f| f.rel != "lib.rs"));
        }
    }
    Ok(files)
}

/// What a scan reports when the crates to audit cannot be derived: one violation naming the
/// manifest, so the ratchet fails loudly and nothing is silently skipped.
fn unreadable_policy(error: &crate_policy::PolicyError) -> Vec<Violation> {
    vec![Violation { rule: "crate_policy", file: error.manifest.display().to_string(), line: 0, source: error.reason.clone() }]
}

/// The repository root, found by ANCHOR rather than by counting directories up.
///
/// Every caller here used to say `../..` or `ancestors().nth(2)`, which was correct
/// only because the scanner happened to sit two levels down. That assumption survived
/// this crate's move by luck — `core/source-hygiene` is also two levels down — and
/// Cormac flagged it in review as the one path the empty-scan guard cannot see: a
/// rule whose root is wrong but EXISTS scans a real directory and finds nothing.
///
/// Walking up to the directory that actually holds the workspace manifest removes the
/// assumption instead of documenting it, so moving this crate anywhere cannot break it.
/// `None` when no ancestor carries the anchor, which callers turn into a loud failure
/// rather than a silent empty scan.
pub fn repo_root() -> Option<PathBuf> {
    workspace_root_above(&audited_crate_root())
}

/// The repository root of THIS checkout (the rules), whatever tree is being audited.
pub fn checkout_root() -> Option<PathBuf> {
    workspace_root_above(Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// The one anchor walk: the nearest ancestor of `start` holding the workspace manifest.
fn workspace_root_above(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(WORKSPACE_ANCHOR).is_file() && dir.join("core").is_dir())
        .map(Path::to_path_buf)
}

/// What marks the repository root. The workspace manifest is the honest anchor: it is
/// the file that DEFINES the tree these rules audit.
const WORKSPACE_ANCHOR: &str = "Cargo.toml";

/// Every `src/` directory in the workspace — this crate's, its sibling crates', and
/// the apps'.
///
/// **Why a rule ever needs more than `crate_src_root`:** some hygiene questions are
/// not answerable from one crate. "Is this `pub` type referenced anywhere?" is the
/// motivating case — `continuum-core` is a LIBRARY, so a type with no in-crate caller
/// may be perfectly well wired from `apps/cli` or `continuum-mcp`. A guard that
/// reported those as dead would be wrong on its first run and deleted by its second.
///
/// So the asymmetry is deliberate and load-bearing: **violations are only ever raised
/// against `crate_src_root()`, while REFERENCES are searched across every root here.**
/// Widening the reference corpus can only ever REMOVE findings, never add one.
///
/// Resolved from the manifest (`core/continuum-core` → repo root two levels up) so the
/// scan does not depend on the process's working directory. Roots that do not exist
/// are skipped, so this stays correct if the layout changes under it.
pub fn workspace_src_roots() -> Vec<PathBuf> {
    // The audited crate's src, not this crate's — same correction as
    // `crate_src_root`: violations are only ever raised against that tree.
    let mut roots = vec![crate_src_root()];
    // Anchored, not counted: see `repo_root`. A missing anchor yields the audited
    // root alone rather than a wrong tree full of nothing.
    let Some(repo) = repo_root() else {
        return roots;
    };
    for group in ["core", "apps"] {
        let Ok(entries) = std::fs::read_dir(repo.join(group)) else {
            continue;
        };
        for entry in entries.flatten() {
            let src = entry.path().join("src");
            // The crate's own root is already first; skip the duplicate rather than
            // paying to read every file in it twice.
            if src.is_dir() && src != roots[0] {
                roots.push(src);
            }
        }
    }
    roots
}


/// Read every `.rs` file under `dir` (a directory relative to `checkout`, `core/<crate>/src`),
/// pre-split into production and test halves; each file's repository path is `dir/<rel>`.
///
/// Unreadable files are skipped rather than failing the scan: a hygiene guard must
/// not turn a transient IO error into a red build that says nothing about hygiene.
pub fn collect_files(checkout: &Path, dir: &str) -> Vec<SourceFile> {
    let root = checkout.join(dir);
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(under_root) = path.strip_prefix(&root) else {
                panic!("{} was found walking {} but is not under it", path.display(), root.display());
            };
            let rel = under_root.to_string_lossy().replace('\\', "/");
            let production = production_half(&src);
            let repo = format!("{dir}/{rel}");
            out.push(SourceFile { rel, production, raw: src, repo });
        }
    }
    out
}

/// Walk this crate's `src/` once and apply every per-file rule to every `.rs` file.
pub fn scan(rules: &[&dyn SourceRule]) -> Vec<Violation> {
    let files = match audited_files() {
        Ok(files) => files,
        Err(e) => return unreadable_policy(&e),
    };
    let mut out = Vec::new();
    for file in &files {
        for rule in rules {
            out.extend(rule.check(file));
        }
    }
    sort_violations(&mut out);
    out
}

/// A rule that needs to see the WHOLE crate at once, not one file at a time.
///
/// [`SourceRule`] covers the common case — a predicate over a single file — and every
/// rule that CAN be expressed that way should be, because per-file rules are cheap and
/// trivially testable. But some hygiene questions are irreducibly cross-file: "does
/// anything reference this?" cannot be answered from the file that declares it. That
/// is this trait, and the reason it is a separate one rather than a widened
/// `SourceRule`: a per-file rule that received the whole crate would invite every
/// future rule to reach for global state it does not need.
pub trait CrateRule {
    fn name(&self) -> &'static str;
    /// `files` are this crate's, the only source of violations. `corpus` is every
    /// production line in the workspace — the reference haystack, never a source of
    /// violations. See [`workspace_src_roots`] for why the two differ.
    fn check_crate(&self, files: &[SourceFile], corpus: &str) -> Vec<Violation>;
}

/// Apply crate-wide rules. Reads this crate once for violations and the workspace once
/// for references.
pub fn scan_crate(rules: &[&dyn CrateRule]) -> Vec<Violation> {
    let files = match audited_files() {
        Ok(files) => files,
        Err(e) => return unreadable_policy(&e),
    };
    // The reference corpus is RAW file text — production AND test halves.
    //
    // This asymmetry against `files` (production-only, the violation source) is
    // deliberate. A type referenced only from another file's `#[cfg(test)]` mod is
    // WIRED — to a test harness, on purpose; `GridSimulator` and `RecordingGridCapture`
    // are exactly that. Counting only production halves reported them as dead, and a
    // guard that cries wolf on legitimate harnesses is one that gets deleted. Asking
    // "referenced NOWHERE, not even by a test" is the sharper question and still
    // catches the motivating defect, whose only mentions are inside its own file.
    let mut corpus = String::new();
    for root in workspace_src_roots() {
        for text in raw_sources(&root) {
            corpus.push_str(&text);
            corpus.push('\n');
        }
    }
    let mut out = Vec::new();
    for rule in rules {
        out.extend(rule.check_crate(&files, &corpus));
    }
    sort_violations(&mut out);
    out
}

/// Every `.rs` file under `root`, unsplit — the whole text including test mods.
fn raw_sources(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    out.push(text);
                }
            }
        }
    }
    out
}

fn sort_violations(v: &mut [Violation]) {
    v.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
}

#[cfg(test)]
mod tests {
    // what this catches: THE EXTRACTION'S OWN FAILURE MODE, and it is the quiet one.
    // These rules used to live inside the crate they audit, so `CARGO_MANIFEST_DIR`
    // and "the tree under audit" were the same directory and could not disagree.
    // They are different directories now. A `crate_src_root()` that resolves to this
    // crate — or to anything that does not exist — makes every rule walk an empty or
    // wrong tree, find nothing, and report GREEN. Forever. No rule fails, no count
    // rises, and the ratchets look healthier than they have ever been while guarding
    // nothing at all. Cormac named this exact risk when reviewing the move.
    //
    // So the root is asserted to be real, to be the audited crate, and to contain
    // enough Rust that an empty scan cannot be mistaken for a clean one.
    // what this catches: the depth assumption that survived this crate's move by LUCK.
    // Every root here used to be `../..` or `ancestors().nth(2)`, correct only because
    // the scanner sat two levels down — and `core/source-hygiene` happens to sit two
    // levels down too, so the move could not expose it (Cormac, review of #4346). It is
    // also the one path `the_audited_root_...` cannot protect: a root that is WRONG but
    // EXISTS scans a real directory and finds nothing. Anchored resolution has to be
    // asserted directly or the next crate move reintroduces the bug silently.
    #[test]
    fn the_repo_root_is_found_by_anchor_and_is_above_this_crate() {
        let root = super::repo_root().expect(
            "no ancestor carries the workspace Cargo.toml — every cross-crate rule \
             would fall back to the audited root alone",
        );
        assert!(
            root.join("Cargo.toml").is_file() && root.join("core").is_dir(),
            "{} does not look like the repo root",
            root.display()
        );
        // The audited crate must live UNDER it — the relationship the old `../..`
        // asserted by arithmetic, now asserted by fact.
        assert!(
            super::crate_src_root().starts_with(&root),
            "audited root {} is not under repo root {}",
            super::crate_src_root().display(),
            root.display()
        );
        // And it must be the workspace manifest, not some nested crate's Cargo.toml
        // that happens to sit beside a `core/` directory.
        let manifest = std::fs::read_to_string(root.join("Cargo.toml"))
            // unwrap_or_default: an unreadable manifest fails the assert below rather
            // than passing on an absent string.
            .unwrap_or_default();
        assert!(
            manifest.contains("[workspace]"),
            "{} is not the WORKSPACE manifest",
            root.display()
        );
    }

    #[test]
    fn the_audited_root_is_the_other_crate_and_actually_has_source_in_it() {
        let root = super::crate_src_root();
        assert!(
            root.is_dir(),
            "the audited src root does not exist ({}) — every rule would scan nothing \
             and pass green",
            root.display()
        );
        assert!(
            root.ends_with("continuum-core/src"),
            "the audited root must be the AUDITED crate's src, not this scanner's ({})",
            root.display()
        );
        let rust_files = std::fs::read_dir(&root)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
                    .count()
            })
            // unwrap_or: an unreadable root is zero files, which the assert below
            // rejects — never a silently satisfied precondition.
            .unwrap_or(0);
        assert!(
            rust_files > 0,
            "the audited root {} contains no .rs files at its top level — a scan \
             rooted here reports zero findings because there is nothing to find",
            root.display()
        );
    }

    use super::*;
}
