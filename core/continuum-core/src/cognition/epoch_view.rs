//! A mind's user-side prompt as one epoch that only appends
//! (docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md, Fault 1).
//!
//! Each act the deliberation faculty renders its sources (conversation turns, the results
//! ring, grounding) as keyed [`Block`]s in their canonical order. This view does not send
//! that render. It appends to the persona's [`Epoch`] only what is new or changed since the
//! last act, and the request carries the epoch, so every act's body is the previous act's
//! body plus a tail and the engine reuses the whole previous prefix. Measured on the M5
//! (2026-10-19, `delib.prompt.reuse_split`): re-rendering moved the prompt where the results
//! ring grew mid-prompt and re-prefilled 17-29k tokens per act; replaying those captures
//! through an epoch reused 92-95%.
//!
//! The epoch turns over, compacting to each source's current rendering, only when the
//! window's budget says the view no longer fits, or when she acts in another activity room.
//! Every turnover is a
//! `context.epoch.turnover` probe with its reason and the characters it reclaimed.
//!
//! A view renders on a copy of the epoch; the faculty commits it only when the model answered
//! that view. A refused, refitted or failed request leaves her epoch as it was.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use continuum_context::epoch::{self, Epoch, Observed, SourceId};
use uuid::Uuid;

use super::working_memory::RingPart;
use super::workspace::BurstTurn;
use crate::ai::types::ChatMessage;

/// One message as the epoch holds it. Shared text: a view renders on a copy of the epoch.
#[derive(Clone, Debug, PartialEq)]
struct Said {
    role: Arc<str>,
    text: Arc<str>,
}

impl epoch::Block for Said {
    fn chars(&self) -> usize {
        self.text.len()
    }
}

impl Said {
    fn message(&self) -> ChatMessage {
        ChatMessage::text(&*self.role, self.text.to_string())
    }
}

/// What one source rendered this act, as one message. A run of same-role turns renders as
/// one message with an item per turn, so a turn that joins the run appends alone.
pub(super) struct Block {
    role: &'static str,
    items: Vec<(SourceId, String)>,
    /// The compact form a saved result may collapse to when the view turns over.
    alternative: Option<String>,
}

impl Block {
    pub(super) fn new(role: &'static str, source: SourceId, text: String) -> Self {
        Self { role, items: vec![(source, text)], alternative: None }
    }

    pub(super) fn turns(role: &'static str, items: Vec<(SourceId, String)>) -> Self {
        Self { role, items, alternative: None }
    }

    pub(super) fn with_alternative(mut self, alternative: Option<String>) -> Self {
        self.alternative = alternative;
        self
    }

    pub(super) fn alternative(&self) -> Option<&str> {
        self.alternative.as_deref()
    }

    /// The canonical rendering: every item, as the source rendered it this act.
    pub(super) fn message(&self) -> ChatMessage {
        ChatMessage::text(self.role, self.items.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n"))
    }
}

impl From<(RingPart, String, Option<String>)> for Block {
    fn from((part, body, pointer): (RingPart, String, Option<String>)) -> Self {
        let source = SourceId(match part {
            RingPart::Banner => "ring/banner".to_owned(),
            RingPart::Result(seq) => format!("ring/#{seq}"),
            RingPart::Collapsed => "ring/collapsed".to_owned(),
        });
        Block::new("user", source, body).with_alternative(pointer)
    }
}

/// Trailing contributions as blocks, each `[faculty]` banner then its body. Keyed by faculty
/// and its ordinal among that faculty's contributions this act, so two never supersede each other.
pub(super) fn contribution_blocks<'a, B: AsRef<str>>(contributions: impl Iterator<Item = (&'a str, B)>) -> Vec<Block> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    contributions
        .map(|(name, body)| {
            let body = body.as_ref();
            let ordinal = seen.entry(name).or_default();
            let source = SourceId(format!("faculty/{name}#{ordinal}"));
            *ordinal += 1;
            let mut text = String::with_capacity(name.len() + body.len() + 4);
            text.push('[');
            text.push_str(name);
            text.push_str("]\n");
            text.push_str(body);
            Block::new("user", source, text)
        })
        .collect()
}

/// Each turn's identity: who said it, when, and what, plus its ordinal among identical turns
/// in the window (a test burst repeats itself with no timestamp).
pub(super) fn turn_sources(turns: &[BurstTurn]) -> Vec<SourceId> {
    let mut seen: HashMap<u64, usize> = HashMap::new();
    turns
        .iter()
        .map(|t| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (t.is_self, &t.author, t.occurred_at_ms, &t.content).hash(&mut h);
            let fp = h.finish();
            let ordinal = seen.entry(fp).or_default();
            let source = SourceId(format!("turn/{fp:016x}#{ordinal}"));
            *ordinal += 1;
            source
        })
        .collect()
}

fn fingerprint(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Why the view started a new epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Turnover {
    /// The window's budget could not hold the epoch with this act appended.
    Budget,
    /// She acts in another activity room: its conversation is not this epoch's.
    RoomChanged,
}

impl Turnover {
    fn as_str(self) -> &'static str {
        match self {
            Self::Budget => "budget",
            Self::RoomChanged => "room_changed",
        }
    }
}

/// One persona's user-side view, held across acts.
#[derive(Clone, Debug)]
pub(super) struct EpochView {
    epoch: Epoch<Said>,
    /// The fingerprint of what each source item last placed in this epoch, or of what a
    /// turnover elided: an item whose rendering matches it is already said.
    known: HashMap<SourceId, u64>,
    /// The source of each message of a compacted render, until the fit says which survive.
    compacted: Vec<SourceId>,
    /// The activity room this epoch's conversation belongs to.
    room: Option<Uuid>,
}

impl Default for EpochView {
    fn default() -> Self {
        Self { epoch: Epoch::new(0, Vec::new()), known: HashMap::new(), compacted: Vec::new(), room: None }
    }
}

impl EpochView {
    fn render(&self) -> Vec<ChatMessage> {
        self.epoch.render().map(Said::message).collect()
    }

    fn start_next(&mut self, reason: Turnover, persona: &str) {
        crate::probe!(
            class = "context.epoch.turnover",
            persona = %persona,
            reason = reason.as_str(),
            epoch = self.epoch.id(),
            superseded_chars = self.epoch.superseded_chars(),
            epoch_chars = self.epoch.chars(),
            "her view starts a new epoch: the next request re-prefills from the system head"
        );
        self.epoch = Epoch::new(self.epoch.id() + 1, Vec::new());
        self.known.clear();
        self.compacted.clear();
    }

    /// Append what each block says that this epoch has not; returns nothing when nothing changed.
    fn observe_blocks(&mut self, blocks: &[Block]) -> Vec<ChatMessage> {
        let mut appended = Vec::new();
        for block in blocks {
            let alternative = block.alternative.as_deref().map(fingerprint);
            let unsaid: Vec<&(SourceId, String)> = block
                .items
                .iter()
                .filter(|(source, text)| {
                    let known = self.known.get(source).copied();
                    known.is_none() || (known != Some(fingerprint(text)) && known != alternative)
                })
                .collect();
            let Some(((first, _), _)) = unsaid.split_first() else { continue };
            let said = Said {
                role: block.role.into(),
                text: unsaid.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n").into(),
            };
            if let Observed::Appended { .. } = self.epoch.observe(first.clone(), said.clone()) {
                appended.push(said.message());
            }
            for (source, text) in unsaid {
                self.known.insert(source.clone(), fingerprint(text));
            }
        }
        appended
    }

    /// This act's body as the epoch: `history[..body_len]` (the canonical render of `body`)
    /// becomes the epoch with this act's news appended. Returns the epoch's length in messages.
    pub(super) fn materialize(
        &mut self,
        history: &mut Vec<ChatMessage>,
        body: &[Block],
        room: Uuid,
        persona: &str,
    ) -> usize {
        if self.room.is_some_and(|old| old != room) {
            self.start_next(Turnover::RoomChanged, persona);
        }
        self.room = Some(room);
        let appended = self.observe_blocks(body);
        crate::probe!(
            class = "context.epoch.append",
            persona = %persona,
            epoch = self.epoch.id(),
            appended_messages = appended.len(),
            appended_chars = appended.iter().map(|m| m.content_text().len()).sum::<usize>(),
            epoch_chars = self.epoch.chars(),
            superseded_chars = self.epoch.superseded_chars(),
            "this act's news appended to her view; everything before it is the last act's prefix"
        );
        let render = self.render();
        let len = render.len();
        history.splice(..body.len(), render);
        len
    }

    /// The window cannot hold the epoch: compact it to each source's current rendering, in the
    /// order those renderings arrived, dropping sources `body` no longer offers. Replaces
    /// `history[..epoch_len]` and returns the compacted length with the saved results' compact
    /// alternatives (index into history), which the fit may substitute. [`Self::keep`] then
    /// records which messages the fit kept.
    pub(super) fn turn_over(
        &mut self,
        history: &mut Vec<ChatMessage>,
        epoch_len: usize,
        body: &[Block],
        persona: &str,
    ) -> (usize, Vec<(usize, ChatMessage)>) {
        crate::probe!(
            class = "context.epoch.turnover",
            persona = %persona,
            reason = Turnover::Budget.as_str(),
            epoch = self.epoch.id(),
            superseded_chars = self.epoch.superseded_chars(),
            epoch_chars = self.epoch.chars(),
            "her view no longer fits its budget: compacted to each source's latest version"
        );
        let offered: HashMap<&SourceId, Option<&String>> = body
            .iter()
            .flat_map(|b| b.items.iter().map(move |(s, _)| (s, b.alternative.as_ref())))
            .collect();
        for source in self.epoch.live_sources() {
            if !offered.contains_key(&source) {
                self.epoch.retire(source.clone(), Said { role: "".into(), text: "".into() });
                self.known.remove(&source);
            }
        }
        self.epoch = self.epoch.turn_over(self.epoch.id() + 1);
        self.compacted = self.epoch.events().iter().map(|e| e.source.clone()).collect();
        let pointers = self
            .compacted
            .iter()
            .enumerate()
            .filter_map(|(index, source)| match offered.get(source) {
                Some(Some(alternative)) => Some((index, ChatMessage::text("user", alternative.as_str()))),
                _ => None,
            })
            .collect();
        let render = self.render();
        let len = render.len();
        history.splice(..epoch_len, render);
        (len, pointers)
    }

    /// The fit kept `survivors`, after dropping the first `dropped` compacted messages. The
    /// epoch becomes exactly what was sent; the dropped sources stay known, so an unchanged
    /// one does not come back on the next act.
    pub(super) fn keep<'a>(&mut self, dropped: usize, survivors: impl IntoIterator<Item = &'a ChatMessage>) {
        let mut next = Epoch::new(self.epoch.id(), Vec::new());
        for (source, message) in self.compacted.iter().skip(dropped).zip(survivors) {
            let text = message.content_text();
            self.known.insert(source.clone(), fingerprint(&text));
            next.observe(source.clone(), Said { role: message.role.as_str().into(), text: text.into() });
        }
        self.epoch = next;
        self.compacted.clear();
    }

    /// Blocks priced after the fit (standing grounding): append their news; returns the
    /// appended messages, which go at the epoch's end.
    pub(super) fn append(&mut self, blocks: &[Block]) -> Vec<ChatMessage> {
        self.observe_blocks(blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(source: &str, text: &str) -> Block {
        Block::new("user", SourceId(source.into()), text.into())
    }

    fn texts(history: &[ChatMessage]) -> Vec<String> {
        history.iter().map(ChatMessage::content_text).collect()
    }

    /// One act: the canonical body followed by one per-act fact, as the faculty renders it.
    fn act(view: &mut EpochView, room: Uuid, body: &[Block]) -> (Vec<ChatMessage>, usize) {
        let mut history: Vec<ChatMessage> = body.iter().map(Block::message).collect();
        history.push(ChatMessage::text("user", "[fact] flickers every act"));
        let len = view.materialize(&mut history, body, room, "test");
        (history, len)
    }

    // what this catches: a source that grows mid-body (the results ring ahead of grounding)
    // moving every message behind it, which re-prefilled 17-29k tokens per act on the M5.
    #[test]
    fn a_result_that_lands_mid_body_appends_at_the_epochs_end() {
        let mut view = EpochView::default();
        let room = Uuid::new_v4();
        let (first, n1) = act(&mut view, room, &[block("ring/#1", "r1"), block("faculty/recall#0", "recall")]);
        let (second, n2) = act(
            &mut view,
            room,
            &[block("ring/#1", "r1"), block("ring/#2", "r2"), block("faculty/recall#0", "recall")],
        );
        assert_eq!(texts(&second[..n1]), texts(&first[..n1]), "the last act's body is this act's prefix");
        assert_eq!(texts(&second[n1..n2]), ["r2"], "only the news appended");
        assert_eq!(second.last().map(ChatMessage::content_text).as_deref(), Some("[fact] flickers every act"));
    }

    // what this catches: a turn that joins a run of same-role turns re-sending the whole run.
    #[test]
    fn a_turn_that_joins_a_run_appends_alone() {
        let mut view = EpochView::default();
        let room = Uuid::new_v4();
        let t = |s: &str, text: &str| (SourceId(s.into()), text.to_owned());
        act(&mut view, room, &[Block::turns("user", vec![t("turn/a", "Ana: hi")])]);
        let (second, n2) =
            act(&mut view, room, &[Block::turns("user", vec![t("turn/a", "Ana: hi"), t("turn/b", "Bo: hey")])]);
        assert_eq!(texts(&second[..n2]), ["Ana: hi", "Bo: hey"]);
    }

    // what this catches: a turnover that loses the view's budget discipline, brings back what
    // the fit dropped, or re-expands a result the fit collapsed to its pointer.
    #[test]
    fn turnover_keeps_what_the_fit_kept_and_nothing_it_dropped_comes_back() {
        let mut view = EpochView::default();
        let room = Uuid::new_v4();
        let ring = |seq: u32, text: &str| {
            Block::new("user", SourceId(format!("ring/#{seq}")), text.into())
                .with_alternative(Some(format!("[result #{seq} — collapsed]")))
        };
        act(&mut view, room, &[block("turn/old", "old turn"), ring(1, "r1 long"), block("faculty/board#0", "board v1")]);
        let body = [block("turn/old", "old turn"), ring(1, "r1 long"), block("faculty/board#0", "board v2")];
        let (mut history, len) = act(&mut view, room, &body);
        let (compacted, pointers) = view.turn_over(&mut history, len, &body, "test");
        assert_eq!(compacted, 3, "board v1 superseded away");
        assert_eq!(texts(&history[..compacted]), ["old turn", "r1 long", "board v2"]);
        assert_eq!(pointers.len(), 1);
        // The fit dropped the old turn and collapsed r1 to its pointer.
        let survivors = [ChatMessage::text("user", "[result #1 — collapsed]"), ChatMessage::text("user", "board v2")];
        view.keep(1, &survivors);
        let (next, n) = act(&mut view, room, &body);
        assert_eq!(texts(&next[..n]), ["[result #1 — collapsed]", "board v2"], "nothing dropped or collapsed returns");
    }

    // what this catches: one room's conversation riding into another room's prompt.
    #[test]
    fn another_room_starts_a_new_epoch() {
        let mut view = EpochView::default();
        act(&mut view, Uuid::new_v4(), &[block("turn/a", "room A talk")]);
        let (other, n) = act(&mut view, Uuid::new_v4(), &[block("turn/b", "room B talk")]);
        assert_eq!(texts(&other[..n]), ["room B talk"]);
    }
}
