//! How deep a turnover collapses her history: the start size that costs her least per act.
//!
//! A start of `S` (her growing sources' share of it) in a room of `W` for those sources, growing `g`
//! per act and keeping `C` of the epoch before (her collapsed log, laid down byte for byte), lives
//! `(W - S) / g` acts and misses `S - C`: `miss(S) = g (S - C) / (W - S)` per act. On that alone the
//! smallest start always wins (the M5 captures, 2026-11-06: Aiko 2,122 -> 198 tokens per act), because
//! it ignores what a collapsed item costs when she needs it again: its whole re-sent, which also
//! grows the epoch toward its end, `whole (1 + (S - C) / (W - S))`, times her measured chance per act
//! of asking for it again at its age (`crate::rereads`).
//!
//! So the start keeps an item whole exactly when collapsing it would cost more in expected re-reads
//! than it saves in turnover misses: walking each instrumented source's items newest first (the view
//! collapses oldest first, so a source keeps a contiguous newest run), an item is kept while
//! `hazard x whole x (1 + (S - C)/(W - S)) > miss(S + saves) - miss(S)` and the room still holds one
//! act's growth after it. Every input is measured: `W` and `g` by the caller from her window and her
//! last epoch, `C` from her log, the hazard from her own re-requests. A source whose items carry no
//! act (her conversation, her thoughts) has no re-request to measure, and a hazard bucket nobody was
//! exposed at is `Unmeasured`: such a source keeps the share the caller's rule gives it, and with no
//! measured source (or no measured growth) the depth is [`Depth::Unmeasured`], the caller's rule.

use crate::rereads::Hazard;

/// One item a start could collapse: everything but a source's newest, not yet in her log.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collapsible {
    /// Its whole, in the caller's cost (`crate::cost`): what a re-read re-sends.
    pub whole: usize,
    /// What a start saves by collapsing it (its whole less its pointer), in the caller's cost.
    pub saves: usize,
    /// Acts since her view first held it, on her one clock (`crate::rereads::Rereads::age`).
    pub age: u64,
    pub hazard: Hazard,
}

/// `items`' collapsible ones (all but the newest, each whole larger than its pointer), newest first,
/// priced by the caller's `cost`, with her measured re-read chance at each one's age, read off the
/// clock that measured it (its recorded birth, never its position: a source idle for acts or
/// batching several items in one would sample the wrong bucket); `Unmeasured` where nothing
/// measured that age.
pub(crate) fn collapsible<T: crate::epoch::Block>(items: &[crate::source::Item<T>], rereads: &crate::rereads::Rereads, cost: &dyn crate::cost::Estimate<T>) -> Vec<Collapsible> {
    items
        .iter()
        .rev()
        .skip(1)
        .filter_map(|item| {
            let whole = crate::cost::whole(item, cost);
            let pointer = item.pointer().map_or(0, |pointer| cost.block(pointer)); // map_or: an item that leaves when collapsed costs nothing as a pointer
            let age = rereads.age(item.id());
            (whole > pointer).then(|| Collapsible { whole, saves: whole - pointer, age, hazard: rereads.hazard(age) })
        })
        .collect()
}

/// One growing source at a start (`view::Claim::growing`), every size in the caller's cost.
#[derive(Clone, Debug)]
pub struct Growing<'a> {
    /// What the caller's rule gives it: kept when it cannot be measured.
    pub share: usize,
    /// Its floor with its newest item whole: the least a measured start gives it.
    pub base: usize,
    /// What of it her collapsed log lays down byte for byte.
    pub logged: usize,
    /// Its items carry acts, so their re-reads are measured.
    pub instrumented: bool,
    pub collapsible: &'a [Collapsible],
}

/// What the room for her growing sources is, in the caller's unit.
#[derive(Clone, Copy, Debug)]
pub struct Room {
    /// What her growing sources' start and its growth reserve share.
    pub growable: usize,
    /// What one act appended in her last epoch; `None` before one was measured.
    pub per_act: Option<usize>,
}

/// The depth a start should collapse to.
#[derive(Clone, Debug, PartialEq)]
pub enum Depth {
    /// Nothing measured decides it: the caller's rule stands.
    Unmeasured,
    Chosen(Chosen),
}

/// A measured depth and what it predicts.
#[derive(Clone, Debug, PartialEq)]
pub struct Chosen {
    /// Each growing source's share, in the order given.
    pub shares: Vec<usize>,
    /// Which of them the measurement decided (the rest kept the caller's share).
    pub measured: Vec<bool>,
    /// Their sum: the growing start.
    pub start: usize,
    /// Items the walk collapsed and kept whole, over the measured sources.
    pub collapsed: usize,
    pub kept: usize,
    /// Predicted turnover miss per act, in the caller's unit.
    pub miss_per_act: f64,
    /// Predicted re-reads of what it collapsed per act (priced with the extra event, and as
    /// observed), and what they re-send per act.
    pub rereads_per_act: f64,
    pub rereads_observed_per_act: f64,
    pub reread_per_act: f64,
}

/// The measured depth for `growing` in `room` (see the module doc).
pub fn choose(room: Room, growing: &[Growing<'_>]) -> Depth {
    let Some(g) = room.per_act.filter(|g| *g > 0).map(|g| g as f64) else {
        return Depth::Unmeasured;
    };
    let mut measured: Vec<bool> = growing.iter().map(|source| source.instrumented && !source.collapsible.is_empty()).collect();
    // A source whose walk reaches an age nobody was exposed at while it still keeps items is not
    // decided by the measurement: it keeps the caller's share, and the rest are walked again.
    loop {
        if !measured.iter().any(|m| *m) {
            return Depth::Unmeasured;
        }
        match walk(room.growable as f64, g, growing, &measured) {
            Ok(chosen) => return Depth::Chosen(chosen),
            Err(undecided) => measured[undecided] = false,
        }
    }
}

/// One walk over the `measured` sources' items, youngest first; `Err` names a source that reached
/// an Unmeasured age while still keeping items.
fn walk(w: f64, g: f64, growing: &[Growing<'_>], measured: &[bool]) -> Result<Chosen, usize> {
    let kept_log: f64 = growing.iter().map(|source| source.logged as f64).sum();
    // A measured source starts at its floor with its newest item whole; the rest keep their share.
    let base: Vec<f64> = growing
        .iter()
        .zip(measured)
        .map(|(source, measured)| if *measured { source.base as f64 } else { source.share as f64 })
        .collect();
    let mut s: f64 = base.iter().sum();
    let miss = |s: f64| if s < w { g * (s - kept_log).max(0.0) / (w - s) } else { f64::INFINITY };
    let mut candidates: Vec<(usize, Collapsible)> = growing
        .iter()
        .enumerate()
        .filter(|(index, _)| measured[*index])
        .flat_map(|(index, source)| source.collapsible.iter().map(move |item| (index, *item)))
        .collect();
    candidates.sort_by_key(|(index, item)| (item.age, *index));
    let mut extra = vec![0.0; growing.len()];
    let mut closed = vec![false; growing.len()];
    let (mut collapsed, mut kept, mut rereads, mut observed, mut reread) = (0usize, 0usize, 0.0f64, 0.0f64, 0.0f64);
    for (index, item) in candidates {
        let (whole, saves) = (item.whole as f64, item.saves as f64);
        let hazard = item.hazard.per_act();
        let keep = !closed[index] && s + saves + g <= w && {
            let Some(hazard) = hazard else { return Err(index) };
            let saving = miss(s + saves) - miss(s);
            let cost = hazard * whole * (1.0 + (s - kept_log).max(0.0) / (w - s));
            cost > saving
        };
        if keep {
            s += saves;
            extra[index] += saves;
            kept += 1;
        } else {
            // Collapsed past the measured ages, its re-read is not predicted: counted, priced at none.
            closed[index] = true;
            collapsed += 1;
            rereads += hazard.unwrap_or(0.0); // unwrap_or: an item past every measured age is counted collapsed, not priced
            observed += item.hazard.observed().unwrap_or(0.0); // unwrap_or: as above
            reread += hazard.unwrap_or(0.0) * whole; // unwrap_or: as above
        }
    }
    let shares: Vec<usize> = base.iter().zip(&extra).map(|(base, extra)| (base + extra).round() as usize).collect();
    Ok(Chosen { start: shares.iter().sum(), shares, measured: measured.to_vec(), collapsed, kept, miss_per_act: miss(s), rereads_per_act: rereads, rereads_observed_per_act: observed, reread_per_act: reread })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rereads::Tally;

    /// An instrumented source at a 1,000-char floor with its 2,000-char newest item.
    fn source(collapsible: &[Collapsible], share: usize) -> Growing<'_> {
        Growing { share, base: 3_000, logged: 0, instrumented: true, collapsible }
    }

    fn item(age: u64, whole: usize, events: f64, exposure: f64) -> Collapsible {
        Collapsible { whole, saves: whole - 100, age, hazard: Hazard::Measured(Tally { exposure, events }) }
    }

    fn room() -> Room {
        Room { growable: 60_000, per_act: Some(2_000) }
    }

    // what this catches: the miss-only depth that collapses everything older than her newest. An
    // item she referenced one act ago and re-requests at a measured one act in four is kept whole,
    // because collapsing it would re-send its whole more often than the smaller start saves; an old
    // one she never asks for again is collapsed.
    #[test]
    fn a_recently_referenced_item_she_re_reads_stays_whole_and_an_old_one_collapses() {
        let items = [item(1, 8_000, 250.0, 1_000.0), item(40, 8_000, 0.0, 5_000.0)];
        let Depth::Chosen(chosen) = choose(room(), &[source(&items, 30_000)]) else {
            panic!("measured");
        };
        assert_eq!((chosen.kept, chosen.collapsed), (1, 1));
        assert_eq!(chosen.shares, vec![1_000 + 2_000 + 7_900]);
        assert!(chosen.rereads_per_act < 1e-3, "the collapsed one is rarely asked for again");
        // Measured rarely re-read, the recent one collapses too: the depth follows her.
        let rare = [item(1, 8_000, 0.0, 1_000.0), item(40, 8_000, 0.0, 5_000.0)];
        let Depth::Chosen(chosen) = choose(room(), &[source(&rare, 30_000)]) else {
            panic!("measured");
        };
        assert_eq!((chosen.kept, chosen.collapsed), (0, 2));
    }

    // what this catches: a guessed rate standing in for none. An unmeasured age the walk reaches
    // while it keeps items, a source with no acts to re-request, or no measured growth leaves the
    // caller's rule in place; an unmeasured age past where the walk already collapses does not.
    #[test]
    fn unmeasured_keeps_the_callers_share() {
        let unmeasured = [Collapsible { whole: 5_000, saves: 4_900, age: 2, hazard: Hazard::Unmeasured }];
        assert_eq!(choose(room(), &[source(&unmeasured, 30_000)]), Depth::Unmeasured);
        let measured = [item(3, 5_000, 0.0, 100.0)];
        assert_eq!(choose(Room { per_act: None, ..room() }, &[source(&measured, 30_000)]), Depth::Unmeasured);
        let turns = [item(1, 5_000, 0.0, 100.0)];
        let conversation = Growing { instrumented: false, ..source(&turns, 12_000) };
        let Depth::Chosen(chosen) = choose(room(), &[source(&measured, 30_000), conversation]) else {
            panic!("one source measured");
        };
        assert_eq!(chosen.measured, vec![true, false]);
        assert_eq!(chosen.shares[1], 12_000, "the conversation keeps its share");
        let past = [item(1, 5_000, 0.0, 1_000.0), Collapsible { whole: 5_000, saves: 4_900, age: 300, hazard: Hazard::Unmeasured }];
        let Depth::Chosen(chosen) = choose(room(), &[source(&past, 30_000)]) else {
            panic!("the walk collapses before the unmeasured age");
        };
        assert_eq!((chosen.kept, chosen.collapsed), (0, 2));
    }
}
