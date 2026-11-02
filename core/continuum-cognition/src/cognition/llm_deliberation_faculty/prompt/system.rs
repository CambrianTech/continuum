use super::*;

impl LlmDeliberationFaculty {
    /// Render the assembled context (the phase-1 winners that hold the workspace and did
    /// not ride her view) into the body of the working-context message, which the prompt
    /// places in the fresh tail after her view, never in the system message. Only
    /// *context* contributions are included (the verdict isn't in the broadcast yet at
    /// phase 2).
    ///
    /// Bounded to `budget_tokens`: context is enrichment, so under window pressure
    /// it yields FIRST (after the essential framing + the burst she is responding
    /// to). Contributions are taken highest-salience-first and dropped WHOLE — a
    /// half-truncated engram is noise, so a lower-salience item that still fits is
    /// preferred over a mangled high-salience one. Same drop-whole-in-priority
    /// philosophy as `FlexboxRagBudgetAdapter`.
    /// What ALL the grounding offered this turn would cost if none of it had to be
    /// dropped — the grounding half of a turn's demand ([`super::super::super::working_set`]).
    ///
    /// Counted over the same set `render_assembled_context_within` selects from, with
    /// the same per-item header charge, so the two agree on what a contribution costs
    /// and only disagree on how much of it survives. Measured 2026-08-06 this is the
    /// dominant term: the work board alone offered a median 5,364 tokens into a
    /// context budget with a median of 55.
    pub(in super::super) fn assembled_context_cost(ws: &Workspace) -> usize {
        ws.broadcast
            .iter()
            .filter(|c| c.decision.is_none() && !c.trailing)
            .map(|c| Self::context_piece_tokens(c.faculty.as_str(), c.content.len()))
            .sum()
    }

    /// Inputs measured at the request boundary, including the selected native
    /// surface, let the render receipt reconcile with the whole-window budget.
    pub(in super::super) fn render_assembled_context_within(
        &self,
        ws: &Workspace,
        budget_tokens: usize,
        plan: &GroundingPlan<'_>,
        composition: ContextComposition,
    ) -> String {
        let ContextComposition {
            context_window,
            framing_tokens,
            conversation_tokens,
            ctx_floor,
            after_framing,
            tool_tokens,
            surface_reason,
        } = composition;
        // Offered trailing content before fitting, not an additional claimant.
        // Its retained portion is already included in conversation_tokens; list
        // reduction or history eviction may make the offered amount much larger.
        let offered_trailing_tokens: usize = ws
            .broadcast
            .iter()
            .filter(|c| c.decision.is_none() && c.trailing)
            .map(|c| estimate_guard_tokens(&c.content))
            .sum();
        if budget_tokens == 0 {
            // Received-vs-rendered receipt even on the zero-budget path — a turn
            // whose entire context vanished must say so, not render silently empty
            // (the silver-harbor failure class: recall surfaced with the winning
            // bid, yet [recall] never reached the prompt; WHERE it vanished was
            // undebuggable without this seam probe).
            continuum_base::probe!(
                class = "delib.context.render",
                persona = %self.persona_name,
                budget_tokens = 0usize,
                context_window,
                framing_tokens,
                conversation_tokens,
                offered_trailing_tokens,
                ctx_floor,
                after_framing,
                tool_tokens,
                registry_tool_tokens = self.describe_tool_tokens(),
                surface_reason = ?surface_reason,
                received = ws.broadcast.iter().filter(|c| c.decision.is_none() && !c.trailing).count(),
                rendered = 0usize,
                dropped = %ws
                    .broadcast
                    .iter()
                    .filter(|c| c.decision.is_none() && !c.trailing)
                    .map(|c| c.faculty.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                "assembled context suppressed: zero token budget"
            );
            return String::new();
        }
        // TRAILING contributions (every grounding source's bid, recall, working memory)
        // are excluded HERE — they are sources of her view (`prompt/epoch.rs`). What is
        // left is whatever no producer marked as a view block; it renders after her view.
        let mut ctx: Vec<_> = ws
            .broadcast
            .iter()
            .filter(|c| c.decision.is_none() && !c.trailing)
            .collect();
        ctx.sort_by(|a, b| {
            // Retain the legacy enrichment ordering. Standing source units have
            // already been reserved and cannot be spent by this optional walk.
            let a_held = a.faculty.as_str() == crate::cognition::faculty_ids::ACTIVE_WORK;
            let b_held = b.faculty.as_str() == crate::cognition::faculty_ids::ACTIVE_WORK;
            b_held.cmp(&a_held).then(
                b.salience
                    .partial_cmp(&a.salience)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });
        let received = ctx.len();
        // SELECTION (by salience): walk highest-salience-first and keep what fits.
        //
        // An INDIVISIBLE contribution (one part — a recalled engram, an affect
        // signal) is still kept whole or dropped whole: half an engram is noise,
        // so a smaller lower-salience item that fits is preferred over a mangled
        // high one. That rule was right, and it is unchanged.
        //
        // A DIVISIBLE one (a grounding source that delivered a LIST, and said so
        // via `Contribution::parts`) instead contributes the longest PREFIX of its
        // own units that fits. Its units are ordered by the source, leads first,
        // so a prefix is exactly the summary the source would have written if
        // asked for less — never a mid-sentence cut. This is what un-hides the
        // work board: measured 2026-08-06, `room-kanban` offered a median 5,364
        // tokens all-or-nothing into a median 55-token budget and was kept 0 of
        // 495 times, while its `[your work]` / `[available work]` leads are ~200
        // and carry every fact a citizen needs to find work. The citizens were
        // saying "there are no open tasks available" about a 61-card board.
        let mut selected: Vec<GroundingPiece<'_>> = plan
            .pieces
            .iter()
            .filter(|piece| !piece.contribution.trailing)
            .map(|piece| GroundingPiece {
                contribution: piece.contribution,
                units: piece.units,
                tokens: piece.tokens,
            })
            .collect();
        let mut dropped: Vec<String> = Vec::new();
        // The SALIENCE of each whole-drop, kept structured beside the formatted
        // string so the inversion check below reads a number instead of parsing one
        // back out of a display string (that parse-your-own-log shape is how a
        // sensor starts lying the first time the format changes).
        let mut dropped_salience: Vec<(String, f32)> = Vec::new();
        let mut partial: Vec<String> = Vec::new();
        let mut used = plan.context_tokens();
        for c in ctx {
            if c.standing_grounding {
                continue; // This exact source already owns its selected units above.
            }
            if let Some(piece) = GroundingPiece::within(c, budget_tokens.saturating_sub(used)) {
                used += piece.tokens;
                selected.push(piece);
            } else {
                let tokens = GroundingPiece::whole(c).tokens;
                dropped.push(format!(
                    "{}(sal={:.2},tok={tokens})",
                    c.faculty.as_str(),
                    c.salience
                ));
                dropped_salience.push((c.faculty.as_str().to_string(), c.salience));
            }
        }
        for piece in &selected {
            if let Some(units) = piece.units {
                let c = piece.contribution;
                partial.push(format!(
                    "{}({units}/{} units,tok={}→{})",
                    c.faculty.as_str(),
                    c.parts.len(),
                    GroundingPiece::whole(c).tokens,
                    piece.tokens
                ));
            }
        }
        // Received-vs-rendered receipt at the ONE seam where a surfaced
        // contribution can silently vanish between attention and the prompt.
        // Glass box: what recall/grounding WON upstream must be attributable
        // HERE if it never renders (#130).
        continuum_base::probe!(
            class = "delib.context.render",
            persona = %self.persona_name,
            budget_tokens,
            context_window,
            framing_tokens,
            conversation_tokens,
            offered_trailing_tokens,
            ctx_floor,
            // Actual selected schemas, not the full registry's withheld cost.
            tool_tokens,
            registry_tool_tokens = self.describe_tool_tokens(),
            // Which whole tool level the shared allocation selected.
            surface_reason = ?surface_reason,
            // The pool the budget is subtracted FROM, read from the local that
            // actually holds it. First written as
            // `framing + conversation + budget` — a RECONSTRUCTION from the
            // outputs, which cannot disagree with them and therefore says nothing.
            // A derived field wearing a measurement's name is the same defect as
            // the double-counted `trailing_tokens` it shipped beside.
            // `after_framing − conversation_tokens − header` should equal
            // `budget_tokens`; where it does not, the missing term is visible
            // instead of inferred.
            after_framing,
            used_tokens = used,
            received,
            rendered = selected.len(),
            kept = %selected
                .iter()
                .map(|piece| { let c = piece.contribution; format!("{}(sal={:.2})", c.faculty.as_str(), c.salience) })
                .collect::<Vec<_>>()
                .join(","),
            dropped = %dropped.join(","),
            // Which divisible blocks contributed a PREFIX rather than their whole —
            // without this a shrunken board and a full one look identical in the log.
            partial = %partial.join(","),
            "assembled context: {}/{} contributions fit", selected.len(), received
        );
        // SALIENCE INVERSION — the sufficiency sensor this seam never had.
        //
        // `min_window_for_agentic_surface` answers "can this window hold framing +
        // tools + a reply at all", and its own doc is explicit that clearing it proves
        // nothing: "Necessary, NOT sufficient — clearing it does not mean she can hear
        // you… never 'you are fine above it'." Measured 2026-08-20 that warning came
        // true exactly as written. Served window 22,528 against a bound of ~16.9k, so
        // the necessary-condition sensor was correctly SILENT — while across 25
        // consecutive turns `workspace-map` (salience 0.90, the top tier) was dropped
        // WHOLE on 20 of them and grounding ran 62-1,083 tokens. Atlas could not see
        // the repo she was staged to edit, produced 0 acts for 3 days, and every
        // instrument said healthy.
        //
        // What makes it detectable WITHOUT inventing a threshold: contributions are
        // taken highest-salience-FIRST, so ranking alone can never drop a 0.90 while
        // keeping a 0.30. When that happens anyway it is the SIZE interaction — a big
        // essential block did not fit, then a small unimportant one did. The budget
        // bought the cheap thing and lost the important one. That inversion is a
        // structural fact with no magic constant behind it, which is why it is the
        // signal rather than "salience >= 0.9", a number that would be a snapshot of
        // one day's tuning ([[a-perception-fact-is-honesty-not-an-actuator]]).
        //
        // Divisible blocks are deliberately NOT counted: a prefix-truncated list
        // announces itself in the prompt ("…N more not shown"), so it is degraded but
        // never silent. Only WHOLE drops vanish without a trace — the failure this
        // exists to end. SENSOR, never a clamp: refusing to render, or amputating the
        // tool surface to make room, is the deleted #206 cliff. It reports; the fix
        // for the budget itself is #460 and is not a decision this function may make.
        // TWO holes were in the first cut of this, and the FIRST live turn after deploy
        // found both (2026-08-20, Atlas/Kira/Benchy on build 2281e8b81). Recorded here
        // because the shape is the one this codebase keeps paying for.
        //
        // NOTE (#3874): the `room-board` in the rows below is the WALL under its OLD
        // id, retired in favour of `room-wall`. These lines RECORD A MEASUREMENT taken
        // while that id was live, so they keep it — rewriting a past observation to
        // match present naming would falsify the evidence the hole was found from.
        // HOLE 1 — strict `>` against a population with NO salience spread. The live
        // rows: Atlas kept `roster(0.90), room-kanban(0.90)` and dropped
        // `workspace-map(0.90)`; Benchy kept `room-board(0.90)` and dropped
        // `workspace-map(0.90), room-kanban(0.90)`. EVERYTHING essential bids exactly
        // 0.90, so `0.90 > 0.90` is false and the sensor stayed silent through the
        // precise condition it was written for. The premise "ranking cannot drop a 0.90
        // while keeping a 0.30" is TRUE and VACUOUS: production has no 0.30 at this
        // tier. What actually decides who survives is arrival order and byte size — a
        // coin flip among equals — and losing the repo map to that flip is the whole
        // harm. `>=` is therefore the honest predicate, and a genuine ranking success
        // (0.30 dropped, 0.90 kept) still cannot fire it.
        //
        // HOLE 2, worse — TOTAL starvation was invisible. `min_by` over an EMPTY
        // `selected` yields `None`, the `if let` fell through, and nothing fired. The
        // one state where the citizen got NO grounding at all was the one state the
        // sensor could not report ([[an-absence-is-an-unfinished-measurement]], and
        // mine, in the instrument built to end exactly this).
        let worst_dropped = dropped_salience
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)) // safe: salience is a finite f32; only NaN returns None, and a NaN salience is an upstream bug the SENSOR must survive to report, never panic on
            .cloned();
        if let Some((worst_name, worst_sal)) = worst_dropped {
            let cheapest_kept = selected
                .iter()
                .map(|piece| piece.contribution.salience)
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); // safe: same NaN-only case as the max_by above — an unorderable salience must not take down the instrument that exists to report it
                                                                                       // `None` = nothing survived at all, which is maximal harm, never "no finding".
            let fires = cheapest_kept.is_none_or(|kept| worst_sal >= kept);
            if fires {
                let verdict = match cheapest_kept {
                    None => "TOTAL: nothing survived the budget".to_string(),
                    Some(k) if (worst_sal - k).abs() < f32::EPSILON => {
                        format!("TIE at sal={worst_sal:.2} — decided by size, not by rank")
                    }
                    Some(k) => format!("dropped {worst_sal:.2} while keeping {k:.2}"),
                };
                continuum_base::probe!(
                    class = "delib.context.salience_inversion",
                    persona = %self.persona_name,
                    dropped_faculty = %worst_name,
                    dropped_salience = worst_sal,
                    // -1.0 is an OUT-OF-DOMAIN sentinel, not a fabricated quantity: salience is
                    // [0,1], so -1 cannot be misread as a real one. `None` here is a KNOWN state
                    // ("nothing survived the budget"), never an unknown one — which is the case
                    // the ratchet's doc warns about — and `verdict` states it in words besides.
                    kept_min_salience = cheapest_kept.unwrap_or(-1.0), // safe: see the two lines above

                    kept_count = selected.len(),
                    verdict = %verdict,
                    budget_tokens,
                    context_window,
                    tool_tokens,
                    framing_tokens,
                    "GROUNDING LOST: {worst_name} dropped WHOLE ({verdict}) — the grounding \
                     budget was {budget_tokens} tok against a {context_window} window with \
                     {tool_tokens} in tool schemas and {framing_tokens} in framing. The citizen \
                     is reasoning without what attention ranked top (#460)."
                );
            }
        }
        // SERIALIZATION (by volatility, then CANONICALLY within the stable tier):
        // stable standing-framing FIRST (roster, doctrine, map) so it lands in the
        // cacheable KV-prefix region adjacent to the static system prompt it resembles;
        // volatile grounding (recall, working memory) LAST, nearest the generation point.
        //
        // WHY THE SECOND KEY EXISTS — this tier-split alone did NOT deliver reuse.
        // The original comment here said a stable sort "preserves the salience order
        // WITHIN each tier, so attention ranking is untouched", and treated that as
        // harmless. It is the remaining half of the #266 cache defect. Salience is
        // recomputed EVERY turn, so preserving it as EMIT ORDER re-shuffles the very
        // tier whose whole purpose is to be byte-identical across turns.
        //
        // Measured live 2026-08-21 in Atlas's `pallets__flask-4045` run, from her own
        // captures: consecutive system prompts shared only ~82% of their prefix, and the
        // divergence points were stable-tier blocks trading places —
        // `[room-kanban]` → `[active-work]` → `[workspace-map]` at chars 8216 / 7879 /
        // 8133. Because the system prompt is FIRST, a swap at token ~2,000 invalidates
        // the KV for EVERYTHING after it, including a 34,000-token conversation tail.
        // `--cache-reuse 256` was passed the whole time and every turn reported
        // `cachedTokens: 0` — the cache was correct; we were destroying the prefix.
        // Cost at the measured 111 tok/s prefill: ~306s of re-prefill PER ACT, growing
        // as her conversation grows. One observed turn was 20 minutes of apparent
        // silence that was a single 36k re-prefill.
        //
        // So within the stable tier, order is CANONICAL (by churn class, then faculty
        // name — `deliberation_prompt::stable_prefix_order`, the ONE rule every
        // grounding renderer lays blocks down by), not by salience. Attention ranking is genuinely untouched: salience still decides
        // WHICH contributions are selected — that happened above, against the budget.
        // It simply stops deciding WHERE the survivors sit, which it never needed to.
        // Same set in, same bytes out, every turn.
        //
        // The volatile tier deliberately KEEPS salience order: it is re-prefilled by
        // construction (it changes every turn), so ordering it canonically would buy no
        // reuse and would cost the "most salient nearest the write point" placement
        // that #205 put there on purpose.
        //
        // `false` (stable) sorts before `true` (volatile). See [`Contribution::stable`].
        selected.sort_by(|a, b| {
            let (a, b) = (a.contribution, b.contribution);
            u8::from(!a.stable).cmp(&u8::from(!b.stable)).then_with(|| {
                match (a.stable, b.stable) {
                    // Stable tier: canonical, so the cacheable prefix is deterministic.
                    (true, true) => deliberation_prompt::stable_prefix_order(
                        a.faculty.as_str(),
                        b.faculty.as_str(),
                    ),
                    // Volatile tier: leave salience order alone (stable sort keeps it).
                    _ => std::cmp::Ordering::Equal,
                }
            })
        });
        let mut block = String::new();
        for piece in selected {
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
