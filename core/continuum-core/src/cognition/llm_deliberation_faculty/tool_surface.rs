use super::*;

impl LlmDeliberationFaculty {
    /// Rebuild the native `commands/list` + `commands/help` offering (the DISCOVERY
    /// PAIR) from the authorized set. Called ONCE whenever the tool set changes (tool
    /// assignment), NEVER on the per-tick path. The prompt-facing tool MENU is no
    /// longer cached here: it is an EXPANDABLE BOOKMARKED render
    /// ([`persona_tools::render_tool_menu`]) whose per-category expansion depends on
    /// what the persona is doing THIS turn, so `compose_system` builds it per-tick
    /// from `self.tools` + the turn's `expanded` set (computed once in
    /// [`Self::prompt_view`] — see [`Self::expanded_categories`]). The menu is small
    /// by construction (a spine of headers + a few opened categories), so it needs no
    /// window-budget reflow.
    pub(super) fn rebuild_tool_surface(&mut self) {
        self.prompt_feedback.store(None); // New schema bytes invalidate prior prompt observations.
        if self.tools.is_empty() {
            self.native_specs.clear();
            self.hands_specs.clear();
            self.review_hands_specs.clear();
            self.native_command_names.clear();
            self.tool_surface_tokens = 0;
            self.hands_surface_tokens = 0;
            self.review_hands_surface_tokens = 0;
            return;
        }
        // Offer the working set in the WIRE DIALECT, charset-legal per the OpenAI
        // function-name spec our slashed names violate. WHICH name a command is
        // offered under is a per-model POLICY ([`tool_dialect::offer_style_for`],
        // the adaptive-surface seam): our canonical `code_read` to converge a model
        // onto our namespace, or its trained reflex `read_file` to meet its tuning.
        // Either way calls map back to canonical commands on return (ONE section:
        // [`crate::ai::tool_dialect`]). [[joel-boundary-design-values]]
        let style =
            crate::ai::tool_dialect::offer_style_for(Some(self.binding.load().requested_model()));
        // The native surface is the DERIVED, bounded agentic core — every command that
        // declares `native: true` at its own site (~a dozen tools), projected once by
        // `native_tool_specs()`. It is offered in FULL, always, in the model's wire dialect.
        //
        // NO second filter here. There used to be a window-vs-surface CLIFF that amputated
        // this set to a two-tool "discovery pair" on a tight window. That is a clamp, and the
        // worst kind: a native-tool-call model can ONLY emit calls for tools in its offered
        // specs (the "long tail reachable by name" is a text-model affordance it lacks), so the
        // discovery-pair-only surface stranded it in a `commands/help(code/write)` loop and
        // wrote nothing (glass-boxed #206). Worse, the threshold sat at the served window's
        // knife-edge, so the SAME model flipped 10/10 ↔ 0/6 on a token — benchmark noise from
        // the filter, not signal from the model.
        //
        // Fitting the surface to the window is ONE decision, and it already lives in exactly
        // one place: `prompt_view_within`, which reserves THESE specs' tokens up front
        // (`describe_tool_tokens`, counted dynamically off this very set) and trims the VOLATILE
        // context (recall/RAG) last — protecting the hands, never amputating them. Filtering the
        // tool surface a second time here, on a whim, is the replicated-logic clamp that breeds
        // brittleness and tanks benchmark hygiene. So it is gone; the budget owns the fit.
        // [[filter-once-centrally-multiple-adhoc-filters-are-clamps-that-tank-benchmarks]]
        // [[budget-at-assembly-never-clamp-the-prompt]]
        let raw = persona_tools::native_tool_specs();
        self.native_command_names = raw.iter().map(|s| s.name.clone()).collect();
        // A capable citizen's EXTENDED hands (the web, push and PR verbs) go on the wire when
        // the served window's tool share holds them; on a window that cannot, she keeps the
        // core working set and the rest stay one `commands/list` away. The window decides
        // what fits, never who is capable: an 8k window spending its whole share on tools
        // leaves her newest line nowhere to go (card dec1a7ff's survival check).
        let to_wire = |specs: Vec<NativeToolSpec>| -> Vec<NativeToolSpec> {
            specs.into_iter().map(|s| crate::ai::tool_dialect::to_wire_spec_with(s, style)).collect()
        };
        let full_hands = to_wire(hands_surface(&raw));
        let share = super::super::context_budget::ContextBudget::from_window(self.binding.load().context_window).tool_surface_tokens();
        self.hands_specs = if Self::tool_surface_tokens_of(&full_hands) <= share {
            full_hands
        } else {
            to_wire(core_hands(&raw))
        };
        self.review_hands_specs = self.hands_specs.clone();
        self.review_hands_specs.extend(to_wire(
            raw.iter()
                .filter(|s| crate::ai::tool_dialect::reviewer_hand(&s.name))
                .cloned()
                .collect(),
        ));
        self.native_specs = raw
            .into_iter()
            .map(|s| crate::ai::tool_dialect::to_wire_spec_with(s, style))
            .collect();
        self.tool_surface_tokens = Self::tool_surface_tokens_of(&self.native_specs);
        self.hands_surface_tokens = Self::tool_surface_tokens_of(&self.hands_specs);
        self.review_hands_surface_tokens = Self::tool_surface_tokens_of(&self.review_hands_specs);
    }

    /// The tool surface THIS turn will send, what it COSTS, and WHY — decided once.
    ///
    /// # Why this is a struct and not a token count
    ///
    /// Before card dec1a7ff the decision lived in `contribute` and the accounting
    /// lived in [`Self::prompt_view_within`], 80 lines and one method apart. The
    /// accounting could not see the decision, so it priced the FULL registry
    /// unconditionally via `describe_tool_tokens`. That was correct until #3829 made
    /// the surface a per-turn choice; afterwards one accessor meant two different
    /// things — "what the whole registry costs" and "what this turn is sending" —
    /// and they diverged on every work turn.
    ///
    /// The harm was not a wrong number. `prompt_view_within` subtracts the tool cost
    /// from the message budget, so over-pricing trims the conversation to fit room
    /// that was never occupied: the citizen lost grounding she had window for. That
    /// is the #206 cliff's harm class arriving through the ACCOUNTING rather than
    /// through the surface.
    ///
    /// So the reason travels WITH the number. It has to, because the same value is
    /// right for one reason and wrong for another — see
    /// [`SelectedSurface::demand_tokens`].
    pub(super) fn select_tool_surface(&self, ws: &Workspace, context_window: u32) -> SelectedSurface<'_> {
        if self.native_specs.is_empty() {
            return SelectedSurface {
                specs: None,
                tokens: 0,
                reason: SurfaceReason::NoTools,
            };
        }
        // S1: the ROOM's recipe decides first. When its affordances name any of her
        // native verbs, that set (plus the discovery pair) is the surface — a rule,
        // authored as data, reaching her hands. It outranks focus and budget because
        // both of those narrow a surface the room has already bounded. Empty (the
        // ordinary room today) or matching nothing native falls through to the
        // pre-S1 arms unchanged.
        if let Some((specs, tokens)) = self.room_surface(ws) {
            return SelectedSurface {
                specs: Some(std::borrow::Cow::Owned(specs)),
                tokens,
                reason: SurfaceReason::RoomAffordances,
            };
        }
        // A work turn takes her HANDS for FOCUS, not for size — that choice predates
        // the window budget and is independent of it.
        let is_work_turn = self.is_work_turn(ws);
        let tool_budget =
            super::super::context_budget::ContextBudget::from_window(context_window).tool_surface_tokens();
        let surface_fits = self.describe_tool_tokens() <= tool_budget;
        // Order matters for the REASON, not for the outcome: a work turn that also
        // overflows is still reported as HandsForWork, because focus is why she was
        // never going to get the full surface on this turn.
        let reason = if is_work_turn {
            SurfaceReason::HandsForWork
        } else if !surface_fits {
            SurfaceReason::HandsForBudget
        } else {
            SurfaceReason::Full
        };
        match reason {
            SurfaceReason::Full => SelectedSurface {
                specs: Some(std::borrow::Cow::Borrowed(&self.native_specs)),
                tokens: self.tool_surface_tokens,
                reason,
            },
            // A REVIEW card's work turn: her hands plus the reviewer verbs, because
            // filing the verdict is the card (Kimi, review card deec4ac2, 2026-10-06).
            SurfaceReason::HandsForWork if ws.reviewing => SelectedSurface {
                specs: Some(std::borrow::Cow::Borrowed(&self.review_hands_specs)),
                tokens: self.review_hands_surface_tokens,
                reason,
            },
            // The fallback is hands, NEVER nothing: `hands_surface` keeps the
            // `commands/` discovery pair, so a withheld verb stays one
            // `commands/list` away. Amputating the surface is the #206 cliff
            // (14/14 SWE acts spent on `commands/help`, 0 edits).
            _ => SelectedSurface {
                specs: Some(std::borrow::Cow::Borrowed(&self.hands_specs)),
                tokens: self.hands_surface_tokens,
                reason,
            },
        }
    }

    /// The surface the ROOM selects, or `None` when the room does not select one.
    ///
    /// `None` on two honest grounds: the recipe declares no affordances (the ordinary
    /// room, pre-S1 behaviour), or it declares some and NONE is a native verb of hers
    /// — a recipe naming `mail/send` on a node with no such tool. The second case is
    /// probed by name, because an author who wrote a rule that binds nothing should
    /// learn that from the ledger, not from a citizen who never acted. In neither
    /// case is the surface ever empty: the fallback is the pre-S1 selection.
    pub(super) fn room_surface(&self, ws: &Workspace) -> Option<(Vec<NativeToolSpec>, usize)> {
        if ws.room_affordances.is_empty() {
            return None;
        }
        let allowed: std::collections::HashSet<&str> =
            ws.room_affordances.iter().map(String::as_str).collect();
        let mut granted = 0usize;
        let specs: Vec<NativeToolSpec> = self
            .native_command_names
            .iter()
            .zip(self.native_specs.iter())
            .filter(|(name, _)| {
                let name = name.as_str();
                if allowed.contains(name) {
                    granted += 1;
                    true
                } else {
                    // The discovery pair always rides: a withheld verb stays one
                    // `commands/list` away (the #206 cliff guard, unchanged). So do her own
                    // mind's verbs (focus/continue, focus/nudge, focus/mute): a room's
                    // recipe is the rules of that room, never rules of her mind (HER-LOOP).
                    // Kimi, 2026-10-05, in a recipe room on the build that put focus/ in
                    // her hands: "help works; nothing to invoke against."
                    name.starts_with("commands/") || name.starts_with("focus/")
                }
            })
            .map(|(_, spec)| spec.clone())
            .collect();
        if granted == 0 {
            continuum_base::probe!(
                class = "delib.surface.room_affordances_unmatched",
                persona = %self.persona_name,
                room = %ws.activity_room_id,
                declared = ws.room_affordances.len(),
                "the room's recipe declares affordances and none names a native verb — \
                 the rule binds nothing here; offering the pre-S1 surface instead"
            );
            return None;
        }
        let tokens = Self::tool_surface_tokens_of(&specs);
        Some((specs, tokens))
    }

    /// Serde-and-count the tool surface ONCE per rebuild — the only place the
    /// per-spec serialization cost is ever paid.
    pub(super) fn tool_surface_tokens_of(specs: &[NativeToolSpec]) -> usize {
        // context-budget-exempt: fixed per-tool schema overhead in the template — same reason as PER_MESSAGE_TEMPLATE_TOKENS
        const PER_TOOL_TEMPLATE_MARGIN_TOKENS: usize = 8;
        specs
            .iter()
            .map(|spec| {
                let serialized = serde_json::to_string(spec).unwrap_or_default();
                estimate_guard_tokens(&serialized) + PER_TOOL_TEMPLATE_MARGIN_TOKENS
            })
            .sum()
    }

    /// A citizen served below [`Self::min_window_for_agentic_surface`] provably cannot hold
    /// her framing, her tools, AND room to answer at the same time. She does not fail loudly
    /// on her own — she goes quiet, or loops asking for help she has no room to receive.
    /// That is the silent-degradation shape this codebase keeps paying for, so make it
    /// LOUD at the moment the window is set, with the arithmetic attached.
    ///
    /// This is a SENSOR, not a policy — deliberately neither a clamp nor a refusal:
    ///   - Not a clamp — amputating the tool surface to fit is exactly the deleted #206
    ///     cliff (see `rebuild_tool_surface`); it produced a model that could not act and
    ///     flipped 10/10 ↔ 0/6 on one token of window. A hardcoded threshold IS the
    ///     brittleness.
    ///   - Not a refusal — a small-window persona is still a citizen who can talk, and on
    ///     the hardware this project exists to serve (an old laptop, no cloud) a
    ///     mute-for-tools persona beats no persona.
    ///
    /// What it emits is a MEASURED DEMAND — served vs needed, with the terms broken out —
    /// which is precisely the input an actuator needs to do something intelligent: raise the
    /// lane's `-c`, re-home to a roomier model, or rebalance against the other consumers.
    /// The decision belongs to the governor, which sees the whole machine and can learn from
    /// outcomes; this faculty only knows its own arithmetic and must not pretend otherwise.
    /// Joel 2026-08-06: "daemons must be intelligent and quality-of-experience driven, not
    /// hardcoded or simplistic algorithms — ever present governors working and LEARNING."
    /// A static clamp here would be the opposite of that; a truthful number is what makes
    /// the learning possible.
    ///
    /// The measured floor as of 2026-08-06: tools 4609 + framing 1643 ⇒ **8336 tokens**.
    /// `inference::planning::MIN_SERVE_CTX` is 2048 — FOUR TIMES below it. So the substrate will
    /// happily serve a window at which no citizen can act, which is #327.
    pub(super) fn warn_if_window_cannot_host_the_agentic_surface(&self) {
        if self.native_specs.is_empty() {
            return; // no hands offered — nothing to fit, nothing to warn about
        }
        let window = self.binding.load().context_window;
        let needed = self.min_window_for_agentic_surface();
        if window >= needed {
            return;
        }
        let tools = self.describe_tool_tokens();
        continuum_base::probe!(
            class = "persona.window.cannot_host_tools",
            persona = %self.persona_name,
            window = window,
            needed = needed,
            tool_tokens = tools,
            framing_tokens = self.framing_floor_tokens(),
            reply_floor_tokens = Self::COMPLETION_FLOOR_TOKENS,
            "served window is too small to hold framing + tools + a reply — this citizen \
             will struggle to ACT (she can still speak). Raise the window to at least \
             `needed`, or serve a model with a larger context."
        );
    }

    /// Conservative token estimate of the ONE natively-offered tool spec
    /// (`commands/help`) the gateway injects via the chat template — its serialized
    /// function schema plus a small template framing margin. 0 when the persona has
    /// no tools. Counted with the same conservative guard ratio as the rest of the
    /// prompt (round UP — under-counting risks the 500). Cheap and pure.
    ///
    /// # This is THOUSANDS of tokens, and the sentence that used to live here said otherwise
    ///
    /// It read "a single tiny schema, so this is a handful of tokens, not the old
    /// full-registry dump" — true when the surface WAS the two-tool discovery pair,
    /// false since #206 deliberately restored the full native set. The probe site in
    /// `render_assembled_context_within` has carried a comment calling this sentence
    /// out as stale since 2026-08-06 ("reading that stale sentence is exactly why the
    /// right suspect got dismissed") — 700 lines from the function it describes, so
    /// anyone reading the function itself still got the lie. Deleting it here is the
    /// half that never got done.
    ///
    /// MEASURED: **4,609 tokens on 2026-08-06. 7,013 on 2026-08-20** — 31% of a 22,528
    /// window, subtracted before framing or a single message, on every turn.
    ///
    /// # It grows on its own, and nothing watches it
    ///
    /// [`persona_tools::native_tool_specs`] is DERIVED — every command declaring
    /// `NATIVE = true` is offered automatically. That is the right architecture (no
    /// central array to edit) and it means the per-turn cost of the tool surface is an
    /// unbounded side effect of a flag set in another file: +52% in two weeks, noticed
    /// by nobody. Nothing connects "I marked my command native" to "every citizen
    /// permanently lost N tokens of grounding" (#460). Do NOT respond by trimming the
    /// set — the amputated surface is the #206 cliff and measured 14/14 SWE acts as
    /// `commands/help` with 0 edits.
    pub(super) fn describe_tool_tokens(&self) -> usize {
        self.tool_surface_tokens
    }
}

/// The tool surface a turn actually sends, its cost, and why it is that surface.
///
/// One decision, consulted by both the budget math and the request builder, so the
/// two can no longer disagree about what is on the wire (card dec1a7ff).
pub(crate) struct SelectedSurface<'a> {
    /// Exactly what goes into the request. `None` only when there are no tools.
    /// Borrowed from the faculty's cache on every pre-S1 arm; owned only when the
    /// room's affordances selected it (a per-turn subset, S1).
    pub(super) specs: Option<std::borrow::Cow<'a, [NativeToolSpec]>>,
    /// Cost of what is actually SENT — the number the message budget must use.
    pub(super) tokens: usize,
    pub(super) reason: SurfaceReason,
}

impl SelectedSurface<'_> {
    /// What an UNTRUNCATED demand measurement should price for this turn.
    ///
    /// `delib.turn.demand` is deliberately the demand, not the send: its contract is
    /// that "every term is the UNTRUNCATED one", so that a p95 of what-was-sent can
    /// never re-derive the cap that produced it and freeze it forever. That contract
    /// makes the right term depend on WHY the surface narrowed:
    ///
    /// - [`SurfaceReason::HandsForBudget`] — the full surface WAS wanted and the
    ///   window refused it. Demand legitimately includes the full cost; reporting
    ///   the hands cost here would hide exactly the pressure the probe exists to
    ///   measure.
    /// - every other arm — the full surface was never wanted (focus, or there are no
    ///   tools, or it fit and was sent). Pricing it would inflate `over_window` and
    ///   report a turn as near-overflow when it is not.
    ///
    /// Same value, two verdicts, decided by the reason. That is the whole argument
    /// for the reason existing.
    pub(super) fn demand_tokens(&self, full_surface_tokens: usize) -> usize {
        match self.reason {
            SurfaceReason::HandsForBudget => full_surface_tokens,
            _ => self.tokens,
        }
    }
}
