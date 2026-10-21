//! Replay a resident's captured prompts through the view and measure what each request
//! shares with the one before it: the design gate of MIND-CONTEXT-AND-ENGINE-CONFIG.md
//! ("offline replay at >= 90% prefix").
//!
//! `cargo run -p continuum-context --example replay -- <prompt-capture.jsonl>...`
//!
//! Each captured request is cut the way the prompt assembly builds it: the system prompt is
//! the head; results, pointers, the steps ledger and the action lines of working memory are
//! ONE action log keyed by `#n`; each `[faculty]` block and each list-shaped block (open
//! views, notices, recall, thoughts) is a source of `- ` items; whatever is left (facts,
//! clock, the ask, the live act) is this act's fresh tail. Both arms are measured in
//! characters: as sent, and as the view would have sent it.

use std::collections::BTreeMap;

use continuum_context::source::{Item, Source};
use continuum_context::view::View;

struct Captured {
    name: String,
    items: Vec<Item<String>>,
}

/// What the allocator weighs each source by: the turn's own text and the board first, her
/// thoughts next, then her results, her open views, and recall last.
fn priority(source: &str) -> u8 {
    match source {
        "recall" => 1,
        "open-views" => 2,
        "actions" => 3,
        "working-memory" => 4,
        _ => 5,
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
    Item { key, pointer: Some(pointer), full: text }
}

/// The one allocation at an epoch start, modelled: every source gets its floor, then the
/// rest of `budget` goes to sources in priority order up to what each wants. (In the core
/// this is `flex_allocate`.)
fn allocate(claims: &[continuum_context::view::Claim], budget: usize) -> Vec<usize> {
    let mut grants: Vec<usize> = claims.iter().map(|claim| claim.pointer_chars).collect();
    let mut spare = budget.saturating_sub(grants.iter().sum());
    let mut order: Vec<usize> = (0..claims.len()).collect();
    order.sort_by_key(|index| std::cmp::Reverse(claims[*index].priority));
    for index in order {
        let more = claims[index].full_chars.saturating_sub(grants[index]).min(spare);
        grants[index] += more;
        spare -= more;
    }
    grants
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

fn cut(request: &serde_json::Value, keyed_turn: bool) -> Cut {
    let head = request["systemPrompt"].as_str().unwrap_or_default().to_owned();
    let mut sources: BTreeMap<String, Vec<Item<String>>> = BTreeMap::new();
    let mut tail = Vec::new();
    let mut sent = head.clone();
    let mut push = |source: &str, items: Vec<Item<String>>| sources.entry(source.to_owned()).or_default().extend(items);
    for message in request["messages"].as_array().into_iter().flatten() {
        let text = match &message["content"] {
            serde_json::Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        sent.push('\n');
        sent.push_str(&text);
        if let Some(seq) = seq_after(&text, "[result #") {
            push("actions", vec![item(seq.to_string(), text)]);
        } else if text.starts_with("Recent results of your own actions") {
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
                .filter(|item| !item.full.starts_with("- [action #"))
                .collect();
            push("working-memory", thoughts);
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
        let held: Vec<String> = sources.get("actions").into_iter().flatten().map(|item| item.key.clone()).collect();
        let missing = ledger.into_iter().filter(|line| !held.contains(&line.key)).collect::<Vec<_>>();
        sources.entry("actions".into()).or_default().extend(missing);
    }
    Cut { head, sources, tail, sent }
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

fn replay(path: &str, cuts: &[Cut], keyed_turn: bool) -> Result<(), Box<dyn std::error::Error>> {
    // The window as the fitter used it: the largest request it actually sent.
    let window = cuts.iter().map(|cut| cut.sent.chars().count()).max().unwrap_or(0);
    let mut view: Option<View<String>> = None;
    let mut previous_sent: Option<&str> = None;
    let mut previous_view = String::new();
    let (mut sent_prefill, mut view_prefill, mut view_share, mut turnovers) = (Vec::new(), Vec::new(), Vec::new(), 0usize);
    // What each source appended per act, and the fresh tail: where the prefill goes.
    let mut held: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut appended: BTreeMap<String, usize> = BTreeMap::new();
    let mut tails = Vec::new();
    for cut in cuts {
        let sources: Vec<Captured> =
            cut.sources.iter().map(|(name, items)| Captured { name: name.clone(), items: items.clone() }).collect();
        let refs: Vec<&dyn Source<String>> = sources.iter().map(|s| s as &dyn Source<String>).collect();
        let tail = cut.tail.join("\n");
        tails.push(tail.chars().count() as f64);
        for (name, items) in &cut.sources {
            for item in items {
                let key = (name.clone(), item.key.clone());
                if held.get(&key) != Some(&item.full) {
                    *appended.entry(name.clone()).or_default() += item.full.chars().count();
                    held.insert(key, item.full.clone());
                }
            }
        }
        // An epoch starts at half the window, so it has room to grow before the next turnover.
        let budget = (window / 2).saturating_sub(cut.head.chars().count() + tail.chars().count());
        let all_full = |claims: &[continuum_context::view::Claim]| allocate(claims, budget);
        match view.as_mut() {
            None => view = Some(View::start(vec![cut.head.clone()], &refs, all_full)?.0),
            Some(open) => {
                open.advance(&refs);
                let reserve = open.appended_per_act().unwrap_or(0);
                if open.chars() + tail.chars().count() + reserve > window {
                    open.turn_over(vec![cut.head.clone()], &refs, all_full)?;
                    turnovers += 1;
                }
            }
        }
        let rendered = match view.as_ref() {
            Some(open) => format!("{}\n{tail}", open.render().cloned().collect::<Vec<_>>().join("\n")),
            None => tail.clone(),
        };
        if let Some(before) = previous_sent {
            let sent = cut.sent.chars().count();
            sent_prefill.push((sent - shared(before, &cut.sent)) as f64);
            let length = rendered.chars().count();
            let kept = shared(&previous_view, &rendered);
            view_prefill.push((length - kept) as f64);
            view_share.push(kept as f64 / length.max(1) as f64);
        }
        previous_sent = Some(&cut.sent);
        previous_view = rendered;
    }
    println!(
        "{path} [{}]\n  {} requests, window {window} chars, {turnovers} turnovers\n  prefill per act, chars: as sent median {:.0} mean {:.0}; the view median {:.0} mean {:.0} ({:.0}% of the request matched)\n  fresh tail median {:.0} chars",
        if keyed_turn { "standing turn text keyed into the epoch" } else { "turn text as fresh tail" },
        cuts.len(),
        median(sent_prefill.clone()),
        mean(&sent_prefill),
        median(view_prefill.clone()),
        mean(&view_prefill),
        100.0 * median(view_share),
        median(tails),
    );
    let acts = cuts.len().max(1);
    let line: Vec<String> = appended.iter().map(|(name, chars)| format!("{name} {}", chars / acts)).collect();
    println!("  appended per act by source: {}", line.join(", "));
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let mut requests = Vec::new();
        for line in std::fs::read_to_string(&path)?.lines() {
            let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            if event["event"] == "submitted" {
                requests.push(event["request"].clone());
            }
        }
        for keyed_turn in [false, true] {
            let cuts: Vec<Cut> = requests.iter().map(|request| cut(request, keyed_turn)).collect();
            replay(&path, &cuts, keyed_turn)?;
        }
    }
    Ok(())
}
