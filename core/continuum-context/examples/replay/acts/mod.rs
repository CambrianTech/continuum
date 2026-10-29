//! `replay --acts`: the act inspector. A glass box over each resident's rounds of inference,
//! one row per request she sent: what it cost the engine, what her prompt was made of, what she
//! did with it, and what looks wrong.
//!
//! ```text
//! cargo run -p continuum-context --example replay -- --acts --since 2026-10-26T22:00Z \
//!     [--until <UTC>] [--persona <name or id prefix>]... [--purpose <suffix>] \
//!     [--engine-log ~/.continuum/logs/llama-server-<port>.log]... [--core-log <core log>]... \
//!     [--compare <UTC>] [--summary] [--json] [<capture.jsonl>...]
//! ```
//!
//! With no capture files it reads every `*.jsonl` under `~/.continuum/fixtures/prompt-captures/`
//! written since `--since`. Engine logs go oldest first (`.log.1` before `.log`); their resume
//! lines join each act by token counts (`engine.rs`). Core logs add her view's probes and
//! the residents' names (`probes.rs`). `--compare <UTC>` splits the window at that instant (a
//! deploy) and prints each resident's summary before and after.
//!
//! Joel, 2026-10-27: "You'll have to analyze, glass box, their rounds of inference and output...
//! half the work is the machinery around the vehicle."

pub mod clock;
pub mod did;
pub mod engine;
pub mod layers;
mod probes;
mod report;

use std::collections::BTreeMap;

use serde_json::Value;

use crate::captures::{self, Captures, Window};
use did::Did;
use engine::Resume;
use layers::{Breakdown, Divergence, Layer};

/// How the request ended.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum Outcome {
    Completed,
    Failed { error: String },
    Cancelled,
    /// No terminal row yet (in flight when the capture was read).
    Pending,
}

/// One request a resident sent, and everything known about it.
#[derive(Clone, serde::Serialize)]
pub struct Act {
    pub resident: String,
    pub persona_id: String,
    pub request_id: String,
    pub at_ms: u64,
    pub at: String,
    pub cycle: Option<u64>,
    pub build_sha: Option<String>,
    pub purpose: String,
    pub context_window: Option<u64>,
    pub outcome: Outcome,
    /// The engine's refusal over the slot's window, when it refused.
    pub refusal: Option<Refusal>,
    /// Submit to terminal row.
    pub elapsed_ms: Option<u64>,
    /// The engine's count, when it answered.
    pub input_tokens: Option<u64>,
    /// `input_tokens`, or the request's characters at her measured tokens per character.
    pub estimated_tokens: u64,
    pub cached_tokens: Option<u64>,
    pub prefill_tokens: Option<u64>,
    pub prefill_ms: Option<f64>,
    pub prefill_tps: Option<f64>,
    pub decode_tokens: Option<u64>,
    pub decode_ms: Option<f64>,
    pub decode_tps: Option<f64>,
    pub tokens_per_char: f64,
    pub layers: Breakdown,
    /// Where this request first differs from her previous one of the same purpose.
    pub divergence: Option<Divergence>,
    pub engine: Option<Resume>,
    /// Her view's own probe for this act (`--core-log`).
    pub view: Option<String>,
    pub did: Did,
    pub repeat: bool,
    pub flags: Vec<String>,
}

impl Act {
    /// Tokens of a layer, at this request's tokens per character.
    pub fn tokens(&self, chars: usize) -> f64 {
        chars as f64 * self.tokens_per_char
    }

    pub fn layer_tokens(&self, layer: Layer) -> f64 {
        self.tokens(self.layers.chars.get(&layer).copied().unwrap_or(0)) // unwrap_or: a layer the request lacks holds nothing
    }
}

/// The options `--acts` takes.
#[derive(Default)]
struct Options {
    window: Window,
    personas: Vec<String>,
    purpose: Option<String>,
    engine_logs: Vec<String>,
    core_logs: Vec<String>,
    compare_ms: Option<u64>,
    summary_only: bool,
    json: bool,
    paths: Vec<String>,
}

fn utc(flag: &str, value: Option<String>) -> Result<u64, String> {
    let value = value.ok_or_else(|| format!("{flag} needs a UTC time, e.g. 2026-10-26T22:00Z"))?;
    clock::parse(&value).ok_or_else(|| format!("{flag} {value}: not a UTC time like 2026-10-26T22:00Z"))
}

fn options(args: Vec<String>) -> Result<Options, String> {
    let mut o = Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--since" => o.window.since_ms = Some(utc("--since", args.next())?),
            "--until" => o.window.until_ms = Some(utc("--until", args.next())?),
            "--compare" => o.compare_ms = Some(utc("--compare", args.next())?),
            "--persona" => o.personas.push(args.next().ok_or("--persona needs a name or an id prefix")?),
            "--purpose" => o.purpose = Some(args.next().ok_or("--purpose needs a suffix")?),
            "--engine-log" => o.engine_logs.push(args.next().ok_or("--engine-log needs a llama-server log")?),
            "--core-log" => o.core_logs.push(args.next().ok_or("--core-log needs a core log")?),
            "--summary" => o.summary_only = true,
            "--json" => o.json = true,
            flag if flag.starts_with("--") => return Err(format!("--acts does not take {flag}")),
            _ => o.paths.push(arg),
        }
    }
    Ok(o)
}

/// Every capture file written since the window opened.
fn default_paths(since_ms: Option<u64>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let since_ms = since_ms.ok_or("--acts needs capture files or --since (to pick the captures written since)")?;
    let home = std::env::var("HOME").map_err(|_| "HOME is not set: pass capture files")?;
    let dir = std::path::Path::new(&home).join(".continuum/fixtures/prompt-captures");
    let since = std::time::UNIX_EPOCH + std::time::Duration::from_millis(since_ms);
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))? {
        let path = entry?.path();
        let meta = std::fs::metadata(&path)?;
        if path.extension().is_some_and(|e| e == "jsonl") && meta.modified()? >= since {
            out.push(path.display().to_string());
        }
    }
    out.sort();
    Ok(out)
}

fn read_all(paths: &[String]) -> Result<String, String> {
    paths.iter().map(|path| std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))).collect()
}

/// Her name as her own roster states it (`<name> (Agent, YOU)`).
fn roster_name(request: &Value) -> Option<String> {
    request["messages"].as_array()?.iter().filter_map(|m| m["content"].as_str()).filter(|t| t.starts_with("[roster]")).find_map(|roster| {
        roster.lines().find(|line| line.contains("(Agent, YOU)")).and_then(|line| line.split(" (").next()).map(str::to_owned)
    })
}

/// The engine's refusal of a prompt over the slot's window, as it words it:
/// `request (231158 tokens) exceeds the slot's available context size (187136 tokens)`.
/// Pinned by the fixtures: a rewording reads as no refusal and fails a test, never as 0 tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Refusal {
    /// The prompt's tokens, as the engine counted them.
    pub tokens: u64,
    /// The slot's available context, in tokens.
    pub window: u64,
}

/// The number in `(<n> tokens)` right after `marker` in `text`.
fn counted_after(text: &str, marker: &str) -> Option<u64> {
    let rest = text[text.find(marker)? + marker.len()..].strip_prefix('(')?;
    rest[..rest.find(" tokens)")?].parse().ok()
}

pub fn refusal(end: &Value) -> Option<Refusal> {
    let error = end["error"].as_str()?;
    Some(Refusal { tokens: counted_after(error, "request ")?, window: counted_after(error, "exceeds the slot's available context size ")? })
}

fn refused_tokens(end: &Value) -> Option<u64> {
    refusal(end).map(|r| r.tokens)
}

/// The replay's median, unknown (not 0) over no values.
fn median(values: Vec<f64>) -> Option<f64> {
    (!values.is_empty()).then(|| crate::median(values))
}

/// One resident's requests, oldest first, as acts (no engine join yet).
fn acts_of(rows: &[&Value], captures: &Captures, resident: &str, view: &probes::Probes) -> Vec<Act> {
    let terminal = |row: &Value| row["request_id"].as_str().and_then(|id| captures.terminal.get(id));
    // The engine's count: billed on an answer, or stated in a refusal.
    let billed = |row: &Value| terminal(row).and_then(|end| end["response"]["usage"]["inputTokens"].as_u64().or_else(|| refused_tokens(end)));
    let ratio = |row: &Value, tokens: u64| tokens as f64 / crate::request_chars(&row["request"], &row["request"]["tools"].to_string()).max(1) as f64;
    // Her tokens per character, as the engine billed her own answered requests.
    let her_ratio = median(rows.iter().filter_map(|&row| Some(ratio(row, billed(row)?))).collect());
    let mut out: Vec<Act> = Vec::new();
    let mut previous: BTreeMap<String, (&Value, Did)> = BTreeMap::new();
    let mut previous_at = 0u64;
    for &row in rows {
        let request = &row["request"];
        let purpose = request["purpose"].as_str().unwrap_or("").to_owned(); // unwrap_or: a request without a purpose groups with the others that lack one
        let end = terminal(row);
        let response = end.map(|end| &end["response"]).filter(|r| r.is_object());
        let timing = response.map(|r| &r["timing"]);
        let input_tokens = response.and_then(|r| r["usage"]["inputTokens"].as_u64());
        let counted = input_tokens.or_else(|| end.and_then(refused_tokens));
        let layers = layers::breakdown(request);
        let tokens_per_char = counted.map(|t| ratio(row, t)).or(her_ratio).unwrap_or(0.0); // unwrap_or: she has no answered request to measure by, so tokens are not estimated
        let outcome = match end.and_then(|end| end["event"].as_str()) {
            Some("completed") => Outcome::Completed,
            Some("failed") => Outcome::Failed { error: end.and_then(|e| e["error"].as_str()).unwrap_or("").to_owned() }, // unwrap_or: a failure that logged no error
            Some("cancelled") => Outcome::Cancelled,
            _ => Outcome::Pending,
        };
        let did = response.map(did::did).unwrap_or_default(); // unwrap_or_default: no response, she did nothing
        let before = previous.get(&purpose);
        let at_ms = row["captured_at_ms"].as_u64().unwrap_or(0); // unwrap_or: the loader only keeps rows it read a stamp from
        let f = |key: &str| timing.and_then(|t| t[key].as_f64());
        let u = |key: &str| timing.and_then(|t| t[key].as_u64());
        out.push(Act {
            resident: resident.to_owned(),
            persona_id: row["persona_id"].as_str().unwrap_or("").to_owned(), // unwrap_or: grouped under the empty id
            request_id: row["request_id"].as_str().unwrap_or("").to_owned(), // unwrap_or: an id-less row is still an act
            at_ms,
            at: clock::iso(at_ms),
            cycle: row["cycle_id"].as_u64(),
            build_sha: row["build_sha"].as_str().map(str::to_owned),
            purpose: purpose.clone(),
            context_window: row["context_window"].as_u64(),
            elapsed_ms: end.and_then(|e| e["elapsed_ms"].as_u64()),
            refusal: end.and_then(refusal),
            estimated_tokens: counted.unwrap_or((layers.total_chars as f64 * tokens_per_char) as u64), // unwrap_or: neither answered nor refused with a count, so estimated
            input_tokens,
            cached_tokens: u("cachedTokens"),
            prefill_tokens: u("prefillTokens"),
            prefill_ms: f("prefillMs"),
            prefill_tps: f("prefillTokensPerSecond"),
            decode_tokens: u("decodeTokens"),
            decode_ms: f("decodeMs"),
            decode_tps: f("decodeTokensPerSecond"),
            tokens_per_char,
            divergence: before.map(|(was, _)| layers::divergence(&was["request"], request)),
            repeat: before.is_some_and(|(_, was)| did.repeats(was)),
            engine: None,
            view: row["persona_id"].as_str().and_then(|id| view.view(id, previous_at, at_ms)),
            layers,
            outcome,
            did: did.clone(),
            flags: Vec::new(),
        });
        previous.insert(purpose, (row, did));
        previous_at = at_ms;
    }
    out
}

/// What looks wrong with an act, once the engine is joined.
fn flag(act: &mut Act) {
    let mut flags = Vec::new();
    match &act.outcome {
        Outcome::Failed { .. } if act.refusal.is_some() => flags.push("refused:over-window".to_owned()),
        Outcome::Failed { .. } => flags.push("failed".to_owned()),
        Outcome::Cancelled => flags.push("cancelled".to_owned()),
        _ => {}
    }
    // The slot's window as the engine stated it, else the window the core recorded.
    if let Some(window) = act.refusal.map(|r| r.window).or(act.context_window) {
        let newest = act.layer_tokens(Layer::NewestResult) as u64;
        if act.estimated_tokens > window && act.estimated_tokens.saturating_sub(newest) <= window {
            flags.push("newest>window".to_owned());
        }
    }
    if let Some(resume) = &act.engine {
        if let Some(paged) = &resume.paged_in {
            if !act.persona_id.starts_with(paged.as_str()) {
                flags.push(format!("page-in:other-mind:{paged}"));
            }
        }
        if resume.full() {
            flags.push("full-reprefill".to_owned());
        } else if resume.lost_more_than_new() {
            flags.push(if resume.paged_in.is_some() { "cache-loss:page-in" } else { "cache-loss:warm" }.to_owned());
        }
    }
    if act.repeat {
        flags.push("repeat".to_owned());
    }
    match &act.divergence {
        Some(Divergence::SystemPrompt | Divergence::Tools) => flags.push("head-changed".to_owned()),
        Some(Divergence::View { layer, .. }) => flags.push(format!("view-break:{}", layer.short())),
        _ => {}
    }
    act.flags = flags;
}

/// Every act in the captures, oldest first, joined to the engine's resumes and her view's probes.
fn inspect(captures: &Captures, resumes: Vec<Resume>, view: &probes::Probes, o: &Options) -> Vec<Act> {
    let mut by_persona: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
    for row in &captures.submitted {
        if o.purpose.as_ref().is_some_and(|suffix| !row["request"]["purpose"].as_str().is_some_and(|p| p.ends_with(suffix.as_str()))) {
            continue;
        }
        by_persona.entry(row["persona_id"].as_str().unwrap_or("")).or_default().push(row); // unwrap_or: grouped under the empty id
    }
    let mut acts = Vec::new();
    for (id, mut rows) in by_persona {
        rows.sort_by_key(|row| row["captured_at_ms"].as_u64());
        let name = rows.iter().rev().find_map(|row| roster_name(&row["request"])).or_else(|| view.names.get(id).cloned()).unwrap_or_else(|| id.chars().take(8).collect()); // unwrap_or_else: unnamed, she reads by her id
        let wanted = o.personas.is_empty() || o.personas.iter().any(|p| id.starts_with(p.as_str()) || name.eq_ignore_ascii_case(p));
        if wanted {
            acts.extend(acts_of(&rows, captures, &name, view));
        }
    }
    acts.sort_by_key(|act| act.at_ms);
    let mut index = engine::Index::new(resumes);
    for act in &mut acts {
        if let (Some(input), Some(cached)) = (act.input_tokens, act.cached_tokens) {
            act.engine = index.take(input, cached);
        }
        flag(act);
    }
    acts
}

pub fn main(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let o = options(args)?;
    let paths = if o.paths.is_empty() { default_paths(o.window.since_ms)? } else { o.paths.clone() };
    let captures = captures::load(&paths, o.window)?;
    let resumes = engine::parse(&read_all(&o.engine_logs)?);
    let view = probes::parse(&read_all(&o.core_logs)?);
    let acts = inspect(&captures, resumes, &view, &o);
    let joined = acts.iter().filter(|act| act.engine.is_some()).count();
    let label = format!(
        "{} acts from {} capture files{}{}; engine resumes joined: {joined}",
        acts.len(),
        paths.len(),
        o.window.since_ms.map(|ms| format!(", since {}", clock::iso(ms))).unwrap_or_default(), // unwrap_or_default: an open start says nothing
        o.window.until_ms.map(|ms| format!(", until {}", clock::iso(ms))).unwrap_or_default(), // unwrap_or_default: an open end says nothing
    );
    report::print(&label, &acts, o.compare_ms, o.summary_only, o.json)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURES: &str = include_str!("fixtures/captures.jsonl");

    fn fixture(personas: &[&str]) -> Vec<Act> {
        let mut captures = Captures::default();
        captures::read(CAPTURES, Window::default(), &mut captures);
        let o = Options { personas: personas.iter().map(|p| p.to_string()).collect(), ..Options::default() };
        inspect(&captures, engine::parse(include_str!("fixtures/engine-log.txt")), &probes::parse(include_str!("fixtures/core-log.txt")), &o)
    }

    // what this catches: the engine rewording its over-window refusal: the act would read 0
    // tokens and lose its `refused:over-window` flag silently instead of failing here. The text
    // is the engine's own (llama-server, 2026-10-26 on the M5); a near miss is not a refusal.
    #[test]
    fn the_engine_refusal_wording_is_pinned() {
        let refused = |error: &str| refusal(&serde_json::json!({ "event": "failed", "error": error }));
        assert_eq!(
            refused("request (231158 tokens) exceeds the slot's available context size (187136 tokens)"),
            Some(Refusal { tokens: 231_158, window: 187_136 })
        );
        assert_eq!(refused("request (231158 tokens) exceeds the slot's context"), None);
        assert_eq!(refused("request future dropped before a terminal response"), None);
        let acts = fixture(&[]);
        let refused_act = acts.iter().find(|act| act.refusal.is_some());
        assert_eq!(refused_act.map(|act| act.flags.first().map(String::as_str)), Some(Some("refused:over-window")));
    }

    // what this catches: the whole path from capture rows to rows of the table: her name from
    // her roster, the engine joined to the right act, a page-in that lost more than was new, a
    // repeated call, a refusal over the window that only her newest result pushed over, and a
    // view break owned by the layer that changed; a refusal sized by the count it states.
    #[test]
    fn captures_engine_and_probes_make_one_row_per_act() {
        let acts = fixture(&[]);
        assert_eq!(acts.len(), 3);
        assert!(acts.iter().all(|act| act.resident == "Testa"));
        let [first, second, third] = [&acts[0], &acts[1], &acts[2]];
        assert_eq!(first.engine.as_ref().map(|r| r.lcp), Some(30_000));
        assert_eq!(first.flags, ["cache-loss:page-in"]);
        assert_eq!(first.view.as_deref(), Some("+5461c"));
        assert_eq!(second.view.as_deref(), Some("turnover 40173t"));
        assert_eq!(first.did.kind, did::Kind::Read);
        assert_eq!(second.engine.as_ref().map(|r| r.past), Some(31_400));
        assert!(second.repeat);
        assert_eq!(second.flags, ["repeat", "view-break:views"]);
        assert!(matches!(third.outcome, Outcome::Failed { .. }));
        // A refusal's prompt is the count the engine stated, not an estimate.
        assert_eq!(third.refusal, Some(Refusal { tokens: 240_000, window: 187_136 }));
        assert_eq!(third.estimated_tokens, 240_000);
        assert_eq!(third.flags, ["refused:over-window", "newest>window"]);
        assert!(third.layer_tokens(Layer::NewestResult) > 0.0);
        // A persona filter by name, case-insensitive; one by an id prefix that matches nobody.
        assert_eq!(fixture(&["testa"]).len(), 3);
        assert!(fixture(&["ffff"]).is_empty());
    }
}
