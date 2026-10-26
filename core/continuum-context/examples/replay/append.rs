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
use continuum_context::view::View;

use super::{median, order, shared, start, validate, Captured, Cut, Order, Rule};

/// The source the act loop's pieces arrive through: the bytes she was sent, whole. No
/// pointer, so admission never reduces them (a reduced piece would not be the bytes sent).
const LOOP: &str = "act-loop";

/// One request under both prompts.
struct Row {
    now: usize,
    append: usize,
    view: usize,
    tail: usize,
    turnover: bool,
}

/// The tail's pieces not yet in this epoch's view, each once, in the order they were sent.
fn unsent(tail: &[String], held: &HashSet<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tail.iter().filter(|piece| !held.contains(*piece) && seen.insert(piece.as_str())).cloned().collect()
}

fn chars(pieces: &[String]) -> usize {
    pieces.iter().map(|piece| piece.chars().count()).sum::<usize>() + pieces.len().saturating_sub(1)
}

/// Replay `cuts` with the act loop appended; print each request and the totals.
pub fn replay(label: &str, cuts: &[Cut], window: usize, share: usize, chars_per_token: f64, tps: f64) -> Result<(), Box<dyn std::error::Error>> {
    let mut restored = validate::Restored::default();
    let mut view: Option<View<String>> = None;
    let mut looped: Vec<Item<String>> = Vec::new();
    let mut held: HashSet<String> = HashSet::new();
    let (mut previous_head, mut previous_sent, mut previous_rendered): (Option<&str>, Option<&str>, Option<String>) = (None, None, None);
    let mut rows = Vec::with_capacity(cuts.len());
    for cut in cuts {
        let action_keys: Vec<String> = cut.sources.get("actions").into_iter().flatten().map(|item| item.key.clone()).collect();
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
            view = None;
        }
        previous_head = Some(&cut.head);
        if view.is_none() {
            looped.clear();
            held.clear();
        }
        let head_chars = cut.head.chars().count();
        let mut tail = unsent(&cut.tail, &held);
        let mut available = window.saturating_sub(head_chars + chars(&tail));
        let every: Vec<&dyn Source<String>> = sources.iter().map(|s| s as &dyn Source<String>).collect();
        let mut turnover = view.is_none();
        if turnover {
            let whole = |claims: &[continuum_context::view::Claim]| claims.iter().map(|claim| claim.full_chars).collect();
            view = Some(View::start(vec![cut.head.clone()], &every, whole)?.0);
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
                open.readmit(&refs, limit, limit / 2);
            }
        }
        if open.chars() > limit {
            // A start renders from the sources: the loop is dropped, the action log takes it
            // back, and this request's tail is sent whole again.
            tail = unsent(&cut.tail, &HashSet::new());
            available = window.saturating_sub(head_chars + chars(&tail));
            let per_act = open.appended_per_act();
            open.turn_over(vec![cut.head.clone()], &every, |claims| start(Rule::Reserved, claims, available, per_act, share))?;
            turnover = true;
        }
        if turnover {
            looped.clear();
            held.clear();
        }
        let rendered = format!("{}\n{}", open.render().cloned().collect::<Vec<_>>().join("\n"), tail.join("\n"));
        let rendered_chars = rendered.chars().count();
        let sent_chars = cut.sent.chars().count();
        rows.push(Row {
            now: sent_chars - previous_sent.map_or(0, |before| shared(before, &cut.sent)), // map_or: the first request is fresh whole
            append: rendered_chars - previous_rendered.as_deref().map_or(0, |before| shared(before, &rendered)), // map_or: the first request is fresh whole
            view: open.chars(),
            tail: chars(&tail),
            turnover,
        });
        open.sent();
        for piece in tail {
            held.insert(piece.clone());
            looped.push(Item { key: looped.len().to_string(), pointer: None, summary: None, full: piece });
        }
        previous_sent = Some(&cut.sent);
        previous_rendered = Some(rendered);
    }
    print(label, &rows, window, chars_per_token, tps);
    Ok(())
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
