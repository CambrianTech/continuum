//! Her act's tail as one source of her view (MIND-CONTEXT, "Revision, 2026-11-11: her act's tail is
//! a source"): the room's working context, the act's framing, the ask, her statement, her words and
//! calls and their results, in the order her prompt sends them. Her view judges its items before
//! the send (`View::judge_tail`: a repeat of an act whose whole she holds arrives as its repeat
//! block, an element she already holds is not shown again) and they join her view once the prompt
//! is served (`View::append_tail`). It is never a start source: its claims are the act's own in the
//! one allocation (required input, her newest result, her last move).
//!
//! The one renderer of what follows her view: every message after it comes from here, in the exact
//! typed sequence the engine is sent (roles, order, part boundaries, call ids, names and argument
//! bytes, which call each result answers), never pushed by hand in the prompt assembly.
//!
//! Keys say what an item is: `act/{seq}/call`, `act/{seq}/result`, `turn/{key}/stimulus`, and a
//! line named by its content (`framing/…`, `context/…`, `statement/…`, `world/…`), so a line her
//! view already holds is the same element and is not sent twice. Her result shares her act's
//! element with her actions (`element(persona, Act, seq)`) and the ask shares its turn's with her
//! conversation, so once held here neither arrives from its source again.

use continuum_context::cause::{Cause, Content, Origin};
use continuum_context::source::{Item, Source};
use continuum_context::view::Judged;
use sha2::Digest;

use super::native::{NativePart, ToolInput};
use super::{element, priority, ContextMessage, Producer, Role};
use crate::ai::types::{ChatMessage, ContentPart, MessageContent, ToolCall, ToolResult};

/// The source's name: its items are `tail/<key>` in her view.
pub(crate) const NAME: &str = "tail";

/// Which part of her prompt a tail item is: each has its slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// The room's working-context block, ahead of the framing (`Tail::context`).
    Context,
    /// The act's framing (the clock, what is in force): optional history before the ask.
    Framing,
    /// The ask (the newest external speech) or an empty room's world state: required.
    Stimulus,
    /// Her public statement when it is not her words in her newest call: required.
    Statement,
    /// Her words and her calls, one assistant turn as she wrote them.
    Call,
    /// Their results (the engine's tool results), or a legacy receipt's text: her newest result.
    Result,
}

/// The ask: the turn held out of her conversation as this act's stimulus.
pub(crate) struct Ask {
    /// Its turn's key in her conversation (its event, or its time and author).
    pub key: String,
    pub role: Role,
    pub line: String,
}

/// What her newest act's results are.
pub(crate) enum Results {
    /// The engine's tool results, one per call, and the label her result carries.
    Native { results: Vec<ToolResult>, label: String },
    /// A legacy receipt with no native ids to echo: its rendered text, whole.
    Receipt(String),
}

/// Her newest act as her tail carries it.
pub(crate) struct Acted {
    pub seq: u64,
    /// Her act as her actions render it (`actions::Actions::newest`): its element, cause, identity
    /// and outcome hash.
    pub newest: Option<Item<ContextMessage>>,
    /// Her words in her call, as she wrote them.
    pub words: Option<String>,
    pub calls: Vec<ToolCall>,
    pub results: Results,
    /// The run of identical requests with identical outcomes her act ends (`RepeatRun`).
    pub run: Option<String>,
    /// Her result's pixels and visual notes, attached when it renders, while it is newest.
    pub attachments: Vec<ContentPart>,
}

/// The parts of this act's tail, as the prompt assembly read them.
#[derive(Default)]
pub(crate) struct Parts {
    pub framing: Option<String>,
    pub ask: Option<Ask>,
    /// An empty room's world state: the ask when nothing else is (one message is the least a
    /// request carries).
    pub world: Option<String>,
    pub statement: Option<String>,
    pub acted: Option<Acted>,
}

/// What a one-call act's repeat block keeps of its result: the call it answers and the run it ends.
struct Answer {
    seq: u64,
    tool_use_id: String,
    is_error: Option<bool>,
    run: Option<String>,
    label: String,
}

struct Entry {
    part: Part,
    item: Item<ContextMessage>,
}

#[derive(Default)]
pub(crate) struct Tail {
    entries: Vec<Entry>,
    answer: Option<Answer>,
    attachments: Vec<ContentPart>,
    /// The tail as her view last judged it, placed in the prompt's slots
    /// (`PromptMessages::place_tail`): what the send carries and what joins her view once served.
    pub judged: Vec<Judged<ContextMessage>>,
}

impl Tail {
    /// This act's tail from its parts, in the order the prompt sends them: with an act, the ask, her
    /// statement, her call and its result (her result nearest generation); without one, her
    /// statement and then the ask (the ask last, where she answers it: the 2026-07-20 parrot fix).
    /// The framing leads both.
    pub(crate) fn of(persona: uuid::Uuid, parts: Parts) -> Self {
        let Parts { framing, ask, world, statement, acted } = parts;
        let mut tail = Self::default();
        if let Some(text) = framing {
            tail.push(Part::Framing, said(persona, "framing", ContextMessage::user(text)));
        }
        let asked = match ask {
            Some(Ask { key, role, line }) => Some(Item::whole(
                element(persona, Producer::Turn, key.as_bytes()),
                Cause::Root(Origin::External),
                format!("turn/{key}/stimulus"),
                ContextMessage::new(role, line),
            )),
            None => world.map(|text| said(persona, "world", ContextMessage::user(text))),
        };
        let stated = statement.map(|text| said(persona, "statement", ContextMessage::new(Role::Assistant, text)));
        match acted {
            Some(acted) => {
                tail.push_some(Part::Stimulus, asked);
                tail.push_some(Part::Statement, stated);
                tail.act(persona, acted);
            }
            None => {
                tail.push_some(Part::Statement, stated);
                tail.push_some(Part::Stimulus, asked);
            }
        }
        tail
    }

    /// The room's working-context block as her tail's item, judged after the fit that sizes it:
    /// it sits ahead of the framing, with the grounding a refusal's refit keeps frozen.
    pub(crate) fn context(persona: uuid::Uuid, block: String) -> Self {
        let mut tail = Self::default();
        tail.push(Part::Context, said(persona, "context", ContextMessage::user(block)));
        tail
    }

    fn push(&mut self, part: Part, item: Item<ContextMessage>) {
        self.entries.push(Entry { part, item });
    }

    fn push_some(&mut self, part: Part, item: Option<Item<ContextMessage>>) {
        if let Some(item) = item {
            self.push(part, item);
        }
    }

    /// Her newest act's call and result. The result is her act's element, following her call, with
    /// her act's identity and outcome hash: a one-call act is judged as every arriving act is (a
    /// batch's identity is the batch's, so only a one-call act carries it).
    fn act(&mut self, persona: uuid::Uuid, acted: Acted) {
        let Acted { seq, newest, words, calls, results, run, attachments } = acted;
        let act = element(persona, Producer::Act, &seq.to_be_bytes());
        let (cause, identity, content) = match &newest {
            Some(item) => (item.cause(), item.act(), item.content()),
            None => (Cause::Root(Origin::ActivityStart), None, Content::Absent),
        };
        let call = (!calls.is_empty()).then(|| {
            let key = format!("act/{seq}/call");
            let mut parts: Vec<NativePart> = words.map(NativePart::Text).into_iter().collect();
            parts.extend(calls.iter().map(|call| NativePart::ToolUse { id: call.id.clone(), name: call.name.clone(), input: ToolInput(call.input.clone()) }));
            Item::whole(element(persona, Producer::Tail, key.as_bytes()), cause, key, ContextMessage::native(Role::Assistant, parts))
        });
        let follows = call.as_ref().map_or(cause, |call| Cause::Of(call.id())); // map_or: a legacy receipt has no call; it follows her act's cause
        let (message, answer) = match results {
            Results::Receipt(text) => (ContextMessage::user(text), None),
            Results::Native { mut results, label } => {
                let answer = match results.as_mut_slice() {
                    [only] => {
                        // The run it ends, whatever its effect (a shell command too): the count moves
                        // every repeat, so her input never equals her last.
                        if let Some(run) = &run {
                            only.content = format!("{run}\n{}", only.content);
                        }
                        Some(Answer { seq, tool_use_id: only.tool_use_id.clone(), is_error: only.is_error, run, label: label.clone() })
                    }
                    _ => None,
                };
                let mut parts: Vec<NativePart> = results
                    .into_iter()
                    .map(|result| NativePart::ToolResult { tool_use_id: result.tool_use_id, content: result.content, is_error: result.is_error })
                    .collect();
                parts.push(NativePart::Text(label));
                (ContextMessage::native(Role::User, parts), answer)
            }
        };
        let result = Item::rendered(act, follows, format!("act/{seq}/result"), message, content);
        let result = match identity.filter(|_| answer.is_some()) {
            Some(identity) => result.with_act(identity),
            None => result,
        };
        self.push_some(Part::Call, call);
        self.push(Part::Result, result);
        self.answer = answer;
        self.attachments = attachments;
    }

    /// Which part of the prompt the item showing `element` is.
    pub(crate) fn part(&self, element: continuum_context::cause::ElementId) -> Option<Part> {
        self.entries.iter().find(|entry| entry.item.id() == element).map(|entry| entry.part)
    }

    /// The element her newest result shows (her act's), when this tail carries one.
    pub(crate) fn result(&self) -> Option<continuum_context::cause::ElementId> {
        self.entries.iter().find(|entry| entry.part == Part::Result).map(|entry| entry.item.id())
    }

    /// Her newest result's call and result, whole and as the engine is sent them, for her result's
    /// page to be cut from (`newest_result.rs`).
    pub(crate) fn result_group(&self) -> Vec<ChatMessage> {
        self.entries.iter().filter(|entry| matches!(entry.part, Part::Call | Part::Result)).map(|entry| entry.item.full().to_chat()).collect()
    }

    /// Her newest result shown at its first page, as the act's allocation cut it: carried as its
    /// item's summary, which her view shows at `Level::Page`.
    pub(crate) fn page_result(&mut self, page: ContextMessage) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.part == Part::Result) {
            entry.item = entry.item.clone().with_summary(Some(page));
        }
    }

    /// Her newest result's `message` as it renders: its pixels and visual notes attached after its
    /// parts while it is newest. Her view holds it without them, so the next
    /// act's prompt shows it as text.
    pub(crate) fn attach(&self, mut message: ChatMessage) -> ChatMessage {
        if self.attachments.is_empty() {
            return message;
        }
        let mut parts = match std::mem::replace(&mut message.content, MessageContent::Parts(Vec::new())) {
            MessageContent::Text(text) => vec![ContentPart::Text { text }],
            MessageContent::Parts(parts) => parts,
        };
        parts.extend(self.attachments.iter().cloned());
        message.content = MessageContent::Parts(parts);
        message
    }
}

/// A line of her tail named by what it says: an identical line is the same element.
fn said(persona: uuid::Uuid, kind: &str, message: ContextMessage) -> Item<ContextMessage> {
    let digest = sha2::Sha256::digest(message.text.as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    let key = format!("{kind}/{hex}");
    Item::whole(element(persona, Producer::Tail, key.as_bytes()), Cause::Root(Origin::External), key, message)
}

impl Source<ContextMessage> for Tail {
    fn name(&self) -> &str {
        NAME
    }

    fn priority(&self) -> u8 {
        priority::TAIL
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        self.entries.iter().map(|entry| entry.item.clone()).collect()
    }

    /// Never folded: it is never a start source, so no start folds it.
    fn fold(&self, _folded: &[Item<ContextMessage>]) -> Option<ContextMessage> {
        None
    }

    /// Her newest result when her view finds it repeats an earlier act whose whole she holds: her
    /// actions' repeat line in the tool result it answers, after the run it ends, and its label, so
    /// the call and result stay one native round (MIND-CONTEXT invariant 4).
    fn repeat(&self, item: &Item<ContextMessage>, ancestor: &Item<ContextMessage>) -> Option<ContextMessage> {
        let answer = self.answer.as_ref().filter(|_| self.part(item.id()) == Some(Part::Result))?;
        let line = super::actions::repeat_line(&answer.seq.to_string(), item, ancestor)?;
        let content = match &answer.run {
            Some(run) => format!("{run}\n{line}"),
            None => line,
        };
        Some(ContextMessage::native(
            Role::User,
            vec![
                NativePart::ToolResult { tool_use_id: answer.tool_use_id.clone(), content, is_error: answer.is_error },
                NativePart::Text(answer.label.clone()),
            ],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_context::cause::{Act, ContentHash, Fingerprint, ToolEffect};

    fn reopen(seq: u64, persona: uuid::Uuid) -> Acted {
        let output = "the saved output, whole";
        let newest = Item::rendered(
            element(persona, Producer::Act, &seq.to_be_bytes()),
            Cause::Root(Origin::ActivityStart),
            seq.to_string(),
            ContextMessage::user(format!("[result #{seq}] {output}")),
            Content::Hashed(ContentHash::of(output)),
        )
        .with_act(Act { fingerprint: Fingerprint::of([("tool/output", br#"{"outputId":"255452cef96d2177"}"#.as_slice())]), effect: ToolEffect::Reopens });
        Acted {
            seq,
            newest: Some(newest),
            words: Some("Reading it again.".to_owned()),
            calls: vec![ToolCall { id: format!("call-{seq}"), name: "tool/output".to_owned(), input: serde_json::json!({"outputId": "255452cef96d2177"}) }],
            results: Results::Native {
                results: vec![ToolResult { tool_use_id: format!("call-{seq}"), content: output.to_owned(), is_error: None, custody: Default::default() }],
                label: format!("[result #{seq}]"),
            },
            run: None,
            attachments: Vec::new(),
        }
    }

    // what this catches: the engine bytes of her newest round (Codex, from `request_body.rs`): her
    // words and calls one assistant turn, then her results with the label, the ask before them and
    // the framing first, each in the exact native protocol the prompt assembly built by hand before.
    #[test]
    fn her_tail_renders_her_round_in_the_order_and_protocol_the_engine_is_sent() {
        let persona = uuid::Uuid::new_v4();
        let ask = Ask { key: "event-1".to_owned(), role: Role::User, line: "Operator: start with f1.rs".to_owned() };
        let tail = Tail::of(persona, Parts { framing: Some("[now 12:00Z]".to_owned()), ask: Some(ask), acted: Some(reopen(2, persona)), ..Default::default() });
        let parts: Vec<Option<Part>> = tail.items().iter().map(|item| tail.part(item.id())).collect();
        assert_eq!(parts, [Some(Part::Framing), Some(Part::Stimulus), Some(Part::Call), Some(Part::Result)]);
        let sent: Vec<ChatMessage> = tail.items().iter().map(|item| item.full().to_chat()).collect();
        assert_eq!((sent[0].role.as_str(), sent[1].role.as_str(), sent[2].role.as_str(), sent[3].role.as_str()), ("user", "user", "assistant", "user"));
        match (&sent[2].content, &sent[3].content) {
            (MessageContent::Parts(call), MessageContent::Parts(result)) => {
                assert!(matches!(&call[0], ContentPart::Text { text } if text == "Reading it again."), "her words lead her turn: {call:?}");
                assert!(matches!(&call[1], ContentPart::ToolUse { id, .. } if id == "call-2"), "then her call: {call:?}");
                assert!(matches!(&result[0], ContentPart::ToolResult { tool_use_id, .. } if tool_use_id == "call-2"), "her result answers it: {result:?}");
                assert!(matches!(&result[1], ContentPart::Text { text } if text == "[result #2]"), "then its label: {result:?}");
            }
            other => panic!("her round is native protocol: {other:?}"),
        }
        let quiet = Tail::of(persona, Parts { statement: Some("Looking at it.".to_owned()), world: Some("the room is quiet".to_owned()), ..Default::default() });
        let parts: Vec<Option<Part>> = quiet.items().iter().map(|item| quiet.part(item.id())).collect();
        assert_eq!(parts, [Some(Part::Statement), Some(Part::Stimulus)], "with no act, the ask is last");
    }

    // what this catches (IntelMac, 2026-11-11: one saved output reopened 20+ times, each arriving
    // byte-identical): her repeat block losing its native round. It stays the tool result answering
    // her call, with its label, so the engine still sees one round.
    #[test]
    fn a_repeat_of_her_newest_result_stays_the_tool_result_answering_her_call() {
        let persona = uuid::Uuid::new_v4();
        let tail = Tail::of(persona, Parts { acted: Some(reopen(2, persona)), ..Default::default() });
        let Some(result) = tail.items().into_iter().find(|item| tail.part(item.id()) == Some(Part::Result)) else {
            panic!("test: her result is an item");
        };
        let earlier = Item::whole(element(persona, Producer::Act, &1u64.to_be_bytes()), Cause::Root(Origin::ActivityStart), "1", ContextMessage::user("[result #1]".to_owned()));
        let Some(block) = tail.repeat(&result, &earlier) else { panic!("test: a one-call act words its repeat") };
        match block.to_chat().content {
            MessageContent::Parts(parts) => {
                assert!(matches!(&parts[0], ContentPart::ToolResult { tool_use_id, content, .. } if tool_use_id == "call-2" && content.contains("repeat of #1")), "{parts:?}");
                assert!(matches!(&parts[1], ContentPart::Text { text } if text == "[result #2]"), "{parts:?}");
            }
            MessageContent::Text(text) => panic!("a repeat is still her tool result: {text}"),
        }
    }
}
