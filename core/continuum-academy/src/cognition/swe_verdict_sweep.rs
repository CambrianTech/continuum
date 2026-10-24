//! The standing refusal store for SWE instances this box cannot grade.
//!
//! A refusal ("ungradeable") is a marker file beside the verdicts, keyed by instance. An
//! ENVIRONMENT refusal (the box cannot grade the instance at all) is deck hygiene: the
//! instance is not offered on this box again until the env changes. The staged-artifact
//! sweep that used to write these markers on a clock is gone (card c4f4b8cd step 1:
//! nothing scans workspaces and pushes grades; a hand-in is graded when it is submitted).

use continuum_base::time::EpochMs;
use std::path::PathBuf;

/// A refusal ("ungradeable") remembered beside the verdicts, keyed by instance.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Refusal {
    pub instance: String,
    pub reason: String,
    pub at_ms: u64,
    pub work_mtime_ms: Option<u64>,
}

fn refusal_path(instance: &str) -> PathBuf {
    crate::cognition::swe_bench::verdict_dir().join(format!("{instance}.ungradeable.json"))
}

pub fn read_refusal(instance: &str) -> Option<Refusal> {
    let text = std::fs::read_to_string(refusal_path(instance)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn record_refusal(instance: &str, reason: &str, work_mtime_ms: Option<u64>) {
    let r = Refusal {
        instance: instance.to_string(),
        reason: reason.chars().take(400).collect(),
        at_ms: EpochMs::now().millis(),  // pre-1970 reads 0: pre-epoch clock — the marker still records the reason and the work mtime
        work_mtime_ms,
    };
    let path = refusal_path(instance);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(&r) {
        let _ = std::fs::write(&path, text);
    }
}

/// Is this refusal about the ENVIRONMENT — the box cannot grade the instance at all
/// (pristine-tree verdicts: PASS_TO_PASS 0/N, FAIL_TO_PASS passing or skipped before the
/// patch; no runnable harness for the era; the repo will not install) — rather than about
/// the WORK (no patch to grade)? An env refusal is deck hygiene: the instance is not
/// offered on this box again until the env changes. A work refusal is the citizen's.
pub fn refusal_is_env_fault(reason: &str) -> bool {
    // A failure to look is neither env nor work: it is no verdict at all (cffc9c5e).
    if reason.starts_with(crate::cognition::swe_bench::COULD_NOT_LOOK) {
        return false;
    }
    reason.starts_with("UNGRADEABLE — ")
        || reason.contains("env has no runnable test harness")
        || reason.contains("could not install")
}

/// The standing ENV refusal recorded for `instance` on this box, if any. Markers outlive
/// the work-change TTL on purpose: an environment does not fix itself between rounds —
/// clear the marker (or run `benchmark/validate`) when it does.
pub fn standing_env_refusal(instance: &str) -> Option<String> {
    let r = read_refusal(instance)?;
    refusal_is_env_fault(&r.reason).then_some(r.reason)
}

/// The (instance, reason) of the standing ENV refusal a bench card's title names on
/// this box, which the claim verb reads so no claim lands on an instance no grade can
/// score (2026-09-13: Atlas claimed requests-1766 by id from a message turn and spent her
/// next hour on it).
pub fn standing_refusal_for_card(title: &str) -> Option<(String, String)> {
    let Some((_, instance)) = crate::recipe::bench_title::parse_card_title(title) else {
        return None;
    };
    let reason = standing_env_refusal(&instance)?;
    static PROBED: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    // Poisoned or contended: the probe is the only thing at stake, never the decision.
    if PROBED.lock().is_ok_and(|mut seen| seen.insert(instance.clone())) {
        continuum_base::probe!(
            class = "bench.round.pull_skipped_ungradeable",
            instance = %instance,
            reason = %reason,
            "a standing env refusal keeps this card off every pull and claim on this box"
        );
    }
    Some((instance, reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (2026-09-13, seed-4): an instance this box cannot grade — pytest-7236
    // (pristine PASS_TO_PASS 0/40), requests-1766 (era pytest cannot run here) — dispatched
    // again the next round while its refusal marker stood; two of five cards burned hours
    // on a wall. A WORK refusal (no patch) must not be mistaken for it.
    #[test]
    fn an_env_refusal_is_told_apart_from_a_work_refusal() {
        assert!(refusal_is_env_fault("UNGRADEABLE — PASS_TO_PASS passes 0 of 40 on the PRISTINE tree: the suite does not run"));
        assert!(refusal_is_env_fault("UNGRADEABLE — every FAIL_TO_PASS test already passes on the pristine tree ([\"a\"])"));
        assert!(refusal_is_env_fault("psf__requests-1766's env has no runnable test harness: era-pinned pytest cannot execute"));
        assert!(refusal_is_env_fault("could not install scikit-learn__scikit-learn-14983's repo into a venv — a cached broken env"));
        assert!(!refusal_is_env_fault("no candidate patch to grade for matplotlib__matplotlib-21568 — the workspace holds no diff"));
        // card cffc9c5e: a failure to LOOK is never an env fault (nor a work fault).
        assert!(!refusal_is_env_fault(&format!(
            "{}the resolver could not reach the package index during `uv pip install [\"cython<3\"]`",
            crate::cognition::swe_bench::COULD_NOT_LOOK
        )));
    }
}
