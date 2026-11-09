//! A grounding contribution's body as her view renders it (`context_streams::blocks`, built in
//! `messages.rs`): whole, with its source's own omission notice when the source cut it, or at its
//! truthful minimum (its first unit and the notice naming the verb that yields the rest), what the
//! view collapses a standing block to when the window is short. Every grounding contribution is a
//! block of her view; none renders outside it.

use super::*;

/// What one contribution renders at one of its levels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) enum Body {
    /// Its content as delivered.
    Whole,
    /// Its first `n` units and the omission notice.
    Units(usize),
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A board as the kanban source delivers it: its two leads, then one line per card.
    fn board(cards: usize, expand: Option<&'static str>) -> Contribution {
        let mut parts = vec!["[your work] you HOLD 1 card".to_string(), "[available work] cards are claimable".to_string()];
        parts.extend((0..cards).map(|i| format!("card {i:08x} [Open] a card on the board")));
        Contribution::context(FacultyId::Custom("room-kanban".to_string()), parts.join("\n"), 0.9, "the board")
            .with_parts(parts)
            .with_expand_command(expand)
            .standing_grounding()
    }

    fn render(piece: GroundingPiece<'_>) -> String {
        let mut text = String::new();
        piece.append_body(&mut text);
        text
    }

    // what this catches: a collapsed view block she cannot act on or that lies about its size
    // (ported from the working-context message, deleted as dead: every grounding is a view block).
    // Its minimum keeps the leads, names the verb verbatim with the true total, and states only the
    // omission when no verb yields the rest; the board's 60 cards never ride along. [[command-names-must-be-accurate]]
    #[test]
    fn a_collapsed_block_keeps_its_leads_and_names_the_exact_verb_with_its_total() {
        let c = board(60, Some("work/list"));
        let minimum = render(GroundingPiece::minimum(&c));
        assert!(minimum.starts_with("[your work]"), "{minimum}");
        assert!(!minimum.contains("card 0000003b"), "{minimum}");
        assert!(minimum.contains(&format!("run `work/list` to see all {}", c.parts.len())), "{minimum}");
        let mute = board(60, None);
        let minimum = render(GroundingPiece::minimum(&mute));
        assert!(minimum.contains("more not shown") && !minimum.contains("run `"), "{minimum}");
    }

    // what this catches (card e049aa82): a cut the SOURCE made, invisible to her. Whole, a block
    // its source cut short still says more exists and names the verb with no invented total; an
    // empty partial delivery keeps its notice; a complete block carries none.
    #[test]
    fn a_block_its_source_cut_short_says_so_even_when_shown_whole() {
        let cut = board(2, Some("work/list")).with_source_truncated(true);
        let whole = render(GroundingPiece::whole(&cut));
        assert!(whole.contains("…more not shown (context budget) — run `work/list` to see all"), "{whole}");
        assert!(!whole.contains("to see all 4"), "the delivered parts are not the source's total: {whole}");
        let empty = Contribution::context(FacultyId::Custom("room-board".to_string()), String::new(), 0.5, "nothing fit")
            .with_expand_command(Some("work/list"))
            .with_source_truncated(true);
        assert!(render(GroundingPiece::whole(&empty)).contains("run `work/list` to see all"));
        assert!(!render(GroundingPiece::whole(&board(2, Some("work/list")))).contains("more not shown"));
    }

    // what this catches: divisibility leaking. An indivisible contribution (an engram) has no
    // truthful cut, so its minimum is its whole, never a confident half-thought.
    #[test]
    fn an_indivisible_contribution_is_never_cut_mid_content() {
        let engram = Contribution::context(FacultyId::Recall, "we agreed the gather promise binds every kernel".repeat(8), 0.9, "recalled");
        assert_eq!(render(GroundingPiece::minimum(&engram)), engram.content);
    }
}
