//! The one owner of the node's deploy hold file, `state/deploy-hold` (a [`Hold`]): the tracker reads it
//! through [`read`], and every change goes through [`change`], one locked read-decide-apply, so a
//! candidate's hold never overwrites or releases an operator's, and two writers never interleave (Codex
//! on 17660bf67f: a JSON-only second reader took a legacy text hold for none and could overwrite it).

use std::io;
use std::path::{Path, PathBuf};

use crate::deploy_tracker::{Hold, HoldOwner};

/// What the hold file says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldRead {
    /// No file: nothing holds the tracker.
    Missing,
    /// A hold: JSON, or a legacy bash-written text file read as a TTL-less hold whose reason is its text.
    Held(Hold),
    /// The file exists and cannot be read: it holds (never read as no hold), flagged with why.
    Unreadable(String),
}

impl HoldRead {
    /// The hold the tracker honors: an unreadable file holds, TTL-less, so it is honored and flagged stale.
    pub fn honored(self) -> Option<Hold> {
        match self {
            HoldRead::Missing => None,
            HoldRead::Held(hold) => Some(hold),
            HoldRead::Unreadable(why) => Some(Hold { reason: format!("the deploy hold file is unreadable: {why}"), created_ms: 0, ttl_ms: None, owner: HoldOwner::Operator }),
        }
    }
}

/// What a change does to the hold file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Keep,
    Write(Hold),
    Remove,
}

fn file(state_dir: &Path) -> PathBuf {
    state_dir.join("deploy-hold")
}

/// The hold file as it is now.
pub fn read(state_dir: &Path) -> HoldRead {
    let path = file(state_dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(missing) if missing.kind() == io::ErrorKind::NotFound => return HoldRead::Missing,
        Err(other) => return HoldRead::Unreadable(format!("{}: {other}", path.display())),
    };
    if let Ok(hold) = serde_json::from_str::<Hold>(&text) {
        return HoldRead::Held(hold);
    }
    // a legacy text hold: created when its file was last written, so it is flagged stale in time
    match std::fs::metadata(&path).and_then(|meta| meta.modified()) {
        Ok(modified) => match modified.duration_since(std::time::UNIX_EPOCH) {
            Ok(at) => HoldRead::Held(Hold { reason: text.trim().to_string(), created_ms: at.as_millis() as u64, ttl_ms: None, owner: HoldOwner::Operator }),
            Err(why) => HoldRead::Unreadable(format!("{}: its modified time is not a time: {why}", path.display())),
        },
        Err(why) => HoldRead::Unreadable(format!("{}: {why}", path.display())),
    }
}

/// Read, decide with `decide`, and apply, under the hold file's own lock (`state/deploy-hold.lock`, taken
/// by waiting): the read and the write are one step for every writer. Returns what was applied.
pub fn change(state_dir: &Path, decide: impl FnOnce(&HoldRead) -> Change) -> io::Result<Change> {
    std::fs::create_dir_all(state_dir)?;
    let _one_writer = continuum_host::durable::lock_path(&state_dir.join("deploy-hold.lock"), true)?;
    let decided = decide(&read(state_dir));
    match &decided {
        Change::Keep => {}
        Change::Write(hold) => {
            let text = serde_json::to_vec(hold).map_err(io::Error::other)?;
            continuum_host::durable::write_replace(&file(state_dir), &text)?;
        }
        Change::Remove => match std::fs::remove_file(file(state_dir)) {
            Ok(()) => {}
            Err(gone) if gone.kind() == io::ErrorKind::NotFound => {}
            Err(other) => return Err(other),
        },
    }
    Ok(decided)
}


/// The running build as the tracker sees it on a tick.
#[derive(Clone, Copy)]
pub struct Running<'a> {
    /// Its build sha.
    pub sha: &'a str,
    /// Whether origin/canary contains it (a build the tracker deploys, or an older one it moves past).
    pub on_canary: bool,
    /// When its serving was verified ready, once it has been.
    pub ready_at: Option<u64>,
    /// How long a candidate keeps its window.
    pub window_ms: u64,
}

/// What the running build means for the hold, decided by the tracker on every tick: a build deployed
/// on purpose that is not on canary (`on_canary` false) holds the tracker for one `window_ms` from the
/// moment its serving was verified ready (`ready_at`; until then the hold is refreshed from `now`, so it
/// lasts through readiness), and a build on canary releases a candidate's hold. An operator's hold, or a
/// file that cannot be read, is never touched (Codex on 17660bf67f).
pub fn for_running(build: &Running<'_>, current: &HoldRead, now: u64) -> Change {
    let Running { sha: running, on_canary, ready_at, window_ms } = *build;
    let ours = match current {
        HoldRead::Held(hold) => match &hold.owner {
            HoldOwner::Candidate { build } => Some(build.as_str()),
            HoldOwner::Operator => None,
        },
        HoldRead::Missing | HoldRead::Unreadable(_) => None,
    };
    if on_canary {
        return if ours.is_some() { Change::Remove } else { Change::Keep };
    }
    match current {
        HoldRead::Held(hold) if ours.is_none() && hold.is_active(now) => return Change::Keep,
        HoldRead::Unreadable(_) => return Change::Keep,
        HoldRead::Held(_) | HoldRead::Missing => {}
    }
    let want = Hold {
        reason: format!("candidate {running} deployed by hand: the tracker leaves it for its live window"),
        created_ms: ready_at.unwrap_or(now),
        ttl_ms: Some(window_ms),
        owner: HoldOwner::Candidate { build: running.to_string() },
    };
    match current {
        HoldRead::Held(hold) if *hold == want => Change::Keep,
        _ => Change::Write(want),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Codex on 17660bf67f): a legacy text hold read as no hold and overwritten, an
    // unreadable file read as no hold, and a change applied to a hold that is not the one decided on.
    #[test]
    fn a_legacy_hold_is_honored_and_a_change_sees_what_it_replaces() {
        let state = tempfile::tempdir().expect("test: a state dir");
        assert_eq!(read(state.path()), HoldRead::Missing);
        std::fs::write(state.path().join("deploy-hold"), "operator: live read of #5917\n").expect("test: a legacy hold");
        let legacy = match read(state.path()) {
            HoldRead::Held(hold) => hold,
            other => panic!("a legacy text hold must be held, got {other:?}"),
        };
        assert_eq!((legacy.reason.as_str(), legacy.ttl_ms), ("operator: live read of #5917", None));
        let seen = change(state.path(), |current| {
            assert_eq!(current, &HoldRead::Held(legacy.clone()), "the change decides on the hold as it is");
            Change::Keep
        })
        .expect("test: a kept hold");
        assert_eq!(seen, Change::Keep);
        let ours = Hold { reason: "ours".into(), created_ms: 5, ttl_ms: Some(10), owner: HoldOwner::Operator };
        change(state.path(), |_| Change::Write(ours.clone())).expect("test: a write");
        assert_eq!(read(state.path()), HoldRead::Held(ours));
        change(state.path(), |_| Change::Remove).expect("test: a release");
        assert_eq!(read(state.path()), HoldRead::Missing);
        assert!(HoldRead::Unreadable("x".into()).honored().is_some_and(|hold| hold.ttl_ms.is_none()), "an unreadable file holds");
    }

    // what this catches (the M5, 2026-11-27: a verified candidate replaced by canary 11 minutes into its
    // window; Codex on 17660bf67f): the window must run from serving readiness, last through a slow
    // readiness, release only the candidate's own hold, and never touch an operator's or an unreadable one.
    #[test]
    fn a_candidate_holds_from_readiness_and_never_touches_anyone_elses_hold() {
        let window_ms = 900_000;
        let candidate = |ready_at| Running { sha: "4da0f81bfa", on_canary: false, ready_at, window_ms };
        // not ready yet: held from now, refreshed each tick, so it lasts through readiness
        let starting = match for_running(&candidate(None), &HoldRead::Missing, 1_000) {
            Change::Write(hold) => hold,
            other => panic!("a candidate must hold the tracker, got {other:?}"),
        };
        assert_eq!((starting.created_ms, starting.ttl_ms), (1_000, Some(window_ms)));
        // ready at 100 s: its window runs from readiness, and a later tick keeps it
        let ready = match for_running(&candidate(Some(100_000)), &HoldRead::Held(starting), 101_000) {
            Change::Write(hold) => hold,
            other => panic!("readiness restarts the window, got {other:?}"),
        };
        assert_eq!(ready.created_ms, 100_000);
        assert_eq!(for_running(&candidate(Some(100_000)), &HoldRead::Held(ready.clone()), 400_000), Change::Keep);
        // canary deployed again: the candidate's hold is released, an operator's never
        let canary = Running { sha: "f6648967d", on_canary: true, ready_at: Some(500_000), window_ms };
        assert_eq!(for_running(&canary, &HoldRead::Held(ready), 600_000), Change::Remove);
        let operator = HoldRead::Held(Hold { reason: "operator: live read".into(), created_ms: 1_000, ttl_ms: None, owner: HoldOwner::Operator });
        assert_eq!(for_running(&canary, &operator, 600_000), Change::Keep);
        assert_eq!(for_running(&candidate(None), &operator, 600_000), Change::Keep);
        assert_eq!(for_running(&candidate(None), &HoldRead::Unreadable("denied".into()), 600_000), Change::Keep);
    }
}
