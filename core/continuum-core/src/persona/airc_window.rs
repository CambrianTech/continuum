//! The WINDOW packer for [`AircRagSource`]: how a channel digest becomes the budget-packed
//! items a persona reads. Split out of `airc_source.rs` (the source: paging, hydration, cursor)
//! by owner: this file owns the packing (share, anchor, receipt collapse) and nothing about I/O.

use std::sync::Arc;

use crate::cognition::channel_digest::ChannelDigest;
use crate::cognition::channel_element::ChannelElement;
use crate::cognition::token_budget::{
    estimate_prompt_tokens as estimate_tokens, fair_share_cap, fit_tail_with_handle,
};
use crate::persona::airc_source::{AircRagSource, EXPAND_COMMAND};
use crate::persona::rag_budget::RagItem;

/// The oldest kept line of a room's window, as the event it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HistoryAnchor {
    pub event: uuid::Uuid,
}

/// The share of the window budget left free when the start jumps, so the next turns'
/// new events fit without moving it again. A quarter: the start moves once per ~quarter
/// window of new traffic, and every turn in between extends the cached prefix instead
/// of replacing it. A larger slack buys fewer jumps at the cost of history shown.
pub(crate) const HISTORY_SLACK: f32 = 0.25;

impl AircRagSource {
    /// Format a digest into budget-packed `RagItem`s. Walks the window newest-first
    /// accumulating tokens until budget, then emits chronological (oldest-first) so
    /// the chat template reads turns in order. Each item is tagged `unread` vs
    /// grounding so the prompt builder / glass box can tell them apart.
    ///
    /// BREADTH OVER VERBATIM DEPTH (#128/#146, measured 2026-07-13): the original
    /// packer kept WHOLE messages newest-first, so a small budget in a verbose
    /// four-persona room held ~2-3 turns TOTAL — the persona's entire perceivable
    /// world. Any low-frequency speaker (the operator) was displaced within
    /// seconds; the room degenerated into parallel monologues that only rapid-fire
    /// exchange survived. A conversation is legible from turn HEADS; it is not
    /// legible from two verbatim essays. So each turn now costs at most a
    /// per-turn cap (a budget fraction, never a hardcoded model tier) and long
    /// turns keep their END behind an explicit marker — the same
    /// straddling-trim law the prompt fitter applies to messages, one level down.
    /// The NEWEST turn is exempt (kept verbatim up to the whole budget): it is
    /// what the persona is responding to.
    /// Returns the packed items, the tokens they cost, and the READ-THROUGH
    /// ELEMENT: the newest element that actually entered her prompt. That last
    /// value is what advances her cursor — she is marked read for what she was
    /// GIVEN, never for what was merely fetched and then dropped by the budget.
    ///
    /// The ELEMENT, not a lamport, because airc's `save_runtime_cursor_for_event`
    /// wants the source event (it carries the room + kind and emits
    /// `SubscriptionAdvanced`). Handing it a bare number would throw that away.
    pub(super) fn pack_digest(
        digest: &ChannelDigest,
        budget: u32,
        working: bool,
        anchor: Option<HistoryAnchor>,
    ) -> (Vec<RagItem>, u32, Option<Arc<ChannelElement>>, Option<HistoryAnchor>) {
        let (units, quarantined) = Self::collapse_work_receipts(digest, working);
        if quarantined > 0 {
            crate::probe!(
                class = "perception.quarantined",
                lines = quarantined as u64,
                working = working,
                "transcript lines the speak gate would refuse were kept out of her burst — the contagion carrier (card 169eb543)"
            );
        }
        // Every unit is a WHOLE message with its measured cost (oldest-first, parallel to
        // `units`; None = a unit with no text). The window below decides how many fit.
        let texts: Vec<Option<&str>> = units
            .iter()
            .map(|unit| match unit.collapsed.as_deref() {
                Some(t) => Some(t),
                None => digest.elements[unit.last_idx].text(),
            })
            .collect();
        let fulls: Vec<Option<u32>> = texts.iter().map(|t| t.map(estimate_tokens)).collect();
        // THE SHARE. The newest turn is what she is answering: it is never collapsed. What
        // the budget has left after it is shared max-min among the earlier messages
        // (`fair_share_cap`): a message that fits its share is delivered whole, and only
        // one bigger than the share collapses to its END behind a marker that names the verb for the whole
        // text. The share is derived from the measured costs and the budget; there is no
        // per-turn constant, so a large window shows essays whole and a small one still
        // holds many turns (breadth over verbatim depth, #128/#146, now by share).
        let newest = fulls.iter().rposition(Option::is_some);
        let share = newest.and_then(|n| {
            let left = budget.saturating_sub(fulls[n].unwrap_or(0));
            let earlier: Vec<u32> = fulls[..n].iter().flatten().copied().collect();
            if left == 0 { None } else { fair_share_cap(&earlier, left) } // nothing left after the newest: the window is the newest alone, nothing to collapse
        });
        let costed: Vec<Option<(u32, Option<String>)>> = units
            .iter()
            .zip(texts.iter().zip(&fulls))
            .enumerate()
            .map(|(i, (unit, (text, full)))| {
                let (text, full) = ((*text)?, (*full)?);
                Some(match share {
                    Some(cap) if full > cap && Some(i) != newest => {
                        let id = digest.elements[unit.last_idx].event_id();
                        let handle = format!("`{EXPAND_COMMAND}` shows it whole, id {id}");
                        // the END is kept: a message's outcome, answer or question sits there
                        match fit_tail_with_handle(text, cap, "message", Some(&handle)) {
                            Some(shown) => (estimate_tokens(&shown), Some(shown)),
                            // the share cannot carry the marker plus any of the end: collapsing
                            // would deliver a handle and no content, so the message stays whole
                            // and the window omits the OLDEST instead
                            None => (full, unit.collapsed.clone()),
                        }
                    }
                    _ => (full, unit.collapsed.clone()),
                })
            })
            .collect();
        // THE START HOLDS STILL. The window is the suffix of units from `start`; its
        // cost is the sum from there to the newest. Where the fill would naturally
        // begin (newest-first to the budget) is `natural`. The anchor from last turn
        // is kept while the suffix from it still fits; when it no longer fits, the
        // start jumps FORWARD past `natural` to leave HISTORY_SLACK of the budget
        // free, so the next turns' new events fit without moving it again. Every
        // turn in between then extends the prefix the engine already holds, instead
        // of replacing its first message. The first fill in a room (no anchor yet)
        // shows everything the budget holds; the room's second turn then pays one
        // jump and holds from there. An anchored event that scrolled out of the
        // digest is a jump too.
        let suffix_cost = |start: usize| -> u32 {
            costed[start..].iter().flatten().map(|(c, _)| *c).fold(0u32, |a, c| a.saturating_add(c))
        };
        let first_start_within = |limit: u32| -> usize {
            // the smallest start whose suffix fits `limit`; units.len() = nothing fits
            (0..=units.len()).find(|&start| suffix_cost(start) <= limit).unwrap_or(units.len()) // unwrap_or: the empty suffix always fits, so this is never reached
        };
        let natural = first_start_within(budget);
        // The anchor is the event at the unit's ORDER position (a run's first receipt),
        // never its newest: keyed on the newest, an open run at the window's start would
        // look scrolled out on every receipt and jump each turn (Cormac on #4806).
        let anchored = anchor.and_then(|a| {
            units.iter().position(|u| digest.elements[u.order_idx].event_id() == a.event)
        });
        let slack_limit = ((budget as f32) * (1.0 - HISTORY_SLACK)) as u32;
        let start = match (anchor, anchored) {
            // the first fill in this room: everything the budget holds
            (None, _) => natural,
            // last turn's start still fits: hold it, even if more would fit now
            (Some(_), Some(a)) if a >= natural => a,
            // it no longer fits, or it scrolled out: jump forward with headroom; never
            // past the newest unit, which is always shown
            _ => first_start_within(slack_limit).min(units.len().saturating_sub(1)),
        };
        let mut keep: Vec<(usize, Option<String>)> = Vec::new();
        let mut tokens_used: u32 = 0;
        for (unit, c) in units[start..].iter().zip(&costed[start..]) {
            if let Some((cost, trimmed)) = c {
                tokens_used = tokens_used.saturating_add(*cost);
                keep.push((unit.last_idx, trimmed.clone()));
            }
        }
        let new_anchor = units.get(start).map(|u| HistoryAnchor { event: digest.elements[u.order_idx].event_id() });
        if anchored.is_some_and(|a| a != start) || (anchored.is_none() && anchor.is_some()) {
            crate::probe!(
                class = "perception.history.anchor_moved",
                room = %digest.room_id,
                from_natural = natural as u64,
                start = start as u64,
                units = units.len() as u64,
                tokens = tokens_used as u64,
                budget = budget as u64,
                "the start of her history in this room jumped: the prefix the engine held is replaced once, then holds again"
            );
        }
        // `keep` is oldest-first after the reverse, so its LAST entry is the newest
        // element that actually fit — how far she genuinely read this turn.
        // the newest element that entered her prompt, by the elements' own order (a run's
        // line sits at its first receipt but reads through its newest)
        let read_through = keep.iter().map(|(idx, _)| *idx).max().map(|idx| digest.elements[idx].clone());
        let items = keep
            .into_iter()
            .map(|(idx, trimmed)| {
                Self::format_item(&digest.elements[idx], idx >= digest.unread_start, trimmed)
            })
            .collect();
        (items, tokens_used, read_through, new_anchor)
    }

    /// COLLAPSE, DON'T CLIP — work receipts. A working citizen radiates one
    /// `💭 thought` + `⚙ verb ✓/✗` receipt per act batch into the room (so
    /// roommates see live work). Grounded verbatim, a run room's window is 140
    /// receipts and no conversation: every citizen reads everyone's "I've been
    /// going in circles" and says it back (12 citizens, live 2026-09-03 — the
    /// loop was the WINDOW). Twelve workers interleave, so receipts are never
    /// consecutive per author (measured after the first cut: 168 items, still
    /// one per receipt). The unit is therefore PER AUTHOR ACROSS THE WINDOW:
    /// all of one author's receipts fold into a single line — her newest
    /// thought + a tally of what she ran — anchored at her newest receipt
    /// (read-through cursor, unread flag). One line per teammate; chat lines
    /// stay verbatim in place.
    ///
    /// QUARANTINE, BEFORE EITHER PATH: a line the speak gate would refuse (a tool
    /// envelope, a peer's voice, a framing echo — [`is_not_a_contribution`]) is not a
    /// message to her either, whoever posted it and whenever. It is the vector by
    /// which every malformed shape spread across the roster (card 169eb543: six
    /// citizens adopted one envelope over 20 hours, by reading it). Returns the
    /// units and how many lines were kept out, so the packer can say so.
    pub(super) fn collapse_work_receipts(digest: &ChannelDigest, working: bool) -> (Vec<PackUnit>, usize) {
        // ONE pass decides both (Cormac's review of #4076): which elements contribute,
        // and how many were kept out — counted only among lines the branch would
        // otherwise have shown (in the working branch the presence plane is dropped
        // anyway, so a receipt-shaped envelope there is not "quarantined").
        let mut quarantined = 0usize;
        let contributes: Vec<bool> = digest
            .elements
            .iter()
            .map(|el| {
                let Some(text) = el.text() else { return true };
                if working && is_work_receipt(text) {
                    return true; // dropped below as presence, not as quarantine
                }
                let ok = !is_not_a_contribution(text);
                if !ok {
                    quarantined += 1;
                }
                ok
            })
            .collect();
        // WORKING (hands rooted at a card): the PRESENCE plane — every citizen's 💭
        // thought broadcast and ⚙ receipt, hers included (her newest thoughts lead
        // the turn from working memory) — is not packed at all: it is state, not a
        // message to her. The MESSAGE plane stays: human/agent lines and real speech.
        // Measured 2026-09-05: every work turn opened with "this room is noisy, let
        // me figure out what is real" (attention spent as deserialization, not sniffing).
        if working {
            let units = digest
                .elements
                .iter()
                .enumerate()
                .filter(|(idx, el)| contributes[*idx] && !el.text().is_some_and(is_work_receipt))
                .map(|(idx, _)| PackUnit { last_idx: idx, order_idx: idx, collapsed: None })
                .collect();
            return (units, quarantined);
        }
        use std::collections::HashMap;
        // RUNS, DELIMITED BY SPEECH: an author's receipts fold into one line per run,
        // and a run ends at the next real line anyone says. "What everyone did since the
        // last thing said." Every run behind the last chat line is therefore FINAL: its
        // text and its place never change again, so the prefix the engine holds survives
        // every new receipt; only the open runs at the tail grow. A run's line sits at
        // its FIRST receipt and reads at its newest (the cursor).
        struct Run {
            first: usize,
            receipts: Vec<usize>,
        }
        let mut runs: Vec<Run> = Vec::new();
        let mut open: HashMap<uuid::Uuid, usize> = HashMap::new(); // author → index into runs
        for (idx, el) in digest.elements.iter().enumerate() {
            if !contributes[idx] {
                continue;
            }
            match el.text() {
                Some(text) if is_work_receipt(text) => {
                    let author = el.sender_id();
                    match open.get(&author) {
                        Some(&r) => runs[r].receipts.push(idx),
                        None => {
                            open.insert(author, runs.len());
                            runs.push(Run { first: idx, receipts: vec![idx] });
                        }
                    }
                }
                _ => open.clear(), // a line said closes every run
            }
        }
        let mut units: Vec<PackUnit> = Vec::new();
        for (idx, el) in digest.elements.iter().enumerate() {
            if !contributes[idx] {
                continue; // refused at the speak seam → never shown at the perception seam
            }
            if !el.text().is_some_and(is_work_receipt) {
                units.push(PackUnit { last_idx: idx, order_idx: idx, collapsed: None });
            }
        }
        for run in &runs {
            let newest = *run.receipts.last().unwrap_or(&run.first); // unwrap_or: a run is made with its first receipt, so this is never reached
            let collapsed = if run.receipts.len() == 1 {
                None
            } else {
                Some(Self::collapsed_receipt_text(
                    run.receipts.iter().filter_map(|i| digest.elements[*i].text()),
                    run.receipts.len(),
                ))
            };
            units.push(PackUnit { last_idx: newest, order_idx: run.first, collapsed });
        }
        units.sort_by_key(|u| u.order_idx);
        (units, quarantined)
    }

    /// The collapsed text of a receipt run: the newest `💭` line, then a tally
    /// of every `⚙ verb mark` across the run (`⚙ code/shell ✓×4 · code/github/issue-create ✗×2`).
    fn collapsed_receipt_text<'a>(texts: impl Iterator<Item = &'a str>, batches: usize) -> String {
        let mut last_thought: Option<&str> = None;
        let mut tally: Vec<(String, usize)> = Vec::new();
        for text in texts {
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with(crate::persona::presence_glyph::THOUGHT) {
                    last_thought = Some(line);
                } else if let Some(rest) = line.strip_prefix(crate::persona::presence_glyph::ACT) {
                    let mut parts = rest.split_whitespace();
                    let verb = parts.next().unwrap_or("?"); // unwrap_or: a bare marker still tallies as unknown
                    let mark = parts.last().unwrap_or("·"); // unwrap_or: a verb without a mark tallies as neutral
                    let key = format!("{verb} {mark}");
                    match tally.iter_mut().find(|(k, _)| *k == key) {
                        Some((_, n)) => *n += 1,
                        None => tally.push((key, 1)),
                    }
                }
            }
        }
        let acts: Vec<String> = tally
            .iter()
            .map(|(k, n)| if *n > 1 { format!("{k}×{n}") } else { k.clone() })
            .collect();
        format!(
            "{} · ⚙ {batches} act batches: {}",
            last_thought.unwrap_or("💭 (working)"), // unwrap_or: a run of bare act lines has no thought to lead with
            if acts.is_empty() { "(no receipts)".to_string() } else { acts.join(" · ") }
        )
    }
}

/// One packable unit of the window: a message, or a collapsed run of work receipts.
pub(super) struct PackUnit {
    /// The element that anchors the unit (the run's newest; the read-through cursor).
    pub(super) last_idx: usize,
    /// WHERE the unit sits in the window: the run's FIRST receipt, so a folded line
    /// never relocates when its author posts another receipt (Cormac on #4806: the fold
    /// sat at the newest receipt and moved from mid-window to the end on every receipt,
    /// so the prefix diverged mid-history whatever the anchor held). A plain line's
    /// order is its own index.
    pub(super) order_idx: usize,
    /// The collapsed text when the unit is a receipt run of two or more; `None`
    /// packs the element's own text.
    pub(super) collapsed: Option<String>,
}

/// A radiated work receipt (`act_observe::apply`): leads with `💭` or `⚙`.
pub(super) fn is_work_receipt(text: &str) -> bool {
    let t = text.trim_start();
    crate::persona::presence_glyph::is_presence_line(t)
}

/// The speak gate's verdict, read at the perception seam — one predicate, two seams.
pub(crate) fn is_not_a_contribution(text: &str) -> bool {
    crate::cognition::not_speech::is_not_a_contribution(text).is_some()
}

