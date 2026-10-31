use super::*;

impl PersonaAircRuntime {
    /// [`Self::bootstrap`] generalized over the citizen KIND — the operator
    /// self-peer (#27) boots the SAME runtime as a persona (identity keypair,
    /// daemon attach, transcript/roster readers) homed under its own kind dir
    /// (`citizens/humans/<label>/airc/`), so it can never be mistaken for a
    /// citizen by the persona resumer or the roster.
    pub async fn bootstrap_as(
        kind: crate::identity::IdentityKind,
        // Agent kinds REQUIRE a provider (citizens/agents/<provider>/<label>/);
        // every other kind passes None (citizen_path enforces the contract).
        provider: Option<&str>,
        persona_id: Uuid,
        agent_name: impl Into<String>,
        continuum_root: &Path,
        daemon_socket: PathBuf,
        source: crate::persona::identity_provider::PersonaIdentitySource,
        executor: Arc<crate::runtime::command_executor::CommandExecutor>,
    ) -> Result<Self, PersonaAircRuntimeError> {
        let agent_name = agent_name.into();
        // Slice 4 of #142: symmetric citizens/<kind>/<label>/airc/
        // layout. Use the shared helper so every actor kind shares
        // the same path schema.
        let home = crate::identity::citizen_path::citizen_home_path(
            continuum_root,
            kind,
            provider,
            &agent_name,
        );

        // Migration: refuse to use the pre-Slice-4 layout. Hard-error
        // with the exact `mv` command per [[no-fallbacks-ever]].
        if let Some(legacy) =
            crate::identity::citizen_path::legacy_home_path(continuum_root, kind, &agent_name)
        {
            if tokio::fs::try_exists(&legacy).await.unwrap_or(false) {
                let new_parent = home
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| home.clone());
                return Err(PersonaAircRuntimeError::LegacyLayoutDetected {
                    legacy,
                    new: home.clone(),
                    new_parent,
                });
            }
        }

        tokio::fs::create_dir_all(&home)
            .await
            .map_err(|e| PersonaAircRuntimeError::HomeCreate(home.clone(), e))?;

        // ── #199 Slice 1b: supply persona_id AS the airc peer_id ─────────
        //
        // Continuum derives the persona's `agent_name` (and thus her home
        // dir) from `persona_id` BEFORE attach. By handing `persona_id` to
        // airc's identity mint (`attach_as_with_peer_id`), airc adopts it as
        // the peer_id on a FRESH mint, so `airc.peer_id() == persona_id ==
        // the seed her name was derived from` — coherent identity from birth
        // ([[airc-peer-id-is-random-stored-not-keypair-derived]]). A RESUMED
        // persona (key + row already on disk) loads her STORED peer_id and
        // the supplied id is ignored — never a rotation — so the living
        // personas keep their identity across this change.
        let airc = Airc::attach_as_with_peer_id(
            home.clone(),
            agent_name.clone(),
            daemon_socket,
            PeerId::from_uuid(persona_id),
        )
        .await
        .map_err(|source| PersonaAircRuntimeError::Attach {
            agent_name: agent_name.clone(),
            home: home.clone(),
            source,
        })?;

        // ── The persona_id == peer_id invariant (guardrail) ──────────────
        //
        // The substrate keys everything downstream of bootstrap (the command
        // inbound pump, roster presence, registry) by `runtime.persona_id()`,
        // which MUST equal the airc peer_id the pump's envelopes address. Post
        // Slice 1b a FRESH mint satisfies this by construction (we supplied
        // persona_id as the peer_id, so the reseat below is a proven no-op),
        // and a RESUMED persona already stored `persona_id == peer_id` at her
        // first mint, so it is a no-op there too. The reseat remains as the
        // guardrail: if a home's stored identity ever diverges from the
        // supplied id (manual tampering, a pre-Slice-1b legacy divergence),
        // the runtime keys by the ACTUAL cryptographic identity, never a
        // stale input. It fires at `debug!` — silent in the coherent case.
        // KIND → wire labels: the heartbeat's runtime tag and the card's
        // continuum_kind. "interactive" is the projection's one Human marker
        // (SenderKind::from_runtime) — the operator self-peer rendering as a
        // persona/agent was the who-panel's you-vs-joel confusion.
        let (runtime_label, kind_label) = match kind {
            crate::identity::IdentityKind::Human => ("interactive", "human"),
            crate::identity::IdentityKind::Agent => ("agent", "agent"),
            _ => ("persona", "persona"),
        };
        let peer_id_uuid = airc.peer_id().as_uuid();
        if persona_id != peer_id_uuid {
            tracing::debug!(
                input_persona_id = %persona_id,
                peer_id = %peer_id_uuid,
                agent_name = %agent_name,
                "PersonaAircRuntime::bootstrap: supplied persona_id diverges from the \
                 stored peer_id (a resumed home minted before Slice 1b) — keying by the \
                 cryptographic peer_id per the persona_id == peer_id invariant."
            );
        }
        let persona_id = peer_id_uuid;

        info!(
            persona_id = %persona_id,
            agent_name = %agent_name,
            peer_id = %airc.peer_id(),
            client_id = %airc.client_id(),
            home = %home.display(),
            "PersonaAircRuntime bootstrap: identity ready (persona_id == peer_id post-collapse)"
        );

        // Resolve the persona's HOME room from her OWN durable
        // subscription state — never from the operator's focus.
        //
        // Task #298 (Joel: "we can't launch with these hacked rooms"):
        // bootstrap used to join the room the OPERATOR's daemon happened
        // to have current (`airc room` discovery), so every persona born
        // on a machine was dumped into whatever coordination room the
        // human/agent CLI last focused — live-found with the whole
        // resident population squatting in a FINISHED ACTIVITY room,
        // because the operator scope happened to be parked there. (A room
        // is an activity with a done-condition, not a standing channel;
        // parking citizens in one whose activity ended is how a fleet
        // ends up with nothing to do and no way to say why.)
        // A persona is her own airc peer with her own home dir;
        // `current_room_landing_in` loads HER durable subscription set
        // (the same state `Airc::join` writes), returns her established
        // default, and on a FRESH scope lands her in the citizen
        // commons `#academy` (CITIZEN_COMMONS_ROOM, card daa01102 —
        // airc's own `current_room` lobby stays `#general`; continuum's
        // citizens root in the academy tree): subscribe, set default,
        // publish presence + identity card. Resumed personas keep their
        // real membership; only new minds land in the commons. The
        // operator's current-room pointer no longer exists on this path.
        let room = airc
            .current_room_landing_in(CITIZEN_COMMONS_ROOM)
            .await
            .map_err(|source| PersonaAircRuntimeError::HomeRoom {
                agent_name: agent_name.clone(),
                source,
            })?;

        info!(
            persona_id = %persona_id,
            agent_name = %agent_name,
            peer_id = %airc.peer_id(),
            joined_room = %room.channel.as_uuid(),
            room_name = %room.name,
            "PersonaAircRuntime bootstrap: joined room"
        );

        // Task #222: install the command inbound pump so this
        // persona becomes a real command-callable peer on the grid.
        // Per [[no-fallbacks-ever]]: subscribe failure is bootstrap
        // failure — we refuse to declare the persona ready while it
        // would silently ignore cross-grid command envelopes.
        let airc_arc = Arc::new(airc);
        let grant_authorizer =
            crate::persona::command_inbound_pump::build_grant_authorizer(&airc_arc, &home)
                .await
                .map_err(|source| PersonaAircRuntimeError::GrantAuthorizerBuild {
                    agent_name: agent_name.clone(),
                    source,
                })?;
        let executor_for_acts = Arc::clone(&executor);
        // Built before the pump so the pump can watch it: a later join_room
        // bumps the epoch and the pump re-opens its stream.
        let membership_epoch = tokio::sync::watch::channel(0u64).0;
        let command_pump = crate::persona::command_inbound_pump::PersonaCommandInboundPump::spawn(
            persona_id,
            Arc::clone(&airc_arc),
            executor,
            grant_authorizer,
            membership_epoch.subscribe(),
        )
        .await
        .map_err(|source| PersonaAircRuntimeError::CommandPumpInstall {
            agent_name: agent_name.clone(),
            source,
        })?;

        info!(
            persona_id = %persona_id,
            agent_name = %agent_name,
            "PersonaAircRuntime bootstrap: command inbound pump installed — \
             persona is now addressable for cross-grid commands"
        );

        // airc#1222 spawn contract: attach_as → join → subscribe →
        // publish_identity. Now that the persona is attached, joined, and
        // subscribed (the pump above), publish its identity card so it is
        // grounded by NAME on the wire, not an anonymous peer-id. This is
        // the continuum half of the contract; it's what lets every peer's
        // RoomRosterSource (continuum#1650) resolve this persona's display
        // name from the IdentityPublished event this emits. Seeds from the
        // agent-name floor.
        //
        // WARN-and-continue, not fatal — asymmetric with the subscribe
        // failure above (which IS fatal): a deaf persona is useless, but an
        // anonymous-but-functional one is fine — it appears by peer-id in
        // rosters until its identity card propagates / is re-published.
        // Publish her FULL identity card (name + pronouns + role + bio +
        // avatar-in-integrations), not just the agent-name floor. `publish_identity`
        // alone seeds ONLY the name; M5's desktop roster needs the whole card
        // (display_name + kind + role + bio + avatar) to render "Kimi" with her face
        // instead of "peer-<uuid>" + default glyph. Source it from her durable
        // PersonaCard — the seed.json one directory up from the airc home
        // (citizens/personas/<name>/seed.json vs …/<name>/airc/). WARN-and-continue:
        // a card that didn't publish leaves her anonymous-but-functional, so fall
        // back to the name-floor publish so she is at least NAMED.
        let seed_path = home
            .parent()
            .map(|p| p.join("seed.json"))
            .unwrap_or_else(|| home.join("seed.json"));
        match crate::persona::seed::read_seed(&seed_path).await {
            Ok(seed) => {
                let identity = persona_identity_card(&seed.card(), kind_label);
                match airc_arc.set_local_identity_card(identity).await {
                    Ok(()) => info!(
                        persona_id = %persona_id,
                        agent_name = %agent_name,
                        "PersonaAircRuntime bootstrap: full identity card published \
                         (name + role + bio + avatar) — named for every peer's roster"
                    ),
                    Err(source) => warn!(
                        persona_id = %persona_id,
                        agent_name = %agent_name,
                        error = %source,
                        "PersonaAircRuntime bootstrap: set_local_identity_card failed — \
                         persona appears by peer-id until her card propagates"
                    ),
                }
            }
            Err(source) => {
                warn!(
                    persona_id = %persona_id,
                    agent_name = %agent_name,
                    seed = %seed_path.display(),
                    error = %source,
                    "PersonaAircRuntime bootstrap: seed read failed — publishing the \
                     name-floor card only (no role/bio/avatar)"
                );
                if let Err(source) = airc_arc.publish_identity().await {
                    warn!(
                        persona_id = %persona_id,
                        agent_name = %agent_name,
                        error = %source,
                        "PersonaAircRuntime bootstrap: name-floor publish_identity also \
                         failed — persona will appear by peer-id until it propagates"
                    );
                }
            }
        }

        // Presence contract: a persona is PRESENT in another citizen's
        // `room_roster` only while it emits `Alive` heartbeats — airc's
        // `active_agents` reduces heartbeat events, not `say()` messages.
        // Publishing the identity card alone (above) resolves the persona's
        // NAME but leaves it absent from `active_agents`, so co-resident
        // personas never see each other in the `[Present in this room]`
        // grounding block. Start the `Alive` pump (runtime "persona") so the
        // persona is a visible present citizen. WARN-and-continue, symmetric
        // with publish_identity: a persona that can't heartbeat is
        // live-but-roster-invisible — degraded, not dead — never a silent
        // fallback ([[fallbacks-are-illegal-fail-loud]]: the cause is named).
        // #260 honest presence: the beat carries LIVE availability derived
        // per tick from real serving state, not a spawn-time snapshot —
        // `away` while her lane is still loading (warming), `ready` once
        // /health answers. The interface must never show a green dot for a
        // mind that cannot yet answer (Joel: "online = able to respond").
        // Continuum owns this pump (airc's `start_agent_heartbeat` freezes
        // availability at spawn); the first beat fires immediately so she is
        // visible-as-warming from second one, and a beat is emitted on every
        // tick thereafter (same DEFAULT_HEARTBEAT_INTERVAL cadence).
        let heartbeat = {
            let hb_airc = airc_arc.clone();
            let hb_persona = persona_id;
            let hb_name = agent_name.clone();
            // What she held before this core: the record beside her scope, so a followed
            // claim is renewed inside its first lease-length after a restart (2d9df546).
            let recorded =
                crate::persona::held_claims::load(hb_airc.peer_id().as_uuid(), hb_airc.home());
            if recorded > 0 {
                continuum_base::probe!(
                    class = "persona.claim.record_loaded",
                    agent_name = %hb_name,
                    recorded,
                    "held-claim record read at bootstrap — renewed beside the board walk"
                );
            }
            // NO birth stamp — renewal is earned ONLY by cognition, never by
            // booting. The grace stamp that used to sit here (one lease-length
            // per spawn, meant to cover the post-boot deaf window #412) made
            // convergence structurally impossible across restarts: every core
            // restart re-armed 30 minutes of "earned" renewals, and the renewal
            // loop below resurrected already-lapsed claims faster than the 180s
            // lapsed-claim sweeper could observe them — an entire overnight round
            // (2026-08-16) sat "actively held" with zero turns because of it.
            // Never-stamped ⇒ `idle_ms` is None ⇒ renewal denied ⇒ holds lapse
            // within one TTL of boot unless she actually thinks. A citizen who
            // wakes later can re-claim; a finished artifact gets swept and graded.
            let handle = tokio::spawn(async move {
                let mut ticker = tokio::time::interval(DEFAULT_HEARTBEAT_INTERVAL);
                let mut last: Option<airc_lib::AgentAvailabilityState> = None;
                // CLAIM RENEWAL RUNS ON ITS OWN, SLOWER CADENCE (Joel 2026-08-15:
                // "Heartbeat needs fix. That is no good"). Presence needs the 60s
                // pulse — roster liveness IS a fast question. A 30-minute claim
                // lease does not: renewing it every presence tick emitted ~30× the
                // events one lease needs, and that churn crossed the wire to EVERY
                // subscriber on every node. Renew at TTL/6 — derived from the one
                // lease constant, never a second magic number — which gives a
                // healthy citizen six renewal opportunities per lease. The stamp
                // is only advanced on a fully-clean pass, so any roster-read or
                // per-card failure falls back to retrying on the NEXT 60s tick
                // (exactly the old cadence) until clean — degraded mode is the old
                // behavior, never a wider gap.
                let renewal_period = std::time::Duration::from_millis(
                    crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS / 6,
                );
                let mut last_clean_renewal: Option<std::time::Instant> = None;
                // Emit-on-transition for the renewal-denied probe: denial is a
                // STANDING condition (an idle citizen stays idle for hours) and
                // this loop ticks every 60s, so an unguarded probe here is the
                // same stream-flooding shape as the per-tick serving.plan probe
                // (#399 — measured 110 identical denial rows in 5 min across
                // the roster). One row when denial BEGINS, one implicit end
                // when renewals resume; the standing state stays queryable via
                // the work board's lease column.
                let mut renewal_denied = false;
                // Lapse-time observability (card 13d7e47a, gaps C/D of 1fc73f18): one
                // probe row per lapse episode — WHY the renewal was denied plus the
                // on-disk evidence re-check. Keyed by (card_id, lease expiry), so the
                // same lapsed lease seen across several denial ticks records once and
                // a card that lapses again under a new claim generation records again.
                let mut recorded_lapses: std::collections::HashSet<(uuid::Uuid, u64)> =
                    std::collections::HashSet::new();
                // What the next denial tick owes for lapse scanning (see
                // `lapse_scan::LapseScanSchedule`): due now on first denial; while
                // denied her holds only change by lapsing — no turns, no claims — so
                // one board fetch per expiry crossing, none in quiescence.
                let mut lapse_schedule = super::lapse_scan::LapseScanSchedule::DueNow;
                loop {
                    ticker.tick().await;
                    let serving = crate::inference::llama_server::current_serving();
                    // READY needs a lane AND her own loop (card 7524aa5b): a runtime whose
                    // cognition loop exited used to beacon ready every minute while deaf.
                    let thinking = crate::persona::cognition_pulse::loop_running(hb_persona);
                    let availability = if serving.ready && thinking {
                        airc_lib::AgentAvailabilityState::Ready
                    } else {
                        // Lane not up (cold boot, relaunch, squeeze), or no cognition loop
                        // running: she cannot think right now, so she is away, never ready.
                        airc_lib::AgentAvailabilityState::Away
                    };
                    if last != Some(availability) {
                        continuum_base::probe!(
                            class = "persona.presence.availability",
                            persona_id = %hb_persona,
                            agent_name = %hb_name,
                            state = ?availability,
                            serving_ready = serving.ready,
                            loop_running = thinking,
                            "presence availability transitioned (derived from live serving state and her own cognition loop)"
                        );
                        last = Some(availability);
                    }
                    let coordination = airc_lib::CoordinationSignal {
                        availability: Some(availability),
                        ..Default::default()
                    };
                    if let Err(error) = hb_airc
                        .emit_agent_heartbeat_with_coordination(
                            airc_lib::HeartbeatKind::Alive,
                            runtime_label.to_string(),
                            None,
                            None,
                            None,
                            coordination,
                        )
                        .await
                    {
                        // WARN-and-continue: a persona that can't heartbeat is
                        // live-but-roster-invisible — degraded, not dead.
                        warn!(
                            persona_id = %hb_persona,
                            agent_name = %hb_name,
                            error = %error,
                            "persona heartbeat emit failed — presence degraded this tick"
                        );
                    }

                    // HER WORK STAYS HERS WHILE SHE BREATHES (2026-08-07).
                    //
                    // Claim leases are 30 min and `work/heartbeat` already existed —
                    // as a VERB THE MODEL HAD TO REMEMBER TO CALL, which no citizen
                    // ever did. Glass-boxed this night from live captures: citizens
                    // claimed cards successfully (`work/claim` → real claim_id), every
                    // lease then lapsed untouched, and their next briefing correctly
                    // read `[board] you hold 0 card(s)` with the SAME card re-offered
                    // under `[available work]`. So they reported "no open tasks" and
                    // passed — accurately describing a board that had quietly taken
                    // their work back. Six citizens, two machines, days of it.
                    //
                    // Presence and claim liveness were two channels answering one
                    // question, and they contradicted each other: this pump was
                    // emitting "I am alive" on the very ticks her holds were dying.
                    // Binding them is the fix — the substrate observes that she is
                    // working instead of asking her to announce it
                    // ([[commands-are-agency]]; Joel: "you've turned convenience into
                    // disability"). The verb stays for deliberate extension.
                    //
                    // Renewal is per-card WARN-and-continue: a failed renewal degrades
                    // to exactly the old lapse, and never takes down presence.
                    let renewal_due =
                        last_clean_renewal.map_or(true, |t| t.elapsed() >= renewal_period);
                    if !renewal_due {
                        continue;
                    }
                    // RENEWAL IS EARNED BY COGNITION, NOT BY BREATHING
                    // (2026-08-16). The presence-bound renewal above this
                    // comment's ancestor fixed #331 by overcorrecting: a
                    // citizen whose cognition had been silent for HOURS still
                    // renewed every minute, so a stalled round read as
                    // "actively held" forever and the lapsed-claim sweeper
                    // could never recover her finished artifact. The gate:
                    // no turn within one lease-length → skip renewal → the
                    // hold lapses naturally → the card becomes re-claimable
                    // and any written artifact gets graded. A turn that
                    // DEFERS on serving pressure still stamps the pulse —
                    // trying to think counts; only true silence lapses.
                    {
                        let now_ms = EpochMs::now().millis(); // pre-1970 reads 0: nothing recorded / a pre-epoch clock = 0, never a guess
                        let idle =
                            crate::persona::cognition_pulse::work_idle_ms(hb_persona, None, now_ms);
                        if !crate::persona::cognition_pulse::renewal_earned(
                            idle,
                            crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS,
                        ) {
                            if !renewal_denied {
                                renewal_denied = true;
                                // `idle` is None when she has NEVER thought this
                                // process — that is a state, not a duration, and
                                // stamping it u64::MAX printed a 584-million-year
                                // idle into the ledger (seen live 2026-08-21).
                                // [[unknown-is-not-a-quantity]]: the unknown gets a
                                // word, never a sentinel number.
                                continuum_base::probe!(
                                    class = "persona.claim.renewal_skipped_idle",
                                    persona_id = %hb_persona,
                                    agent_name = %hb_name,
                                    idle = %idle
                                        .map(|ms| ms.to_string())
                                        .unwrap_or_else(|| "never-thought".to_string()),
                                    ttl_ms = crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS,
                                    "no cognition within one lease-length — renewals \
                                     DENIED from here until she thinks again; holds \
                                     lapse naturally so the work can be recovered"
                                );
                            }
                            // LAPSE-TIME OBSERVABILITY (card 13d7e47a, gaps C/D of
                            // 1fc73f18): denial is what lapses her holds — record WHY at
                            // the moment a hold has in fact lapsed. The pulse says idle;
                            // disk may disagree (a restart wipes the stamp), so each
                            // lapsed checkout gets an on-disk re-check and its evidence
                            // rides the row: one per lapse episode, keyed by lease
                            // generation. Fetch discipline (one board read per expiry
                            // crossing, none in quiescence) lives with the scan itself.
                            if lapse_schedule.is_due(now_ms) {
                                lapse_schedule = super::lapse_scan::record_lapsed_holds(
                                    &hb_airc,
                                    hb_persona,
                                    &hb_name,
                                    idle,
                                    now_ms,
                                    &mut recorded_lapses,
                                )
                                .await;
                            }
                            continue;
                        }
                        if renewal_denied {
                            renewal_denied = false;
                            // Do NOT clear the episode keys on resume: while a lapsed
                            // board row persists, the same stale (card, expiry) could be
                            // recorded again at the next denial. Keys are pruned per scan,
                            // dropping only what the authoritative holds say has ended.
                            lapse_schedule = super::lapse_scan::LapseScanSchedule::DueNow; // re-establish from a fresh scan on the next denial
                            continuum_base::probe!(
                                class = "persona.claim.renewal_resumed",
                                persona_id = %hb_persona,
                                agent_name = %hb_name,
                                "cognition resumed — claim renewals earned again"
                            );
                        }
                    }
                    // RECOVER MY OWN LAPSED HOLDS. The roster lists only live claims,
                    // so a hold that lapsed (a missed renewal, a reboot) was never
                    // renewed again: the board kept her as owner, she read as free,
                    // and the card was re-pulled by someone else — or by her, twice.
                    // A lapsed hold nobody else has taken is re-claimed here.
                    {
                        let now_ms = EpochMs::now().millis(); // pre-1970 reads 0: nothing recorded / a pre-epoch clock = 0, never a guess
                        if let Ok(board) = hb_airc
                            .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                            .await
                        {
                            let me = hb_airc.peer_id();
                            let snapshot = board.snapshot();
                            // WIP = 1 survives the recovery: if she already holds a LIVE
                            // card, a lapsed one is left for the deck (2026-09-05: a
                            // citizen ended up on two).

                            // ONE card per citizen holds through the recovery too: of
                            // several lapsed holds, prefer the latest explicit choice,
                            // then recency. Recovery preserves decision time; live
                            // automatic work cannot suppress an earned explicit choice.
                            // The rest stay on the deck for anyone (2026-09-12: a
                            // coder recovered four at one boot and pulled nothing for an
                            // hour behind the lane cap).
                            let recoverable = snapshot
                                .cards
                                .iter()
                                .filter(|c| {
                                    crate::persona::cognition_pulse::renewal_earned(
                                        crate::persona::cognition_pulse::work_idle_ms(
                                            hb_persona,
                                            Some(c.card_id.as_uuid()),
                                            now_ms,
                                        ),
                                        crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS,
                                    ) && c.owner == Some(me)
                                        && c.claim_expires_at_ms.is_some_and(|e| e <= now_ms)
                                        && crate::cognition::bench_round::card_round_is_working(
                                            c.card_id.as_uuid(),
                                        )
                                        && matches!(
                                            c.state,
                                            airc_work::model::CardState::Claimed
                                                | airc_work::model::CardState::InProgress
                                        )
                                })
                                .max_by_key(|c| {
                                    (
                                        crate::persona::work_focus::explicit_choice_key(
                                            c.card_id.as_uuid(),
                                            c.claim_provenance.as_ref(),
                                        ),
                                        c.updated_at_ms,
                                    )
                                })
                                .filter(|candidate| {
                                    crate::persona::work_focus::recovery_preferred_over_live(
                                        candidate,
                                        snapshot.cards.iter().filter(|c| {
                                            c.owner == Some(me)
                                                && c.claim_expires_at_ms.is_some_and(|e| e > now_ms)
                                        }),
                                    )
                                });
                            if let Some(card) = recoverable {
                                match hb_airc
                                    .claim_work_card_with_provenance(
                                        airc_lib::ClaimWorkCard {
                                            card_id: card.card_id,
                                            ttl_ms:
                                                crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS,
                                        },
                                        card.claim_provenance
                                            .as_ref()
                                            .map(|p| p.origin)
                                            .unwrap_or_default(), // unwrap_or_default: legacy claim has unknown origin
                                        card.claim_provenance.as_ref().map(|p| p.selected_at_ms),
                                    )
                                    .await
                                {
                                    Ok(_) => continuum_base::probe!(
                                        class = "persona.claim.recovered",
                                        agent_name = %hb_name,
                                        card_id = %card.card_id.as_uuid(),
                                        "my own lapsed hold re-claimed before anyone else took it"
                                    ),
                                    Err(error) => continuum_base::probe!(
                                        class = "persona.claim.recover_failed",
                                        agent_name = %hb_name,
                                        card_id = %card.card_id.as_uuid(),
                                        error = %error,
                                        "my lapsed hold could not be re-claimed (someone else may hold it now)"
                                    ),
                                }
                            }
                        }
                    }
                    // Renew what the BOARD says she holds (board_held_by) — the roster
                    // reads empty right after a boot, and a lease not renewed in that
                    // window lapses out from under her.
                    match scoped_board_held_by(hb_airc.as_ref()).await {
                        Ok(mine) => {
                            let mut renewed = 0usize;
                            let mut failed = 0usize;
                            for (room, card) in &mine {
                                if !crate::persona::cognition_pulse::renewal_earned(
                                    crate::persona::cognition_pulse::work_idle_ms(
                                        hb_persona,
                                        Some(card.card_id.as_uuid()),
                                        EpochMs::now().millis(),
                                    ),
                                    crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS,
                                ) {
                                    continue;
                                }
                                let Some(claim_id) = card.claim_id else {
                                    continue;
                                };
                                if let Err(error) = hb_airc
                                    .heartbeat_work_claim_in(
                                        room,
                                        airc_lib::HeartbeatWorkClaim {
                                            card_id: card.card_id,
                                            claim_id,
                                            ttl_ms:
                                                crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS,
                                        },
                                    )
                                    .await
                                {
                                    failed += 1;
                                    warn!(
                                        persona_id = %hb_persona,
                                        agent_name = %hb_name,
                                        card_id = %card.card_id.as_uuid(),
                                        error = %error,
                                        "claim renewal failed — this card can lapse out from \
                                         under her while she is still working it"
                                    );
                                } else {
                                    renewed += 1;
                                }
                            }
                            // THE FOLLOWED CLAIMS: what she holds beyond the walk — a card
                            // claimed on a room this scope does not subscribe to (card
                            // 2d9df546: the claim path follows the card to its room, the
                            // walk cannot). Renewed through the room the claim landed in,
                            // on the same earned-by-cognition contract; a hold the board
                            // refuses three times running is gone, and its record with it.
                            let peer = hb_airc.peer_id().as_uuid();
                            let walked: Vec<uuid::Uuid> =
                                mine.iter().map(|(_, c)| c.card_id.as_uuid()).collect();
                            let followed = crate::persona::held_claims::beyond_the_walk(
                                &walked,
                                &crate::persona::held_claims::held(peer),
                            );
                            for h in followed {
                                // A SETTLED CARD IS NOBODY'S WORK, here as in the walk
                                // (`held_of_owned`). The walk drops a merged card, which
                                // then read as "beyond the walk", and this path renewed it
                                // with no state check: the heartbeat is accepted on a merged
                                // card, so the hold never ended. Kimi held d33e928a (its PR
                                // merged) for hours on 2026-09-27, pulled nothing, and read
                                // 21 acts and 0 writes an hour. Read the card in the room
                                // the claim landed in; settled forgets the record. An
                                // unreadable board renews as before.
                                if let Ok(board) = hb_airc.work_board_in(&h.room).await {
                                    let settled = board
                                        .card(airc_lib::WorkCardId::from_uuid(h.card_id))
                                        .is_some_and(|card| settled_card(&card.state));
                                    if settled {
                                        let dropped = crate::persona::held_claims::forget(
                                            peer,
                                            hb_airc.home(),
                                            h.card_id,
                                        );
                                        continuum_base::probe!(
                                            class = "persona.claim.followed_settled",
                                            agent_name = %hb_name,
                                            card_id = %h.card_id,
                                            room = %h.room.name,
                                            dropped,
                                            "a followed claim's card is merged or closed: its record is forgotten, never renewed"
                                        );
                                        continue;
                                    }
                                }
                                if !crate::persona::cognition_pulse::renewal_earned(
                                    crate::persona::cognition_pulse::work_idle_ms(
                                        hb_persona,
                                        Some(h.card_id),
                                        EpochMs::now().millis(),
                                    ),
                                    crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS,
                                ) {
                                    continue;
                                }
                                match hb_airc
                                    .heartbeat_work_claim_in(
                                        &h.room,
                                        airc_lib::HeartbeatWorkClaim {
                                            card_id: airc_lib::WorkCardId::from_uuid(h.card_id),
                                            claim_id: airc_lib::ClaimId::from_uuid(h.claim_id),
                                            ttl_ms:
                                                crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS,
                                        },
                                    )
                                    .await
                                {
                                    Ok(_) => {
                                        renewed += 1;
                                        crate::persona::held_claims::note_renewed(peer, h.card_id);
                                        continuum_base::probe!(
                                            class = "persona.claim.renewed_followed",
                                            agent_name = %hb_name,
                                            card_id = %h.card_id,
                                            room = %h.room.name,
                                            "a claim beyond the subscription walk renewed through its recorded room"
                                        );
                                    }
                                    Err(error) => {
                                        failed += 1;
                                        let dropped = crate::persona::held_claims::note_refusal(
                                            peer,
                                            hb_airc.home(),
                                            h.card_id,
                                        );
                                        continuum_base::probe!(
                                            class = "persona.claim.followed_renewal_refused",
                                            agent_name = %hb_name,
                                            card_id = %h.card_id,
                                            room = %h.room.name,
                                            error = %error,
                                            dropped,
                                            "the recorded room refused the heartbeat; the record is dropped at the third refusal in a row"
                                        );
                                    }
                                }
                            }
                            if renewed > 0 {
                                continuum_base::probe!(
                                    class = "persona.claim.renewed",
                                    persona_id = %hb_persona,
                                    agent_name = %hb_name,
                                    renewed = renewed,
                                    held = mine.len(),
                                    ttl_ms = crate::cognition::card_facts::DEFAULT_CLAIM_TTL_MS,
                                    "held work-card claims renewed on the claim cadence (TTL/6)"
                                );
                            }
                            // Only a fully-clean pass earns the slow cadence; any
                            // failure retries on the next 60s presence tick.
                            if failed == 0 {
                                last_clean_renewal = Some(std::time::Instant::now());
                            }
                        }
                        Err(error) => {
                            warn!(
                                persona_id = %hb_persona,
                                agent_name = %hb_name,
                                error = %error,
                                "claim-renewal roster read failed — holds may lapse; retrying \
                                 on the next presence tick"
                            );
                        }
                    }
                }
            });
            info!(
                persona_id = %persona_id,
                agent_name = %agent_name,
                interval_s = DEFAULT_HEARTBEAT_INTERVAL.as_secs(),
                "PersonaAircRuntime bootstrap: dynamic-availability heartbeat pump started — \
                 presence tracks live serving state (away=warming, ready=able to respond)"
            );
            Some(AbortOnDrop(handle))
        };

        // Re-publish the identity card on the heartbeat cadence so it SURVIVES. The
        // card emit at attach is one-shot: a peer that joins the room AFTER this
        // persona attached (M5's desktop reconnecting, a fresh grid peer) missed it
        // and would render her by peer-id forever. `publish_identity()` re-emits the
        // already-set full card to every subscribed room; on the 60s cadence, any
        // late arrival is grounded within a heartbeat window. Handle held for the
        // persona's lifetime; dropped on teardown (mirrors the heartbeat pump).
        let identity_republish = {
            let airc = airc_arc.clone();
            let persona_id = persona_id;
            let agent_name = agent_name.clone();
            Some(tokio::spawn(async move {
                let mut ticker = tokio::time::interval(DEFAULT_HEARTBEAT_INTERVAL);
                // Skip the immediate tick — bootstrap already emitted the card.
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    if let Err(source) = airc.publish_identity().await {
                        warn!(
                            persona_id = %persona_id,
                            agent_name = %agent_name,
                            error = %source,
                            "PersonaAircRuntime: periodic identity re-publish failed \
                             (transient) — will retry next cadence"
                        );
                    }
                }
            }))
        };

        // Resume-visibility contract (Joel 2026-07-13: post-reboot the team
        // resumed correctly — woke, restored, yielded per cadence — but from
        // the outside that was indistinguishable from a stall, because a
        // persona's state has no surface other than speech). Status is a
        // PROJECTION, not a message: if this peer still holds work-card
        // claims, publish typed Busy availability naming them — the board /
        // HUD / work_queue_status then show "back and still on card X"
        // without a single chat line. WARN-and-continue like the identity
        // card: a persona whose availability didn't publish is degraded-
        // visible, not broken. No claims → nothing to announce (the quiet
        // Ready story needs per-room repo resolution — a later slice).
        match airc_arc
            .work_roster_status(airc_lib::WorkRosterQuery::default())
            .await
        {
            Ok(status) => {
                let me = airc_arc.peer_id();
                let claims: Vec<airc_lib::WorkCard> = status
                    .rows
                    .into_iter()
                    .find(|r| r.peer == me)
                    .map(|r| r.active_claims)
                    .unwrap_or_default(); // unwrap_or: nothing recorded = empty, the probe says so
                let mut repos: Vec<airc_lib::RepoId> =
                    claims.iter().map(|c| c.repo.clone()).collect();
                repos.sort();
                repos.dedup();
                for repo in repos {
                    let held: Vec<String> = claims
                        .iter()
                        .filter(|c| c.repo == repo)
                        .map(|c| format!("{} ({})", c.title, c.card_id))
                        .collect();
                    let report = airc_lib::ReportAgentAvailability {
                        repo: repo.clone(),
                        state: airc_lib::AgentAvailabilityState::Busy,
                        note: Some(format!("resumed — holding: {}", held.join("; "))),
                        ttl_ms: RESUME_AVAILABILITY_TTL_MS,
                    };
                    match airc_arc.report_agent_availability(report).await {
                        Ok(()) => info!(
                            persona_id = %persona_id,
                            agent_name = %agent_name,
                            repo = %repo,
                            cards = held.len(),
                            "PersonaAircRuntime bootstrap: resume availability published — \
                             board shows this persona back on its claimed work"
                        ),
                        Err(source) => warn!(
                            persona_id = %persona_id,
                            agent_name = %agent_name,
                            repo = %repo,
                            error = %source,
                            "PersonaAircRuntime bootstrap: resume availability publish failed — \
                             persona is live but the board cannot show it resumed its claims"
                        ),
                    }
                }
            }
            Err(source) => warn!(
                persona_id = %persona_id,
                agent_name = %agent_name,
                error = %source,
                "PersonaAircRuntime bootstrap: work roster read failed — cannot publish \
                 resume availability for held claims"
            ),
        }

        // Every id this process wears is registered so no placement path ever treats
        // it as another machine (card 2500d2f1).
        crate::persona::self_peer::register(airc_arc.peer_id().as_uuid());
        Ok(Self {
            persona_id,
            agent_name,
            home,
            airc: airc_arc,
            // The ACTUALLY-joined home room (from her durable subscription
            // state above) — before #298 this stored the passed-in operator
            // room, which could even diverge from the joined channel.
            default_room: RoomId::from_uuid(room.channel.as_uuid()),
            membership_epoch,
            inbound_handle: None,
            command_pump: Some(command_pump),
            executor: Some(executor_for_acts),
            _heartbeat: heartbeat,
            identity_republish,
            owned_tasks: std::sync::Mutex::new(Vec::new()),
            source,
        })
    }
}
