//! Her durable state of mind: what she has perceived, what she is working on, and
//! how wide her attention is. Saved at the seam, loaded at boot.
//!
//! EVENT-MIND.md §1 (resume) and §6. Rule 8 of HER-LOOP-IS-HER-OWN.md: a restart
//! must find her where she was, like waking from anesthesia. On 2026-09-22, 76 of
//! 80 deploy stops tore a citizen mid-thought because nothing durable said what she
//! was doing. This file is that record: per-activity cursors (so nothing is
//! re-perceived and nothing is skipped), her continuation (what she resumes) and
//! her dial (how wide she had the door).
//!
//! The world is NOT copied here: the activities keep their own truth; this holds
//! only her position in it. Typed, versioned JSON in her own peer dir; a missing
//! or unreadable file is a fresh mind, never a crash; cursors never regress.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::attention::{AttentionDial, Continuation};

pub const FILE_NAME: &str = "mind-state.json";
const VERSION: u32 = 1;

/// Her cursor into one activity's truth: the transcript position she has
/// perceived through, and the revision of each view kind she has seen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ActivityCursor {
    pub chat_lamport: u64,
    pub chat_event_id: Option<Uuid>,
    /// view kind (e.g. "kanban", "wall") → last revision perceived.
    pub views: BTreeMap<String, u64>,
}

impl ActivityCursor {
    /// Advance to `other` where it is ahead; never move backwards (a stale save on
    /// a slower node must not make her re-read a day).
    pub fn advance_to(&mut self, other: &ActivityCursor) {
        if other.chat_lamport > self.chat_lamport {
            self.chat_lamport = other.chat_lamport;
            self.chat_event_id = other.chat_event_id;
        }
        for (kind, rev) in &other.views {
            let mine = self.views.entry(kind.clone()).or_default();
            if *rev > *mine {
                *mine = *rev;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MindState {
    pub version: u32,
    pub cursors: BTreeMap<Uuid, ActivityCursor>,
    pub continuation: Option<Continuation>,
    pub dial: AttentionDial,
    pub saved_at_ms: u64,
}

impl Default for MindState {
    fn default() -> Self {
        Self { version: VERSION, cursors: BTreeMap::new(), continuation: None, dial: AttentionDial::default(), saved_at_ms: 0 }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MindStateError {
    #[error("write {path}: {source}")]
    Write { path: PathBuf, #[source] source: std::io::Error },
    #[error("encode: {0}")]
    Encode(#[from] serde_json::Error),
}

impl MindState {
    pub fn path_in(peer_dir: &Path) -> PathBuf {
        peer_dir.join(FILE_NAME)
    }

    /// Load from her peer dir. Absent, unreadable, undecodable or a newer version
    /// than this build knows → a fresh mind, with the reason probed. A fresh mind
    /// re-perceives from the durable airc cursors, so nothing is lost, only
    /// her note and her dial.
    pub fn load(peer_dir: &Path) -> MindState {
        let path = Self::path_in(peer_dir);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return MindState::default(),
            Err(e) => {
                crate::probe!(class = "mind.state.unreadable", path = %path.display(), error = %e, "fresh mind");
                return MindState::default();
            }
        };
        match serde_json::from_slice::<MindState>(&bytes) {
            Ok(s) if s.version <= VERSION => s,
            Ok(s) => {
                crate::probe!(class = "mind.state.newer_version", found = s.version, known = VERSION, "fresh mind");
                MindState::default()
            }
            Err(e) => {
                crate::probe!(class = "mind.state.undecodable", path = %path.display(), error = %e, "fresh mind");
                MindState::default()
            }
        }
    }

    /// Save atomically (write-then-rename) so a stop mid-write never leaves half a
    /// mind on disk.
    pub fn save(&self, peer_dir: &Path, now_ms: u64) -> Result<(), MindStateError> {
        let path = Self::path_in(peer_dir);
        let tmp = path.with_extension("json.tmp");
        let mut me = self.clone();
        me.saved_at_ms = now_ms;
        me.version = VERSION;
        let bytes = serde_json::to_vec_pretty(&me)?;
        std::fs::create_dir_all(peer_dir).map_err(|source| MindStateError::Write { path: peer_dir.to_path_buf(), source })?;
        std::fs::write(&tmp, bytes).map_err(|source| MindStateError::Write { path: tmp.clone(), source })?;
        std::fs::rename(&tmp, &path).map_err(|source| MindStateError::Write { path: path.clone(), source })?;
        Ok(())
    }

    /// Her cursor into `activity`, advanced to `seen` (never regressed).
    pub fn perceived(&mut self, activity: Uuid, seen: &ActivityCursor) {
        self.cursors.entry(activity).or_default().advance_to(seen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::salience::Expectation;

    // what this catches: rule 8. A saved mind comes back as the same mind: cursors,
    // her note and her dial; an absent file is a fresh mind (not a crash); a
    // garbled file is a fresh mind (not a crash); a newer version is a fresh mind.
    #[test]
    fn a_saved_mind_resumes_and_a_missing_or_broken_one_is_fresh() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(MindState::load(dir.path()), MindState::default(), "absent = fresh");

        let act = Uuid::from_u128(7);
        let mut m = MindState::default();
        m.perceived(act, &ActivityCursor { chat_lamport: 42, chat_event_id: Some(Uuid::from_u128(1)), views: [("kanban".to_string(), 3u64)].into() });
        m.continuation = Some(Continuation {
            activity: act,
            note: "upgrade test done; next: run the suite".into(),
            expectation: Some(Expectation { text: "review passes".into(), by_ms: Some(10), verdict: None }),
            written_at_ms: 5,
        });
        m.dial = AttentionDial::deep();
        m.save(dir.path(), 99).unwrap();

        let back = MindState::load(dir.path());
        assert_eq!(back.cursors[&act].chat_lamport, 42);
        assert_eq!(back.continuation.as_ref().unwrap().note, "upgrade test done; next: run the suite");
        assert_eq!(back.dial, AttentionDial::deep());
        assert_eq!(back.saved_at_ms, 99);
        assert!(!dir.path().join("mind-state.json.tmp").exists(), "atomic: no temp file left");

        std::fs::write(MindState::path_in(dir.path()), b"{ not json").unwrap();
        assert_eq!(MindState::load(dir.path()), MindState::default(), "garbled = fresh");

        std::fs::write(MindState::path_in(dir.path()), serde_json::json!({"version": 999, "cursors": {}, "continuation": null, "dial": {"depth":"Normal","pass":"Addressed"}, "saved_at_ms": 1}).to_string()).unwrap();
        assert_eq!(MindState::load(dir.path()), MindState::default(), "newer version = fresh");
    }

    // what this catches: a cursor never regresses. A stale save (a slower node, an
    // older file) advancing her position backwards would make her re-perceive a
    // day of a room as new and act on it twice.
    #[test]
    fn cursors_only_move_forward() {
        let mut c = ActivityCursor { chat_lamport: 10, chat_event_id: Some(Uuid::from_u128(1)), views: [("kanban".to_string(), 5u64)].into() };
        c.advance_to(&ActivityCursor { chat_lamport: 3, chat_event_id: Some(Uuid::from_u128(2)), views: [("kanban".to_string(), 2u64), ("wall".to_string(), 1u64)].into() });
        assert_eq!(c.chat_lamport, 10);
        assert_eq!(c.chat_event_id, Some(Uuid::from_u128(1)));
        assert_eq!(c.views["kanban"], 5);
        assert_eq!(c.views["wall"], 1, "a kind never seen before is adopted");
        c.advance_to(&ActivityCursor { chat_lamport: 11, chat_event_id: Some(Uuid::from_u128(3)), views: Default::default() });
        assert_eq!((c.chat_lamport, c.chat_event_id), (11, Some(Uuid::from_u128(3))));
    }
}
