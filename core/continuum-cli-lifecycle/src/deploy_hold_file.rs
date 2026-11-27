//! The one owner of the node's deploy hold file, `state/deploy-hold` (a [`Hold`]): the tracker reads it
//! through [`read`], and every change goes through [`change`], one locked read-decide-apply, so a
//! candidate's hold never overwrites or releases an operator's, and two writers never interleave (Codex
//! on 17660bf67f: a JSON-only second reader took a legacy text hold for none and could overwrite it).

use std::io;
use std::path::{Path, PathBuf};

use crate::deploy_tracker::Hold;

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
            HoldRead::Unreadable(why) => Some(Hold { reason: format!("the deploy hold file is unreadable: {why}"), created_ms: 0, ttl_ms: None }),
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
            Ok(at) => HoldRead::Held(Hold { reason: text.trim().to_string(), created_ms: at.as_millis() as u64, ttl_ms: None }),
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
        let ours = Hold { reason: "ours".into(), created_ms: 5, ttl_ms: Some(10) };
        change(state.path(), |_| Change::Write(ours.clone())).expect("test: a write");
        assert_eq!(read(state.path()), HoldRead::Held(ours));
        change(state.path(), |_| Change::Remove).expect("test: a release");
        assert_eq!(read(state.path()), HoldRead::Missing);
        assert!(HoldRead::Unreadable("x".into()).honored().is_some_and(|hold| hold.ttl_ms.is_none()), "an unreadable file holds");
    }
}
