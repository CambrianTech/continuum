//! Her actions as one stream: one item per act, keyed by its `#n`. This one rendering
//! replaces the three her prompt used to carry (the results ring, the collapsed-pointer
//! index and the steps-taken ledger).
//!
//! It is never silent. With no act on record it says so, in one of the two honest forms the
//! steps ledger established: nothing has executed (an implicit void becomes a confabulation
//! shelter: Anwen parked fabricated receipts in one within minutes), or acts happened whose
//! details aged out (denying her own work is the inverse failure, 2026-07-13: Asha).

use continuum_context::cause::{Cause, Content, Origin};
use continuum_context::source::{Item, Source, StoredWhole};

use super::{element, priority, ContextMessage, Producer};
use crate::cognition::working_memory::WorkingMemory;

pub(crate) struct Actions<'a> {
    pub memory: &'a WorkingMemory,
    /// Whose spill directory a stored whole is read from (`load`).
    pub persona: uuid::Uuid,
}

impl Actions<'_> {
    fn standing(&self, key: &str, text: String) -> Item<ContextMessage> {
        super::standing(self.persona, "actions", key, ContextMessage::user(text))
    }

    fn act(&self, seq: u64) -> continuum_context::cause::ElementId {
        element(self.persona, Producer::Act, &seq.to_be_bytes())
    }
}

impl Source<ContextMessage> for Actions<'_> {
    fn name(&self) -> &str {
        "actions"
    }

    fn priority(&self) -> u8 {
        priority::ACTIONS
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        let banner = self.standing(
            "banner",
            "[actions] What I have done this session, oldest first: each #n is one act; a \
             collapsed one names the handle that reopens it."
                .to_owned(),
        );
        let (acts, in_flight) = self.memory.action_renderings();
        if acts.is_empty() && in_flight {
            // Her only act so far is the one she just took: it rides whole after her view.
            return vec![banner];
        }
        if acts.is_empty() {
            let taken = self.memory.actions_taken();
            let none = if taken == 0 {
                "(nothing has executed yet — anything described as already run, created, \
                 tested, committed, or merged does not exist, whether in the messages you can \
                 see or before them; running a tool is what makes it real)"
                    .to_owned()
            } else {
                // No recall promise: act receipts are deliberately outside semantic recall
                // (#166, #414), so pointing her there sends her after a door that is not there.
                format!(
                    "({taken} step{} executed earlier this session — you did that work; the details \
                     aged out of working memory and cannot be retrieved, so re-check the workspace \
                     or the board rather than trusting memory of them. Nothing NEW has executed since.)",
                    if taken == 1 { "" } else { "s" }
                )
            };
            return vec![banner, self.standing("none", none)];
        }
        let mut items = vec![banner];
        // Her acts in one room are one chain: each follows the act before it there; the first is
        // where that activity's chain starts.
        let mut last_in_room: std::collections::HashMap<Option<uuid::Uuid>, u64> = std::collections::HashMap::new();
        for act in acts {
            let cause = last_in_room.insert(act.room, act.seq).map_or(Cause::Root(Origin::ActivityStart), |prev| Cause::Of(self.act(prev)));
            let stored = act.stored.map(|(chars, handle)| StoredWhole { chars, handle, content: Content::Absent });
            let item = Item::rendered(self.act(act.seq), cause, act.seq.to_string(), ContextMessage::user(act.full), act.content)
                .with_summary(act.summary.map(ContextMessage::user))
                .with_pointer(ContextMessage::user(act.pointer))
                .with_stored(stored);
            items.push(match act.act {
                Some(identity) => item.with_act(identity),
                None => item,
            });
        }
        items
    }

    /// A whole her working memory evicted from RAM (storage, never visibility: FOCUS-CONTRACT
    /// rule 7), read back from its spill file when her view grants it whole, labelled as it
    /// read resident. `None` (the resident pointer renders) when the file is gone or unreadable.
    fn load(&self, item: &Item<ContextMessage>) -> Option<ContextMessage> {
        let stored = item.stored()?;
        let seq: u64 = item.key().parse().ok()?;
        let label = self.memory.action_whole_label(seq)?;
        let body = crate::cognition::tool_executor::spill::saved_whole(self.persona, &stored.handle).ok()?;
        Some(ContextMessage::user(format!("{label}{body}")))
    }

    /// Her oldest acts as one line: the first and last `#n`, how many, and how one reopens
    /// (`tool/output` with `act`), so her floor is one block however many acts she has taken
    /// (Aris, the M5, 2026-10-29: 95 pointers at ~190 chars before anything whole fit). Reads
    /// only the run's ends: the banner, when it is in the run, is its first item and folds
    /// with it. `None` for a run with no act in it (the banner alone): nothing to name.
    fn fold(&self, folded: &[Item<ContextMessage>]) -> Option<ContextMessage> {
        // The banner stands as itself ahead of the run (the view never folds a standing head),
        // so the run is her acts alone, keyed by their #n.
        let first: u64 = folded.first()?.key().parse().ok()?;
        let last: u64 = folded.last()?.key().parse().ok()?;
        let count = folded.len();
        Some(ContextMessage::user(format!(
            "[actions #{first}-#{last}: {count} earlier acts folded; reopen one with tool/output act=<n>]"
        )))
    }

    /// An act the view found repeats an earlier one whose whole she has in front of her (the same
    /// request to a command that only observes, the same outcome): one line naming it and how to
    /// reopen it, never a second copy of the whole (MIND-CONTEXT invariant 4).
    fn repeat(&self, item: &Item<ContextMessage>, ancestor: &Item<ContextMessage>) -> Option<ContextMessage> {
        Some(ContextMessage::user(format!(
            "[result #{}: repeat of #{}, outcome unchanged; reopen with tool/output act={}]",
            item.key(),
            ancestor.key(),
            ancestor.key()
        )))
    }

    /// Once she has acted, the void said earlier in this epoch is superseded, never edited:
    /// her view reads as the timeline it is (the statement was true when it was made).
    fn retired(&self) -> Vec<Item<ContextMessage>> {
        let (acts, in_flight) = self.memory.action_renderings();
        if acts.is_empty() && !in_flight {
            return Vec::new();
        }
        vec![self.standing("none", "(that was before my first act; my acts follow)".to_owned())]
    }
}
