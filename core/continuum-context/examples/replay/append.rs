//! Append-tail mode (`--append-tail <prefill tok/s>`): what a resident's prefill would be if
//! every request were the previous request's bytes plus only what is new (plan line S1).
//!
//! Today each act re-renders her own loop: the tool call and its result she was sent at the
//! end of one request come back in the next as a `[result #n ...]` item of the action log,
//! in other bytes, and everything after the first difference is prefilled again. Here the
//! fresh tail of each request (her act's loop, the live act, whatever the cut left unkeyed)
//! becomes the next request's view items in the exact bytes it was sent, appended after the
//! view; the action log is not re-rendered within an epoch. A tail piece the epoch already
//! holds (the standing turn text, a room message seen before) is not sent again. A turnover
//! happens only when the view plus this request's tail exceeds what the window leaves the
//! view, and starts the way the core's `start_grants` does (`Rule::Reserved`, the settled
//! blocks first, admission on): the action log renders from its source at the new start and
//! the appended loop is dropped.
//!
//! Fresh is measured per request as the characters after the prefix it shares with the
//! request before it (the first request is fresh whole), priced in tokens at the engine's
//! billed chars per token and in seconds at the measured prefill rate.

use std::collections::HashSet;

use continuum_context::source::{Item, Source};
use continuum_context::view::{claims, View};

use super::outline::{self, Mode, Paging};
use super::paged::{self, Pager};
use super::sediment::{self, Sediment};
use super::{growable, median, order, shared, start, validate, Captured, Cut, Order, Rule};

/// The source the act loop's pieces arrive through: the bytes she was sent, whole (or, under
/// `--outline-results`, at the outline the act's rule chose). No pointer, so admission never
/// reduces them further.
const LOOP: &str = "act-loop";

/// What one append-tail run models beyond the bytes she was sent.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub outline: Mode,
    pub generational: bool,
    /// Unranged reads answer with their first page (`replay/paged.rs`).
    pub paged: bool,
    /// Turnovers start at her measured collapse depth (`Rule::Measured`).
    pub measured: bool,
}

/// One request under both prompts.
struct Row {
    now: usize,
    append: usize,
    view: usize,
    tail: usize,
    turnover: bool,
}

/// One run's totals, in tokens, for the comparison table.
pub struct Summary {
    pub requests: usize,
    pub now_tokens: f64,
    pub tokens: f64,
    pub turnovers: usize,
    /// Fresh tokens of each request that turned over.
    pub turnover_tokens: Vec<f64>,
    pub sediment_rerenders: usize,
    pub sediment_dropped: usize,
    pub outlined: usize,
    pub paged: usize,
    /// The per-act headroom each oversized-result decision read, in characters.
    pub headroom: Vec<f64>,
    /// Reads answered with a first page, how many of those held every line she later used, and
    /// the extra requests that appended a later page.
    pub paged_reads: usize,
    pub first_page_held: usize,
    pub used_later: usize,
    pub extra_requests: usize,
    /// Re-reads of what the starts collapsed, as the measured depth predicted them (its per-act
    /// rate over each act of the epoch it started), and re-requests the view counted: all, and of
    /// an occurrence held collapsed (`continuum_context::rereads`).
    pub predicted_rereads: (f64, f64),
    pub counted_rereads: (u64, u64),
    /// Starts the measured depth decided, of all starts it was asked about, and the growing start
    /// it chose beside the half's, summed over those it decided (chars).
    pub decided: (usize, usize),
    pub depth_chars: (usize, usize),
}

/// The tail's pieces not yet in this epoch's view, each once, in the order they were sent.
fn unsent(tail: &[String], held: &HashSet<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tail.iter().filter(|piece| !held.contains(*piece) && seen.insert(piece.as_str())).cloned().collect()
}

fn chars(pieces: &[String]) -> usize {
    pieces.iter().map(|piece| piece.chars().count()).sum::<usize>() + pieces.len().saturating_sub(1)
}

/// One tail piece as this request handles it: the piece as captured (the epoch's key for it),
/// its unranged reads at their first page, and its outline, if the rules gave them.
struct Shown {
    piece: String,
    paged: Option<paged::Paged>,
    outlined: Option<outline::Outlined>,
}

impl Shown {
    /// What her own act reads: the piece, its reads at their first page under `--paged-reads`.
    fn answered(&self) -> &str {
        self.paged.as_ref().map_or(&self.piece, |paged| &paged.piece) // map_or: an unpaged piece is answered as captured
    }
}

/// `pieces` as this request sends them: unranged reads past their share of `headroom` at their
/// first page (when `options.paged`), then, past `headroom`, results at their outline.
fn shown(pieces: Vec<String>, options: Options, headroom: Option<usize>, whole_reads: &HashSet<String>) -> Vec<Shown> {
    pieces
        .into_iter()
        .map(|piece| {
            let paged = headroom.filter(|_| options.paged).and_then(|headroom| paged::page(&piece, whole_reads, headroom));
            let mut shown = Shown { piece, paged, outlined: None };
            shown.outlined = match (options.outline, headroom) {
                (Mode::Whole, _) | (_, None) => None,
                (_, Some(headroom)) => outline::outline(shown.answered(), headroom),
            };
            shown
        })
        .collect()
}

/// What a request's tail sends: the outline under [`Mode::Strict`], else what the tool answered.
fn sent(tail: &[Shown], mode: Mode) -> Vec<String> {
    tail.iter()
        .map(|shown| match &shown.outlined {
            Some(outlined) if mode == Mode::Strict => outlined.piece.clone(),
            _ => shown.answered().to_owned(),
        })
        .collect()
}

/// Replay `cuts` with the act loop appended; print each request (when `table`) and the totals.
pub fn replay(label: &str, cuts: &[Cut], window: usize, share: usize, (chars_per_token, tps): (f64, f64), options: Options, table: bool) -> Result<Summary, Box<dyn std::error::Error>> {
    let mut restored = validate::Restored::default();
    let mut view: Option<View<String>> = None;
    let mut sediment = Sediment::default();
    let mut paging = Paging::default();
    let mut pager = Pager::default();
    let mut looped: Vec<Item<String>> = Vec::new();
    let mut held: HashSet<String> = HashSet::new();
    let mut carried_headroom: Option<usize> = None;
    let mut headrooms = Vec::new();
    let (mut previous_head, mut previous_sent, mut previous_rendered): (Option<&str>, Option<&str>, Option<String>) = (None, None, None);
    let (mut rate, mut predicted, mut counted) = ((0.0f64, 0.0f64), (0.0f64, 0.0f64), (0u64, 0u64));
    let (mut decided, mut depth_chars) = ((0usize, 0usize), (0usize, 0usize));
    let count = |view: &View<String>, counted: &mut (u64, u64)| {
        let (all, collapsed) = view.motion().rereads().totals();
        *counted = (counted.0 + all, counted.1 + collapsed);
    };
    let mut rows = Vec::with_capacity(cuts.len());
    for (n, cut) in cuts.iter().enumerate() {
        paging.said(n, &cut.said);
        // The pages her last act needed past a first page: one extra request each, appended.
        let extra_pages = if options.paged { pager.said(n, &cut.said, &cut.reads) } else { Vec::new() };
        let action_keys: Vec<String> = cut.sources.get("actions").into_iter().flatten().map(|item| item.key().to_owned()).collect();
        restored.learn(&action_keys, &cut.act);
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
        sources.sort_by_key(|source| order(Order::SettledFirst, &source.name));
        if previous_head.is_some_and(|head| head != cut.head) {
            if let Some(view) = &view {
                count(view, &mut counted);
            }
            view = None;
            sediment.restart();
        }
        previous_head = Some(&cut.head);
        if view.is_none() {
            looped.clear();
            held.clear();
        }
        let offered = sediment::offered(&sources);
        sediment.filter(&mut sources);
        // The per-act headroom: what one act of this epoch appends (median), carried over a start.
        let headroom = view.as_ref().and_then(View::appended_per_act).or(carried_headroom);
        carried_headroom = headroom;
        let mut tail = shown(unsent(&cut.tail, &held), options, headroom, &cut.reads.whole);
        let mut head_chars = cut.head.chars().count() + sediment.chars();
        let mut available = window.saturating_sub(head_chars + chars(&sent(&tail, options.outline)) + chars(&extra_pages));
        let mut turnover = view.is_none();
        if turnover {
            let every: Vec<&dyn Source<String>> = sources.iter().map(|s| s as &dyn Source<String>).collect();
            let whole = |claims: &[continuum_context::view::Claim]| claims.iter().map(|claim| claim.full).collect();
            view = Some(View::start(&continuum_context::cost::Chars, sediment.heads(&cut.head), &every, whole)?.0);
        }
        let Some(open) = view.as_mut() else { continue };
        let limit = head_chars + available;
        if !turnover {
            // Mid-epoch: her loop arrives as the bytes she was sent, first, then what changed in
            // every other source. The action log is not re-rendered.
            let act_loop = Captured { name: LOOP.to_owned(), items: looped.clone() };
            let mut refs: Vec<&dyn Source<String>> = vec![&act_loop];
            refs.extend(sources.iter().filter(|s| s.name != "actions").map(|s| s as &dyn Source<String>));
            open.advance(&refs);
            if open.chars() > limit {
                open.readmit(&continuum_context::cost::Chars, &refs, limit, limit / 2);
            }
        }
        if open.chars() > limit {
            // A start renders from the sources: the loop is dropped, the action log takes it
            // back, and this request's tail is sent whole again.
            tail = shown(unsent(&cut.tail, &HashSet::new()), options, headroom, &cut.reads.whole);
            available = window.saturating_sub(head_chars + chars(&sent(&tail, options.outline)) + chars(&extra_pages));
            let per_act = open.appended_per_act();
            if options.generational {
                // The young generation's oldest items settle into the sediment, at the grants
                // the start would give them; a sediment past its share re-renders.
                let (asked, grants) = {
                    let every: Vec<&dyn Source<String>> = sources.iter().map(|s| s as &dyn Source<String>).collect();
                    let asked = claims(&continuum_context::cost::Chars, &every);
                    let grants = start(Rule::Reserved, &asked, available, per_act, share);
                    (asked, grants)
                };
                let by_source = sediment::grants_by_source(&sources, &asked, &grants);
                sediment.settle(&mut sources, &by_source);
                let sediment_share = growable(&asked, available).1 / share;
                if sediment.overgrown(sediment_share) {
                    sediment.rerender(&offered, sediment_share);
                }
                head_chars = cut.head.chars().count() + sediment.chars();
                available = window.saturating_sub(head_chars + chars(&sent(&tail, options.outline)) + chars(&extra_pages));
            }
            let every: Vec<&dyn Source<String>> = sources.iter().map(|s| s as &dyn Source<String>).collect();
            let chosen = std::cell::Cell::new(None);
            open.turn_over(&continuum_context::cost::Chars, sediment.heads(&cut.head), &every, |claims| {
                if options.measured {
                    let (grants, depth) = crate::measured(claims, available, per_act, share);
                    chosen.set(Some(depth.map(|(depth, half)| ((depth.rereads_per_act, depth.rereads_observed_per_act), depth.start, half))));
                    grants
                } else {
                    start(Rule::Reserved, claims, available, per_act, share)
                }
            })?;
            rate = match chosen.take() {
                Some(Some((per_act, start, half))) => {
                    decided = (decided.0 + 1, decided.1 + 1);
                    depth_chars = (depth_chars.0 + start, depth_chars.1 + half);
                    per_act
                }
                Some(None) => {
                    decided.1 += 1;
                    (0.0, 0.0)
                }
                None => (0.0, 0.0),
            };
            turnover = true;
        }
        if turnover {
            looped.clear();
            held.clear();
            paging.turned_over();
        }
        predicted = (predicted.0 + rate.0, predicted.1 + rate.1);
        let mut sending = sent(&tail, options.outline);
        sending.extend(extra_pages.iter().cloned());
        let rendered = format!("{}\n{}", open.render().cloned().collect::<Vec<_>>().join("\n"), sending.join("\n"));
        let rendered_chars = rendered.chars().count();
        let sent_chars = cut.sent.chars().count();
        rows.push(Row {
            now: sent_chars - previous_sent.map_or(0, |before| shared(before, &cut.sent)), // map_or: the first request is fresh whole
            append: rendered_chars - previous_rendered.as_deref().map_or(0, |before| shared(before, &rendered)), // map_or: the first request is fresh whole
            view: open.chars(),
            tail: chars(&sending),
            turnover,
        });
        open.sent();
        // From the next act on, an outlined result stands at its outline in either mode; her act
        // on the next request is the first that read it whole (next-act) or never did (strict).
        // Her act's result arrives with what it asked for, as the core's action source stamps it,
        // so a later read with the same outcome is the same observation (`continuum_context::rereads`).
        // A tool_result message is captured as its JSON parts, so the result shows escaped in it.
        let escaped = cut.act.result.as_deref().map(|result| serde_json::to_string(result).unwrap_or_default().trim_matches('"').to_owned()); // unwrap_or_default: a string always serializes
        let identity = cut.act.identity().zip(cut.act.result.as_deref()).zip(escaped.as_deref());
        let mut stamped = false;
        for shown in tail {
            held.insert(shown.piece.clone());
            // Her act on the next request is the first that read the page.
            if let Some(paged) = &shown.paged {
                headrooms.push(headroom.unwrap_or_default() as f64); // unwrap_or_default: a page is only cut under a measured headroom
                paged.reads.iter().for_each(|read| pager.register(n + 1, read));
            }
            let full = match &shown.outlined {
                Some(outlined) => {
                    headrooms.push(headroom.unwrap_or_default() as f64); // unwrap_or_default: an outline is only chosen under a measured headroom
                    paging.outlined(if options.outline == Mode::Strict { n + 1 } else { n + 2 }, outlined);
                    outlined.piece.clone()
                }
                None => shown.answered().to_owned(),
            };
            let item = crate::captured(looped.len().to_string(), full);
            looped.push(match identity {
                Some(((act, result), escaped)) if !stamped && (shown.piece.contains(result) || shown.piece.contains(escaped)) => {
                    stamped = true;
                    let content = continuum_context::cause::Content::Hashed(continuum_context::cause::ContentHash::of(result));
                    Item::rendered(item.id(), item.cause(), item.key(), item.full().clone(), content).with_act(act)
                }
                _ => item,
            });
        }
        for full in extra_pages {
            looped.push(crate::captured(looped.len().to_string(), full));
        }
        previous_sent = Some(&cut.sent);
        previous_rendered = Some(rendered);
    }
    if let Some(view) = &view {
        count(view, &mut counted);
    }
    if table {
        print(label, &rows, window, chars_per_token, tps);
    }
    let tokens = |chars: usize| chars as f64 / chars_per_token;
    Ok(Summary {
        requests: rows.len(),
        now_tokens: rows.iter().map(|row| tokens(row.now)).sum(),
        tokens: rows.iter().map(|row| tokens(row.append)).sum(),
        turnovers: rows.iter().filter(|row| row.turnover).count(),
        turnover_tokens: rows.iter().filter(|row| row.turnover).map(|row| tokens(row.append)).collect(),
        sediment_rerenders: sediment.rerenders,
        sediment_dropped: sediment.dropped,
        outlined: paging.outlined,
        paged: paging.paged,
        headroom: headrooms,
        paged_reads: pager.totals().0,
        first_page_held: pager.totals().1,
        used_later: pager.totals().2,
        extra_requests: pager.extra,
        predicted_rereads: predicted,
        counted_rereads: counted,
        decided,
        depth_chars,
    })
}

/// One line of the comparison table: `variant`'s prefill, pace and turnovers.
pub fn line(resident: &str, variant: &str, summary: &Summary, tps: f64) -> String {
    let per_request = summary.tokens / tps / summary.requests.max(1) as f64;
    let per_turnover = summary.turnover_tokens.iter().sum::<f64>() / summary.turnover_tokens.len().max(1) as f64;
    let with_extras = summary.tokens / tps / (summary.requests + summary.extra_requests).max(1) as f64;
    format!(
        "TABLE {resident} | {variant} | prefill {:.0} tok | {per_request:.0} s/request ({with_extras:.0} counting extra requests) | {} turnovers | re-prefill {per_turnover:.0} tok/turnover (median {:.0}) | re-reads predicted {:.2} (as observed {:.2}), counted {} (of a collapsed one {}) | depth decided {} of {} starts, growing start {} chars vs the half's {} | sediment re-renders {} (handles dropped {}) | outlined {}, paged back {} | headroom median {:.0} chars | paged reads {} (first page held every used line in {}; {} used later at all) | extra page requests {}",
        summary.tokens,
        summary.turnovers,
        median(summary.turnover_tokens.clone()),
        summary.predicted_rereads.0,
        summary.predicted_rereads.1,
        summary.counted_rereads.0,
        summary.counted_rereads.1,
        summary.decided.0,
        summary.decided.1,
        summary.depth_chars.0,
        summary.depth_chars.1,
        summary.sediment_rerenders,
        summary.sediment_dropped,
        summary.outlined,
        summary.paged,
        median(summary.headroom.clone()),
        summary.paged_reads,
        summary.first_page_held,
        summary.used_later,
        summary.extra_requests,
    )
}

fn print(label: &str, rows: &[Row], window: usize, chars_per_token: f64, tps: f64) {
    let tokens = |chars: usize| chars as f64 / chars_per_token;
    println!("APPEND-TAIL {label}\n  view allowance {window} chars; per request, tokens: fresh now | fresh append-tail | view | tail | turnover");
    for (n, row) in rows.iter().enumerate() {
        println!(
            "  {n:>4} {:>8.0} {:>8.0} {:>8.0} {:>7.0}{}",
            tokens(row.now),
            tokens(row.append),
            tokens(row.view),
            tokens(row.tail),
            if row.turnover { "  T" } else { "" }
        );
    }
    let now: Vec<f64> = rows.iter().map(|row| tokens(row.now)).collect();
    let append: Vec<f64> = rows.iter().map(|row| tokens(row.append)).collect();
    let turnovers = rows.iter().filter(|row| row.turnover).count();
    let (now_total, append_total) = (now.iter().sum::<f64>(), append.iter().sum::<f64>());
    println!(
        "APPEND-TAIL TOTAL {} requests at {chars_per_token:.2} chars/token, {tps} tok/s prefill: fresh now total {now_total:.0} tokens (median {:.0}, {:.0} s/request), append-tail total {append_total:.0} tokens (median {:.0}, {:.0} s/request), {turnovers} turnovers (the first start included)",
        rows.len(),
        median(now.clone()),
        now_total / tps / rows.len().max(1) as f64,
        median(append.clone()),
        append_total / tps / rows.len().max(1) as f64,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a standing piece re-sent within an epoch, or one piece sent twice in
    // one tail, would count as fresh prefill the append-only prompt never sends.
    #[test]
    fn a_tail_piece_the_epoch_holds_is_not_sent_again() {
        let held: HashSet<String> = ["[work turn] x".to_owned()].into();
        let tail = vec!["[work turn] x".to_owned(), "result".to_owned(), "result".to_owned()];
        assert_eq!(unsent(&tail, &held), ["result"]);
        assert_eq!(chars(&unsent(&tail, &HashSet::new())), "[work turn] x\nresult".chars().count());
    }
}
