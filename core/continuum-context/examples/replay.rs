//! Replay a resident's captured prompts through the view and measure what each request
//! shares with the one before it: the design gate of MIND-CONTEXT-AND-ENGINE-CONFIG.md
//! ("offline replay at >= 90% prefix").
//!
//! `cargo run -p continuum-context --example replay -- [--room <id prefix>] [--purpose <suffix>] [--tools <schemas.json>] <prompt-capture.jsonl>...`
//!
//! Each captured request is cut the way the prompt assembly builds it: the system prompt and
//! then the tool schemas, where the chat template renders them, are the head (a head that
//! changes between acts restarts the view: nothing after it is reusable); room turns (`[occurred <when>] <who>:`) are her conversation, keyed by when and
//! who; results, pointers, the steps ledger and the action lines of working memory are ONE
//! action log keyed by `#n`; each `[faculty]` block and each list-shaped block (open views,
//! notices, recall, thoughts) is a source of `- ` items; whatever is left (facts, clock, the
//! ask, the live act) is this act's fresh tail. Everything is measured in characters: as
//! sent, and as the view would have sent it.
//!
//! The view is fitted to the window each row recorded, less the reply's runaway (the largest
//! `maxTokens` sent), at the chars per token the engine billed (`completed` rows), and turns
//! over the way the core's `fit_view` does, under two rules: `whole` (before 2026-10-23: a
//! source granted all it wants restarts whole, a cut one at half its grant) and `reserved`
//! (`start_grants`: the grounding blocks whole; the conversation, results and thoughts at
//! most half of what is left, and at most that less one act's appends). Each rule runs under
//! two source orders (`Order`): her conversation first (before 2026-10-24), and the settled
//! blocks first, as the core's `view_sources` now places them.
//!
//! `--tools <schemas.json>` sends every act this one tool surface instead of the one it
//! captured: the stable surface of goal 10, where the schemas are the same bytes on every act.
//! Each run prints the tool share of the prompt (schema chars over request chars).
//!
//! `--append-tail <prefill tok/s>` replays the core's rule with her act's loop appended in the
//! bytes it was sent instead of re-rendered (`replay/append.rs`), and prints each request's
//! fresh tokens now and append-tail, priced at the measured prefill rate.
//!
//! `--outline-results` (with `--append-tail`) also replays an oversized result entering her view
//! at its outline, whole on its own act and outlined from the next (`replay/outline.rs`), and
//! the strict variant outlined even on its own act. `--generational` (with `--append-tail`) also
//! replays a sediment of handles frozen after the head, so a turnover re-prefills only the young
//! generation (`replay/sediment.rs`, card f746143e). `--paged-reads` (with `--append-tail`) also
//! replays a read with no range answering with its first page and the line naming the rest,
//! charging each later page her acts show she needed as an extra request (`replay/paged.rs`).
//! Each run prints one `TABLE` line.
//!
//! `--chars-per-token <r>` prices the window at the core's own estimate instead of the
//! engine's bill; `--keyed-turn` keys the standing turn text into the epoch; `--trace`
//! prints each act's view against what it may hold.
//!
//! `--acts` is the act inspector (`replay/acts/`): one row per request each resident sent, joined
//! to the engine log and her view's probes, with her summary; see `replay/acts/mod.rs`.

use std::collections::BTreeMap;

use continuum_context::source::{Item, Source};

#[path = "replay/acts/mod.rs"]
mod acts;
#[path = "replay/captures.rs"]
mod captures;
#[path = "replay/validate.rs"]
mod validate;
#[path = "replay/append.rs"]
mod append;
#[path = "replay/outline.rs"]
mod outline;
#[path = "replay/paged.rs"]
mod paged;
#[path = "replay/sediment.rs"]
mod sediment;
use continuum_context::view::View;

struct Captured {
    name: String,
    items: Vec<Item<String>>,
}

/// What the allocator weighs each source by, as the core's `context_streams::priority` does:
/// her conversation, results and thoughts at 5, every standing block at 6.
fn priority(source: &str) -> u8 {
    match source {
        "conversation" | "actions" | "working-memory" | "turn" => 5,
        _ => 6,
    }
}

/// Where the view's sources render at an epoch start.
#[derive(Clone, Copy, Debug)]
enum Order {
    /// Before 2026-10-24: her conversation, her actions, each block, her thoughts.
    ConversationFirst,
    /// The core's `view_sources`: the settled blocks (standing and board churn, as
    /// `deliberation_prompt::churn_of` classes them), her actions, her conversation, the
    /// blocks that count or change within an epoch (the kanban, recall), her thoughts. A turnover keeps the prefix up to the
    /// first source whose front changed, and a growing source changes at its front.
    SettledFirst,
}

fn order(rule: Order, source: &str) -> u8 {
    match rule {
        Order::ConversationFirst => match source {
            "conversation" => 0,
            "actions" => 1,
            "working-memory" => 3,
            _ => 2,
        },
        Order::SettledFirst => match source {
            "room-doctrine" | "room-roster" | "media-perception" | "mission" | "workspace-map" => 0,
            "roster" | "room-wall" | "active-work" => 1,
            "actions" => 3,
            "conversation" => 4,
            "working-memory" => 6,
            _ => 5,
        },
    }
}

impl Source<String> for Captured {
    fn name(&self) -> &str {
        &self.name
    }
    fn priority(&self) -> u8 {
        priority(&self.name)
    }
    fn items(&self) -> Vec<Item<String>> {
        self.items.clone()
    }
}

/// The captures hold what was sent, not each item's pointer; a pointer is modelled as the
/// item's first line cut to the length of a real result pointer (~185 chars).
fn item(key: String, text: String) -> Item<String> {
    let pointer: String = text.lines().next().unwrap_or_default().chars().take(185).collect();
    let pointer = if pointer.len() < text.len() { pointer } else { text.clone() };
    captured(key, text).with_pointer(pointer)
}

/// A captured element: a capture records what was sent, never what caused it, so its cause is
/// `Unrecorded` (counted, never toward closure) and its id comes from its key.
fn captured(key: String, full: String) -> Item<String> {
    let id = continuum_context::cause::ElementId::of(&[key.as_bytes()]);
    Item::whole(id, continuum_context::cause::Cause::Unrecorded, key, full)
}

/// The one allocation, modelled: every source gets its floor, then the rest of `budget`
/// goes to sources by priority level, highest first, sources of one level sharing in
/// proportion to what each wants. (In the core this is `flex_allocate`.)
fn allocate(claims: &[continuum_context::view::Claim], budget: usize) -> Vec<usize> {
    let mut grants: Vec<usize> = claims.iter().map(|claim| claim.floor_chars).collect();
    let mut spare = budget.saturating_sub(grants.iter().sum());
    let mut levels: Vec<u8> = claims.iter().map(|claim| claim.priority).collect();
    levels.sort_unstable_by(|a, b| b.cmp(a));
    levels.dedup();
    for level in levels {
        let wants: Vec<(usize, usize)> = (0..claims.len())
            .filter(|index| claims[*index].priority == level)
            .map(|index| (index, claims[index].full_chars.saturating_sub(grants[index])))
            .collect();
        let wanted: usize = wants.iter().map(|(_, want)| want).sum();
        let given = wanted.min(spare);
        for (index, want) in wants {
            grants[index] += (want as u128 * given as u128 / wanted.max(1) as u128) as usize;
        }
        spare -= given;
    }
    grants
}

/// What the grounding blocks (every source above the lowest priority) are granted of
/// `available`, and the room the growing sources share once they have it.
fn growable(claims: &[continuum_context::view::Claim], available: usize) -> (usize, usize) {
    let granted = allocate(claims, available);
    let lowest = claims.iter().map(|claim| claim.priority).min().unwrap_or(0); // unwrap_or: no claims means nothing to grant
    let grounding: usize = claims.iter().zip(&granted).filter(|(claim, _)| claim.priority > lowest).map(|(_, grant)| grant).sum();
    (grounding, available.saturating_sub(grounding))
}

#[derive(Clone, Copy, Debug)]
enum Rule {
    /// Before 2026-10-23: a source granted all it wants restarts whole, a cut one at half.
    Whole,
    /// `start_grants`: grounding whole; the growing sources at most half of what is left, less
    /// one act's appends.
    Reserved,
}

/// The grants a turnover starts the next epoch at, `available` being what the window leaves
/// the view this act.
fn start(rule: Rule, claims: &[continuum_context::view::Claim], available: usize, per_act: Option<usize>, share: usize) -> Vec<usize> {
    match rule {
        Rule::Whole => {
            let granted = allocate(claims, available);
            claims
                .iter()
                .zip(granted)
                .map(|(claim, grant)| if grant >= claim.full_chars { claim.full_chars } else { grant / 2 })
                .collect()
        }
        Rule::Reserved => {
            // The grounding blocks start whole at their grant; the growing sources (the
            // lowest priority) start at half of what is left, less one act's appends.
            let (grounding, growable) = growable(claims, available);
            let growing = (growable / share).min(per_act.map_or(growable, |chars| growable.saturating_sub(chars))); // map_or: no act measured, the half start alone
            allocate(claims, grounding + growing)
        }
    }
}

/// `#n` right after `prefix` at the head of `text`.
fn seq_after(text: &str, prefix: &str) -> Option<u64> {
    let digits = text.strip_prefix(prefix)?;
    let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
    digits[..end].parse().ok()
}

/// A list-shaped block as its header plus one item per `- ` bullet (continuation lines stay
/// with their bullet). The key is the bullet's text up to its first comma: an open view's
/// path, or the whole line for a thought, a notice or a memory.
fn bullets(source: &str, text: &str) -> Vec<Item<String>> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut header = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("- ") {
            if let Some(done) = current.take() {
                out.push(done);
            }
            current = Some(format!("- {rest}"));
        } else if let Some(open) = current.as_mut() {
            open.push('\n');
            open.push_str(line);
        } else {
            header.push_str(line);
            header.push('\n');
        }
    }
    out.extend(current);
    let mut items = vec![item(format!("{source}/header"), header)];
    for bullet in out {
        let key = bullet.split(',').next().unwrap_or(&bullet).to_owned();
        items.push(item(key, bullet));
    }
    items
}

struct Cut {
    head: String,
    sources: BTreeMap<String, Vec<Item<String>>>,
    tail: Vec<String>,
    sent: String,
    /// The pinned result and the reads of the act this request answers (the validator).
    act: validate::Act,
    /// The quotable lines of the act it carries (the outline mode's paging proxy).
    said: Vec<String>,
    /// Its read calls (`--paged-reads`).
    reads: paged::Reads,
}

/// A tail piece that repeats from act to act (the work-turn ask, the own-time framing, the
/// context line, a room message): keyed by its first line so the epoch carries it once.
fn standing(text: &str) -> Option<String> {
    let first = text.trim_start().lines().next()?;
    let standing = first.starts_with("[work turn]")
        || first.starts_with("[Your own time]")
        || first.starts_with("[context]")
        || first.starts_with("[now ");
    standing.then(|| first.chars().take(120).collect())
}

/// A room turn's key: when it occurred and who said it (`[occurred <when>] <who>:`).
fn turn_key(text: &str) -> Option<String> {
    let rest = text.strip_prefix("[occurred ")?;
    let (when, said) = rest.split_once("] ")?;
    let who = said.split_once(':')?.0;
    Some(format!("{when} {who}"))
}

fn cut(request: &serde_json::Value, tools: &str, keyed_turn: bool) -> Cut {
    let head = format!("{}\n{tools}", request["systemPrompt"].as_str().unwrap_or_default());
    let mut sources: BTreeMap<String, Vec<Item<String>>> = BTreeMap::new();
    let mut tail = Vec::new();
    let mut sent = head.clone();
    let mut push = |source: &str, items: Vec<Item<String>>| sources.entry(source.to_owned()).or_default().extend(items);
    for message in request["messages"].as_array().into_iter().flatten() {
        let text = captures::text(message);
        sent.push('\n');
        sent.push_str(&text);
        if let Some(key) = turn_key(&text) {
            push("conversation", vec![item(key, text)]);
        } else if let Some(seq) = seq_after(&text, "[result #") {
            push("actions", vec![item(seq.to_string(), text)]);
        } else if text.starts_with("Recent results of your own actions") || text.starts_with("[actions] ") {
            push("actions", vec![item("banner".into(), text)]);
        } else if text.starts_with("Older results, collapsed to pointers") {
            for line in text.lines().skip(1) {
                if let Some(seq) = seq_after(line, "[result #") {
                    push("actions", vec![item(seq.to_string(), line.to_owned())]);
                }
            }
        } else if text.starts_with("[steps taken this session]") {
            // The ledger is the action log's pointer rendering: one item per act.
            for line in text.lines().skip(1) {
                if let Some(seq) = seq_after(line, "[action #") {
                    push("ledger", vec![item(seq.to_string(), line.to_owned())]);
                }
            }
        } else if text.starts_with("[working-memory]") {
            // Its action lines are the action log again; its thoughts are its own items.
            let thoughts = bullets("working-memory", &text)
                .into_iter()
                .filter(|item| !item.full().starts_with("- [action #"))
                .collect();
            push("working-memory", thoughts);
        } else if text.starts_with("My own recent thoughts") {
            push("working-memory", vec![item("working-memory/header".into(), text)]);
        } else if text.starts_with("- ") {
            // One of her thoughts, a message of its own since the view keyed them.
            push("working-memory", vec![item(text.clone(), text)]);
        } else if text.starts_with("[open views]") {
            push("open-views", bullets("open-views", &text));
        } else if text.starts_with("Notices my substrate posted") {
            push("notices", bullets("notices", &text));
        } else if text.starts_with("[recall]") {
            push("recall", bullets("recall", &text));
        } else if let Some(name) = text.strip_prefix('[').and_then(|rest| rest.split_once("]\n")).map(|(name, _)| name)
            .filter(|name| !name.contains(' '))
        {
            push(name, vec![item("block".into(), text.clone())]);
        } else if let Some(key) = standing(&text).filter(|_| keyed_turn) {
            push("turn", vec![item(key, text)]);
        } else {
            tail.push(text);
        }
    }
    // A ledger line stands in for an act only when the log holds no rendering of it.
    if let Some(ledger) = sources.remove("ledger") {
        let held: Vec<String> = sources.get("actions").into_iter().flatten().map(|item| item.key().to_owned()).collect();
        let missing = ledger.into_iter().filter(|line| !held.iter().any(|key| key == line.key())).collect::<Vec<_>>();
        sources.entry("actions".into()).or_default().extend(missing);
    }
    Cut { head, sources, tail, sent, act: validate::act_of(request), said: outline::said(request), reads: paged::reads(request) }
}

fn shared(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len().max(1) as f64
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

fn percent(values: &[f64]) -> String {
    values.iter().map(|share| format!("{:.0}", 100.0 * share)).collect::<Vec<_>>().join(" ")
}

/// Replay `cuts` (one resident's requests in one room, oldest first) through the view fitted
/// to `window` characters for head, view and tail, turning over under `rule`.
/// What a run measured, for the validator's one-line comparison of policies.
#[derive(Default)]
struct Outcome {
    turnovers: usize,
    prefill_chars: Vec<f64>,
    rereads: usize,
    retained: usize,
    /// The view's prefill on each act that turned over.
    turnover_prefill: Vec<f64>,
}

fn replay(label: &str, cuts: &[Cut], window: usize, (rule, placed): (Rule, Order), keyed_turn: bool, admit: bool, trace: bool, share: usize) -> Result<Outcome, Box<dyn std::error::Error>> {
    let mut restored = validate::Restored::default();
    let (mut rereads, mut retained) = (0usize, 0usize);
    let mut repeats = validate::Repeats::default();
    let mut view: Option<View<String>> = None;
    let mut previous_sent: Option<&str> = None;
    let mut previous_head: Option<&str> = None;
    let mut head_changes = 0usize;
    let mut previous_view = String::new();
    let (mut sent_share, mut view_share, mut view_prefill, mut turnovers) = (Vec::new(), Vec::new(), Vec::new(), 0usize);
    let mut admitted = 0usize;
    let (mut collapsed_at_starts, mut summarized_at_starts) = (0usize, 0usize);
    // What each source appended per act, and the fresh tail: where the prefill goes.
    let mut held: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut appended: BTreeMap<String, usize> = BTreeMap::new();
    let mut tails = Vec::new();
    let mut turnover_prefill = Vec::new();
    for cut in cuts {
        let action_keys: Vec<String> = cut.sources.get("actions").into_iter().flatten().map(|item| item.key().to_owned()).collect();
        restored.learn(&action_keys, &cut.act);
        let counted = restored.rereads(&cut.act, &previous_view);
        rereads += counted.total;
        retained += counted.held;
        let identity = restored.repeats(&cut.act, &previous_view);
        repeats.requests += identity.requests;
        repeats.unchanged += identity.unchanged;
        repeats.rendered += identity.rendered;
        let mut sources: Vec<Captured> = cut
            .sources
            .iter()
            .map(|(name, items)| {
                let mut items = items.clone();
                if name == "actions" {
                    restored.enrich(&mut items);
                }
                Captured { name: name.clone(), items }
            })
            .collect();
        sources.sort_by_key(|source| order(placed, &source.name));
        let refs: Vec<&dyn Source<String>> = sources.iter().map(|s| s as &dyn Source<String>).collect();
        let tail = cut.tail.join("\n");
        let tail_chars = tail.chars().count();
        tails.push(tail_chars as f64);
        for (name, items) in &cut.sources {
            for item in items {
                let key = (name.clone(), item.key().to_owned());
                if held.get(&key) != Some(item.full()) {
                    *appended.entry(name.clone()).or_default() += item.full().chars().count();
                    held.insert(key, item.full().clone());
                }
            }
        }
        // What the window leaves her view's sources this act, and the view whole (head
        // included) may grow to before it turns over: the core's `granted + room`.
        // The head is fixed for an epoch: one that changed re-prefills everything after it.
        if previous_head.is_some_and(|head| head != cut.head) {
            view = None;
            head_changes += 1;
        }
        previous_head = Some(&cut.head);
        let head_chars = cut.head.chars().count();
        let available = window.saturating_sub(head_chars + tail_chars);
        let whole = |claims: &[continuum_context::view::Claim]| claims.iter().map(|claim| claim.full_chars).collect();
        let open = match view.as_mut() {
            None => view.insert(View::start(vec![cut.head.clone()], &refs, whole)?.0),
            Some(open) => {
                open.advance(&refs);
                open
            }
        };
        if trace {
            let whole: usize = continuum_context::view::claims(&refs).iter().map(|c| c.full_chars).sum();
            eprintln!("  act: view {} chars (head included) of {} it may hold; sources whole {whole}, tail {tail_chars}", open.chars(), head_chars + available);
        }
        // Admission first (`View::readmit`, the core's `fit_view`): an arrival too large for
        // what the act grants arrives as its pointer, and only a view still over turns over.
        if admit && open.chars() > head_chars + available && open.readmit(&refs, head_chars + available, (head_chars + available) / 2).is_some() && open.chars() <= head_chars + available {
            admitted += 1;
        }
        let turned = open.chars() > head_chars + available;
        if turned {
            let per_act = open.appended_per_act();
            let begun = open.turn_over(vec![cut.head.clone()], &refs, |claims| start(rule, claims, available, per_act, share))?;
            collapsed_at_starts += begun.collapsed;
            summarized_at_starts += begun.summarized;
            turnovers += 1;
        }
        let rendered = format!("{}\n{tail}", open.render().cloned().collect::<Vec<_>>().join("\n"));
        if let Some(before) = previous_sent {
            sent_share.push(shared(before, &cut.sent) as f64 / cut.sent.chars().count().max(1) as f64);
            let length = rendered.chars().count();
            let kept = shared(&previous_view, &rendered);
            view_prefill.push((length - kept) as f64);
            if turned {
                turnover_prefill.push((length - kept) as f64);
            }
            view_share.push(kept as f64 / length.max(1) as f64);
        }
        previous_sent = Some(&cut.sent);
        previous_view = rendered;
    }
    println!(
        "{label} [{rule:?}, {placed:?}{}]\n  {} requests, window {window} chars, growing share 1/{share}, {turnovers} turnovers ({collapsed_at_starts} items collapsed and {summarized_at_starts} summarized at starts), {admitted} admitted as pointers, {head_changes} head changes\n  shared prefix per act, %: as sent [{}] mean {:.0} median {:.0}\n  shared prefix per act, %: the view [{}] mean {:.0} median {:.0}\n  the view's prefill per act, chars: median {:.0} mean {:.0}; fresh tail median {:.0} chars",
        format!("{}{}", if keyed_turn { ", standing turn text keyed into the epoch" } else { "" }, if admit { ", admission" } else { "" }),
        cuts.len(),
        percent(&sent_share),
        100.0 * mean(&sent_share),
        100.0 * median(sent_share.clone()),
        percent(&view_share),
        100.0 * mean(&view_share),
        100.0 * median(view_share.clone()),
        median(view_prefill.clone()),
        mean(&view_prefill),
        median(tails),
    );
    let acts = cuts.len().max(1);
    let line: Vec<String> = appended.iter().map(|(name, chars)| format!("{name} {}", chars / acts)).collect();
    println!("  appended per act by source: {}", line.join(", "));
    println!("  her re-reads of a file read before: {rereads}, of which the prompt still held the earlier read whole or as its outline (a retention proxy): {retained}; acts whose result was ambiguous (not restored): {}", restored.ambiguous);
    println!(
        "  invariant 4: reads asking an earlier read's exact request again {}, with the outcome unchanged {}, of which the view renders as a repeat (the earlier whole in her prompt) {}",
        repeats.requests, repeats.unchanged, repeats.rendered
    );
    Ok(Outcome { turnovers, prefill_chars: view_prefill, rereads, retained, turnover_prefill })
}

/// Characters a request sends: the system prompt, every message, and `tools`.
fn request_chars(request: &serde_json::Value, tools: &str) -> usize {
    let messages: usize = request["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|message| match &message["content"] {
            serde_json::Value::String(text) => text.chars().count(),
            other => other.to_string().chars().count(),
        })
        .sum();
    request["systemPrompt"].as_str().map_or(0, |text| text.chars().count()) + messages + tools.chars().count()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--acts") {
        return acts::main(args.into_iter().filter(|arg| arg != "--acts").collect());
    }
    let (mut room, mut purpose, mut keyed_turn, mut trace, mut paths) = (String::new(), String::from("deliberation"), false, false, Vec::new());
    // The growing sources' share of what a start leaves: `start_grants` gives them half.
    let mut share = 2usize;
    let mut priced: Option<f64> = None;
    // `--validate <prefill tok/s>`: compare the growing shares on one line each, priced at a
    // measured prefill rate (the node's `state/serving-rates.json`, or a live read).
    let mut validate_tps: Option<f64> = None;
    let mut append_tps: Option<f64> = None;
    let mut reply_override: Option<u64> = None;
    let mut fixed_tools: Option<String> = None;
    let (mut outline_results, mut generational, mut paged_reads) = (false, false, false);
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--room" => room = args.next().ok_or("--room needs an id prefix")?,
            "--purpose" => purpose = args.next().ok_or("--purpose needs a suffix")?,
            "--keyed-turn" => keyed_turn = true,
            "--outline-results" => outline_results = true,
            "--generational" => generational = true,
            "--paged-reads" => paged_reads = true,
            "--trace" => trace = true,
            "--reply" => reply_override = Some(args.next().ok_or("--reply needs a token count")?.parse()?),
            "--validate" => {
                let tps: f64 = args.next().ok_or("--validate needs the prefill rate in tokens/s")?.parse()?;
                if !(tps.is_finite() && tps > 0.0) {
                    return Err("--validate needs a finite, positive measured prefill rate".into());
                }
                validate_tps = Some(tps);
            }
            "--append-tail" => {
                let tps: f64 = args.next().ok_or("--append-tail needs the prefill rate in tokens/s")?.parse()?;
                if !(tps.is_finite() && tps > 0.0) {
                    return Err("--append-tail needs a finite, positive measured prefill rate".into());
                }
                append_tps = Some(tps);
            }
            "--share" => {
                share = args.next().ok_or("--share needs a divisor")?.parse()?;
                if share == 0 {
                    return Err("--share is a divisor of the free room: 1 or more".into());
                }
            }
            "--tools" => fixed_tools = Some(std::fs::read_to_string(args.next().ok_or("--tools needs a schemas file")?)?.trim().to_owned()),
            "--chars-per-token" => priced = Some(args.next().ok_or("--chars-per-token needs a number")?.parse()?),
            _ => paths.push(arg),
        }
    }
    let captured = captures::load(&paths, captures::Window::default())?;
    let billed: BTreeMap<&str, u64> = captured
        .terminal
        .iter()
        .filter(|(_, end)| end["event"] == "completed")
        .filter_map(|(id, end)| Some((id.as_str(), end["response"]["usage"]["inputTokens"].as_u64()?)))
        .collect();
    let mut rows = captured.submitted;
    // Chars per token as the engine billed this resident's own requests.
    let per_token: Vec<f64> = rows
        .iter()
        .filter_map(|row| {
            let tokens = billed.get(row["request_id"].as_str()?)?;
            Some(request_chars(&row["request"], &row["request"]["tools"].to_string()) as f64 / *tokens.max(&1) as f64)
        })
        .collect();
    // The core prices its claims with its own estimator, not the engine's bill: pass its
    // ratio (`delib.view.sized` view_chars / view_tokens) to fit the view the way it does.
    let chars_per_token = match priced {
        Some(ratio) => ratio,
        None => median(per_token.clone()),
    };
    rows.retain(|row| {
        row["room_id"].as_str().is_some_and(|id| id.starts_with(&room))
            && row["request"]["purpose"].as_str().is_some_and(|p| p.ends_with(&purpose))
    });
    rows.sort_by_key(|row| row["captured_at_ms"].as_u64());
    let context_window = rows.iter().filter_map(|row| row["context_window"].as_u64()).max().unwrap_or(0); // unwrap_or: no rows in the room, nothing replays
    let captured_reply = rows.iter().filter_map(|row| row["request"]["maxTokens"].as_u64()).max().unwrap_or(0); // unwrap_or: no rows in the room, nothing replays
    // `--reply <tokens>` replays with another reply reserve: what a reserve sized from her
    // measured output would give her view (the captured one held 12,288 on the M5, 2026-10-25,
    // while acts produced ~250-700).
    let reply = reply_override.unwrap_or(captured_reply);
    // The schemas each act sends: the captured ones, or the one stable surface under `--tools`.
    let sent_tools: Vec<String> =
        rows.iter().map(|row| match &fixed_tools {
            Some(tools) => tools.clone(),
            None => row["request"]["tools"].to_string(),
        }).collect();
    let tool_share: Vec<f64> = rows
        .iter()
        .zip(&sent_tools)
        .map(|(row, tools)| tools.chars().count() as f64 / request_chars(&row["request"], tools).max(1) as f64)
        .collect();
    // The head carries the schemas, so the window is not pre-reduced by them.
    let window = (context_window.saturating_sub(reply) as f64 * chars_per_token).max(0.0) as usize;
    let label = format!(
        "{} room {room} purpose {purpose}: window {context_window} - reply {reply} tokens at {chars_per_token:.2} chars/token ({} billed rows), {}; tool share of the prompt mean {:.0}% median {:.0}%",
        paths.join(" "),
        per_token.len(),
        if fixed_tools.is_some() { "one stable tool surface (--tools)" } else { "tools as captured" },
        100.0 * mean(&tool_share),
        100.0 * median(tool_share.clone()),
    );
    let cuts: Vec<Cut> = rows
        .iter()
        .zip(&sent_tools)
        .map(|(row, tools)| {
            let mut cut = cut(&row["request"], tools, keyed_turn);
            cut.act = std::mem::take(&mut cut.act).at(row);
            cut
        })
        .collect();
    if let Some(tps) = append_tps {
        // The core's rule as it renders today, then the same rule with the act loop appended.
        let today = replay(&label, &cuts, window, (Rule::Reserved, Order::SettledFirst), keyed_turn, true, trace, share)?;
        let resident = paths.iter().map(|path| path.rsplit('/').next().unwrap_or(path).chars().take(8).collect::<String>()).collect::<Vec<_>>().join("+"); // unwrap_or: rsplit always yields one piece
        let rate = (chars_per_token, tps);
        let whole = append::Options { outline: outline::Mode::Whole, generational: false, paged: false, young_acts: None };
        let base = append::replay(&label, &cuts, window, share, rate, whole, true)?;
        let tokens = |chars: &[f64]| chars.iter().sum::<f64>() / chars_per_token;
        let per_turnover = tokens(&today.turnover_prefill) / today.turnover_prefill.len().max(1) as f64;
        println!(
            "TABLE {resident} | today (as sent; turnovers as the core's rule replays them) | prefill {:.0} tok | {:.0} s/request | {} turnovers | re-prefill {per_turnover:.0} tok/turnover (median {:.0})",
            base.now_tokens,
            base.now_tokens / tps / base.requests.max(1) as f64,
            today.turnovers,
            median(today.turnover_prefill.clone()) / chars_per_token,
        );
        println!("{}", append::line(&resident, "append-tail", &base, tps));
        let mut variants = Vec::new();
        if outline_results {
            variants.push(("append-tail + outline from the next act", append::Options { outline: outline::Mode::NextAct, generational: false, paged: false, young_acts: None }));
            variants.push(("append-tail + outline strict (own act too)", append::Options { outline: outline::Mode::Strict, generational: false, paged: false, young_acts: None }));
        }
        if paged_reads {
            variants.push(("append-tail + paged reads", append::Options { outline: outline::Mode::Whole, generational: false, paged: true, young_acts: None }));
            variants.push(("append-tail + paged reads + outline from the next act", append::Options { outline: outline::Mode::NextAct, generational: false, paged: true, young_acts: None }));
        }
        if generational {
            variants.push(("append-tail + generational", append::Options { outline: outline::Mode::Whole, generational: true, paged: false, young_acts: None }));
            for (label, k) in [("append-tail + LSM young 1 act", 1), ("append-tail + LSM young 2 acts", 2), ("append-tail + LSM young 4 acts", 4), ("append-tail + LSM young 8 acts", 8), ("append-tail + fold only when full (young = all the room)", usize::MAX)] {
                variants.push((label, append::Options { outline: outline::Mode::Whole, generational: true, paged: false, young_acts: Some(k) }));
            }
            variants.push(("append-tail + generational + outline from the next act", append::Options { outline: outline::Mode::NextAct, generational: true, paged: false, young_acts: None }));
        }
        for (variant, options) in variants {
            let summary = append::replay(&label, &cuts, window, share, rate, options, false)?;
            println!("{}", append::line(&resident, variant, &summary, tps));
        }
        return Ok(());
    }
    if let Some(tps) = validate_tps {
        // The validator: the core's rule (reserved, settled first, admission) at each growing share,
        // one comparable line each. Prefill seconds at the measured rate `tps`.
        let seconds_per_char = 1.0 / (chars_per_token * tps);
        for share in [2usize, 3, 4, 6] {
            let o = replay(&label, &cuts, window, (Rule::Reserved, Order::SettledFirst), keyed_turn, true, trace, share)?;
            println!(
                "VALIDATE {label} share 1/{share}: {} acts, {} turnovers, prefill {:.1} s/act mean ({:.0} chars), re-reads {} held {}",
                cuts.len(),
                o.turnovers,
                mean(&o.prefill_chars) * seconds_per_char,
                mean(&o.prefill_chars),
                o.rereads,
                o.retained
            );
        }
        return Ok(());
    }
    for rule in [Rule::Whole, Rule::Reserved] {
        for placed in [Order::ConversationFirst, Order::SettledFirst] {
            for admit in [false, true] {
                replay(&label, &cuts, window, (rule, placed), keyed_turn, admit, trace, share)?;
            }
        }
    }
    Ok(())
}
