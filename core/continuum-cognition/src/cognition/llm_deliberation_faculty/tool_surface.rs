use super::*;

impl LlmDeliberationFaculty {
    /// Rebuild her native surfaces from the registry, in the model's wire dialect. Called
    /// whenever the tool set or the model changes (tool assignment, a window or model set
    /// at spawn), NEVER on the per-tick path. Which name a command is offered under is a
    /// per-model POLICY ([`crate::ai::tool_dialect::offer_style_for`]): our canonical
    /// `code_read`, or a model's trained reflex `read_file`; calls map back to canonical
    /// commands on return. [[joel-boundary-design-values]]
    pub(super) fn rebuild_tool_surface(&mut self) {
        self.prompt_feedback.store(None); // New schema bytes invalidate prior prompt observations.
        *self.tools_epoch.get_mut() = None; // a new surface is selected afresh, never rebuilt from the old one's names
        if self.tools.is_empty() {
            self.surface = ToolSurface::default();
            return;
        }
        let style =
            crate::ai::tool_dialect::offer_style_for(Some(self.binding.load().requested_model()));
        self.surface = ToolSurface::build(persona_tools::native_tool_specs(), self.tools.clone(), |spec| {
            crate::ai::tool_dialect::to_wire_spec_with(spec, style)
        });
        self.share_vocabulary();
    }

    /// Hand her tool head the verbs it chooses from, priced as this surface sends them.
    pub(super) fn share_vocabulary(&self) {
        let Some(memory) = self.working_memory.as_deref() else {
            return;
        };
        match self.surface.vocabulary() {
            Ok(vocab) => memory.tool_head().set_vocabulary(vocab),
            Err(error) => continuum_base::probe!(
                class = "delib.tool_head.vocabulary_refused",
                persona = %self.persona_name,
                error = %error,
                "her tool surface could not become the head's vocabulary; her head stays static"
            ),
        }
    }

    /// One act's price, measured: the engine's prefill and decode rates for her model and the
    /// tokens her acts decode (`working_set::need_of`, p90 with headroom, so a miss is priced
    /// high and the head errs toward holding a verb). The request overhead is not measured
    /// here and is left out. `None` while any of them is unmeasured: then her head stays the
    /// activity's static surface.
    fn act_cost(&self) -> Option<tool_working_set::CostModel> {
        let binding = self.binding.load();
        let model = binding.requested_model();
        let prefill = crate::inference::prefill_rate::measured_rate_for(model).tps?;
        let decode = crate::inference::decode_knee::rate_for(model).tps?;
        let need = self.working_set.as_ref()?.need_of(self.persona_id)?;
        (prefill > 0.0 && decode > 0.0).then(|| tool_working_set::CostModel {
            prefill_tokens_per_s: prefill,
            decode_tokens_per_s: decode,
            act_decode_tokens: f64::from(need.reasoning.saturating_add(need.answer)),
            act_overhead_ms: 0.0,
            samples: need.turns as usize,
        })
    }

    /// Her learned head over `selected` (the activity's surface): that surface plus the verbs her
    /// head holds beyond it (`ToolSurface::learned`), never fewer. It always computes and
    /// reserves the head the next epoch would hold, so a turnover can adopt it and the shadow
    /// receipt (`delib.tool_head.epoch`) measures it. It changes what is SENT only on a server
    /// that takes `tail_tools`, where a verb outside the head stays callable.
    fn with_learned_head<'a>(&'a self, selected: SelectedSurface<'a>, ws: &Workspace, budget: usize) -> SelectedSurface<'a> {
        let Some(memory) = self.working_memory.as_deref() else {
            return selected;
        };
        let binding = self.binding.load();
        let head = memory.tool_head();
        // The next head is chosen only with a measured act cost; the verbs help loaded ride the
        // tail without one (help's "callable on your next act" is its contract, not the head's).
        let next = self.act_cost().and_then(|cost| {
            head.next(
                ws.now_ms.map_or_else(continuum_base::time::EpochMs::now, continuum_base::time::EpochMs::from_millis), // map_or_else: a workspace that carries no clock reading decides at now
                Some(ws.room_id),
                u32::try_from(budget).unwrap_or(u32::MAX), // unwrap_or: a share past u32 bounds nothing a schema set can reach
                &cost,
            )
        });
        if !binding.adapter.accepts_tail_tools(binding.requested_model()) {
            return selected;
        }
        let reserve = next.as_ref().map_or(0, |next| next.tokens() as usize); // map_or: no next head reserves nothing beyond this one
        let (held, loaded) = (head.held(), head.loaded());
        if held.is_none() && loaded.is_empty() {
            return selected.reserving(reserve);
        }
        let held = held.unwrap_or_default(); // unwrap_or_default: no adopted head adds no held verbs; help's loads still ride the tail
        let floor: &[crate::ai::types::NativeToolSpec] = match selected.specs.as_deref() {
            Some(specs) => specs,
            None => &[],
        };
        let learned = self.surface.learned(floor, &held, &loaded, reserve);
        if learned.tokens <= budget {
            return learned;
        }
        // Her floor plus her head's extras exceed the grant: the activity's surface, which fit,
        // goes out instead, and the shortfall is loud (the extras stay one `commands/help` away).
        continuum_base::probe!(
            class = "delib.tool_head.over_grant",
            persona = %self.persona_name,
            head_tokens = learned.tokens as u64,
            grant = budget as u64,
            "her learned head (floor plus held extras) did not fit the tools grant; her activity's surface went out without the extras"
        );
        selected
    }

    /// Her native tool surfaces, as built for her tool set and model.
    pub fn tool_surface(&self) -> &ToolSurface {
        &self.surface
    }

    /// Whole demand before focus allocation. The epoch owner records this beside
    /// the grant; the grant never becomes a demand observation.
    pub fn tool_claim_tokens(&self, ws: &Workspace) -> (usize, usize) {
        let held = self.working_memory.as_ref().and_then(|wm| wm.tool_head().held()).unwrap_or_default(); // unwrap_or_default: no learned head contributes no extra verbs
        self.surface.claim_with_head(&ws.room_affordances, &held)
    }

    /// Called at epoch start with the ONE allocation's tools grant. Retain the
    /// returned selection for accounting and the wire until the next epoch.
    pub fn select_tool_surface_granted(&self, ws: &Workspace, grant_tokens: usize) -> Result<SelectedSurface<'_>, usize> {
        let (_, want) = self.tool_claim_tokens(ws);
        let activity = self.surface.select_granted(&ws.room_affordances, grant_tokens)?;
        let selected = self.with_learned_head(activity, ws, grant_tokens);
        if selected.tokens > grant_tokens { return Err(selected.tokens); }
        Ok(selected.with_demand(want))
    }

    /// Her view started a new epoch, the one moment her head may change: her tool head learns
    /// from the epoch that ended and holds the head this act reserved. The receipt is the
    /// miss/waste read of the head that epoch held (`sent`: whether it was on the wire, or a
    /// shadow head measured while her server cannot take `tail_tools`).
    pub(super) fn tool_head_turned_over(&self) {
        let Some(memory) = self.working_memory.as_deref() else {
            return;
        };
        let Some(cost) = memory.tool_head().turn_over_reserved(continuum_base::time::EpochMs::now()) else {
            return;
        };
        continuum_base::probe!(
            class = "delib.tool_head.epoch",
            persona = %self.persona_name,
            sent = { let binding = self.binding.load(); binding.adapter.accepts_tail_tools(binding.requested_model()) },
            learned = cost.learned,
            calls = cost.calls,
            misses = cost.misses,
            waste_tokens = cost.waste_tokens,
            head_tokens = cost.head_tokens,
            "an epoch ended: the verbs she called that her head did not hold (one find each) and \
             the head schema she never called"
        );
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
        if self.surface.registry().is_empty() {
            return; // no hands offered — nothing to fit, nothing to warn about
        }
        let window = self.working_window();
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
    pub fn describe_tool_tokens(&self) -> usize {
        self.surface.registry_tokens()
    }
}
