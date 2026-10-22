//! Which results-ring entries ride as their pointer instead of their body: sticky, and all
//! at once.
//!
//! The ring's saved results each offer a compact pointer (`tool/output` expands it). When
//! the fitted history is over its budget, results collapse to pointers. Measured on the M5
//! (prompt captures, 104 consecutive act pairs across six residents, 2026-10-18): in 46% of
//! the re-prefilled characters the first changed byte was a ring result rewritten to its
//! pointer, or back. The old rule collapsed oldest-first "until it fits" each act, against a
//! budget that swings with the newest result's size, so a result collapsed on one act came
//! back whole on the next, and the collapse frontier crept one result per act through the
//! middle of the ring, re-prefilling everything behind it every time.
//!
//! The rule here:
//! - a result once collapsed stays collapsed while it is offered (in the ring);
//! - when the budget forces a collapse, every collapsible result goes in that one rewrite,
//!   so the acts after it append instead of rewriting.
//!
//! Bodies stay reachable by handle. The cost, stated: after one tight act, older results
//! stay pointers when a later act has room.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use crate::ai::types::ChatMessage;

/// One resident's collapsed ring results, by the identity of their pointer text (which
/// names the result's seq, room, operation and handle, so it is stable per result).
#[derive(Debug, Default)]
pub struct StickyCollapse {
    collapsed: HashSet<u64>,
}

fn identity(pointer: &ChatMessage) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    pointer.content_text().hash(&mut h);
    h.finish()
}

impl StickyCollapse {
    /// Apply the rule to `history` (indexes in `pointers` address it), lowering `tokens` by
    /// what each collapse saves, and return the pointers still open.
    pub fn apply(
        &mut self,
        history: &mut [ChatMessage],
        pointers: Vec<(usize, ChatMessage)>,
        tokens: &mut usize,
        budget: usize,
        cost: impl Fn(&ChatMessage) -> usize,
    ) -> Vec<(usize, ChatMessage)> {
        let offered: Vec<(usize, ChatMessage, u64)> = pointers
            .into_iter()
            .map(|(index, pointer)| {
                let id = identity(&pointer);
                (index, pointer, id)
            })
            .collect();
        // Results that left the ring leave the set: it never outgrows what is offered.
        self.collapsed.retain(|id| offered.iter().any(|(_, _, o)| o == id));
        // Swap a body for its pointer when that saves tokens; give the pointer back if not.
        let collapse = |history: &mut [ChatMessage], index: usize, pointer: ChatMessage, tokens: &mut usize| {
            let Some(message) = history.get_mut(index) else { return Some(pointer) };
            let (old, new) = (cost(message), cost(&pointer));
            if new >= old {
                return Some(pointer);
            }
            *tokens = tokens.saturating_sub(old - new);
            *message = pointer;
            None
        };
        let mut open = Vec::new();
        for (index, pointer, id) in offered {
            let still_open = if self.collapsed.contains(&id) {
                collapse(history, index, pointer, tokens)
            } else {
                Some(pointer)
            };
            if let Some(pointer) = still_open {
                open.push((index, pointer, id));
            }
        }
        if *tokens > budget {
            for (index, pointer, id) in std::mem::take(&mut open) {
                match collapse(history, index, pointer, tokens) {
                    None => {
                        self.collapsed.insert(id);
                    }
                    Some(pointer) => open.push((index, pointer, id)),
                }
            }
        }
        open.into_iter().map(|(index, pointer, _)| (index, pointer)).collect()
    }

    /// Forget every collapse: the next fit starts from whole bodies.
    #[cfg(test)]
    pub fn forget(&mut self) {
        self.collapsed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(n: usize) -> (Vec<ChatMessage>, Vec<(usize, ChatMessage)>) {
        let history = (0..n).map(|i| ChatMessage::text("user", format!("[result #{i}] {}", "x".repeat(100)))).collect();
        let pointers = (0..n).map(|i| (i, ChatMessage::text("user", format!("[result #{i} — collapsed]")))).collect();
        (history, pointers)
    }

    fn len(m: &ChatMessage) -> usize {
        m.content_text().len()
    }

    // what this catches: the collapse frontier moving with each act's budget (a result
    // collapsed on one act, whole on the next; or one more result collapsed mid-ring per act),
    // which re-prefilled everything behind the rewritten result on every act.
    #[test]
    fn a_forced_collapse_takes_every_result_at_once_and_stays_when_room_returns() {
        let mut sticky = StickyCollapse::default();
        let (mut history, pointers) = ring(4);
        let mut tokens: usize = history.iter().map(len).sum();
        let budget = tokens - 10;
        // Over budget by a little: all four collapse, not just the oldest.
        let open = sticky.apply(&mut history, pointers, &mut tokens, budget, len);
        assert!(open.is_empty());
        assert!(history.iter().all(|m| m.content_text().contains("collapsed")));
        // Next act, roomy: the same four are offered again and stay collapsed.
        let (mut history, pointers) = ring(4);
        let mut tokens: usize = history.iter().map(len).sum();
        let open = sticky.apply(&mut history, pointers, &mut tokens, usize::MAX, len);
        assert!(open.is_empty());
        assert!(history.iter().all(|m| m.content_text().contains("collapsed")));
        // A new result appended after them rides whole: only the frontier grows.
        let (mut history, pointers) = ring(5);
        let mut tokens: usize = history.iter().map(len).sum();
        let open = sticky.apply(&mut history, pointers, &mut tokens, usize::MAX, len);
        assert_eq!(open.len(), 1);
        assert!(!history[4].content_text().contains("collapsed"));
        // A result that left the ring leaves the set.
        let (mut history, pointers) = ring(0);
        let mut tokens = 0;
        sticky.apply(&mut history, pointers, &mut tokens, usize::MAX, len);
        assert!(sticky.collapsed.is_empty());
    }
}
