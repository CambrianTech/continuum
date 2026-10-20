//! The standing grounding a turn admits: each source's truthful minimum, then the optional
//! units the room left over (or her epoch's admitted size) can hold. Split out of
//! `llm_deliberation_faculty.rs`, which prices and renders it.

use super::deliberation_budget::GUARD_CHARS_PER_TOKEN;
use super::epoch_view;
use super::llm_deliberation_faculty::LlmDeliberationFaculty;
use super::workspace::{Contribution, Workspace};

/// A borrowed selection of whole source content or its declared atomic prefix.
/// Reserving and expanding it never copies the source payload.
pub(super) struct GroundingPiece<'a> {
    pub(super) contribution: &'a Contribution,
    /// None keeps the source's original rendering, including its separators.
    pub(super) units: Option<usize>,
    pub(super) tokens: usize,
}

impl<'a> GroundingPiece<'a> {
    pub(super) fn cost(c: &Contribution, body_bytes: usize) -> usize {
        if c.trailing {
            (c.faculty.as_str().len() + body_bytes + "[]\n".len()).div_ceil(GUARD_CHARS_PER_TOKEN)
                + LlmDeliberationFaculty::PER_MESSAGE_TEMPLATE_TOKENS
        } else {
            LlmDeliberationFaculty::context_piece_tokens(c.faculty.as_str(), body_bytes)
        }
    }

    pub(super) fn whole(c: &'a Contribution) -> Self {
        // A block whose SOURCE cut it short still owes its reader the notice, even
        // when every delivered part fits: reserve it here so the budget stays exact.
        let notice = if c.source_truncated {
            usize::from(!c.content.is_empty()) + Self::notice_bytes(c, 0)
        } else {
            0
        };
        Self {
            contribution: c,
            units: None,
            tokens: Self::cost(c, c.content.len() + notice),
        }
    }

    const NOTICE_SUFFIX: &'static str = " more not shown (context budget)";
    const EXPAND_PREFIX: &'static str = " — run `";
    const EXPAND_SUFFIX: &'static str = "` to see all ";

    /// The omission notice, exactly as `append_notice` writes it. `omitted` is what
    /// THIS packing left out of `parts`. A source that cut its own list short adds
    /// an uncounted remainder: the count becomes `N+` (or no number when the packing
    /// omitted nothing), and the verb says "see all" with no total, because the
    /// true total is unknown.
    pub(super) fn notice_text(c: &Contribution, omitted: usize) -> String {
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

    pub(super) fn notice_bytes(c: &Contribution, omitted: usize) -> usize {
        Self::notice_text(c, omitted).len()
    }

    pub(super) fn append_notice(c: &Contribution, omitted: usize, out: &mut String) {
        out.push_str(&Self::notice_text(c, omitted));
    }

    pub(super) fn minimum(c: &'a Contribution) -> Self {
        let whole = Self::whole(c);
        if c.parts.len() < 2 {
            return whole;
        }
        let tokens = Self::cost(
            c,
            c.parts[0].len() + 1 + Self::notice_bytes(c, c.parts.len() - 1),
        );
        if tokens >= whole.tokens {
            return whole;
        }
        Self {
            contribution: c,
            units: Some(1),
            tokens,
        }
    }

    /// The piece at exactly `units` (None = whole): how her epoch admitted it.
    pub(super) fn at(c: &'a Contribution, units: Option<usize>) -> Self {
        match units.filter(|&n| n > 0 && n < c.parts.len()) {
            None => Self::whole(c),
            Some(n) => {
                let bytes = c.parts[..n].iter().map(String::len).sum::<usize>() + n - 1;
                Self { contribution: c, units: Some(n), tokens: Self::cost(c, bytes + 1 + Self::notice_bytes(c, c.parts.len() - n)) }
            }
        }
    }

    pub(super) fn within(c: &'a Contribution, budget: usize) -> Option<Self> {
        let whole = Self::whole(c);
        if whole.tokens <= budget {
            return Some(whole);
        }
        // Reserve the actual omission notice before accepting complete units.
        let mut bytes = 0usize;
        let mut selected = None;
        for (index, unit) in c
            .parts
            .iter()
            .take(c.parts.len().saturating_sub(1))
            .enumerate()
        {
            bytes += unit.len() + usize::from(index > 0);
            let units = index + 1;
            let tokens = Self::cost(c, bytes + 1 + Self::notice_bytes(c, c.parts.len() - units));
            if tokens > budget {
                break;
            }
            selected = Some(Self {
                contribution: c,
                units: Some(units),
                tokens,
            });
        }
        selected
    }

    pub(super) fn append_body(&self, out: &mut String) {
        let c = self.contribution;
        match self.units {
            None => {
                out.push_str(&c.content);
                if c.source_truncated {
                    if !c.content.is_empty() {
                        out.push('\n');
                    }
                    Self::append_notice(c, 0, out);
                }
            }
            Some(units) => {
                for (index, unit) in c.parts[..units].iter().enumerate() {
                    if index > 0 {
                        out.push('\n');
                    }
                    out.push_str(unit);
                }
                out.push('\n');
                Self::append_notice(c, c.parts.len() - units, out);
            }
        }
    }
}

/// The reservation retains the exact source units rendering will emit. An
/// equal-salience list cannot spend a peer's reservation; KV placement never
/// determines whether standing activity context may be evicted.
#[derive(Default)]
pub(super) struct GroundingPlan<'a> {
    pub(super) pieces: Vec<GroundingPiece<'a>>,
}

/// A plan that fit as asked, or one that had to give something up — with the size it
/// WANTED carried out, so the caller can say by how much rather than only that it did.
/// A reduction that cannot report its own magnitude is the saturating-subtract mistake
/// one layer up.
pub(super) enum Fitted<T> {
    Whole(T),
    Reduced { plan: T, wanted: usize },
}

impl<'a> GroundingPlan<'a> {
    /// The standing sources' truthful minimums, whatever the budget — the plan the
    /// caller WANTS. Fitting it is [`GroundingPlan::within_budget`]'s job.
    pub(super) fn minimum(ws: &'a Workspace) -> Self {
        Self {
            pieces: ws
                .broadcast
                .iter()
                .filter(|c| c.decision.is_none() && c.standing_grounding)
                .map(GroundingPiece::minimum)
                .collect(),
        }
    }

    /// Fit the standing minimums into `budget`, SHEDDING rather than failing.
    ///
    /// This returned `Err` until 2026-09-09, and that error muted citizens: it was
    /// promoted ahead of every other capacity check into a hard deliberation fault, so a
    /// persona whose enrichment overflowed refused the turn entirely — and kept her held
    /// card while refusing. Grounding is enrichment; its shortfall is not a reason to
    /// stop thinking.
    ///
    /// What sheds first, and why: the SYSTEM half goes before the TRAILING half. Trailing
    /// pieces sit nearest the generation and carry the act-shaped facts (the pinned
    /// result, the live board); the system half is standing background, and dropping it
    /// also frees `working_context_header_cost` in one go. Pieces are shed newest-kept /
    /// oldest-first within each half, so what survives is the freshest world she has.
    pub(super) fn within_budget(ws: &'a Workspace, budget: usize) -> Fitted<Self> {
        let whole = Self::minimum(ws);
        let wanted = whole.system_reserve() + whole.trailing_tokens();
        if wanted <= budget {
            return Fitted::Whole(whole);
        }
        // Trailing-only: keep the half nearest generation, drop the standing background
        // and its header.
        let mut plan = whole;
        plan.pieces.retain(|p| p.contribution.trailing);
        // Still over: shed oldest-first until it fits. A single piece larger than the
        // whole budget leaves an empty plan — honestly nothing, never a truncated
        // fragment that reads as complete.
        while plan.trailing_tokens() > budget && !plan.pieces.is_empty() {
            plan.pieces.remove(0);
        }
        Fitted::Reduced { plan, wanted }
    }

    /// Spend `budget` on optional units. A source `admitted` names keeps that size while it
    /// fits; the rest take what they can.
    ///
    /// `budget` is the room for a new admission; `held_budget` is the room an admitted source
    /// may keep its size in (it includes the growth room the admission left, which the body's
    /// growth is entitled to spend, never the head).
    pub(super) fn expand_within(&mut self, budget: usize, held_budget: usize, admitted: impl Fn(&Contribution) -> Option<Option<usize>>) {
        let mut used = self.system_reserve() + self.trailing_tokens();
        // Every minimum remains owned before optional details can spend spare
        // room. Preserve broadcast order for standing sources' trailing KV tier.
        for piece in &mut self.pieces {
            let c = piece.contribution;
            let available = budget.saturating_sub(used) + piece.tokens;
            let held_room = held_budget.saturating_sub(used) + piece.tokens;
            let held = admitted(c).map(|units| GroundingPiece::at(c, units)).filter(|p| p.tokens <= held_room);
            if let Some(expanded) = held.or_else(|| GroundingPiece::within(c, available)) {
                used = used - piece.tokens + expanded.tokens;
                *piece = expanded;
            }
        }
    }

    pub(super) fn trailing_tokens(&self) -> usize {
        self.pieces
            .iter()
            .filter(|p| p.contribution.trailing)
            .map(|p| p.tokens)
            .sum()
    }

    pub(super) fn system_tokens(&self) -> usize {
        self.pieces
            .iter()
            .filter(|p| !p.contribution.trailing)
            .map(|p| p.tokens)
            .sum()
    }

    pub(super) fn system_reserve(&self) -> usize {
        let tokens = self.system_tokens();
        tokens
            + if tokens > 0 {
                LlmDeliberationFaculty::working_context_header_cost()
            } else {
                0
            }
    }

    /// The trailing standing pieces as keyed blocks for her epoch, in plan order: within an
    /// epoch an unchanged piece is already said wherever it sits, so no wire order is needed
    /// for stability (the `stable_prefix_order` sort that lived here, card c119ace7).
    pub(super) fn trailing_blocks(&self) -> Vec<epoch_view::Block> {
        epoch_view::contribution_blocks(self.pieces.iter().filter(|p| p.contribution.trailing).map(|piece| {
            let mut body = String::with_capacity(piece.tokens * GUARD_CHARS_PER_TOKEN);
            piece.append_body(&mut body);
            (piece.contribution.faculty.as_str(), body)
        }))
    }
}
