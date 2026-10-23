//! Her attention: the dial and the continuation. Both are HERS.
//!
//! EVENT-MIND.md §1, §6. Two kinds of focus exist and only one is a seam: the
//! system narrowing her (a wake room, a computed focus card) is deleted; her own
//! focus is agency. This module holds that agency as data:
//!
//! - [`AttentionDial`]: deep ↔ broad, and which salience may interrupt her. Deep:
//!   a short strip, only `Urgent` passes the door. Broad: the whole world beside
//!   her task. The cost of spreading thin is real and hers to pay knowingly; the
//!   substrate shows the load (`awareness::Load`) and never caps it.
//! - [`Continuation`]: her own note of what she is working on, what is next and
//!   what she expects, re-weighed by her against everything unread every turn,
//!   never a rail. It is also what she resumes after a restart (rule 8).
//!
//! Written ONLY by her acts (`self/attention`, `self/continue`). No loop code may
//! set either (BigMama, phase 2). Pure data; persistence is `mind_state`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::salience::{Expectation, SalienceLevel};

/// How much of the world she lets in beside her task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Depth {
    /// Door shut: a few lines of strip; only `Urgent` interrupts. The engineer
    /// ninety percent into a hard problem.
    Deep,
    /// The default: the strip at normal depth; `Addressed` interrupts.
    Normal,
    /// Everything: the full inbox beside her task; `Notable` interrupts. Juggling.
    Broad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionDial {
    pub depth: Depth,
    /// The lowest salience that may interrupt her. `Urgent` always passes whatever
    /// this says (a human in distress, a blocker, the world contradicting her).
    pub pass: SalienceLevel,
}

impl Default for AttentionDial {
    fn default() -> Self {
        Self { depth: Depth::Normal, pass: SalienceLevel::Addressed }
    }
}

impl AttentionDial {
    pub fn deep() -> Self {
        Self { depth: Depth::Deep, pass: SalienceLevel::Urgent }
    }
    pub fn broad() -> Self {
        Self { depth: Depth::Broad, pass: SalienceLevel::Notable }
    }

    /// Whether `level` may interrupt her at this setting. `Urgent` always may.
    pub fn admits(&self, level: SalienceLevel) -> bool {
        level == SalienceLevel::Urgent || level >= self.pass
    }

    /// Lines of strip she sees at this depth (one per activity, loudest first).
    pub fn strip_lines(&self) -> usize {
        match self.depth {
            Depth::Deep => 3,
            Depth::Normal => 8,
            Depth::Broad => usize::MAX,
        }
    }
}

/// What she is working on now, in her own words, and what she expects next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Continuation {
    /// The activity (room) she is working in; the depth of her perception is on it.
    pub activity: Uuid,
    /// Her note to herself: "upgrade test done; next: run the suite, then deploy".
    pub note: String,
    /// What she expects to happen, judged by `salience` for `Surprise` / `Silent`.
    pub expectation: Option<Expectation>,
    pub written_at_ms: u64,
}

impl Continuation {
    /// Due when she set a time she expected something by and it has passed: the
    /// perception region wakes her with `Wake::Continuation` so she can look.
    pub fn is_due(&self, now_ms: u64) -> bool {
        self.expectation
            .as_ref()
            .and_then(|e| e.by_ms)
            .is_some_and(|by| now_ms >= by)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the door. Deep admits only Urgent; Normal admits Addressed;
    // Broad admits Notable; Urgent passes every setting. If Urgent could ever be
    // shut out, a human in distress would wait for her dial.
    #[test]
    fn the_dial_admits_by_level_and_urgent_always_passes() {
        let deep = AttentionDial::deep();
        assert!(!deep.admits(SalienceLevel::Addressed));
        assert!(deep.admits(SalienceLevel::Urgent));
        let normal = AttentionDial::default();
        assert!(normal.admits(SalienceLevel::Addressed));
        assert!(!normal.admits(SalienceLevel::Notable));
        let broad = AttentionDial::broad();
        assert!(broad.admits(SalienceLevel::Notable));
        assert!(!broad.admits(SalienceLevel::Quiet));
        let shut = AttentionDial { depth: Depth::Deep, pass: SalienceLevel::Urgent };
        assert!(shut.admits(SalienceLevel::Urgent));
        assert!(deep.strip_lines() < normal.strip_lines());
    }

    // what this catches: a continuation is due exactly when the time she expected
    // something by has passed, and never when she set no time. A due continuation
    // is a wake she asked for, not one the loop invented.
    #[test]
    fn a_continuation_is_due_only_past_her_own_deadline() {
        let mut c = Continuation {
            activity: Uuid::from_u128(1),
            note: "next: run the suite".into(),
            expectation: Some(Expectation { text: "Cormac replies".into(), by_ms: Some(1_000), verdict: None }),
            written_at_ms: 0,
        };
        assert!(!c.is_due(999));
        assert!(c.is_due(1_000));
        c.expectation = None;
        assert!(!c.is_due(u64::MAX));
    }
}
