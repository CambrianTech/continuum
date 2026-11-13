//! A ratchet compares the tree under review with the tree it is based on. It never compares
//! against a number written into this crate.
//!
//! The checked-in baselines (`const BASELINE_…`) were this crate's merge-conflict magnet: every PR
//! that lowered a count edited the same constant, so any two improving PRs conflicted with each
//! other. On 2026-10-22 most conflicts across nine rebased PRs were baseline lines alone, and a
//! baseline had to be lowered by hand in each PR. The constant also lied in between: the moment
//! one PR merged, every other PR's copy was stale.
//!
//! Now each ratchet counts, and [`hold`] judges the count against the base tree's count for the
//! same name, measured by the same binary (the PR's own rules, run over both trees). A count may
//! only stay or fall. `cargo run -p source-hygiene --bin ratchets -- origin/canary` runs both
//! measurements, in CI and locally; a plain `cargo test -p source-hygiene` reports counts
//! without judging them.
//!
//! - `SOURCE_HYGIENE_RECORD=<file>`: record every count as `name count` (both runs).
//! - `SOURCE_HYGIENE_BASE=<file>`: judge every count against the recorded base (the head run).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use crate::Violation;

const RECORD: &str = "SOURCE_HYGIENE_RECORD";
const BASE: &str = "SOURCE_HYGIENE_BASE";
const TOUCHED: &str = "SOURCE_HYGIENE_TOUCHED";

/// Hold the ratchet `name` at `count`. Record it, judge it against the base, or report it,
/// by the environment the script sets. `advice` is rendered only on a failure: what to do
/// instead, and the newest offending sites.
pub fn hold(name: &str, count: usize, advice: impl FnOnce() -> String) {
    // Both runs record: the base run so the head run can be judged, the head run so the
    // driver can print every count's movement (a passing test's stderr is captured).
    if let Ok(path) = std::env::var(RECORD) {
        record(&path, name, count);
    }
    let Ok(path) = std::env::var(BASE) else {
        eprintln!("{name}: {count} (unjudged: `cargo run -p source-hygiene --bin ratchets -- origin/canary` compares with the base)");
        return;
    };
    let base = read_base(&path);
    if let Some(before) = rose(name, &base, count) {
        panic!("{name} rose from {before} to {count}. {}", advice());
    }
    eprintln!("{name}: {} -> {count}", base[name]); // index: `rose` already failed loudly on a base without `name`
}

/// The base count `name` rose from, when `count` exceeds it; `None` when it held or fell. Only the
/// total is judged: per-crate rows are receipts, so a crate cut that moves sites between crates,
/// new or existing, passes with its total unchanged. A base that never recorded `name` fails
/// loudly (its scan failed, or the ratchet was renamed), never judges nothing.
pub(crate) fn rose(name: &str, base: &BTreeMap<String, usize>, count: usize) -> Option<usize> {
    let Some(&before) = base.get(name) else {
        panic!(
            "{name}: the base tree recorded no count. Its scan failed on the base, or the ratchet \
             was renamed; read the base run's output. Head count: {count}."
        );
    };
    (count > before).then_some(before)
}

/// What a change owes a site ratchet in the files it touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touched {
    /// A file a change touches leaves with none of the rule's sites: whoever next changes a
    /// file cleans it, which is what moves a count to zero.
    EndsClean,
    /// The count is held; a touched file may keep its sites, for the named reason.
    HeldOnly(HeldOnly),
}

/// Why a site ratchet cannot ask a touched file to end clean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldOnly {
    /// The site's verdict depends on other files (a cycle, a reference corpus, the ring an
    /// imported item lives in): the fix is in another file or crate, not the touched one.
    CrossFile,
    /// The rule counts every instance of a shape that is sometimes right, and no sanctioned form
    /// exists yet to write the right ones in: a touched file would have nothing to move to.
    NoSanctionedForm,
}

/// Hold the ratchet `name` at its violations' count, and record where they live: one
/// `name@crate count` line per crate beside the total, so the receipt says which crate owns
/// the debt. Every rule that has its sites holds through this, never through a bare count, and
/// says what a file the change touches owes it.
pub fn hold_sites(name: &str, violations: &[Violation], touched: Touched, advice: impl Fn() -> String) {
    if let Ok(path) = std::env::var(RECORD) {
        for (owner, count) in count_by_crate(violations) {
            record(&path, &format!("{name}{BY_CRATE}{owner}"), count);
        }
    }
    if touched == Touched::EndsClean {
        touched_clean(name, violations, &advice);
    }
    hold(name, violations.len(), advice);
}

fn count_by_crate(violations: &[Violation]) -> BTreeMap<&str, usize> {
    let mut by_crate: BTreeMap<&str, usize> = BTreeMap::new();
    for violation in violations {
        *by_crate.entry(crate_of(&violation.file)).or_default() += 1;
    }
    by_crate
}

/// Hold the rule `name` at ZERO: a shape that is gone stays gone. Recorded like a ratchet so the
/// receipt shows it, but judged without a base: any site fails, on any tree.
pub fn zero(name: &str, violations: &[Violation], advice: impl Fn() -> String) {
    if let Ok(path) = std::env::var(RECORD) {
        record(&path, name, violations.len());
    }
    let sites: Vec<String> = violations.iter().map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect();
    assert!(sites.is_empty(), "{name} must be zero and has {} site(s):\n{}\nHow to fix it: {}", sites.len(), sites.join("\n"), advice());
}

/// [`zero`] for a rule that counts without sites (a program's spawns, orphaned files).
pub fn zero_count(name: &str, count: usize, advice: impl Fn() -> String) {
    if let Ok(path) = std::env::var(RECORD) {
        record(&path, name, count);
    }
    assert!(count == 0, "{name} must be zero and is {count}. {}", advice());
}

/// The separator between a ratchet and the crate a per-crate record line counts.
pub const BY_CRATE: &str = "@";

/// The crate a repository path belongs to (`core/<crate>/...`), or its first directory.
fn crate_of(file: &str) -> &str {
    let mut parts = file.split('/');
    match (parts.next(), parts.next()) {
        (Some("core"), Some(owner)) => owner,
        (Some(first), _) => first,
        (None, _) => file,
    }
}

/// Judge that a change leaves none of `name`'s violations in a file it touched. The driver
/// lists the change's files (repository paths, one per line) in `SOURCE_HYGIENE_TOUCHED` for
/// the head run; unset, as in the base run or a plain `cargo test`, there is nothing to judge.
/// This is what moves a count to zero: a file is cleaned by whoever next changes it.
fn touched_clean(name: &str, violations: &[Violation], advice: impl Fn() -> String) {
    let Ok(path) = std::env::var(TOUCHED) else {
        return;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => panic!("cannot read the touched-file list {path}: {e}"),
    };
    let touched: BTreeSet<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let dirty = dirty_sites(violations, &touched);
    // The offenders lead, under their own heading: the advice can carry a whole-tree sample,
    // and printed first it read as the change's own sites (an agent spent an hour on #5945
    // chasing files the change never touched, 2026-11-11).
    assert!(
        dirty.is_empty(),
        "{name}: {} site(s) remain in files this change touches; a touched file leaves with none.\n\
         In files this change touches:\n{}\n\
         How to fix it (any sample below is from the whole tree and may include these):\n{}",
        dirty.len(),
        dirty.join("\n"),
        advice()
    );
}

/// The violations that sit in a touched file, as failure lines. A violation names its file by
/// repository path, the same form the touched list holds, so the two always compare.
pub(crate) fn dirty_sites(violations: &[Violation], touched: &BTreeSet<&str>) -> Vec<String> {
    violations
        .iter()
        .filter(|v| touched.contains(v.file.as_str()))
        .map(|v| format!("  {}:{}  {}", v.file, v.line, v.source))
        .collect()
}

/// One crate's movement for one ratchet: its base count (absent when the base never recorded
/// it) and its head count (zero when the head has none left, so a crate swept to zero reports
/// `N -> 0` instead of vanishing from the receipt).
#[derive(Debug, PartialEq, Eq)]
pub struct CrateMovement {
    pub ratchet: String,
    pub owner: String,
    pub before: Option<usize>,
    pub after: usize,
}

/// Every per-crate row either run recorded, the union of base and head.
pub fn crate_movement(base: &BTreeMap<String, usize>, head: &BTreeMap<String, usize>) -> Vec<CrateMovement> {
    let keys: BTreeSet<&String> = base.keys().chain(head.keys()).filter(|k| k.contains(BY_CRATE)).collect();
    keys.into_iter()
        .filter_map(|key| {
            let (ratchet, owner) = key.split_once(BY_CRATE)?;
            Some(CrateMovement {
                ratchet: ratchet.to_owned(),
                owner: owner.to_owned(),
                before: base.get(key).copied(),
                after: head.get(key).copied().unwrap_or(0), // unwrap_or: a crate the head never recorded has none left
            })
        })
        .collect()
}

/// One line per count. A ratchet runs in its own test thread, and a single short append is
/// one `write` on an `O_APPEND` file, so concurrent records do not interleave.
fn record(path: &str, name: &str, count: usize) {
    let mut file = match std::fs::OpenOptions::new().create(true).append(true).open(path) {
        Ok(file) => file,
        Err(e) => panic!("cannot open the ratchet record {path}: {e}"),
    };
    if let Err(e) = file.write_all(format!("{name} {count}\n").as_bytes()) {
        panic!("cannot write the ratchet record {path}: {e}");
    }
}

pub fn read_base(path: &str) -> BTreeMap<String, usize> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => panic!("cannot read the base record {path}: {e}"),
    };
    parse(&text)
}

/// `name count` lines. A malformed line fails loudly: a record that silently lost a ratchet
/// would read as "no base" and fail confusingly later, or worse, judge nothing.
pub fn parse(text: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let parsed = line.rsplit_once(' ').and_then(|(name, n)| Some((name.to_owned(), n.parse().ok()?)));
        let Some((name, n)) = parsed else {
            panic!("malformed ratchet record line: {line:?}");
        };
        counts.insert(name, n);
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a per-crate record keyed by the wrong path segment, which would
    // attribute every count to `core` and name no owner.
    // what this catches: a crate swept to zero disappearing from the receipt (the head records
    // only nonzero crates), which hides exactly the result a sweep exists to show.
    #[test]
    fn a_crate_swept_to_zero_reports_its_fall() {
        let base: BTreeMap<String, usize> = [("u@a".to_owned(), 3), ("u@b".to_owned(), 2)].into();
        let head: BTreeMap<String, usize> = [("u@b".to_owned(), 1), ("u@c".to_owned(), 4)].into();
        let rows = crate_movement(&base, &head);
        let row = |owner: &str| rows.iter().find(|r| r.owner == owner).map(|r| (r.before, r.after));
        assert_eq!(row("a"), Some((Some(3), 0)), "swept to zero");
        assert_eq!(row("b"), Some((Some(2), 1)));
        assert_eq!(row("c"), Some((None, 4)), "new in head");
    }

    // what this catches: a judgement that fails a pure move. A phase-2 crate cut moves sites from
    // one crate into another, new or EXISTING, with the total unchanged; it must pass, while the
    // per-crate rows still show where the sites went.
    #[test]
    fn moving_sites_into_an_existing_crate_passes_and_is_recorded() {
        let site = |file: &str| Violation { rule: "s", file: file.to_owned(), line: 1, source: String::new() };
        let base: BTreeMap<String, usize> = [("s".to_owned(), 5), ("s@a".to_owned(), 3), ("s@b".to_owned(), 2)].into();
        let moved: Vec<Violation> =
            ["core/a/src/x.rs", "core/b/src/x.rs", "core/b/src/y.rs", "core/b/src/z.rs", "core/b/src/w.rs"].map(site).into();
        assert_eq!(rose("s", &base, moved.len()), None, "3 -> 1 in a, 2 -> 4 in existing b, total 5: a move passes");
        assert_eq!(count_by_crate(&moved), [("a", 1), ("b", 4)].into(), "the rows still record the move");
        assert_eq!(rose("s", &base, moved.len() + 1), Some(5), "a real rise in the total fails");
    }

    #[test]
    fn a_site_is_attributed_to_its_crate() {
        assert_eq!(crate_of("core/continuum-mind/src/persona/act.rs"), "continuum-mind");
        assert_eq!(crate_of("tools/scripts/x.rs"), "tools");
    }

    // what this catches: a record the head run misreads (a name with a colon or space, a
    // trailing newline), which would judge the wrong ratchet or none.
    #[test]
    fn a_record_parses_back_to_its_counts() {
        let counts = parse("unwraps 2355\nspawn:airc 6\n\n");
        assert_eq!(counts.get("unwraps"), Some(&2355));
        assert_eq!(counts.get("spawn:airc"), Some(&6));
        assert_eq!(counts.len(), 2);
    }
}
