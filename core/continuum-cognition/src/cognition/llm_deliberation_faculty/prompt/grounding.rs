//! The standing grounding as a budget consumer: each contribution an item with levels generated
//! from its shape, so under pressure she gets less of each, then handles she can expand, never
//! "assembled context suppressed: zero token budget" (the M5, 2026-11-07: Aiko's grounding granted 0).
//!
//! A contribution's levels, most detail first, each measured from what it renders:
//! - **whole**: its content (with its source's own omission notice when the source cut it);
//! - **summaries**: a source that delivered a LIST (`Contribution::parts`) gives the longest prefix
//!   of its units with the omission notice, one unit fewer per level (its units are ordered by the
//!   source, leads first, so a prefix is the summary the source would write if asked for less); an
//!   indivisible one gives its summaries by shape (`continuum_context::levels::summarize`, Output:
//!   its head and tail, every rung of the ladder);
//! - **its floor**: its handle (the omission notice naming the verb that yields the rest,
//!   `Contribution::expand_command`) when it has one, else nothing: it leaves the turn.
//!
//! Which keeps detail longest: a standing source (`standing_grounding`) over an optional one, then
//! by the salience its faculty measured. A less important contribution is never shown in more
//! detail than a more important one (the ladder's rule), so a top-salience block can no longer lose
//! its place to a cheaper lesser one (the inversion `delib.context.salience_inversion` reported).
//! The block's header rides the most important item's levels: nothing renders when it leaves.

use continuum_context::ladder::{Leveled, Rungs};

use super::*;

/// What one contribution renders at one of its levels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) enum Body {
    /// Its content as delivered.
    Whole,
    /// Its first `n` units and the omission notice.
    Units(usize),
    /// A summary of its content by shape.
    Summary(String),
    /// Only the notice naming the verb that yields it.
    Handle,
}

/// A borrowed contribution at one level: what it renders and what it costs.
pub struct GroundingPiece<'a> {
    pub(in super::super) contribution: &'a Contribution,
    pub(in super::super) body: Body,
    pub tokens: usize,
}

impl<'a> GroundingPiece<'a> {
    pub(in super::super) fn cost(c: &Contribution, body_bytes: usize) -> usize {
        LlmDeliberationFaculty::context_piece_tokens(c.faculty.as_str(), body_bytes)
    }

    pub fn whole(c: &'a Contribution) -> Self {
        // A block whose SOURCE cut it short still owes its reader the notice, even
        // when every delivered part fits: reserve it here so the budget stays exact.
        let notice = if c.source_truncated { usize::from(!c.content.is_empty()) + Self::notice_bytes(c, 0) } else { 0 };
        Self { contribution: c, body: Body::Whole, tokens: Self::cost(c, c.content.len() + notice) }
    }

    pub(in super::super) const NOTICE_SUFFIX: &'static str = " more not shown (context budget)";

    pub(in super::super) const EXPAND_PREFIX: &'static str = " — run `";

    pub(in super::super) const EXPAND_SUFFIX: &'static str = "` to see all ";

    /// The omission notice, exactly as `append_notice` writes it. `omitted` is what
    /// THIS packing left out of `parts`. A source that cut its own list short adds
    /// an uncounted remainder: the count becomes `N+` (or no number when the packing
    /// omitted nothing), and the verb says "see all" with no total, because the
    /// true total is unknown.
    pub(in super::super) fn notice_text(c: &Contribution, omitted: usize) -> String {
        let count = match (omitted, c.source_truncated) {
            (0, true) => String::new(),
            (n, true) => format!("{n}+"),
            (n, false) => n.to_string(),
        };
        // With no number to show, the suffix's leading space would dangle ("… more").
        let suffix = if count.is_empty() { Self::NOTICE_SUFFIX.trim_start() } else { Self::NOTICE_SUFFIX };
        let mut text = format!("…{count}{suffix}");
        if let Some(cmd) = c.expand_command {
            text.push_str(Self::EXPAND_PREFIX);
            text.push_str(cmd);
            if c.source_truncated {
                text.push_str(Self::EXPAND_SUFFIX.trim_end());
            } else {
                text.push_str(Self::EXPAND_SUFFIX);
                text.push_str(&c.parts.len().to_string());
            }
        }
        text
    }

    pub(in super::super) fn notice_bytes(c: &Contribution, omitted: usize) -> usize {
        Self::notice_text(c, omitted).len()
    }

    /// The handle alone: what is not shown and the verb that yields it. `None` when no verb does.
    fn handle_text(c: &Contribution) -> Option<String> {
        let cmd = c.expand_command?;
        Some(if c.parts.len() >= 2 {
            Self::notice_text(c, c.parts.len())
        } else {
            format!("…{}{}{cmd}`", Self::NOTICE_SUFFIX.trim_start(), Self::EXPAND_PREFIX)
        })
    }

    /// Every level of `c`, most detail first, each smaller than the one above; its last is its
    /// handle, or absent (it leaves the turn) when no verb yields it.
    pub(in super::super) fn levels(c: &'a Contribution) -> Vec<Self> {
        let mut levels = vec![Self::whole(c)];
        if c.parts.len() >= 2 {
            let mut bytes: Vec<usize> = Vec::with_capacity(c.parts.len());
            let mut total = 0;
            for (index, unit) in c.parts.iter().enumerate() {
                total += unit.len() + usize::from(index > 0);
                bytes.push(total);
            }
            for units in (1..c.parts.len()).rev() {
                let tokens = Self::cost(c, bytes[units - 1] + 1 + Self::notice_bytes(c, c.parts.len() - units));
                levels.push(Self { contribution: c, body: Body::Units(units), tokens });
            }
        } else {
            let mut above = c.content.clone();
            while let Some(summary) = continuum_context::levels::summarize(&above, continuum_context::levels::Shape::Output) {
                let tokens = Self::cost(c, summary.len());
                levels.push(Self { contribution: c, body: Body::Summary(summary.clone()), tokens });
                above = summary;
            }
        }
        if let Some(handle) = Self::handle_text(c) {
            levels.push(Self { contribution: c, body: Body::Handle, tokens: Self::cost(c, handle.len()) });
        }
        let mut kept: Vec<Self> = Vec::with_capacity(levels.len());
        for level in levels {
            if kept.last().is_none_or(|above| level.tokens < above.tokens) {
                kept.push(level);
            }
        }
        kept
    }

    /// Its truthful minimum: the source's first unit and the omission notice, when it delivered a
    /// list and that is smaller than its whole; else its whole. A view block's summary level
    /// (`context_streams::blocks::Grounding::minimum`).
    pub fn minimum(c: &'a Contribution) -> Self {
        let whole = Self::whole(c);
        if c.parts.len() < 2 {
            return whole;
        }
        let tokens = Self::cost(c, c.parts[0].len() + 1 + Self::notice_bytes(c, c.parts.len() - 1));
        if tokens >= whole.tokens {
            return whole;
        }
        Self { contribution: c, body: Body::Units(1), tokens }
    }

    pub(in super::super) fn append_notice(c: &Contribution, omitted: usize, out: &mut String) {
        out.push_str(&Self::notice_text(c, omitted));
    }

    pub(in super::super) fn append_body(&self, out: &mut String) {
        let c = self.contribution;
        match &self.body {
            Body::Whole => {
                out.push_str(&c.content);
                if c.source_truncated {
                    if !c.content.is_empty() {
                        out.push('\n');
                    }
                    Self::append_notice(c, 0, out);
                }
            }
            Body::Units(units) => {
                for (index, unit) in c.parts[..*units].iter().enumerate() {
                    if index > 0 {
                        out.push('\n');
                    }
                    out.push_str(unit);
                }
                out.push('\n');
                Self::append_notice(c, c.parts.len() - units, out);
            }
            Body::Summary(summary) => out.push_str(summary),
            Body::Handle => out.push_str(&Self::handle_text(c).unwrap_or_default()), // unwrap_or_default: a handle level exists only where a verb yields it
        }
    }
}

/// This act's grounding as the allocation's consumer: its contributions, least important first,
/// each at its levels.
pub(in super::super) struct GroundingOffer<'a> {
    /// Per contribution (importance order), its levels.
    pieces: Vec<Vec<GroundingPiece<'a>>>,
    /// Per item, the piece level of each rung, exactly one per rung (`None`: it left the turn);
    /// charged costs no smaller than the rung above are not rungs (`Rungs::kept`).
    rung_levels: Vec<Vec<Option<usize>>>,
    pub(in super::super) leveled: Leveled,
}

impl<'a> GroundingOffer<'a> {
    /// Every contribution that renders in the assembled-context message (not a view block, not
    /// the verdict), each at its levels.
    pub(in super::super) fn of(ws: &'a Workspace, charge: &dyn Fn(usize) -> u32) -> Self {
        let mut offered: Vec<&Contribution> = ws.broadcast.iter().filter(|c| c.decision.is_none() && !c.trailing).collect();
        offered.sort_by(|a, b| a.standing_grounding.cmp(&b.standing_grounding).then(a.salience.total_cmp(&b.salience)));
        let header = LlmDeliberationFaculty::working_context_header_cost();
        let count = offered.len();
        let mut pieces = Vec::with_capacity(count);
        let mut rung_levels = Vec::with_capacity(count);
        let mut items = Vec::with_capacity(count);
        for (index, c) in offered.into_iter().enumerate() {
            let levels = GroundingPiece::levels(c);
            // The header renders with the block, so the most important item carries it.
            let with_header = |tokens: usize| if index + 1 == count { tokens + header } else { tokens };
            // Below its last level it leaves the turn, unless that level is its handle or it is
            // standing grounding, whose last level is its minimum: a greedier, more important item
            // taking one more unit must never drop the activity's wall (the M5 work-tools change:
            // two schemas fewer in her hands let the roster take a unit and the wall left).
            let leaves = (!c.standing_grounding && levels.last().is_some_and(|last| last.body != Body::Handle)).then_some((0, None));
            let smaller = levels.iter().enumerate().skip(1).map(|(level, piece)| (charge(with_header(piece.tokens)), Some(level))).chain(leaves);
            let (rungs, map) = Rungs::kept(charge(with_header(levels[0].tokens)), Some(0), smaller);
            items.push(rungs);
            pieces.push(levels);
            rung_levels.push(map);
        }
        Self { pieces, rung_levels, leveled: Leveled::new(items) }
    }

    /// The pieces `rungs` (one per item, or none: the grounding left the turn) render, in the
    /// block's canonical order. Nothing renders when the most important item left.
    pub(in super::super) fn rendered(self, rungs: &[usize]) -> Vec<GroundingPiece<'a>> {
        if rungs.len() != self.pieces.len() {
            return Vec::new();
        }
        let chosen: Vec<Option<usize>> = self.rung_levels.iter().zip(rungs).map(|(map, &rung)| map[rung]).collect();
        if chosen.last().is_some_and(Option::is_none) {
            return Vec::new();
        }
        let mut out: Vec<GroundingPiece<'a>> = self
            .pieces
            .into_iter()
            .zip(chosen)
            .filter_map(|(levels, level)| level.and_then(|level| levels.into_iter().nth(level)))
            .collect();
        // SERIALIZATION (by volatility, then CANONICALLY within the stable tier): stable standing
        // framing FIRST, so it lands in the cacheable prefix region; volatile grounding LAST,
        // nearest the generation point, most salient last (the KV-prefix rule of #266: salience
        // decides how much of each is shown, never where the survivors sit).
        out.sort_by(|a, b| {
            let (a, b) = (a.contribution, b.contribution);
            u8::from(!a.stable).cmp(&u8::from(!b.stable)).then_with(|| match (a.stable, b.stable) {
                (true, true) => deliberation_prompt::stable_prefix_order(a.faculty.as_str(), b.faculty.as_str()),
                _ => a.salience.total_cmp(&b.salience).reverse(),
            })
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Contribution {
        let parts: Vec<String> = (0..12).map(|i| format!("[card {i}] a card on the board with its title and its state")).collect();
        Contribution::context(FacultyId::Custom("room-kanban".to_string()), parts.join("\n"), 0.9, "the board")
            .with_parts(parts)
            .with_expand_command(Some("work/list"))
            .standing_grounding()
    }

    // what this catches (Aiko on the M5, 2026-11-07: "assembled context suppressed: zero token
    // budget"): grounding with no level between whole and nothing. A list contribution reduces by
    // units to its handle, which names the verb that yields it; each level is smaller.
    #[test]
    fn a_list_contribution_reduces_by_units_to_a_handle_that_names_its_verb() {
        let c = board();
        let levels = GroundingPiece::levels(&c);
        assert!(levels.len() > 2, "whole, unit prefixes, handle");
        assert!(levels.windows(2).all(|w| w[0].tokens > w[1].tokens));
        let handle = levels.last().expect("levels"); // JUSTIFIED: test fixture
        assert_eq!(handle.body, Body::Handle);
        let mut text = String::new();
        handle.append_body(&mut text);
        assert!(text.contains("work/list"), "{text}");
    }

    // what this catches: the inversion the salience sensor reported (a top-salience block dropped
    // whole while a cheaper lesser one kept its slot): within any budget the more salient block is
    // shown at least as whole as a less salient one, and below every handle nothing renders.
    #[test]
    fn a_more_salient_block_is_never_shown_less_whole_than_a_lesser_one() {
        let top = board();
        // A lesser block with levels of its own (an atomic one is its own summary and may stay
        // whole beside a summarized top block, the view's rule).
        let notes: Vec<String> = (0..12).map(|i| format!("note {i}: a feeling about the work, said at some length")).collect();
        let lesser = Contribution::context(FacultyId::Custom("affect".to_string()), notes.join("\n"), 0.3, "a feeling").with_parts(notes);
        let mut ws = Workspace::new("what is on the board?");
        ws.broadcast.push(lesser);
        ws.broadcast.push(top);
        let offer = GroundingOffer::of(&ws, &|tokens| tokens as u32);
        let whole_top = GroundingPiece::whole(&ws.broadcast[1]).tokens as u32;
        let choice = offer.leveled.reduce(whole_top);
        let rendered = offer.rendered(&choice.rungs);
        let top_piece = rendered.iter().find(|p| p.contribution.faculty.as_str() == "room-kanban").expect("the top block renders"); // JUSTIFIED: test fixture
        let lesser_whole = rendered.iter().any(|p| p.contribution.faculty.as_str() == "affect" && p.body == Body::Whole);
        assert!(top_piece.body == Body::Whole || !lesser_whole, "a lesser block whole beside a collapsed top one");
        let none = GroundingOffer::of(&ws, &|tokens| tokens as u32);
        assert!(none.rendered(&[]).is_empty(), "grounding that left the turn renders nothing");
    }
}
