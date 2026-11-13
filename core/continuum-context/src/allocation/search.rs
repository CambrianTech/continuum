//! The act's search over levels (Joel, 2026-11-07: "It's an optimizer too. It sort of searches to
//! fit."): which level each leveled consumer ([`crate::ladder::Leveled`]) shows, chosen together,
//! so a high-priority item is never collapsed where collapsing two cheaper, lower-priority ones
//! would fit.
//!
//! What is maximized: `Σ priority × ln(1 + tokens shown)` over every claim. The continuous claims
//! (her view's sources, the fresh history, the reply above its need) take what the levels leave
//! by water-filling that same sum, whose continuous optimum is a share in proportion to priority:
//! the flexbox's grow pass, so the search and the flexbox agree on what a token is worth. No
//! weight table: priority is each claim's own, and a consumer's items are ordered by their own
//! measured importance (recency, salience) before they reach the ladder.
//!
//! How it searches (ballpark first, exact for the few contested choices):
//! 1. the cheap first guess, given by the allocation (the one caller of the flexbox): the flexbox
//!    over every claim, each leveled grant rounded down to its largest level;
//! 2. a dynamic programme over the leveled consumers' options, each option's cost above its hard
//!    limit rounded UP to a unit derived from the instance (the span to decide over divided by the
//!    count of options, so the table has about as many cells as there are options), so every
//!    combination it finds fits exactly;
//! 3. an exact climb from the best cell: any consumer a level up, alone or while another gives a
//!    level back, while the exact costs fit and the value rises.
//!
//! Error bound, stated and checked (`search/tests.rs`): rounding adds less than one unit per
//! leveled consumer, and the table spans the slack plus one unit per consumer, so every combination
//! that fits exactly has a cell; what the table can miss is a combination whose rounded cell a
//! higher-valued one that does not fit exactly took (two within `consumers × unit` of each other),
//! and the climb's exchanges recover the neighbours of the one it keeps. The result is never worse
//! than the first guess: the better of the two is returned.

use crate::flex::FlexClaim;

/// One leveled consumer as the search sees it: its options' costs, most detail first, the last
/// its hard limit. Never empty: a consumer with nothing to show has the one option 0
/// ([`crate::ladder::Leveled::options`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub priority: u8,
    pub options: Vec<u32>,
}

impl Group {
    pub fn limit(&self) -> u32 {
        self.options.last().copied().unwrap_or(0) // unwrap_or: a consumer with no option shows nothing and costs nothing
    }

    pub fn want(&self) -> u32 {
        self.options.first().copied().unwrap_or(0) // unwrap_or: as above
    }
}

/// Every hard limit together does not fit: the one refusal. `binding` is the consumer whose hard
/// limit is the largest, the one a larger window or a smaller limit must answer first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overflow {
    pub needed: u32,
    pub window: u32,
    pub binding: String,
    pub binding_limit: u32,
}

/// The search's answer: an option index per group, and how it was reached.
#[derive(Debug, Clone, PartialEq)]
pub struct Searched {
    pub picks: Vec<usize>,
    pub value: f64,
    /// The first guess's value: the search's answer is never below it.
    pub first_guess_value: f64,
    /// Whether the search kept a different combination than the first guess.
    pub refined: bool,
    /// Table cells visited: the search's cost per act, in work.
    pub cells: usize,
}

fn worth(priority: u8, tokens: u32) -> f64 {
    f64::from(priority) * f64::from(tokens).ln_1p()
}

/// The continuous claims' value with `room` tokens to share: each at least its floor when it is
/// required, at most its max, the rest water-filled by priority. `None` when the required floors
/// alone exceed `room`.
pub fn continuous_value(room: u32, claims: &[FlexClaim]) -> Option<f64> {
    let lower = |c: &FlexClaim| if c.required { f64::from(c.floor_tokens) } else { 0.0 };
    let upper = |c: &FlexClaim| f64::from(c.max_tokens.max(if c.required { c.floor_tokens } else { 0 }));
    let room = f64::from(room);
    let floors: f64 = claims.iter().map(lower).sum();
    if floors > room {
        return None;
    }
    let total = |share: &dyn Fn(&FlexClaim) -> f64| -> f64 {
        claims.iter().map(|c| f64::from(c.priority) * share(c).ln_1p()).sum()
    };
    if claims.iter().map(upper).sum::<f64>() <= room {
        return Some(total(&upper));
    }
    // x(λ) = clamp(p/λ − 1, lower, upper): Σx falls as λ rises. Bisect λ until Σx meets the room
    // (to f64 precision: the bound is the arithmetic, never a chosen tolerance).
    let at = |lambda: f64| move |c: &FlexClaim| (f64::from(c.priority) / lambda - 1.0).clamp(lower(c), upper(c));
    let sum = |lambda: f64| claims.iter().map(at(lambda)).sum::<f64>();
    let mut hi = claims.iter().map(|c| f64::from(c.priority)).fold(0.0, f64::max).max(f64::MIN_POSITIVE);
    let mut lo = f64::MIN_POSITIVE;
    while hi - lo > hi * f64::EPSILON {
        let mid = lo + (hi - lo) / 2.0;
        if sum(mid) > room {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(total(&at(hi)))
}

/// The binding hard limit when every hard limit together exceeds `window`.
pub fn overflow(window: u32, groups: &[Group], continuous: &[FlexClaim]) -> Option<Overflow> {
    let limits = groups
        .iter()
        .map(|g| (g.id.as_str(), g.limit()))
        .chain(continuous.iter().filter(|c| c.required).map(|c| (c.source_id.as_str(), c.floor_tokens)));
    let needed = limits.clone().map(|(_, limit)| limit).fold(0, u32::saturating_add);
    if needed <= window {
        return None;
    }
    let (binding, binding_limit) = limits.max_by_key(|&(_, limit)| limit).unwrap_or(("", 0)); // unwrap_or: an overflow has at least one limit
    Some(Overflow { needed, window, binding: binding.to_owned(), binding_limit })
}

/// The value of one combination, exactly: `None` when it does not fit.
pub(crate) fn value_of(window: u32, groups: &[Group], picks: &[usize], continuous: &[FlexClaim]) -> Option<f64> {
    let spent = groups.iter().zip(picks).map(|(g, &pick)| g.options[pick]).fold(0, u32::saturating_add);
    let rest = window.checked_sub(spent)?;
    let leveled: f64 = groups.iter().zip(picks).map(|(g, &pick)| worth(g.priority, g.options[pick])).sum();
    continuous_value(rest, continuous).map(|value| leveled + value)
}

/// Which option each group shows within `window`, beside `continuous`, refining `guess` (the
/// allocation's first guess, one option per group: `allocation::first_guess`, the flexbox's
/// grants rounded down). `Err` only when every hard limit together does not fit.
pub fn search(window: u32, groups: &[Group], continuous: &[FlexClaim], guess: Vec<usize>) -> Result<Searched, Overflow> {
    if let Some(over) = overflow(window, groups, continuous) {
        return Err(over);
    }
    let first_guess_value = value_of(window, groups, &guess, continuous).unwrap_or(f64::NEG_INFINITY); // unwrap_or: a guess that does not fit is worth nothing
    let limits: u32 = groups.iter().map(Group::limit).fold(0, u32::saturating_add);
    let slack = window - limits.min(window);
    let span: u32 = groups.iter().map(|g| g.want() - g.limit()).fold(0, u32::saturating_add).min(slack);
    let count: u32 = groups.iter().map(|g| g.options.len() as u32).sum::<u32>().max(1);
    let unit = span.div_ceil(count).max(1);
    // Rounding up adds less than one unit per group, so every combination that fits exactly lies
    // within the slack's units plus one per group: the table spans them, and the exact check below
    // drops the rounded cells that do not fit.
    let cells = (span / unit) as usize + groups.len() + 1;
    // best[b]: the best leveled value whose rounded extras sum to b units, its exact extras (never
    // past the slack: a partial that cannot fit is never kept), and its picks.
    type Cell = Option<(f64, u32, Vec<usize>)>;
    let mut best: Vec<Cell> = vec![None; cells];
    best[0] = Some((0.0, 0, Vec::new()));
    let mut visited = 0usize;
    for group in groups {
        let mut next: Vec<Cell> = vec![None; cells];
        for (b, state) in best.iter().enumerate() {
            let Some((value, extra, picks)) = state else { continue };
            for (pick, &cost) in group.options.iter().enumerate() {
                visited += 1;
                let above = cost - group.limit();
                let at = b + above.div_ceil(unit) as usize;
                let exact = extra.saturating_add(above);
                if at >= cells || exact > slack {
                    continue;
                }
                let candidate = value + worth(group.priority, cost);
                if next[at].as_ref().is_none_or(|(held, held_extra, _)| candidate > *held || (candidate == *held && exact < *held_extra)) {
                    let mut picks = picks.clone();
                    picks.push(pick);
                    next[at] = Some((candidate, exact, picks));
                }
            }
        }
        best = next;
    }
    let mut chosen = best
        .into_iter()
        .flatten()
        .filter_map(|(_, _, picks)| value_of(window, groups, &picks, continuous).map(|value| (value, picks)))
        .max_by(|a, b| a.0.total_cmp(&b.0));
    // The exact climb: any one consumer a level up, alone or while another gives a level back,
    // wherever the exact costs fit and the value rises, until no such move does.
    if let Some((value, picks)) = chosen.as_mut() {
        loop {
            let mut moves: Vec<Vec<usize>> = Vec::new();
            for up in (0..groups.len()).filter(|&g| picks[g] > 0) {
                let mut raised = picks.clone();
                raised[up] -= 1;
                for down in (0..groups.len()).filter(|&h| h != up && picks[h] + 1 < groups[h].options.len()) {
                    let mut swapped = raised.clone();
                    swapped[down] += 1;
                    moves.push(swapped);
                }
                moves.push(raised);
            }
            let best_move = moves
                .into_iter()
                .filter_map(|trial| value_of(window, groups, &trial, continuous).map(|v| (v, trial)))
                .max_by(|a, b| a.0.total_cmp(&b.0));
            match best_move {
                Some((v, trial)) if v > *value => {
                    *value = v;
                    *picks = trial;
                }
                _ => break,
            }
        }
    }
    let (value, picks, refined) = match chosen {
        Some((value, picks)) if value > first_guess_value => { let refined = picks != guess; (value, picks, refined) },
        _ => (first_guess_value, guess, false),
    };
    Ok(Searched { picks, value, first_guess_value, refined, cells: visited })
}

#[cfg(test)]
mod tests;
