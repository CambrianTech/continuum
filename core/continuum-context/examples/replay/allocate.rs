//! `replay --allocate`: the allocation's dyno. Each captured act's claims are driven through the
//! ONE allocation (`allocation::allocate`: levels, search, the time price), priced and unpriced,
//! so every allocation change can be measured on the residents' real acts before a live read.
//!
//! ```text
//! cargo run -p continuum-context --example replay -- --allocate --since 2026-11-07T00:00Z \
//!     [--until <UTC>] [--trace <act index>,...] [<capture.jsonl>...]
//! ```
//!
//! Each completed request becomes an `ActClaims`, priced in tokens at that act's own billed
//! density (request chars over `usage.inputTokens`):
//! - framing: the system prompt whole (its menu's levels are not in the capture);
//! - tools: each schema whole, at name and description, at its name; actionable;
//! - her view: the messages before her act loop, cut into sources as the view replay cuts them
//!   (`cut`), each source's floor its items' pointers and its want their wholes; the held-work
//!   block actionable; `[recall]` a source like any other (grounding is her view's);
//! - the act loop (from her first assistant message): the ask before it is the required input,
//!   its last tool result her newest (whole, or a page no smaller than its first line), the assistant message before
//!   that her last move, the rest optional history;
//! - the reply: her largest output in her previous twenty acts (at least 256) up to `maxTokens`.
//!
//! Price, two ways: NODE, one `ActTimeFit` over every completed act of every file given (the
//! `--act-price` curve); LIVE, what the core holds: her own running fit over her acts before this
//! one on the same model, kept across window changes and restarts (`working_memory::act_times`).
//! Per resident it prints: sent tokens, kept tokens priced and unpriced, kept over sent, the held
//! window over the served one, predicted seconds per act at sent and at kept, her tools sent and
//! kept, what the price decided (held / window binds / outside the measured contexts / unmeasured)
//! and how many acts the LIVE fit priced. `--trace` prints every claim of the acts named.

use std::collections::{BTreeMap, HashMap};

use continuum_context::allocation::{allocate, id, ActClaims, ActPlan, Consumer, Head, Leveled, Newest, Priced, Rungs, Span, ViewClaim};
use continuum_context::flex::FlexClaim;
use continuum_context::price::{ActSample, ActTimeFit, ContextPrice};
use serde_json::Value;

use crate::acts::{default_paths, utc};
use crate::captures::{self, Window};

/// One completed act: its request, its window and its engine timing.
struct Act {
    at_ms: u64,
    purpose: String,
    window: u32,
    model: String,
    sample: ActSample,
    request: Value,
}

fn sample(row: &Value) -> Option<ActSample> {
    if row["event"].as_str() != Some("completed") {
        return None;
    }
    let response = &row["response"];
    let timing = &response["timing"];
    let count = |value: &Value| value.as_u64().and_then(|n| u32::try_from(n).ok());
    Some(ActSample {
        input: count(&response["usage"]["inputTokens"])?,
        cached: count(&timing["cachedTokens"])?,
        fresh: count(&timing["prefillTokens"])?,
        prefill_ms: timing["prefillMs"].as_f64()?,
        output: count(&timing["decodeTokens"])?,
        decode_ms: timing["decodeMs"].as_f64()?,
    })
}

/// Every completed act of one capture file inside `window`, oldest first.
fn acts_of(path: &str, window: Window) -> Result<Vec<Act>, Box<dyn std::error::Error>> {
    let rows = captures::load(&[path.to_owned()], window)?;
    let mut acts: Vec<Act> = rows
        .submitted
        .iter()
        .filter_map(|row| {
            let end = rows.terminal.get(row["request_id"].as_str()?)?;
            Some(Act {
                at_ms: row["captured_at_ms"].as_u64()?,
                purpose: row["request"]["purpose"].as_str()?.to_owned(),
                window: u32::try_from(row["context_window"].as_u64()?).ok()?,
                model: end["response"]["model"].as_str()?.to_owned(),
                sample: sample(end)?,
                request: row["request"].clone(),
            })
        })
        .collect();
    acts.sort_by_key(|act| act.at_ms);
    Ok(acts)
}

fn first_line(text: &str) -> usize {
    text.lines().next().map_or(0, |line| line.chars().take(185).count()).max(1) // map_or: an empty text points at nothing
}

/// One act's claims, in tokens at its own billed density.
fn claims(act: &Act, reply_need: u32, price: ContextPrice) -> ActClaims {
    let request = &act.request;
    let messages: Vec<(&str, String)> = request["messages"].as_array().into_iter().flatten().map(|m| (m["role"].as_str().unwrap_or_default(), captures::text(m))).collect(); // unwrap_or_default: a message without a role is no assistant
    let system = request["systemPrompt"].as_str().unwrap_or_default(); // unwrap_or_default: a request without a system prompt frames nothing
    let tools: Vec<&Value> = request["tools"].as_array().into_iter().flatten().collect();
    let chars = system.chars().count() + tools.iter().map(|t| t.to_string().chars().count()).sum::<usize>() + messages.iter().map(|(_, t)| t.chars().count()).sum::<usize>();
    let per_char = f64::from(act.sample.input) / chars.max(1) as f64;
    let tok = |chars: usize| (chars as f64 * per_char).ceil() as u32;
    let verbs: Vec<Rungs> = tools
        .iter()
        .filter_map(|tool| {
            let name = tool["name"].as_str().map_or(0, str::len); // map_or: a schema without a name names nothing
            let description = tool["description"].as_str().map_or(0, str::len); // map_or: as above
            Rungs::new([tok(tool.to_string().chars().count()), tok(name + description + 16), tok(name + 8)])
        })
        .collect();
    // Her act loop starts at her first assistant message; the ask is the message before it.
    let loop_at = messages.iter().position(|(role, _)| *role == "assistant").unwrap_or(messages.len()); // unwrap_or: an act with no loop is all view and ask
    let ask_at = loop_at.checked_sub(1);
    let required = Leveled::one(Rungs::whole(ask_at.map_or(0, |at| tok(messages[at].1.chars().count())))); // map_or: no ask is no required input
    let (mut newest, mut last_move, mut history) = (Newest::Absent, Leveled::default(), 0usize);
    let looped = &messages[loop_at..];
    let newest_at = looped.iter().rposition(|(role, text)| *role == "user" && text.contains("\"tool_result\""));
    let move_at = newest_at.and_then(|at| looped[..at].iter().rposition(|(role, _)| *role == "assistant"));
    for (at, (_, text)) in looped.iter().enumerate() {
        let size = text.chars().count();
        if Some(at) == newest_at {
            newest = Newest::Pageable { whole: tok(size), least: tok(first_line(text)) };
        } else if Some(at) == move_at {
            last_move = Leveled::one(Rungs::whole(tok(size)));
        } else {
            history += size;
        }
    }
    // Her view: the messages before the ask, cut as the view replay cuts them.
    let mut before = request.clone();
    before["messages"] = Value::Array(request["messages"].as_array().map_or_else(Vec::new, |all| all[..ask_at.unwrap_or(0)].to_vec())); // map_or_else / unwrap_or: no messages, no view
    let cut = crate::cut(&before, "", false);
    let acting = act.purpose.ends_with("act");
    let mut view = Vec::new();
    for (name, items) in &cut.sources {
        let full: usize = items.iter().map(|item| item.full().chars().count()).sum();
        let floor: usize = items.iter().map(|item| item.pointer().map_or_else(|| item.full().chars().count(), |p| p.chars().count())).sum(); // map_or_else: an item with no pointer is its own floor
        let claim = FlexClaim { source_id: format!("{}{name}", id::VIEW), priority: crate::priority(name), floor_tokens: tok(floor), min_tokens: tok(floor), max_tokens: tok(full).max(tok(floor)), required: acting };
        let consumer = if name == "held-work" { Consumer::Actionable } else { Consumer::Context };
        view.push(ViewClaim { claim, consumer });
    }
    let want = request["maxTokens"].as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(reply_need); // unwrap_or: a request with no cap wants its need
    ActClaims {
        framing: Head::free(Leveled::one(Rungs::whole(tok(system.chars().count())))),
        required,
        newest,
        tools: Head { consumer: Consumer::Actionable, ..Head::free(Leveled::new(verbs)) },
        last_move,
        history: Span { floor: 0, min: 0, want: tok(history) },
        reply: FlexClaim { source_id: id::REPLY.into(), priority: if acting { 10 } else { 4 }, floor_tokens: reply_need, min_tokens: reply_need, max_tokens: want.max(reply_need), required: true },
        view,
        price,
        // A capture does not carry the model row: the dyno bounds her need by her grant.
        trained: continuum_context::allocation::TrainedContext::Unreported,
    }
}

/// What the plan holds for her prompt: every grant but the reply's.
fn kept(plan: &ActPlan) -> u32 {
    plan.claims().iter().filter(|claim| claim.source_id != id::REPLY).map(|claim| plan.granted(&claim.source_id)).sum()
}

fn pct(mut values: Vec<f64>, at: f64) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    values.get(((values.len().max(1) - 1) as f64 * at).round() as usize).copied().unwrap_or(f64::NAN) // unwrap_or: no values have no percentile
}

/// What one resident's acts measured, by column.
#[derive(Default)]
struct Columns {
    sent: Vec<f64>,
    kept: Vec<f64>,
    unpriced: Vec<f64>,
    ratio: Vec<f64>,
    held: Vec<f64>,
    seconds_sent: Vec<f64>,
    seconds_kept: Vec<f64>,
    tools_sent: Vec<f64>,
    tools_kept: Vec<f64>,
    decided: [usize; 4],
    live_priced: usize,
    live_kept: Vec<f64>,
}

pub fn main(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut window = Window::default();
    let (mut paths, mut trace) = (Vec::new(), Vec::new());
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--since" => window.since_ms = Some(utc("--since", args.next())?),
            "--until" => window.until_ms = Some(utc("--until", args.next())?),
            "--trace" => trace = args.next().ok_or("--trace needs act indexes, e.g. 3,17")?.split(',').filter_map(|i| i.parse::<usize>().ok()).collect(),
            flag if flag.starts_with("--") => return Err(format!("--allocate does not take {flag}").into()),
            _ => paths.push(arg),
        }
    }
    let paths = if paths.is_empty() { default_paths(window.since_ms)? } else { paths };
    let mut residents: Vec<(String, Vec<Act>)> = Vec::new();
    for path in &paths {
        let name = std::path::Path::new(path).file_stem().map_or_else(|| path.clone(), |stem| stem.to_string_lossy().chars().take(8).collect()); // map_or_else: a path without a stem is its own name
        let acts = acts_of(path, window)?;
        if !acts.is_empty() {
            residents.push((name, acts));
        }
    }
    let mut node = ActTimeFit::default();
    residents.iter().flat_map(|(_, acts)| acts).for_each(|act| node.observe(&act.sample));
    let ContextPrice::Measured(time) = node.price() else {
        println!("allocate: the node's act time is unmeasured over {} acts: nothing to price", node.acts());
        return Ok(());
    };
    println!("node act time: {} acts, measured contexts {:?}, {:.1} s at 20k, {:.1} s at 40k", node.acts(), time.measured(), time.seconds(20_000.0), time.seconds(40_000.0));
    println!("resident | acts | sent med/p90 | kept med/p90 | unpriced kept med | kept/sent med | held/window med | s/act at sent, kept (curve) | tools sent, kept med | held/binds/outside/unmeasured | live priced | live kept med");
    for (name, acts) in &residents {
        let mut c = Columns::default();
        let mut outputs: Vec<u32> = Vec::new();
        let mut live: HashMap<String, ActTimeFit> = HashMap::new();
        for (index, act) in acts.iter().enumerate() {
            let need = outputs.iter().rev().take(20).copied().max().unwrap_or(256).max(256); // unwrap_or: no output yet needs the least a reply holds
            let sent = act.sample.input;
            let priced = allocate(act.window, claims(act, need, ContextPrice::Measured(time)));
            let unpriced = allocate(act.window, claims(act, need, ContextPrice::Unmeasured));
            let k = kept(&priced).min(sent);
            c.sent.push(f64::from(sent));
            c.kept.push(f64::from(k));
            c.unpriced.push(f64::from(kept(&unpriced).min(sent)));
            c.ratio.push(f64::from(k) / f64::from(sent.max(1)));
            c.seconds_sent.push(time.seconds(f64::from(sent)));
            c.seconds_kept.push(time.seconds(f64::from(k)));
            c.tools_sent.push(f64::from(unpriced.chosen(id::TOOLS).map_or(0, |choice| choice.cost))); // map_or: tools are always chosen
            c.tools_kept.push(f64::from(priced.chosen(id::TOOLS).map_or(0, |choice| choice.cost))); // map_or: as above
            let (slot, held) = match priced.priced {
                Priced::Held { held, .. } => (0, f64::from(held) / f64::from(act.window.max(1))),
                Priced::WindowBinds => (1, 1.0),
                Priced::OutsideMeasured { .. } => (2, 1.0),
                Priced::Unmeasured => (3, 1.0),
            };
            c.decided[slot] += 1;
            c.held.push(held);
            // LIVE: her own fit over her acts before this one on this model, whatever her window.
            let fit = live.entry(act.model.clone()).or_default();
            let live_price = fit.price();
            c.live_priced += usize::from(matches!(live_price, ContextPrice::Measured(_)));
            c.live_kept.push(f64::from(kept(&allocate(act.window, claims(act, need, live_price))).min(sent)));
            fit.observe(&act.sample);
            outputs.push(act.sample.output);
            if trace.contains(&index) {
                println!("  {name} act {index} {} window {} sent {sent}: {:?}", act.purpose, act.window, priced.priced);
                for claim in priced.claims() {
                    println!("    {:<28} p{:>2} floor {:>6} want {:>7} | priced {:>6} | unpriced {:>6}", claim.source_id, claim.priority, claim.floor_tokens, claim.max_tokens, priced.granted(&claim.source_id), unpriced.granted(&claim.source_id));
                }
                println!("    newest: priced {:?} unpriced {:?} least {:?}", priced.newest_shown(), unpriced.newest_shown(), priced.hard_limit(id::NEWEST));
                for part in [id::FRAMING, id::TOOLS] {
                    println!("    chosen {part}: priced {:?} unpriced {:?} hard limit {:?}", priced.chosen(part).map(|c| c.cost), unpriced.chosen(part).map(|c| c.cost), priced.hard_limit(part));
                }
            }
        }
        let n = acts.len();
        let by_model: BTreeMap<&str, usize> = acts.iter().fold(BTreeMap::new(), |mut m, act| {
            *m.entry(act.model.as_str()).or_default() += 1;
            m
        });
        println!(
            "{name} | {n} | {:.0}/{:.0} | {:.0}/{:.0} | {:.0} | {:.0}% | {:.0}% | {:.0}, {:.0} | {:.0}, {:.0} | {}/{}/{}/{} | {}/{n} | {:.0}   ({} models)",
            pct(c.sent.clone(), 0.5),
            pct(c.sent, 0.9),
            pct(c.kept.clone(), 0.5),
            pct(c.kept, 0.9),
            pct(c.unpriced, 0.5),
            100.0 * pct(c.ratio, 0.5),
            100.0 * pct(c.held, 0.5),
            pct(c.seconds_sent, 0.5),
            pct(c.seconds_kept, 0.5),
            pct(c.tools_sent, 0.5),
            pct(c.tools_kept, 0.5),
            c.decided[0],
            c.decided[1],
            c.decided[2],
            c.decided[3],
            c.live_priced,
            pct(c.live_kept, 0.5),
            by_model.len(),
        );
    }
    Ok(())
}
