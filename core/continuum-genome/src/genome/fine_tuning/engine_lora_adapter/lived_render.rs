//! A lived capture rendered for training at the trainer's grant, through the context codec
//! (card 1c071615).
//!
//! A retained capture is one served call: her history, the turn she answered, and her reply. It
//! was sized by the window she was served at THEN; the node that trains it may have far less room
//! now (Kimi's 27B on the Windows node, 2026-11-08: a 32,995-token prompt against a grant near
//! 4,864, so all 16 examples parked). Training never truncates it and never parks it waiting for
//! room: it renders the call the way serving would render it at this grant, through
//! `continuum_context::view` (Joel: "measured and adjusted/collapsed prompts using solid arch",
//! "must be dynamic always"). The oldest exchanges collapse first, to their summaries then their
//! pointers; her newest turn and her reply, which the lesson is about, stay whole; a tool call
//! and the results that answer it are one exchange, so no level ever separates them. A grant
//! below what cannot collapse is [`NotRendered::Unfit`], with the numbers, never a smaller lie.
//!
//! The price is the capture's own: the engine counted its prompt (`usage.input_tokens`), so its
//! tokens per character price every block of it. That is an ESTIMATE for any one block (code,
//! prose and JSON tokenize differently), so a render is a candidate, never a fit: the trainer's
//! measure of the rendered example decides what trains. A capture without that count has no price
//! and is [`NotRendered::Unmeasured`], never priced at a made-up one.

use continuum_context::cause::{Cause, ElementId};
use continuum_context::cost::Estimate;
use continuum_context::epoch::Block;
use continuum_context::levels::{summarize, Shape};
use continuum_context::source::{Item, Source};
use continuum_context::view::View;

use crate::ai::types::{ChatMessage, ContentPart, MessageContent};
use crate::genome::fine_tuning::types::{LivedCall, TrainingExample};

/// One step of her history as the view holds it: a message, with the tool results that answer it
/// when it made tool calls. Its text is its messages' JSON: every id, input and result counted.
#[derive(Clone, Debug)]
pub(super) struct Exchange {
    messages: Vec<ChatMessage>,
    text: String,
}

impl Exchange {
    fn of(messages: Vec<ChatMessage>) -> Result<Self, NotRendered> {
        // Priced as the trainer's wire carries it: `/train` receives these messages as JSON.
        let text = serde_json::to_string(&messages).map_err(|e| NotRendered::Unserializable(e.to_string()))?; // /train HTTP wire form
        Ok(Self { messages, text })
    }

    /// This exchange with every text and result shown through `shrink`, its tool calls and the
    /// ids that pair them kept, so a collapsed result still answers the call that made it.
    fn shown(&self, shrink: &dyn Fn(&str) -> String) -> Result<Self, NotRendered> {
        let messages = self
            .messages
            .iter()
            .map(|message| ChatMessage {
                role: message.role.clone(),
                name: message.name.clone(),
                content: match &message.content {
                    MessageContent::Text(text) => MessageContent::Text(shrink(text)),
                    MessageContent::Parts(parts) => MessageContent::Parts(
                        parts
                            .iter()
                            .map(|part| match part {
                                ContentPart::Text { text } => ContentPart::Text { text: shrink(text) },
                                ContentPart::ToolResult { tool_use_id, content, is_error } => ContentPart::ToolResult {
                                    tool_use_id: tool_use_id.clone(),
                                    content: shrink(content),
                                    is_error: *is_error,
                                },
                                other => other.clone(),
                            })
                            .collect(),
                    ),
                },
            })
            .collect();
        Self::of(messages)
    }
}

impl PartialEq for Exchange {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl std::hash::Hash for Exchange {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

impl Block for Exchange {
    fn text(&self) -> &str {
        &self.text
    }
}

/// The capture's own price: the engine's exact token count of its prompt over the characters it
/// was sent as, applied to every block (rounded up, so it errs high).
#[derive(Clone, Copy, Debug)]
pub(super) struct CaptureDensity {
    tokens_per_char: f64,
}

impl CaptureDensity {
    fn tokens(&self, chars: usize) -> u32 {
        (chars as f64 * self.tokens_per_char).ceil() as u32
    }
}

impl Estimate<Exchange> for CaptureDensity {
    fn block(&self, block: &Exchange) -> usize {
        self.tokens(block.chars()) as usize
    }

    fn stored(&self, _resident: &Exchange, chars: usize) -> usize {
        self.tokens(chars) as usize
    }
}

/// Her history before the turn she answered, oldest first, one item per exchange.
struct History {
    items: Vec<Item<Exchange>>,
}

impl Source<Exchange> for History {
    fn name(&self) -> &str {
        "lived-history"
    }

    fn priority(&self) -> u8 {
        1
    }

    fn items(&self) -> Vec<Item<Exchange>> {
        self.items.clone()
    }
}

/// The call rendered at a grant: the messages to train on (her history at the levels the grant
/// allows, then the turn she answered, whole), and what the whole example costs by the capture's
/// price, reply included.
#[derive(Clone, Debug)]
pub(super) struct Rendered {
    pub messages: Vec<ChatMessage>,
    pub cost: u32,
    /// Exchanges shown below whole (summary or pointer), of all the exchanges in her history.
    pub collapsed: usize,
    pub exchanges: usize,
}

/// Why a capture was not rendered at a grant; each says what would let it render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NotRendered {
    /// The grant is too small even for what cannot collapse: the system prompt, her tools, the
    /// turn she answered, her reply, and every older exchange at its pointer.
    Unfit { needed: u32, grant: u32 },
    /// The capture carries no engine count of its prompt (`usage.input_tokens` is 0), so there is
    /// no price to render it by; the trainer's measure of it whole is all there is.
    Unmeasured,
    /// Its messages did not serialize to the trainer's wire form.
    Unserializable(String),
    /// The context codec refused the history at this grant.
    Codec(String),
}

/// Where the turn she answered begins: after her last message, so the trailing user and tool
/// turns she was answering stay whole; and AT her last message when only the results of its tool
/// calls follow it (or nothing does), so her newest tool exchange is never split or collapsed.
fn newest_start(messages: &[ChatMessage]) -> usize {
    let Some(last) = messages.iter().rposition(|m| m.role == "assistant") else {
        return 0; // no turn of hers yet: the whole history is what she answers
    };
    if messages[last + 1..].iter().all(answers_tools) {
        last
    } else {
        last + 1
    }
}

/// Whether `message` only carries tool results (it answers the exchange before it).
fn answers_tools(message: &ChatMessage) -> bool {
    message.role == "tool"
        || matches!(&message.content, MessageContent::Parts(parts) if !parts.is_empty() && parts.iter().all(|p| matches!(p, ContentPart::ToolResult { .. })))
}

/// `messages` as exchanges: each message with the tool results that follow and answer it.
fn exchanges(messages: &[ChatMessage]) -> Result<Vec<Exchange>, NotRendered> {
    let mut grouped: Vec<Vec<ChatMessage>> = Vec::new();
    for message in messages {
        match grouped.last_mut() {
            Some(open) if answers_tools(message) => open.push(message.clone()),
            _ => grouped.push(vec![message.clone()]),
        }
    }
    grouped.into_iter().map(Exchange::of).collect()
}

fn item(capture: &str, index: usize, exchange: Exchange) -> Result<Item<Exchange>, NotRendered> {
    let id = ElementId::of(&[capture.as_bytes(), &index.to_le_bytes()]);
    let summary = exchange.shown(&|text: &str| summarize(text, Shape::Output).unwrap_or_else(|| text.to_string()))?; // unwrap_or_else: a text with no smaller output level stays as it is
    let pointer = exchange.shown(&|text: &str| format!("[{} characters, collapsed to fit this node's training grant]", count(text)))?;
    let summary = (summary.chars() < exchange.chars()).then_some(summary);
    let pointer = (pointer.chars() < summary.as_ref().unwrap_or(&exchange).chars()).then_some(pointer); // unwrap_or: with no summary, the pointer must beat the whole exchange
    // A retained capture carries no causes: it was recorded before the view kept them.
    Ok(Item::whole(id, Cause::Unrecorded, format!("exchange/{index}"), exchange).with_summary(summary).with_pointer(pointer))
}

/// Characters in `text` (`Block::chars` is in scope here and answers for a `str` too).
fn count(text: &str) -> usize {
    str::chars(text).count()
}

/// Render `call` for training within `grant` tokens, through the context codec.
pub(super) fn render_at(call: &LivedCall, grant: u32) -> Result<Rendered, NotRendered> {
    let request = &call.request;
    let counted = call.response.usage.input_tokens;
    if counted == 0 {
        return Err(NotRendered::Unmeasured);
    }
    let system = request.system_prompt.as_deref().unwrap_or_default(); // unwrap_or_default: a call served with no system prompt has none to price
    let tools = match &request.tools {
        Some(tools) => serde_json::to_string(tools).map_err(|e| NotRendered::Unserializable(e.to_string()))?, // /train HTTP wire form of her tools
        None => String::new(),
    };
    let sent_chars = count(system) + count(&tools) + Exchange::of(request.messages.clone())?.chars();
    // never zero: the messages' wire form is at least "[]"
    let density = CaptureDensity { tokens_per_char: f64::from(counted) / sent_chars as f64 };
    let split = newest_start(&request.messages);
    let (older, newest) = request.messages.split_at(split);
    let reply = call.response.usage.output_tokens.max(density.tokens(count(&call.response.text)));
    let irreducible = density
        .tokens(count(system) + count(&tools))
        .saturating_add(density.tokens(Exchange::of(newest.to_vec())?.chars()))
        .saturating_add(reply);
    let items = exchanges(older)?.into_iter().enumerate().map(|(i, e)| item(&call.capture, i, e)).collect::<Result<Vec<_>, _>>()?;
    let history = History { items };
    let exchanges = history.items.len();
    let claims = continuum_context::view::claims(&density, &[&history]);
    let floor = claims.first().map_or(0, |claim| claim.floor) as u32; // map_or: no history, nothing to floor
    let needed = irreducible.saturating_add(floor);
    if grant < needed {
        return Err(NotRendered::Unfit { needed, grant });
    }
    let left = (grant - irreducible) as usize;
    let (view, _start) = View::start(&density, Vec::new(), &[&history], |_| vec![left]).map_err(|e| NotRendered::Codec(format!("{e:?}")))?;
    let shown: Vec<&Exchange> = view.render().collect();
    let collapsed = history.items.iter().filter(|item| !shown.iter().any(|block| *block == item.full())).count();
    let history_cost: u32 = shown.iter().map(|block| density.tokens(block.chars())).sum();
    let mut messages: Vec<ChatMessage> = shown.into_iter().flat_map(|block| block.messages.clone()).collect();
    messages.extend_from_slice(newest);
    Ok(Rendered { messages, cost: irreducible.saturating_add(history_cost), collapsed, exchanges })
}

/// The smallest window `example` trains in: a lived call's history at its pointers beside what
/// cannot collapse. `None` when it cannot be rendered (not a lived call, or one without a price):
/// it trains whole or not at all, so its measured length is its floor.
pub(super) fn smallest_window(example: &TrainingExample) -> Option<u32> {
    match render_at(example.lived.as_ref()?, 0) {
        Ok(rendered) => Some(rendered.cost),
        Err(NotRendered::Unfit { needed, .. }) => Some(needed),
        Err(_) => None,
    }
}

/// `examples` fitted to `window`: every lived call rendered at it (one that fits stays exactly as
/// served); one that cannot be rendered there goes whole, for the trainer's measure to decide.
pub(super) struct Fitted {
    pub examples: Vec<TrainingExample>,
    /// Lived calls whose history collapsed to fit.
    pub collapsed: usize,
    /// Lived calls not rendered at this window, and why.
    pub not_rendered: Vec<NotRendered>,
}

pub(super) fn fit_to_window(examples: &[TrainingExample], window: u32) -> Fitted {
    let mut fitted = Fitted { examples: Vec::with_capacity(examples.len()), collapsed: 0, not_rendered: Vec::new() };
    for example in examples {
        let Some(call) = example.lived.as_ref() else {
            fitted.examples.push(example.clone());
            continue;
        };
        match render_at(call, window) {
            Ok(rendered) if rendered.collapsed > 0 => {
                fitted.collapsed += 1;
                let mut shown = example.clone();
                if let Some(lived) = shown.lived.as_mut() {
                    lived.request.messages = rendered.messages;
                }
                fitted.examples.push(shown);
            }
            Ok(_) => fitted.examples.push(example.clone()),
            Err(why) => {
                fitted.not_rendered.push(why);
                fitted.examples.push(example.clone());
            }
        }
    }
    fitted
}

#[cfg(test)]
mod tests;
