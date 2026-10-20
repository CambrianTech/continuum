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
//! The epoch also binds the rendered system head and tool surface, inside the grant the head
//! was admitted with, and each standing source's admitted size: body growth never re-renders
//! either; only a change in what they say does.
//!
//! The epoch turns over, compacting to each source's current rendering, only when the
//! served window cannot hold it (leaving room for the measured per-act growth), when one act
//! adds more than the lane prefills in the audience's time to first token, when she acts in
//! another activity room, or when the head changes. Every turnover
//! is a `context.epoch.turnover` probe with its reason and the characters it reclaimed.
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

/// One source item as the epoch holds it: one event per item, so a turnover keeps or retires
/// each item on its own. Items appended together from one block share a `run` and render as
/// one message; a later item never joins an earlier run, so a rendered message never changes.
/// Shared text: a view renders on a copy of the epoch.
#[derive(Clone, Debug, PartialEq)]
struct Said {
    role: Arc<str>,
    text: Arc<str>,
    run: u64,
}

impl epoch::Block for Said {
    fn chars(&self) -> usize {
        self.text.len()
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

/// The standing pieces a turn renders after its body: each changes only when what it says
/// does, so each is an epoch source.
pub(super) struct Standing {
    /// Her own-time and presence framing (`deliberation_prompt::trailing_standing`).
    pub framing: String,
    /// The ask of a turn she began herself (the `[work turn]`); an external ask stays per act.
    pub own_ask: Option<String>,
    /// The steps-taken ledger, one item per line.
    pub ledger: Option<String>,
    /// Substrate notices by fact id; the open-views notice is replaced by `open_views`.
    pub notices: Vec<(&'static str, String)>,
    /// Her open views: a header and one `(identity, line)` per view.
    pub open_views: Option<(String, Vec<(String, String)>)>,
}

/// One item per line, keyed by what the line says: a new line appends alone.
fn lines(key: &str, text: &str) -> Vec<(SourceId, String)> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    text.lines()
        .map(|line| {
            // Identical lines (a notice posted twice) are distinct items, by ordinal.
            let ordinal = seen.entry(line).or_default();
            let source = SourceId(format!("standing/{key}/{line}#{ordinal}"));
            *ordinal += 1;
            (source, line.to_owned())
        })
        .collect()
}

/// The standing pieces as epoch sources: one per piece, one per ledger or notice line, one per
/// open view.
pub(super) fn standing_blocks(s: Standing) -> Vec<Block> {
    let mut blocks = Vec::new();
    let one = |key: &str, text: String| Block::new("user", SourceId(format!("standing/{key}")), text);
    if !s.framing.trim().is_empty() {
        blocks.push(one("framing", s.framing));
    }
    if let Some(ask) = s.own_ask {
        blocks.push(one("own-ask", ask));
    }
    if let Some(ledger) = s.ledger {
        blocks.push(Block::turns("user", lines("ledger", &ledger)));
    }
    for (id, text) in s.notices.into_iter().filter(|(id, _)| *id != "open_views") {
        blocks.push(Block::turns("user", lines(&format!("notice/{id}"), &text)));
    }
    if let Some((header, views)) = s.open_views {
        blocks.push(one("open-views", header));
        blocks.extend(views.into_iter().map(|(identity, line)| one(&format!("open-views/{identity}"), line)));
    }
    blocks
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
pub(super) enum Turnover {
    /// The served window could not hold the epoch with this act appended.
    Window,
    /// She acts in another activity room: its conversation is not this epoch's.
    RoomChanged,
    /// The head's rendering changed (policy, identity, its grounding's content).
    HeadChanged,
    /// The offered tool surface changed.
    ToolsChanged,
    /// The model (and with it the template and tokenizer) changed.
    ModelChanged,
}

impl Turnover {
    fn as_str(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::RoomChanged => "room_changed",
            Self::HeadChanged => "head_changed",
            Self::ToolsChanged => "tools_changed",
            Self::ModelChanged => "model_changed",
        }
    }
}

/// Why a budget turnover happened, with the numbers that decided it.
#[derive(Clone, Copy, Debug)]
pub(super) struct TurnoverCause {
    pub reason: Turnover,
    /// Tokens after her epoch's cached prefix: what this act would prefill.
    pub fresh_tokens: usize,
}

/// The rendered system head and tool surface an epoch was opened with (Codex on #5210): the
/// head renders inside the grant it was admitted with, so body growth never re-renders it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Head {
    pub system: Arc<str>,
    /// The offered tool surface's identity (names and priced schema size).
    pub tools: u64,
    /// The model the request names; its chat template and tokenizer come with it.
    pub model: Arc<str>,
    /// Tokens the head's grounding context was admitted with, from the act's one allocation.
    pub grant: usize,
}

/// One persona's user-side view, held across acts.
#[derive(Clone, Debug)]
pub(super) struct EpochView {
    epoch: Epoch<Said>,
    /// The fingerprint of what each source item last placed in this epoch, or of what a
    /// turnover elided: an item whose rendering matches it is already said.
    known: HashMap<SourceId, u64>,
    /// The events of each message of a compacted render, until the fit says which survive.
    compacted: Vec<Vec<epoch::Event<Said>>>,
    next_run: u64,
    head: Option<Head>,
    /// Each standing source's admitted units, by its content fingerprint: an unchanged source
    /// renders at its admitted size for the epoch's life, whatever room is left over.
    standing: HashMap<String, (u64, Option<usize>)>,
    /// Tokens the body grew by on the last act that appended to a live epoch: the measured
    /// room a turnover leaves for the next act.
    growth: usize,
    /// Tokens this act appended to a live epoch so far; `None` once the act turned it over
    /// (a turnover's re-appends are not growth).
    act_growth: Option<usize>,
    /// Tokens of the body as the last answered request carried it: the prefix her slot holds,
    /// so an act prefills only what follows it.
    cached: usize,
    // TODO(fable, follow-up to #5210 with Codex): one epoch across her activities. Source ids
    // carry their room, access scope is bound separately from attention, and a scope change
    // that must remove data turns over at once; until then another room starts a new epoch.
    /// The activity room this epoch's conversation belongs to.
    room: Option<Uuid>,
}

impl Default for EpochView {
    fn default() -> Self {
        Self {
            epoch: Epoch::new(0, Vec::new()),
            known: HashMap::new(),
            compacted: Vec::new(),
            next_run: 0,
            head: None,
            standing: HashMap::new(),
            growth: 0,
            act_growth: None,
            cached: 0,
            room: None,
        }
    }
}

/// Consecutive events of one run as one message, with the events it carries.
fn messages<'a>(events: impl Iterator<Item = &'a epoch::Event<Said>>) -> Vec<(ChatMessage, Vec<epoch::Event<Said>>)> {
    let mut runs: Vec<(String, Vec<epoch::Event<Said>>)> = Vec::new();
    for e in events {
        match runs.last_mut() {
            Some((text, group)) if group.last().is_some_and(|g| g.text.run == e.text.run) => {
                text.push('\n');
                text.push_str(&e.text.text);
                group.push(e.clone());
            }
            _ => runs.push((e.text.text.to_string(), vec![e.clone()])),
        }
    }
    runs.into_iter()
        .filter_map(|(text, group)| Some((ChatMessage::text(&*group.first()?.text.role, text), group)))
        .collect()
}

impl EpochView {
    fn render(&self) -> Vec<ChatMessage> {
        messages(self.epoch.events().iter()).into_iter().map(|(m, _)| m).collect()
    }

    fn probe_turnover(&self, reason: Turnover, persona: &str) {
        crate::probe!(
            class = "context.epoch.turnover",
            persona = %persona,
            reason = reason.as_str(),
            epoch = self.epoch.id(),
            superseded_chars = self.epoch.superseded_chars(),
            epoch_chars = self.epoch.chars(),
            "her view starts a new epoch: the next request re-prefills from where it changed"
        );
    }

    /// A new epoch with nothing carried: everything is re-admitted.
    fn start_next(&mut self, reason: Turnover, persona: &str) {
        self.probe_turnover(reason, persona);
        *self = Self { epoch: Epoch::new(self.epoch.id() + 1, Vec::new()), next_run: self.next_run, ..Self::default() };
    }

    /// Append what each block says that this epoch has not, one event per item; returns one
    /// message per block that appended (its items together), nothing when nothing changed.
    fn observe_blocks(&mut self, blocks: &[Block]) -> Vec<ChatMessage> {
        let mut appended = Vec::new();
        for block in blocks {
            let unsaid = self.unsaid(block);
            if unsaid.is_empty() {
                continue;
            }
            let run = self.next_run;
            self.next_run += 1;
            // The message sent is exactly what the epoch appended, so the next act renders it
            // byte for byte.
            let mut said = Vec::with_capacity(unsaid.len());
            for (source, text) in &unsaid {
                let event = Said { role: block.role.into(), text: text.as_str().into(), run };
                if let Observed::Appended { .. } = self.epoch.observe(source.clone(), event) {
                    said.push(text.as_str());
                }
                self.known.insert(source.clone(), fingerprint(text));
            }
            if !said.is_empty() {
                appended.push(ChatMessage::text(block.role, said.join("\n")));
            }
        }
        appended
    }

    /// The items of `block` this epoch does not hold yet (new, or changed since it last said them).
    fn unsaid<'b>(&self, block: &'b Block) -> Vec<&'b (SourceId, String)> {
        let alternative = block.alternative.as_deref().map(fingerprint);
        block
            .items
            .iter()
            .filter(|(source, text)| {
                let known = self.known.get(source).copied();
                known.is_none() || (known != Some(fingerprint(text)) && known != alternative)
            })
            .collect()
    }

    /// What appending `blocks` would cost, without appending: the price of their news.
    pub(super) fn unsaid_tokens(&self, blocks: &[Block], cost: impl Fn(&ChatMessage) -> usize) -> usize {
        blocks
            .iter()
            .map(|block| self.unsaid(block))
            .filter(|unsaid| !unsaid.is_empty())
            .map(|unsaid| cost(&ChatMessage::text("user", unsaid.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n"))))
            .sum()
    }

    /// This act's body as the epoch: `history[..body.len()]` (the canonical render of `body`)
    /// becomes the epoch with this act's news appended. Returns the epoch's length in messages.
    /// `cost` prices the news, which a live epoch records as its measured growth.
    pub(super) fn materialize(
        &mut self,
        history: &mut Vec<ChatMessage>,
        body: &[Block],
        room: Uuid,
        cost: impl Fn(&ChatMessage) -> usize,
        persona: &str,
    ) -> usize {
        if self.room.is_some_and(|old| old != room) {
            self.start_next(Turnover::RoomChanged, persona);
        }
        self.room = Some(room);
        let live = !self.epoch.events().is_empty();
        self.cached = self.render().iter().map(&cost).sum();
        let appended = self.observe_blocks(body);
        let grown: usize = appended.iter().map(&cost).sum();
        self.act_growth = live.then_some(grown);
        self.note_growth();
        crate::probe!(
            class = "context.epoch.append",
            persona = %persona,
            epoch = self.epoch.id(),
            appended_messages = appended.len(),
            appended_tokens = grown,
            measured_growth_tokens = self.growth,
            epoch_chars = self.epoch.chars(),
            superseded_chars = self.epoch.superseded_chars(),
            "this act's news appended to her view; everything before it is the last act's prefix"
        );
        let render = self.render();
        let len = render.len();
        history.splice(..body.len(), render);
        len
    }

    fn note_growth(&mut self) {
        if let Some(grown) = self.act_growth {
            self.growth = self.growth.max(grown);
        }
    }

    /// Tokens of the body her slot already holds from the last answered request.
    pub(super) fn cached_tokens(&self) -> usize {
        self.cached
    }

    /// The most a live epoch has grown by in one act: the room a turnover leaves.
    pub(super) fn growth(&self) -> usize {
        self.growth
    }

    /// Whether this act's prompt still extends her epoch. Only the window turns it over: a
    /// turnover re-prefills the whole new epoch, so it is never cheaper than sending a tail
    /// (turning over on the prefill-time bound took reuse from 0.92 to 0.35 on the M5,
    /// c30e660d9, 2026-10-20).
    pub(super) fn turnover_for(&self, prompt_tokens: usize, window_tokens: usize) -> Option<TurnoverCause> {
        (prompt_tokens > window_tokens).then(|| TurnoverCause {
            reason: Turnover::Window,
            fresh_tokens: prompt_tokens.saturating_sub(self.cached),
        })
    }

    /// The window cannot hold the epoch: compact it to each source's current rendering, in the
    /// order those renderings arrived, retiring each item `body` no longer offers. Replaces
    /// `history[..epoch_len]` and returns the compacted length with the saved results' compact
    /// alternatives (index into history), which the fit may substitute. [`Self::keep`] then
    /// records which messages the fit kept. The head and standing grants are re-admitted.
    pub(super) fn turn_over(
        &mut self,
        history: &mut Vec<ChatMessage>,
        epoch_len: usize,
        body: &[Block],
        cause: TurnoverCause,
        persona: &str,
    ) -> (usize, Vec<(usize, ChatMessage)>) {
        crate::probe!(
            class = "context.epoch.turnover",
            persona = %persona,
            reason = cause.reason.as_str(),
            epoch = self.epoch.id(),
            superseded_chars = self.epoch.superseded_chars(),
            epoch_chars = self.epoch.chars(),
            cached_tokens = self.cached,
            fresh_tokens = cause.fresh_tokens,
            "her view no longer fits: compacted to each source's latest version"
        );
        let offered: HashMap<&SourceId, Option<&String>> = body
            .iter()
            .flat_map(|b| b.items.iter().map(move |(s, _)| (s, b.alternative.as_ref())))
            .collect();
        for source in self.epoch.live_sources() {
            if !offered.contains_key(&source) {
                self.epoch.retire(source.clone(), Said { role: "".into(), text: "".into(), run: 0 });
                self.known.remove(&source);
            }
        }
        self.epoch = self.epoch.turn_over(self.epoch.id() + 1);
        self.act_growth = None;
        self.head = None;
        self.standing.clear();
        let (render, groups): (Vec<ChatMessage>, Vec<_>) = messages(self.epoch.events().iter()).into_iter().unzip();
        let pointers = groups
            .iter()
            .enumerate()
            .filter_map(|(index, group)| match group.as_slice() {
                [single] => offered.get(&single.source).copied().flatten().map(|alt| (index, ChatMessage::text("user", alt.as_str()))),
                _ => None,
            })
            .collect();
        self.compacted = groups;
        let len = render.len();
        history.splice(..epoch_len, render);
        (len, pointers)
    }

    /// The fit kept `survivors`, after dropping the first `dropped` compacted messages. The
    /// epoch becomes exactly what was sent; the dropped items stay known, so an unchanged one
    /// does not come back on the next act. A survivor the fit replaced is a result collapsed
    /// to its pointer, held under that result's source.
    pub(super) fn keep<'a>(&mut self, dropped: usize, survivors: impl IntoIterator<Item = &'a ChatMessage>) {
        let mut next = Epoch::new(self.epoch.id(), Vec::new());
        for (group, message) in std::mem::take(&mut self.compacted).into_iter().skip(dropped).zip(survivors) {
            let text = message.content_text();
            match group.as_slice() {
                [single] if *single.text.text != *text => {
                    self.known.insert(single.source.clone(), fingerprint(&text));
                    next.observe(single.source.clone(), Said { text: text.into(), ..single.text.clone() });
                }
                _ => group.into_iter().for_each(|e| {
                    next.observe(e.source, e.text);
                }),
            }
        }
        self.epoch = next;
    }

    /// The head this epoch renders inside: its grant bounds the head, never the body's leftover.
    pub(super) fn head_grant(&self) -> Option<usize> {
        self.head.as_ref().map(|h| h.grant)
    }

    /// Bind this act's rendered head. Within an epoch it renders inside the same grant, so a
    /// difference is a real change of policy, identity, grounding, tools or model: the binding
    /// is invalidated once, with its reason, and the body (already fitted) carries into the
    /// next epoch unchanged.
    pub(super) fn bind_head(&mut self, head: Head, persona: &str) {
        let reason = match &self.head {
            None => None,
            Some(old) if old.model != head.model => Some(Turnover::ModelChanged),
            Some(old) if old.tools != head.tools => Some(Turnover::ToolsChanged),
            Some(old) if old.system != head.system => Some(Turnover::HeadChanged),
            Some(_) => return,
        };
        if let Some(reason) = reason {
            self.probe_turnover(reason, persona);
            let mut next = Epoch::new(self.epoch.id() + 1, Vec::new());
            for e in self.epoch.events() {
                next.observe(e.source.clone(), e.text.clone());
            }
            self.epoch = next;
        }
        self.head = Some(head);
    }

    /// The units this epoch admitted a standing source at, while its content is unchanged.
    pub(super) fn admitted(&self, source: &str, content: u64) -> Option<Option<usize>> {
        self.standing.get(source).filter(|(fp, _)| *fp == content).map(|(_, units)| *units)
    }

    /// Record a standing source's admitted units for the epoch.
    pub(super) fn admit(&mut self, source: &str, content: u64, units: Option<usize>) {
        self.standing.insert(source.to_owned(), (content, units));
    }

    /// Blocks priced after the fit (standing grounding): append their news; returns the
    /// appended messages, which go at the epoch's end.
    pub(super) fn append(&mut self, blocks: &[Block], cost: impl Fn(&ChatMessage) -> usize) -> Vec<ChatMessage> {
        let appended = self.observe_blocks(blocks);
        if let Some(grown) = self.act_growth.as_mut() {
            *grown += appended.iter().map(&cost).sum::<usize>();
        }
        self.note_growth();
        appended
    }
}

/// The offered tool surface's identity: its names and the priced size of their schemas.
pub(super) fn tools_fingerprint<'a>(names: impl Iterator<Item = &'a str>, tokens: usize) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    names.for_each(|n| n.hash(&mut h));
    tokens.hash(&mut h);
    h.finish()
}

/// A standing source's content identity: what it says, never how much room was left for it.
pub(super) fn content_fingerprint(content: &str, parts: &[String]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (content, parts).hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cause() -> TurnoverCause {
        TurnoverCause { reason: Turnover::Window, fresh_tokens: 0 }
    }

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
        let len = view.materialize(&mut history, body, room, |m| m.content_text().len(), "test");
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
        let (compacted, pointers) = view.turn_over(&mut history, len, &body, cause(), "test");
        assert_eq!(compacted, 3, "board v1 superseded away");
        assert_eq!(texts(&history[..compacted]), ["old turn", "r1 long", "board v2"]);
        assert_eq!(pointers.len(), 1);
        // The fit dropped the old turn and collapsed r1 to its pointer.
        let survivors = [ChatMessage::text("user", "[result #1 — collapsed]"), ChatMessage::text("user", "board v2")];
        view.keep(1, &survivors);
        let (next, n) = act(&mut view, room, &body);
        assert_eq!(texts(&next[..n]), ["[result #1 — collapsed]", "board v2"], "nothing dropped or collapsed returns");
    }

    // what this catches: a merged run of turns kept and retired as ONE event under its first
    // turn (Codex on #5210): when the window slides past the first turn but still offers the
    // later ones, a turnover dropped the later turns while marking them already said.
    #[test]
    fn turnover_keeps_the_turns_a_sliding_window_still_offers() {
        let mut view = EpochView::default();
        let room = Uuid::new_v4();
        let t = |s: &str| (SourceId(format!("turn/{s}")), format!("Ana: {s}"));
        act(&mut view, room, &[Block::turns("user", vec![t("a"), t("b"), t("c")])]);
        let body = [Block::turns("user", vec![t("b"), t("c"), t("d")])];
        let (mut history, len) = act(&mut view, room, &body);
        let (compacted, _) = view.turn_over(&mut history, len, &body, cause(), "test");
        let kept = texts(&history[..compacted]).join("\n");
        assert_eq!(kept, "Ana: b\nAna: c\nAna: d", "the turns still offered survive the turnover");
    }

    // what this catches: a head change that goes unrecorded, or that rewrites the body that
    // was already fitted and sent with it.
    #[test]
    fn a_changed_head_starts_one_new_epoch_and_keeps_the_body() {
        let mut view = EpochView::default();
        let (_, n) = act(&mut view, Uuid::new_v4(), &[block("ring/#1", "r1")]);
        let head = |system: &str| Head { system: system.into(), tools: 1, model: "m".into(), grant: 10 };
        view.bind_head(head("identity v1"), "test");
        let (id, body) = (view.epoch.id(), view.render());
        view.bind_head(head("identity v1"), "test");
        assert_eq!(view.epoch.id(), id, "an unchanged head is no turnover");
        view.bind_head(head("identity v2"), "test");
        assert_eq!(view.epoch.id(), id + 1, "a changed head is a new epoch");
        assert_eq!(texts(&view.render()), texts(&body[..n]), "the fitted body carries over unchanged");
    }

    // what this catches: turnover headroom measured from a cold start's whole first render
    // (which would leave no body), or a standing source re-admitted when only room changed.
    #[test]
    fn growth_is_measured_on_a_live_epoch_and_an_unchanged_source_keeps_its_size() {
        let mut view = EpochView::default();
        let room = Uuid::new_v4();
        act(&mut view, room, &[block("ring/#1", "a long first render")]);
        assert_eq!(view.growth(), 0, "a cold start's first render is not growth");
        act(&mut view, room, &[block("ring/#1", "a long first render"), block("ring/#2", "r2")]);
        assert_eq!(view.growth(), 2, "the second act appended two chars");
        view.admit("room-kanban", 7, Some(3));
        assert_eq!(view.admitted("room-kanban", 7), Some(Some(3)));
        assert_eq!(view.admitted("room-kanban", 8), None, "a changed source is admitted afresh");
    }

    // what this catches: the whole open-views block (18.9k chars on the M5) re-sent when one
    // file's read changed, or a standing piece re-sent when nothing changed.
    #[test]
    fn one_changed_open_view_appends_only_that_view() {
        let mut view = EpochView::default();
        let room = Uuid::new_v4();
        let standing = |b_line: &str| standing_blocks(Standing {
            framing: "[Your own time] yours".into(),
            own_ask: None,
            ledger: Some("[steps taken this session]\n[action #1] code/read".into()),
            notices: vec![("memory_notices", "Notices: hands rooted".into()), ("open_views", "superseded by items".into())],
            open_views: Some(("[open views] header".into(), vec![
                ("a.rs".into(), "- a.rs: read 1-40".into()),
                ("b.rs".into(), b_line.into()),
            ])),
        });
        act(&mut view, room, &[]);
        let first = view.append(&standing("- b.rs: read 1-10"), |m| m.content_text().len());
        assert!(texts(&first).iter().all(|t| !t.contains("superseded by items")), "open views render from their items");
        assert!(view.append(&standing("- b.rs: read 1-10"), |m| m.content_text().len()).is_empty(), "nothing changed, nothing appended");
        assert_eq!(texts(&view.append(&standing("- b.rs: read 1-80"), |m| m.content_text().len())), ["- b.rs: read 1-80"], "only the view that changed");
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
