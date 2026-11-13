use super::*;
use super::grounding::{Body, GroundingOffer, GroundingPiece};

impl LlmDeliberationFaculty {
    /// What ALL the grounding offered this turn would cost if every contribution were shown whole:
    /// the grounding half of a turn's demand ([`super::super::super::working_set`]). Measured
    /// 2026-08-06 this is the dominant term: the work board alone offered a median 5,364 tokens
    /// into a context budget with a median of 55.
    pub(in super::super) fn assembled_context_cost(ws: &Workspace) -> usize {
        ws.broadcast
            .iter()
            .filter(|c| c.decision.is_none() && !c.trailing)
            .map(|c| GroundingPiece::whole(c).tokens)
            .sum()
    }

    /// The assembled context within `budget_tokens` for its pieces (its header and message wrapper
    /// beside them): each contribution at the most detail the budget holds, the most important
    /// first (`grounding.rs`). What the act's one allocation does with its grounding claim, for a
    /// caller holding only a budget.
    #[cfg(test)]
    pub(in super::super) fn render_assembled_context_within(&self, ws: &Workspace, budget_tokens: usize, composition: ContextComposition) -> String {
        let offer = GroundingOffer::of(ws, &|tokens| tokens as u32);
        let budget = (budget_tokens + Self::working_context_header_cost()) as u32;
        let choice = offer.leveled.reduce(budget);
        let rungs = if choice.cost <= budget { choice.rungs } else { Vec::new() };
        self.render_grounding(ws, offer, &rungs, ContextComposition { ctx_floor: budget_tokens, ..composition })
    }

    /// Render the assembled context (the phase-1 winners that hold the workspace and did not ride
    /// her view) at the levels the act's one allocation chose (`rungs`, one per contribution in
    /// the offer's order; none when the grounding left the turn), into the body of the
    /// working-context message, which rides the fresh tail after her view, never the system
    /// message.
    pub(in super::super) fn render_grounding(&self, ws: &Workspace, offer: GroundingOffer<'_>, rungs: &[usize], composition: ContextComposition) -> String {
        let ContextComposition { context_window, framing_tokens, conversation_tokens, ctx_floor, after_framing, tool_tokens, surface_reason } = composition;
        // Offered trailing content before fitting, not an additional claimant: its retained
        // portion is already in conversation_tokens.
        let offered_trailing_tokens: usize = ws.broadcast.iter().filter(|c| c.decision.is_none() && c.trailing).map(|c| estimate_guard_tokens(&c.content)).sum();
        let received = ws.broadcast.iter().filter(|c| c.decision.is_none() && !c.trailing).count();
        let pieces = offer.rendered(rungs);
        let level = |piece: &GroundingPiece<'_>| match &piece.body {
            Body::Whole => "whole".to_string(),
            Body::Units(units) => format!("{units}/{} units", piece.contribution.parts.len()),
            Body::Summary(_) => "summary".to_string(),
            Body::Handle => "handle".to_string(),
        };
        // Received-vs-rendered receipt at the ONE seam where a surfaced contribution's detail is
        // decided (#130): every contribution at its level, and what left the turn.
        continuum_base::probe!(
            class = "delib.context.render",
            persona = %self.persona_name,
            budget_tokens = ctx_floor,
            context_window,
            framing_tokens,
            conversation_tokens,
            offered_trailing_tokens,
            after_framing,
            // Actual selected schemas, not the full registry's withheld cost.
            tool_tokens,
            registry_tool_tokens = self.describe_tool_tokens(),
            surface_reason = ?surface_reason,
            used_tokens = pieces.iter().map(|p| p.tokens).sum::<usize>(),
            received,
            rendered = pieces.len(),
            kept = %pieces.iter().map(|p| format!("{}({}, sal={:.2})", p.contribution.faculty.as_str(), level(p), p.contribution.salience)).collect::<Vec<_>>().join(","),
            "assembled context: {}/{} contributions, each at the level the allocation chose", pieces.len(), received
        );
        let mut block = String::new();
        for piece in pieces {
            let c = piece.contribution;
            block.push_str("\n[");
            block.push_str(c.faculty.as_str());
            block.push_str("]\n");
            piece.append_body(&mut block);
            block.push('\n');
        }
        block
    }

    /// Assemble the system prompt for this turn. Thin adapter over
    /// [`deliberation_prompt::compose_split`]: the faculty owns the *inputs* (its identity,
    /// tools, the turn's structural flags); the framing prose + block ordering live in
    /// [`super::super::super::deliberation_prompt`] as a procedural list of named, gated blocks.
    /// It takes no room context: the system message is hers in every room, and a room's
    /// assembled context renders after her view (`prompt_view_with_visual_feedback`).
    pub fn compose_system(
        &self,
        expanded: &BTreeSet<String>,
        directed: bool,
        self_initiated: bool,
        now_ms: Option<u64>,
    ) -> String {
        self.compose_system_holding(expanded, directed, self_initiated, now_ms, false)
    }

    /// [`Self::compose_system`] with the held-work contract flag. The plain form
    /// defaults it false so the many framing-shape tests (which have no workspace)
    /// keep their signature; the live prompt path derives it from `ws` and calls
    /// this directly.
    fn compose_system_holding(
        &self,
        expanded: &BTreeSet<String>,
        directed: bool,
        self_initiated: bool,
        now_ms: Option<u64>,
        holds_live_work: bool,
    ) -> String {
        // Whole-string form (stable ++ trailing), byte-identical to the pre-split output.
        // Kept for the many framing-shape tests that assert against the composed whole; the
        // LIVE prompt path calls `compose_system_split` and places the two parts separately.
        let c = self.compose_system_split(
            expanded,
            directed,
            self_initiated,
            now_ms,
            holds_live_work,
        );
        let mut s = c.stable;
        s.push_str(&c.trailing);
        s
    }

    /// The system prompt split at the KV-cache boundary (#266): a persona-invariant
    /// [`stable`](deliberation_prompt::ComposedSystemPrompt::stable) prefix
    /// (identity + `[Taking your turn]` + tools) that belongs in the cacheable system
    /// message, and the per-turn
    /// [`trailing`](deliberation_prompt::ComposedSystemPrompt::trailing) framing + clock
    /// that the live path rides on the newest turn so it never invalidates the cached
    /// prefix. The whole-string [`compose_system_holding`] is `stable ++ trailing` of
    /// exactly this.
    pub fn compose_system_split(
        &self,
        expanded: &BTreeSet<String>,
        directed: bool,
        self_initiated: bool,
        now_ms: Option<u64>,
        holds_live_work: bool,
    ) -> deliberation_prompt::ComposedSystemPrompt {
        deliberation_prompt::compose_split(&deliberation_prompt::SystemPromptParts {
            system_prompt: &self.system_prompt,
            persona_name: &self.persona_name,
            tools: &self.tools,
            expanded,
            directed,
            now_ms,
            self_initiated,
            holds_live_work,
        })
    }
}
