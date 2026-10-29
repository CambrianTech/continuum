//! Her actions as one stream: one item per act, keyed by its `#n`. This one rendering
//! replaces the three her prompt used to carry (the results ring, the collapsed-pointer
//! index and the steps-taken ledger).
//!
//! It is never silent. With no act on record it says so, in one of the two honest forms the
//! steps ledger established: nothing has executed (an implicit void becomes a confabulation
//! shelter: Anwen parked fabricated receipts in one within minutes), or acts happened whose
//! details aged out (denying her own work is the inverse failure, 2026-07-13: Asha).

use continuum_context::source::{Item, Source};

use super::{priority, ContextMessage};
use crate::cognition::working_memory::WorkingMemory;

pub(crate) struct Actions<'a> {
    pub memory: &'a WorkingMemory,
    /// Whose spill directory a stored whole is read from (`load`).
    pub persona: uuid::Uuid,
}

impl Actions<'_> {
    fn standing(key: &str, text: String) -> Item<ContextMessage> {
        let message = ContextMessage::user(text);
        Item { key: key.to_owned(), full: message.clone(), summary: None, pointer: Some(message), stored: None }
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
        let banner = Self::standing(
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
            return vec![banner, Self::standing("none", none)];
        }
        let mut items = vec![banner];
        items.extend(acts.into_iter().map(|(seq, full, summary, pointer, stored)| Item {
            stored: stored.map(|(chars, handle)| continuum_context::source::StoredWhole { chars, handle }),
            key: seq.to_string(),
            full: ContextMessage::user(full),
            summary: summary.map(ContextMessage::user),
            pointer: Some(ContextMessage::user(pointer)),
        }));
        items
    }

    /// A whole her working memory evicted from RAM (storage, never visibility: FOCUS-CONTRACT
    /// rule 7), read back from its spill file when her view grants it whole, labelled as it
    /// read resident. `None` (the resident pointer renders) when the file is gone or unreadable.
    fn load(&self, item: &Item<ContextMessage>) -> Option<ContextMessage> {
        let stored = item.stored.as_ref()?;
        let seq: u64 = item.key.parse().ok()?;
        let label = self.memory.action_whole_label(seq)?;
        let body = crate::cognition::tool_executor::spill::saved_whole(self.persona, &stored.handle).ok()?;
        Some(ContextMessage::user(format!("{label}{body}")))
    }

    /// Her oldest acts as one line: the first and last `#n` and how many, so her floor is one
    /// block however many acts she has taken (Aris, the M5, 2026-10-29: 95 pointers at ~190
    /// chars before anything whole fit). The banner folds with them; the line says what it is.
    /// `None` for a run with no act in it (the banner alone): nothing to name.
    fn fold(&self, folded: &[Item<ContextMessage>]) -> Option<ContextMessage> {
        let mut acts = folded.iter().filter_map(|item| item.key.parse::<u64>().ok());
        let first = acts.next()?;
        let (last, count) = acts.fold((first, 1_usize), |(_, count), seq| (seq, count + 1));
        Some(ContextMessage::user(format!("[actions #{first}-#{last}: {count} earlier acts, folded at this start]")))
    }

    /// Once she has acted, the void said earlier in this epoch is superseded, never edited:
    /// her view reads as the timeline it is (the statement was true when it was made).
    fn retired(&self) -> Vec<Item<ContextMessage>> {
        let (acts, in_flight) = self.memory.action_renderings();
        if acts.is_empty() && !in_flight {
            return Vec::new();
        }
        vec![Self::standing("none", "(that was before my first act; my acts follow)".to_owned())]
    }
}
