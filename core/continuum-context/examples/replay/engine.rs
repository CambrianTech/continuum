//! What the engine keeps, and what her own later acts supersede, read back from the captures
//! (`--engine <prefill tok/s>`).
//!
//! The other modes price a request at the characters it does not share with the request before
//! it: an engine that never loses her slot. On the M5, 2026-10-26, about half of the residents'
//! acts reused only the shared system head, and those acts prefilled their whole prompt. Each
//! captured request's own receipt says how much of what it shared with the request before it the
//! engine kept (`cachedTokens`); a policy's request is priced at its own fresh end plus the share
//! of its carried prefix that receipt says the engine lost. The loss pattern is taken as given
//! (other minds' prefill evicts her slot): a policy whose prompts are smaller is credited only
//! with what it re-prefills less, never with evicting less, so the comparison is conservative.
//!
//! It also replays the start decision in `continuum_context::view` that an act read again or a
//! file changed since takes its summary at a start (`Source::superseded`, as the core's
//! `ActionLog::superseded` decides from the typed acts; here from the rendered call).

use std::collections::BTreeMap;

use continuum_context::source::Item;

use super::{mean, median, replay, Cut, Order, Policy, Rule};

/// The share of what a captured request shared with the request before it (as sent) that the
/// engine reused: its `cachedTokens` over that shared prefix in tokens, the head included. A
/// request the run itself started anew shares about the head only, and is read as kept.
pub fn kept(previous_sent: &str, sent: &str, cached_tokens: u64, chars_per_token: f64) -> f64 {
    let shared = super::shared(previous_sent, sent) as f64 / chars_per_token;
    if shared <= 0.0 {
        return 1.0;
    }
    (cached_tokens as f64 / shared).min(1.0)
}

/// What one rendered act did to a file: the core's `Touch`, read from the call it renders.
#[derive(Debug, PartialEq)]
enum Touch {
    Read { path: String, lines: (u32, u32) },
    Changed { path: String },
}

/// `key=value` arguments of the rendered call `name(...)` in `text`.
fn args<'a>(text: &'a str, name: &str) -> Option<Vec<(&'a str, &'a str)>> {
    let start = text.find(&format!("{name}("))? + name.len() + 1;
    let end = start + text[start..].find(')')?;
    Some(text[start..end].split(", ").filter_map(|pair| pair.split_once('=')).collect())
}

/// The workspace label of a rendered result (`workspace 20b44763]`), its scope.
fn workspace(text: &str) -> Option<&str> {
    let header = text.split_once(']')?.0;
    let at = header.find("workspace ")? + "workspace ".len();
    Some(header[at..].split([' ', ';']).next().unwrap_or_default()) // unwrap_or_default: split yields at least one piece
}

/// The touch of an act whose operation is one file read or change; `None` for anything else
/// (a batch, a search, a shell), which never supersedes and is never superseded.
fn touch(text: &str) -> Option<Touch> {
    let operation = text.split("operation ").nth(1)?.split(';').next()?;
    let path_of = |args: &[(&str, &str)]| args.iter().find(|(k, _)| *k == "file_path").map(|(_, v)| (*v).to_owned());
    match operation {
        "code/read" => {
            let args = args(text, "code/read")?;
            let line = |key: &str, unset: u32| args.iter().find(|(k, _)| *k == key).and_then(|(_, v)| v.parse().ok()).unwrap_or(unset); // unwrap_or: an unranged read is the whole file, as the read's header reports it
            Some(Touch::Read { path: path_of(&args)?, lines: (line("start_line", 1), line("end_line", u32::MAX)) })
        }
        "code/edit" | "code/write" => Some(Touch::Changed { path: path_of(&args(text, operation)?)? }),
        _ => None,
    }
}

/// Keys of the action items a later item supersedes: a read of the same file in the same
/// workspace showing at least the same lines, or a change of it.
pub fn superseded(items: &[Item<String>]) -> Vec<String> {
    let acts: Vec<(&str, Option<&str>, Touch)> =
        items.iter().filter_map(|item| Some((item.key.as_str(), workspace(&item.full), touch(&item.full)?))).collect();
    acts.iter()
        .enumerate()
        .filter(|(at, (_, scope, touch))| match touch {
            Touch::Read { path, lines } => acts[at + 1..].iter().any(|(_, later_scope, later)| {
                later_scope == scope
                    && match later {
                        Touch::Changed { path: changed } => changed == path,
                        Touch::Read { path: read, lines: shown } => read == path && shown.0 <= lines.0 && lines.1 <= shown.1,
                    }
            }),
            Touch::Changed { .. } => false,
        })
        .map(|(_, (key, _, _))| (*key).to_owned())
        .collect()
}

/// Each request's receipt: the tokens the engine counted and the tokens it reused.
pub type Receipts = BTreeMap<String, (u64, u64)>;

/// Stamp each cut with the share of its carried prefix its receipt says the engine kept.
pub fn stamp(cuts: &mut [Cut], rows: &[serde_json::Value], receipts: &Receipts, chars_per_token: f64) {
    for n in 1..cuts.len() {
        let cached = rows[n]["request_id"].as_str().and_then(|id| receipts.get(id)).map(|(_, cached)| *cached);
        cuts[n].kept = cached.map(|cached| kept(&cuts[n - 1].sent, &cuts[n].sent, cached, chars_per_token));
    }
}

/// One `TABLE` line per policy: the run as it was sent (its receipts), then the core's rule
/// today, then each change added, priced at the engine's measured keep and `tps`.
#[allow(clippy::too_many_arguments)] // the replay's knobs, each a flag of the CLI
pub fn compare(
    label: &str,
    resident: &str,
    (by_name, by_room): (&[Cut], &[Cut]),
    rows: &[serde_json::Value],
    receipts: &Receipts,
    window: usize,
    share: usize,
    (chars_per_token, tps): (f64, f64),
) -> Result<(), Box<dyn std::error::Error>> {
    let billed: Vec<(u64, u64)> = rows.iter().filter_map(|row| receipts.get(row["request_id"].as_str()?).copied()).collect();
    let fresh: f64 = billed.iter().map(|(prompt, cached)| prompt.saturating_sub(*cached) as f64).sum();
    let prompts: Vec<f64> = billed.iter().map(|(prompt, _)| *prompt as f64).collect();
    println!(
        "TABLE {resident} | as sent (receipts) | prefill {fresh:.0} tok | {:.0} s/request | prompt mean {:.0} tok, median {:.0} | {} requests with receipts",
        fresh / tps / billed.len().max(1) as f64,
        mean(&prompts),
        median(prompts.clone()),
        billed.len(),
    );
    let variants: [(&str, &[Cut], Policy); 3] = [
        ("core rule today", by_name, Policy::default()),
        ("+ room keys", by_room, Policy::default()),
        ("+ room keys + superseded", by_room, Policy { superseded: true }),
    ];
    for (variant, cuts, policy) in variants {
        let o = replay(label, cuts, window, (Rule::Reserved, Order::SettledFirst), false, true, false, share, policy)?;
        let tokens = |chars: &[f64]| chars.iter().sum::<f64>() / chars_per_token;
        let prompt: Vec<f64> = o.prompt_chars.iter().map(|chars| chars / chars_per_token).collect();
        println!(
            "TABLE {resident} | {variant} | prefill {:.0} tok | {:.0} s/request | prompt mean {:.0} tok, median {:.0} | {} turnovers | {} summarized, {} collapsed at starts | re-reads {} held {} | prefill if the engine kept every slot {:.0} tok",
            tokens(&o.engine_prefill),
            tokens(&o.engine_prefill) / tps / o.engine_prefill.len().max(1) as f64,
            mean(&prompt),
            median(prompt.clone()),
            o.turnovers,
            o.summarized,
            o.collapsed,
            o.rereads,
            o.retained,
            tokens(&o.prefill_chars),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(n: u32, call: &str) -> Item<String> {
        let operation = call.split('(').next().unwrap_or_default(); // unwrap_or_default: split yields at least one piece
        let full = format!("[result #{n}; room r; operation {operation}; output handle: tool/output h{n}; workspace w] {call} because x");
        Item { key: n.to_string(), pointer: None, summary: None, full }
    }

    // what this catches: the replay's superseded rule disagreeing with the core's (a partial
    // re-read counted as covering, an edit missed, or an edit's own result summarized).
    #[test]
    fn a_read_is_superseded_by_a_covering_read_or_a_change() {
        let items = vec![
            act(1, "code/read(file_path=a.rs)"),
            act(2, "code/read(file_path=b.rs)"),
            act(3, "code/read(end_line=200, file_path=a.rs, start_line=75)"),
            act(4, "code/edit(file_path=b.rs, old=x)"),
            act(5, "code/read(file_path=a.rs)"),
        ];
        assert_eq!(superseded(&items), ["1", "2", "3"]);
    }

    // what this catches: a cold act (only the head reused) read as warm, or a run's own start
    // read as the engine losing her slot.
    #[test]
    fn kept_is_the_share_of_the_shared_prefix_the_engine_reused() {
        let before = "h".repeat(100) + &"v".repeat(900);
        let after = before.clone() + "new";
        assert!((kept(&before, &after, 100, 1.0) - 0.1).abs() < 1e-9);
        assert_eq!(kept(&before, &after, 1_000, 1.0), 1.0);
        assert_eq!(kept("x", "y", 0, 1.0), 1.0, "nothing shared, nothing to keep");
    }
}
