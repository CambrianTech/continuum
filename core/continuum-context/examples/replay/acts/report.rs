//! The act inspector's output: per resident, one row per act (and the layers of its prompt
//! beneath it), then her summary; or, with `--compare`, her summary before and after an instant;
//! or all of it as JSON.

use std::collections::BTreeMap;

use crate::acts::did::Kind;
use crate::acts::layers::{Divergence, Layer};
use crate::acts::{clock, median, Act, Outcome};

/// One resident's acts in numbers.
#[derive(Default, serde::Serialize)]
pub struct Summary {
    pub resident: String,
    pub acts: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
    /// Median seconds from submit to the terminal row, cancelled requests left out.
    pub median_act_s: Option<f64>,
    /// Median seconds between two of her submissions.
    pub median_gap_s: Option<f64>,
    /// Prefill and decode time over the acts' total time (answered acts).
    pub prefill_share: Option<f64>,
    pub decode_share: Option<f64>,
    pub median_prefill_tps: Option<f64>,
    pub median_decode_tps: Option<f64>,
    pub median_input_tokens: Option<f64>,
    /// Cached prompt tokens over prompt tokens (answered acts).
    pub reuse: Option<f64>,
    /// The context law's health number (MIND-CONTEXT-AND-ENGINE-CONFIG.md, "a context is a
    /// FIFO"): tokens the engine prefilled per act over the tokens her prompt grew since her
    /// previous answered act; healthy is about 1. An act whose prompt shrank (a turnover) has
    /// no append to measure against and is left out.
    pub reprefill_per_appended: Option<f64>,
    /// Acts joined to an engine resume.
    pub joined: usize,
    /// Matched tokens the engine prefilled again (lcp past the resume point) over all prefilled
    /// tokens, joined acts.
    pub cache_loss_share: Option<f64>,
    pub cache_loss_acts: usize,
    pub page_ins: usize,
    pub full_reprefills: usize,
    pub view_breaks: usize,
    pub head_changes: usize,
    pub refusals: usize,
    pub repeats: usize,
    pub kinds: BTreeMap<String, usize>,
    pub tool_calls: usize,
    pub tool_calls_per_hour: Option<f64>,
}

fn flagged(act: &Act, prefix: &str) -> bool {
    act.flags.iter().any(|flag| flag.starts_with(prefix))
}

fn ratio(part: f64, whole: f64) -> Option<f64> {
    (whole > 0.0).then_some(part / whole)
}

/// Prefilled over appended, summed over consecutive answered acts whose prompt grew.
fn reprefill_per_appended(answered: &[&Act]) -> Option<f64> {
    let (prefilled, appended) = answered.windows(2).fold((0u64, 0u64), |(prefilled, appended), pair| {
        match (pair[0].input_tokens, pair[1].input_tokens, pair[1].prefill_tokens) {
            (Some(before), Some(now), Some(fresh)) if now > before => (prefilled + fresh, appended + (now - before)),
            _ => (prefilled, appended),
        }
    });
    ratio(prefilled as f64, appended as f64)
}

pub fn summarize(resident: &str, acts: &[&Act]) -> Summary {
    let answered: Vec<&Act> = acts.iter().copied().filter(|a| matches!(a.outcome, Outcome::Completed)).collect();
    let sum = |f: &dyn Fn(&Act) -> Option<f64>| answered.iter().filter_map(|a| f(a)).sum::<f64>();
    let elapsed = sum(&|a| a.elapsed_ms.map(|ms| ms as f64));
    let joined: Vec<&Act> = acts.iter().copied().filter(|a| a.engine.is_some()).collect();
    let lost: u64 = joined.iter().filter_map(|a| a.engine.as_ref()).map(|r| r.lost()).sum();
    let prefilled: u64 = joined.iter().filter_map(|a| a.engine.as_ref()).map(|r| r.total.saturating_sub(r.past)).sum();
    let span_h = match (acts.first(), acts.last()) {
        (Some(first), Some(last)) if last.at_ms > first.at_ms => Some((last.at_ms - first.at_ms) as f64 / 3_600_000.0),
        _ => None,
    };
    let tool_calls: usize = acts.iter().map(|a| a.did.calls.len()).sum();
    let mut kinds = BTreeMap::new();
    for act in acts {
        *kinds.entry(format!("{:?}", act.did.kind).to_lowercase()).or_default() += 1;
    }
    Summary {
        resident: resident.to_owned(),
        acts: acts.len(),
        completed: answered.len(),
        failed: acts.iter().filter(|a| matches!(a.outcome, Outcome::Failed { .. })).count(),
        cancelled: acts.iter().filter(|a| matches!(a.outcome, Outcome::Cancelled)).count(),
        // A cancelled request was dropped by her caller, not served: it is not an act's length.
        median_act_s: median(acts.iter().filter(|a| !matches!(a.outcome, Outcome::Cancelled)).filter_map(|a| a.elapsed_ms).map(|ms| ms as f64 / 1_000.0).collect()),
        median_gap_s: median(acts.windows(2).map(|w| (w[1].at_ms - w[0].at_ms) as f64 / 1_000.0).collect()),
        prefill_share: ratio(sum(&|a| a.prefill_ms), elapsed),
        decode_share: ratio(sum(&|a| a.decode_ms), elapsed),
        median_prefill_tps: median(answered.iter().filter_map(|a| a.prefill_tps).collect()),
        median_decode_tps: median(answered.iter().filter_map(|a| a.decode_tps).collect()),
        median_input_tokens: median(acts.iter().map(|a| a.estimated_tokens as f64).collect()),
        reuse: ratio(sum(&|a| a.cached_tokens.map(|t| t as f64)), sum(&|a| a.input_tokens.map(|t| t as f64))),
        reprefill_per_appended: reprefill_per_appended(&answered),
        joined: joined.len(),
        cache_loss_share: ratio(lost as f64, prefilled as f64),
        cache_loss_acts: acts.iter().filter(|a| flagged(a, "cache-loss")).count(),
        page_ins: joined.iter().filter(|a| a.engine.as_ref().is_some_and(|r| r.paged_in.is_some())).count(),
        full_reprefills: acts.iter().filter(|a| flagged(a, "full-reprefill")).count(),
        view_breaks: acts.iter().filter(|a| flagged(a, "view-break")).count(),
        head_changes: acts.iter().filter(|a| flagged(a, "head-changed")).count(),
        refusals: acts.iter().filter(|a| flagged(a, "refused")).count(),
        repeats: acts.iter().filter(|a| a.repeat).count(),
        kinds,
        tool_calls,
        tool_calls_per_hour: span_h.map(|h| tool_calls as f64 / h),
    }
}

fn k(tokens: f64) -> String {
    format!("{:.1}k", tokens / 1_000.0)
}

fn opt(value: Option<f64>, render: impl Fn(f64) -> String) -> String {
    value.map(render).unwrap_or_else(|| "-".to_owned()) // unwrap_or_else: unknown shows as `-`
}

fn pct(value: Option<f64>) -> String {
    opt(value, |v| format!("{:.0}%", 100.0 * v))
}

/// The act's row: when, what the engine did, what it cost, what she did, what looks wrong.
fn row(act: &Act) -> String {
    let engine = act.engine.as_ref().map_or_else(
        // A slot with nothing cached logs no resume line.
        || if act.cached_tokens == Some(0) { "cold (no resume logged)".to_owned() } else { "-".to_owned() },
        |r| {
            let how = if r.full() { "full".to_owned() } else { format!("ck{}", r.checkpoints) };
            format!("lcp {}→{} {how}{}", r.lcp, r.past, if r.paged_in.is_some() { " paged" } else { "" })
        },
    ); // map_or_else: unjoined acts show `-`
    let prefill = match (act.prefill_tokens, act.prefill_ms, act.prefill_tps) {
        (Some(t), Some(ms), Some(tps)) => format!("{t}t {:.1}s {tps:.0}t/s", ms / 1_000.0),
        _ => "-".to_owned(),
    };
    let decode = match (act.decode_tokens, act.decode_ms, act.decode_tps) {
        (Some(t), Some(ms), Some(tps)) => format!("{t}t {:.1}s {tps:.1}t/s", ms / 1_000.0),
        _ => "-".to_owned(),
    };
    let input = act.input_tokens.map_or_else(|| format!("~{}", act.estimated_tokens), |t| t.to_string()); // map_or_else: unanswered acts show the estimate
    format!(
        "{} c{:<4} in {:>7} cached {:>7} | {:<26} | pre {:<24} | dec {:<20} | {:>6} | {:<16} | {:<20} | {:<6} {} | {}",
        clock::hms(act.at_ms),
        act.cycle.map_or_else(|| "-".to_owned(), |c| c.to_string()), // map_or_else: a row without a cycle
        input,
        act.cached_tokens.map_or_else(|| "-".to_owned(), |t| t.to_string()), // map_or_else: unanswered
        engine,
        prefill,
        decode,
        opt(act.elapsed_ms.map(|ms| ms as f64 / 1_000.0), |s| format!("{s:.0}s")),
        act.view.as_deref().unwrap_or("-"), // unwrap_or: no probe for this act
        act.divergence.as_ref().map_or_else(|| "first".to_owned(), Divergence::label), // map_or_else: her first act in the window
        format!("{:?}", act.did.kind).to_lowercase(),
        act.did.summary(40),
        if act.flags.is_empty() { "-".to_owned() } else { act.flags.join(",") },
    )
}

/// The act's prompt by layer, in tokens at its tokens per character.
fn layers(act: &Act) -> String {
    let parts: Vec<String> = Layer::ALL
        .iter()
        .filter(|layer| act.layers.chars.get(layer).is_some_and(|c| *c > 0))
        .map(|layer| format!("{} {}", layer.short(), k(act.layer_tokens(*layer))))
        .collect();
    format!(
        "    layers: {} | dup {} reread {} older-whole {}",
        parts.join(" "),
        k(act.tokens(act.layers.duplicate_chars)),
        k(act.tokens(act.layers.duplicate_read_chars)),
        k(act.tokens(act.layers.older_whole_chars)),
    )
}

const SUMMARY_HEADER: &str = "| resident | acts | done/failed/cancelled | median s/act | median s between | prefill share | decode share | prefill t/s | decode t/s | median in tok | reuse | re-prefill per appended | cache-loss share (acts) | page-ins | full | view breaks | head changes | refusals | repeats | writes | reads | shell | board | talk | tool calls/h |";

fn summary_row(s: &Summary) -> String {
    let kind = |k: Kind| s.kinds.get(&format!("{k:?}").to_lowercase()).copied().unwrap_or(0); // unwrap_or: a kind she never did
    format!(
        "| {} | {} | {}/{}/{} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} ({}) | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        s.resident,
        s.acts,
        s.completed,
        s.failed,
        s.cancelled,
        opt(s.median_act_s, |v| format!("{v:.0}")),
        opt(s.median_gap_s, |v| format!("{v:.0}")),
        pct(s.prefill_share),
        pct(s.decode_share),
        opt(s.median_prefill_tps, |v| format!("{v:.0}")),
        opt(s.median_decode_tps, |v| format!("{v:.1}")),
        opt(s.median_input_tokens, k),
        pct(s.reuse),
        opt(s.reprefill_per_appended, |v| format!("{v:.1}:1")),
        pct(s.cache_loss_share),
        s.cache_loss_acts,
        s.page_ins,
        s.full_reprefills,
        s.view_breaks,
        s.head_changes,
        s.refusals,
        s.repeats,
        kind(Kind::Write),
        kind(Kind::Read),
        kind(Kind::Shell),
        kind(Kind::Board),
        kind(Kind::Talk),
        opt(s.tool_calls_per_hour, |v| format!("{v:.0}")),
    )
}

fn by_resident(acts: &[Act]) -> BTreeMap<&str, Vec<&Act>> {
    let mut out: BTreeMap<&str, Vec<&Act>> = BTreeMap::new();
    for act in acts {
        out.entry(act.resident.as_str()).or_default().push(act);
    }
    out
}

fn summaries(acts: &[Act]) -> Vec<Summary> {
    by_resident(acts).iter().map(|(name, acts)| summarize(name, acts)).collect()
}

pub fn print(label: &str, acts: &[Act], compare_ms: Option<u64>, summary_only: bool, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(split) = compare_ms {
        let (before, after): (Vec<Act>, Vec<Act>) = acts.iter().cloned().partition(|act| act.at_ms < split);
        let (before, after) = (summaries(&before), summaries(&after));
        if json {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({"label": label, "split": clock::iso(split), "before": before, "after": after}))?);
            return Ok(());
        }
        println!("{label}; compared before and after {}\n\n{SUMMARY_HEADER}", clock::iso(split));
        println!("{}", "|---".repeat(SUMMARY_HEADER.matches(" |").count()) + "|");
        let names: std::collections::BTreeSet<&str> = before.iter().chain(&after).map(|s| s.resident.as_str()).collect();
        for name in names {
            for (when, side) in [("before", &before), ("after", &after)] {
                if let Some(s) = side.iter().find(|s| s.resident == name) {
                    println!("{}", summary_row(s).replacen(&format!("| {name} |"), &format!("| {name} {when} |"), 1));
                }
            }
        }
        return Ok(());
    }
    let summaries = summaries(acts);
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"label": label, "acts": acts, "summary": summaries}))?);
        return Ok(());
    }
    println!("{label}");
    if !summary_only {
        for (name, acts) in by_resident(acts) {
            println!("\n### {name} ({} acts)\ntime     cycle  in tokens  cached | engine resume (matched→resumed) | prefill | decode | act | her view | divergence from her last | did | flags", acts.len());
            for act in acts {
                println!("{}\n{}", row(act), layers(act));
            }
        }
        println!();
    }
    println!("{SUMMARY_HEADER}");
    println!("{}", "|---".repeat(SUMMARY_HEADER.matches(" |").count()) + "|");
    for s in &summaries {
        println!("{}", summary_row(s));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acts::did::Did;
    use crate::acts::engine::Resume;
    use crate::acts::layers::Breakdown;

    fn act(at_ms: u64, elapsed_ms: u64, lost: (i64, u64, u64), repeat: bool) -> Act {
        let (lcp, past, total) = lost;
        Act {
            resident: "Testa".into(),
            persona_id: "0000aaaa".into(),
            request_id: String::new(),
            at_ms,
            at: String::new(),
            cycle: None,
            build_sha: None,
            purpose: String::new(),
            context_window: None,
            outcome: Outcome::Completed,
            refusal: None,
            elapsed_ms: Some(elapsed_ms),
            input_tokens: Some(total),
            estimated_tokens: total,
            cached_tokens: Some(past),
            prefill_tokens: Some(total - past),
            prefill_ms: Some(elapsed_ms as f64 / 2.0),
            prefill_tps: None,
            decode_tokens: None,
            decode_ms: None,
            decode_tps: None,
            tokens_per_char: 0.25,
            layers: Breakdown::default(),
            divergence: None,
            engine: Some(Resume { process: 0, slot: 0, task: 0, what: "restored a checkpoint".into(), lcp, slot_tokens: 0, checkpoints: 1, past, total, paged_in: None }),
            view: None,
            did: Did::default(),
            repeat,
            flags: if repeat { vec!["repeat".into()] } else { Vec::new() },
        }
    }

    // what this catches: a summary that counts the cache loss over the wrong denominator (the
    // whole prompt instead of what was prefilled), a prefill share over the wrong time, or a
    // median taken over the unsorted values.
    #[test]
    fn a_summary_states_its_shares_over_what_was_prefilled() {
        let acts = [act(0, 60_000, (900, 100, 1_000), false), act(90_000, 20_000, (1_000, 1_000, 1_100), true), act(120_000, 40_000, (0, 0, 500), false)];
        let refs: Vec<&Act> = acts.iter().collect();
        let s = summarize("Testa", &refs);
        assert_eq!(s.median_act_s, Some(40.0));
        assert_eq!(s.median_gap_s, Some(90.0));
        assert_eq!(s.prefill_share, Some(0.5));
        // Lost 800 of the 900 + 100 + 500 tokens prefilled.
        assert_eq!(s.cache_loss_share, Some(800.0 / 1_500.0));
        assert_eq!(s.repeats, 1);
        assert_eq!(s.reuse, Some(1_100.0 / 2_600.0));
        // The second act grew her prompt by 100 and prefilled 100 (1:1); the third shrank
        // it (a turnover), so it has no append to measure against.
        assert_eq!(s.reprefill_per_appended, Some(1.0));
    }
}
