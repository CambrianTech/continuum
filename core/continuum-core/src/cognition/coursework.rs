//! Coursework: lessons as cards in a room (ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM §10).
//!
//! A coursework round is the benchmark-round pipeline with a different card source, never a
//! runner beside it (docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md): the teach
//! selector's TESTED tasks become a content-addressed lesson set, imported as gym cards, worked
//! on her ordinary turns, graded by the same `test_grade`, settled through the same credit
//! path. This module owns the three decisions only coursework makes, once each:
//!
//! - **The set's name is its content.** `coursework-<sha12 of the canonical rows>`: the same
//!   lessons are the same set on every node and in every round, which is the TASK IDENTITY
//!   §10.2 keys the judge on (a lesson repeats by design; a gene must never be judged on the
//!   lesson it was trained on).
//! - **Where a set lives** (`set_path`), under the tracked `benchmarks` dir.
//! - **Whether a suite name is coursework** (`is_coursework`): the round, its cards' titles and
//!   the staged turns all carry the set name, so provenance is read from data, never inferred.

use crate::cognition::eval::EvalTask;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Every coursework set's name starts with this. The bench slot of a card title
/// (`[bench coursework-<sha12>] <task>:`) carries the whole name, so the grader resolves the
/// set from the title like any gym suite.
pub const SET_PREFIX: &str = "coursework-";

/// Hex digits of the content hash in a set's name: 48 bits, enough to never collide across
/// the handful of lesson sets a node holds, short enough for a card title.
const SHA_HEX_LEN: usize = 12;

/// Is this suite name a coursework set? The ONE predicate every reader uses (the pull order,
/// the settle path, the grader's resolver).
pub fn is_coursework(suite: &str) -> bool {
    suite
        .strip_prefix(SET_PREFIX)
        .is_some_and(|sha| sha.len() == SHA_HEX_LEN && sha.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The directory sets live in: under `~/.continuum/benchmarks`, which the disk reporter
/// already tracks and the eviction story already covers.
pub fn sets_dir(continuum_home: &Path) -> PathBuf {
    continuum_home.join("benchmarks").join("coursework")
}

/// The file a set name resolves to. `None` for a name that is not a coursework set, so a
/// caller can never be pointed at an arbitrary path through a card title.
pub fn set_path(continuum_home: &Path, suite: &str) -> Option<PathBuf> {
    is_coursework(suite).then(|| sets_dir(continuum_home).join(format!("{suite}.jsonl")))
}

/// The canonical JSONL of a lesson set and its name: only TESTED tasks (a lesson without an
/// oracle cannot be a verdict), sorted by id so the same lessons always hash the same.
pub fn canonical_set(tasks: &[EvalTask]) -> Result<(String, String), String> {
    let mut tested: Vec<&EvalTask> = tasks.iter().filter(|t| t.test.is_some()).collect();
    if tested.is_empty() {
        return Err("no lesson carries a test: a coursework card's verdict is its test, so there is nothing to grade".into());
    }
    tested.sort_by(|a, b| a.id.cmp(&b.id));
    let mut text = String::new();
    for t in tested {
        text.push_str(&serde_json::to_string(t).map_err(|e| format!("lesson {}: {e}", t.id))?);
        text.push('\n');
    }
    let sha = format!("{:x}", Sha256::digest(text.as_bytes()));
    Ok((format!("{SET_PREFIX}{}", &sha[..SHA_HEX_LEN]), text))
}

/// Write a lesson set under its content name and return the name. Idempotent: the same
/// lessons write the same file, and an existing file is left as it is (its name IS its
/// content).
pub fn write_set(continuum_home: &Path, tasks: &[EvalTask]) -> Result<String, String> {
    let (name, text) = canonical_set(tasks)?;
    let path = set_path(continuum_home, &name).ok_or_else(|| format!("{name} is not a coursework name"))?;
    if !path.is_file() {
        let dir = sets_dir(continuum_home);
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        // Write beside, then rename: a reader never sees a half-written set.
        let partial = path.with_extension("jsonl.partial");
        std::fs::write(&partial, &text).map_err(|e| format!("cannot write {}: {e}", partial.display()))?;
        std::fs::rename(&partial, &path).map_err(|e| format!("cannot place {}: {e}", path.display()))?;
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lesson(id: &str, test: Option<&str>) -> EvalTask {
        EvalTask {
            id: id.to_string(),
            prompt: format!("implement {id}"),
            test: test.map(str::to_string),
            lang: Some("rust".to_string()),
            ..Default::default()
        }
    }

    /// what this catches: a lesson set whose name is not its content. §10.2 keys the judge on
    /// TASK IDENTITY: the same lessons must be the same set whatever order the selector
    /// returned them in, a different lesson must be a different set, and a testless task can
    /// never become a card (its verdict would be nothing). The name must also read back as
    /// coursework, and a lookalike name must not resolve to a path.
    #[test]
    fn a_lesson_set_is_named_by_its_tested_content() {
        let a = lesson("sum_evens", Some("assert_eq!(sum_evens(&[2]), 2);"));
        let b = lesson("max_of", Some("assert_eq!(max_of(&[1, 3]), 3);"));
        let untested = lesson("essay", None);

        let (one, text) = canonical_set(&[a.clone(), b.clone(), untested.clone()]).expect("set");
        let (two, _) = canonical_set(&[b.clone(), a.clone()]).expect("set");
        assert_eq!(one, two, "order and testless rows do not change the set");
        assert_eq!(text.lines().count(), 2, "only tested lessons are in the set");
        assert!(is_coursework(&one), "{one} reads back as coursework");

        let (other, _) = canonical_set(&[a]).expect("set");
        assert_ne!(one, other, "different lessons, different set");

        assert!(canonical_set(&[untested]).is_err(), "no tested lesson, no set");
        assert!(!is_coursework("coursework-../../etc"), "a lookalike is not a set");
        assert!(set_path(Path::new("/h"), "hard-rs").is_none(), "a gym suite is not a coursework path");
    }

    /// what this catches: a set written twice, or read half-written. The same lessons land at
    /// the one path their name names, and a second write leaves it untouched.
    #[test]
    fn writing_a_set_is_idempotent_at_its_content_path() {
        let home = tempfile::tempdir().expect("home");
        let tasks = [lesson("sum_evens", Some("assert!(true);"))];
        let name = write_set(home.path(), &tasks).expect("written");
        let path = set_path(home.path(), &name).expect("path");
        let first = std::fs::read_to_string(&path).expect("set on disk");
        assert_eq!(write_set(home.path(), &tasks).expect("again"), name);
        assert_eq!(std::fs::read_to_string(&path).expect("still"), first);
        assert!(!path.with_extension("jsonl.partial").exists(), "no partial left behind");
    }
}
