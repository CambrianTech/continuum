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
    let Some(&before) = base.get(name) else {
        panic!(
            "{name}: the base tree recorded no count. Its scan failed on the base, or the ratchet \
             was renamed; read the base run's output. Head count: {count}."
        );
    };
    assert!(count <= before, "{name} rose from {before} to {count}. {}", advice());
    eprintln!("{name}: {before} -> {count}");
}

/// Judge that a change leaves none of `name`'s violations in a file it touched. The driver
/// lists the change's files (repository paths, one per line) in `SOURCE_HYGIENE_TOUCHED` for
/// the head run; unset, as in the base run or a plain `cargo test`, there is nothing to judge.
/// This is what moves a count to zero: a file is cleaned by whoever next changes it.
pub fn touched_clean(name: &str, violations: &[Violation], advice: impl FnOnce() -> String) {
    let Ok(path) = std::env::var(TOUCHED) else {
        return;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => panic!("cannot read the touched-file list {path}: {e}"),
    };
    let touched: BTreeSet<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let dirty: Vec<String> = violations
        .iter()
        .filter(|v| touched.contains(v.file.as_str()))
        .map(|v| format!("  {}:{}  {}", v.file, v.line, v.source))
        .collect();
    assert!(
        dirty.is_empty(),
        "{name}: {} site(s) remain in files this change touches; a touched file leaves with none. {}\n{}",
        dirty.len(),
        advice(),
        dirty.join("\n")
    );
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
