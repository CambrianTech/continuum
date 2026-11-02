use super::*;

pub async fn materialize_adapters(
    plans: Vec<MaterializedPersonaPlan>,
    factory: &dyn PersonaAdapterFactory,
    runtime_lookup: impl Fn(uuid::Uuid) -> Option<Arc<dyn AircCitizen>>,
    // The persona's HANDS, built per persona by the caller (the `ipc` bootstrap,
    // which holds the command `ModuleRegistry`). Closure DI keeps the supervisor
    // decoupled from the command runtime — same shape as `runtime_lookup`. Returns
    // `None` → that persona is speak-only. The executor carries the persona's
    // identity, so the `GridTrustAuthPolicy` ACL gates what its hands may touch.
    tool_executor_for: impl Fn(
        uuid::Uuid,
    ) -> Option<Arc<dyn crate::cognition::tool_executor::ToolExecutor>>,
) -> Vec<Result<PersonaContext, SupervisorError>> {
    let mut out = Vec::with_capacity(plans.len());
    for (slot_index, plan) in plans.into_iter().enumerate() {
        let mut profile = match plan.profile {
            Ok(p) => p,
            Err(source) => {
                out.push(Err(SupervisorError::Profile {
                    slot_index,
                    role: plan.role,
                    source,
                }));
                continue;
            }
        };
        let identity = plan.instance;
        // ADMISSION FIRST. This restore opens the persona's engram store; when it fails
        // the seat is refused (6d17695c: never host a blank admission store). Until
        // 2026-09-12 it ran AFTER every daemon-facing adapter had been built — a
        // subscription to every room, the doctrine/wall invalidator task, the pumps —
        // and a refused seat abandoned them all, un-aborted. One citizen whose store
        // would not decode was re-attempted every reconciler pass: 24 daemon streams
        // leaked per attempt, ~150 sockets/s, both descriptor tables full, the host at
        // ENFILE. A seat that will be refused must build nothing first.
        let persisted_admission = {
            let home = crate::cognition::home::PersonaHome::from_root(identity.home.clone());
            let recall_meta =
                std::sync::Arc::new(crate::cognition::recall_metadata::RecallMetadataRegistry::new());
            match crate::cognition::admission_state::AdmissionState::for_persona(&home, recall_meta).await
            {
                Ok(persisted) => persisted,
                Err(source) => {
                    out.push(Err(SupervisorError::AdmissionRestore {
                        slot_index,
                        role: plan.role,
                        persona_id: identity.peer_id.as_uuid(),
                        source,
                    }));
                    continue;
                }
            }
        };
        let runtime = match runtime_lookup(identity.peer_id.as_uuid()) {
            Some(r) => r,
            None => {
                out.push(Err(SupervisorError::RuntimeMissing {
                    slot_index,
                    role: plan.role,
                    persona_id: identity.peer_id.as_uuid(),
                }));
                continue;
            }
        };
        let adapter = match factory.build_adapter(&profile).await {
            Ok(a) => a,
            Err(message) => {
                out.push(Err(SupervisorError::AdapterFactory {
                    slot_index,
                    role: plan.role,
                    message,
                }));
                continue;
            }
        };
        // Warm the adapter's KV-cache / kernels BEFORE the persona
        // enters her service loop. Per [[init-once-handle-then-lease-zero-copy-refs]]
        // the substrate pays init costs at boot, not on Joel's first
        // message. Per [[no-fallbacks-ever]] warmup failure surfaces
        // as a typed slot failure — the persona doesn't reach
        // "hosted" state if her adapter refuses to warm.
        if let Err(message) = adapter.warmup().await {
            out.push(Err(SupervisorError::AdapterWarmup {
                slot_index,
                role: plan.role,
                message,
            }));
            continue;
        }

        // Reconcile the persona's effective context window to the TRUTH the
        // running gateway serves. When the snapshot is ready, `current_serving()`
        // carries the real per-slot window the daemon read from llama-server's own
        // `/props` — the authoritative model metadata. It need NOT be ready here (a
        // persona materialized while the engine is still loading after a deploy):
        // she is then bound at the provisional window, and the served-model re-home
        // reconciler (`ipc/start_server/rehome_reconciler.rs`) rebinds her to the
        // served window on the first ready snapshot. That reconciler owns the
        // correction; this pin only spares the first turns a wrong window. The profile's window was provisionally set at spawn-plan
        // time from the planner's `served_context_window`, which the daemon
        // RE-computes every tick against live memory and which drifts ABOVE the
        // running server's frozen, 256-padded slot. Budgeting a prompt to that
        // drifted value overflows the slot → llama-server 500 "Compute error" and
        // the persona abstains on every tick. Pin `profile.context_length` (the
        // single source of the persona's compute envelope — read downstream by the
        // RAG composer, the deliberation faculty's prompt cap, and the ctx_len
        // probe) to the served truth so every budget is correct by construction.
        // Cloud-routed personas (tier `Cloud`) keep their model's full window —
        // their adapter owns its own context and there is no local slot to fit.
        // An OFF-BOX brain is budgeted against the RESPONDER's slot, never the
        // local gateway's: measured 2026-09-07 03:1xZ, two citizens bound to a
        // 5090 slot were pinned to the M5's 50,944-token lane below, sent
        // 35–43k-token prompts, and every answer died at `Length`.
        // Her override lives in her PERSONA home (`citizens/personas/<Name>`), the
        // home the factory reads it from — `identity.home` can be a different root
        // (measured 2026-09-07 04:48Z: both remote-bound citizens took the local
        // pin because this read looked in the wrong home). Try the name-resolved
        // home first, then the identity's.
        let remote_window = remote_override_for(&profile.persona_name, &identity.home)
            .filter(|o| o.is_remote())
            .map(|o| o.remote_context_window);
        if let Some(remote) = remote_window {
            match remote {
                Some(window) => {
                    continuum_base::probe!(
                        class = "persona.upstart.window",
                        persona = %profile.persona_name,
                        persona_id = %profile.persona_id,
                        planned = profile.context_length,
                        served = window,
                        source = "remote_override",
                        "pinning persona context window to the RESPONDER's slot (her brain runs off-box)",
                    );
                    profile.context_length = window;
                }
                None => {
                    // The planned value IS the outage (it is the local lane's size).
                    // Unknown responder → the conservative floor: briefly under-
                    // budgeted costs context; over-budgeted costs every answer.
                    // The planner's own full-turn window floor — the one substrate-owned
                    // minimum every other bound derives from; never a number of our own.
                    let floor = crate::inference::planning::serving_plan::BOOTSTRAP_WORKING_SET
                        .min(profile.context_length);
                    continuum_base::probe!(
                        class = "persona.upstart.window",
                        persona = %profile.persona_name,
                        persona_id = %profile.persona_id,
                        planned = profile.context_length,
                        served = floor,
                        source = "remote_floor",
                        "off-box brain with no recorded responder window — sized to the floor; \
                         pass --context_window to persona/reassign-model for her real slot",
                    );
                    profile.context_length = floor;
                }
            }
        } else if profile.tier_category != crate::persona::hw_tier_descriptor::HwTierCategory::Cloud
        {
            let snap = crate::inference::llama_server::current_serving();
            // A ready snapshot always carries a real window (the daemon refuses to
            // publish ready with 0). Guard on both so a not-yet-ready/empty
            // snapshot never clobbers the provisional window with 0; the re-home
            // reconciler corrects the binding once serving is ready.
            if snap.ready && snap.served_context_window > 0 {
                if snap.served_context_window != profile.context_length {
                    continuum_base::probe!(
                        class = "persona.upstart.window",
                        persona = %profile.persona_name,
                        persona_id = %profile.persona_id,
                        planned = profile.context_length,
                        served = snap.served_context_window,
                        "pinning persona context window to the gateway's real /props slot (was the drifting plan value)",
                    );
                }
                profile.context_length = snap.served_context_window;
            }
        }
        // Register the persona's adapter in the global provider
        // registry so the cognition layer (evaluate_response,
        // analyze, etc.) can reach it via `global_registry()` per
        // task #161. `ArcAdapterShim` lets the supervisor keep its
        // `Arc<dyn AIProviderAdapter>` ownership while the registry
        // (which holds Box<dyn ...>) sees a delegating handle. The
        // shim's `initialize`/`shutdown` are no-ops because the
        // factory + warmup above already paid the init cost per
        // [[init-once-handle-then-lease-zero-copy-refs]].
        {
            let registry_arc = crate::ai::adapter::global_registry();
            let mut registry = registry_arc.write().await;
            registry.register(adapter.clone(), slot_index);
        }

        // Build the persona's brain at boot. Bind airc_source via
        // set_airc_source so compose_for_turn has engram + airc both
        // available the moment her service loop iterates (task #148).
        // The runtime IS an AircTranscriptReader by trait bound.
        let rag_engine = Arc::new(crate::rag::RagEngine::new());
        let mut cognition = crate::persona::unified::PersonaCognition::new(
            identity.peer_id.as_uuid(),
            identity.agent_name.clone(),
            rag_engine,
        );
        // One airc source: it delivers her rooms to cognition and takes the read receipt
        // of every sent prompt (her cursor moves through what she was shown, never at delivery).
        let airc = Arc::new(crate::persona::airc_source::AircRagSource::new(identity.peer_id.as_uuid(), runtime.clone()));
        cognition.set_airc_source(Arc::clone(&airc) as Arc<dyn crate::cognition::rag_budget::RagSource>);

        // WHO IS PRESENT, read from the SAME `RosterViewState` the browser renders
        // (#408 + the RenderTarget pattern). Her room's OWN store, so a citizen in
        // room B is never handed room A's people and never handed nothing —
        // `PerRoomSubstrates` keeps each room's view instead of one focused slot.
        //
        // This is the repair for the measured defect: a live peer's name appeared
        // ZERO times in a citizen's prompt while the browser rendered that peer
        // fine, because the two read different code
        // ([[citizens-cannot-see-each-other-the-prompt-promises-presence-and-delivers-nothing]]).
        // One definition, two render targets — eyes and mind cannot drift.
        // The REGISTRY, not `for_room(identity.default_room)`. Binding one room here
        // is what made the promise in the comment above false: the source answered
        // with the default room's roster on every turn, so `room_scope_allows`
        // abstained for a citizen working anywhere else — measured 2026-09-07 at 662
        // abstains of 873 ticks, bound #general, turns in #continuum, and the citizen
        // never learned who was in the room she was standing in (#3862). The store
        // was never the problem: `positron_source::sink` writes EVERY room's view
        // into `PerRoomSubstrates` and mirrors only the focused one onto the node
        // substrate, so #continuum's roster was being stored the whole time with
        // nobody reading it.
        let roster_source: Arc<dyn crate::cognition::rag_budget::RagSource> =
            Arc::new(crate::persona::viewstate_rag::ViewStateRagSource::<
                continuum_positron::RosterViewState,
            >::per_room(crate::cognition::room_substrates::global_room_substrates()));
        cognition.set_roster_source(roster_source.clone());


        // Her awareness across ALL her activities (EVENT-MIND §6, the strip): a line per
        // activity, loudest first, with her continuation and her dial, read from her
        // perception region by persona. The region boots when her conversation attaches;
        // until then the source delivers nothing, which is the truth. A grounding faculty
        // below: it was bound only to compose_for_turn, whose delivery of it nothing read,
        // so the strip never reached her prompt.
        let awareness_source: Arc<dyn crate::cognition::rag_budget::RagSource> =
            Arc::new(crate::persona::awareness_source::AwarenessRagSource::new(
                identity.peer_id.as_uuid(),
                Some(runtime.clone()),
            ));

        // Bind the room-doctrine source from the same runtime (upcasts to
        // `AircDoctrineReader`). Grounds the persona in the room's nature
        // — the airc-published operating contract. Slice 2.
        let raw_doctrine: Arc<dyn crate::cognition::rag_budget::RagSource> = Arc::new(
            crate::persona::room_doctrine_source::RoomDoctrineSource::new(
                identity.peer_id.as_uuid(),
                runtime.clone(),
            )
            // Bound to the room she joined at bootstrap — the room her airc
            // connection (the reader) answers for. The room gate in deliver then
            // keeps this grounding out of turns in OTHER contexts (another room,
            // the eval fork's nil room) — the exam-bleed fix (#127).
            .for_room(identity.default_room),
        );

        // Active-work source: grounds the persona in ITS OWN live work across all
        // rooms (claimed cards + states), read from airc's work roster. The dynamic
        // fix for "misremembering my own card" — never hardcode the state; surface
        // it. Reads the persona's own airc handle (acts as itself).
        let active_work_source: Arc<dyn crate::cognition::rag_budget::RagSource> =
            Arc::new(crate::persona::active_work_source::ActiveWorkSource::new(
                identity.peer_id.as_uuid(),
                runtime.clone(),
            ));

        // The persona's HANDS, built once here — consumed by the brain config
        // below AND by the workspace-map cache wire (its command executor's bus
        // is where write-completion events land).
        let tool_executor = tool_executor_for(identity.peer_id.as_uuid());

        // Workspace map: grounds the persona in WHERE code lives — the real root
        // and top-level layout the code tools resolve against. NOT airc-backed
        // (reads the same cwd-rooted FileEngine as the persona's hands, so it
        // cannot drift from what code/glob sees); brain-only, no legacy compose
        // wire. Closes the grounding hole behind the `src/**/*.rs` glob failures:
        // the layout was only ever an echoed error in recall, never standing
        // framing. Grounding, not steering — names the dirs, never which holds
        // the answer. Swaps to the airc-leased root when #49 lands.
        //
        // Wrapped as an event-invalidated cache (#398): the dir re-walk ran on
        // EVERY compose, so it serves last-good until a workspace-mutating
        // command completes on the bus. No wrap without a wire — a speak-only
        // persona (no hands → no bus) keeps the raw source, because her map
        // can still be mutated by OTHERS' hands and an unwired cache would be
        // stale forever.
        let raw_workspace_map: Arc<dyn crate::cognition::rag_budget::RagSource> = Arc::new(
            crate::persona::workspace_map_source::WorkspaceMapSource::for_peer_layer(
                identity.peer_id.as_uuid(),
            ),
        );
        let workspace_map_source: Arc<dyn crate::cognition::rag_budget::RagSource> =
            match tool_executor
                .as_ref()
                .and_then(|t| t.command_executor())
                .and_then(|c| c.message_bus())
            {
                Some(bus) => {
                    let (cached, dirty) =
                        crate::cognition::cached_source::CachedRagSource::new(raw_workspace_map);
                    crate::cognition::grounding_invalidation::spawn_workspace_invalidator(
                        bus,
                        dirty.downgrade(),
                    );
                    crate::cognition::grounding_invalidation::register_workspace_map_dirty(
                        identity.peer_id.as_uuid(),
                        dirty.downgrade(),
                    );
                    cached
                }
                None => raw_workspace_map,
            };

        // Wall source: grounds the persona in the room's LIVING SHARED
        // DOCUMENTS — the airc-pinned plan, coding instructions, agenda,
        // principles, recipe. These are the SAME airc rows a human edits on
        // the room wall (`airc publish --room …`) and a widget renders: one
        // shared data layer, two faces, no continuum-side copy. Reads the
        // persona's own airc handle (upcasts to `WallReader`, a supertrait of
        // AircCitizen). Enriching framing, NOT a participation gate (that is
        // doctrine) — bound brain-only and defer-tolerant below, like the
        // active-work + workspace-map sources. See
        // docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 and
        // [[airc-generic-per-user-room-state]].
        let raw_wall: Arc<dyn crate::cognition::rag_budget::RagSource> = Arc::new(
            crate::persona::wall_source::WallSource::new(
                identity.peer_id.as_uuid(),
                runtime.clone(),
            )
            // Bound to the room she joined at bootstrap — the room her airc
            // connection (the reader) answers for. The room gate in deliver then
            // keeps this grounding out of turns in OTHER contexts (another room,
            // the eval fork's nil room) — the exam-bleed fix (#127).
            .for_room(identity.default_room),
        );

        // Doctrine + wall as event-invalidated caches (#398): these are pure
        // event-folds — their projections change ONLY when a peer publishes
        // (TranscriptKind::DoctrinePublished / WallPostPublished), which is
        // rare, yet doctrine was the ONE SYNCHRONOUS airc round-trip on every
        // live compose (ColdStartCritical, never deferred). ONE subscribe
        // stream per persona marks both caches; the invalidator holds weak
        // handles and dies with them. No wrap without a wire: if subscribe
        // fails, both stay raw (correct, just slow) and we log loud. NOTE the
        // roster is deliberately NOT cached — room_roster(within=120s, …) is
        // a recency projection that DECAYS with no event firing; a cached
        // roster shows ghosts.
        let (doctrine_source, wall_source): (
            Arc<dyn crate::cognition::rag_budget::RagSource>,
            Arc<dyn crate::cognition::rag_budget::RagSource>,
        ) = match runtime.subscribe_all_rooms().await {
            Ok(stream) => {
                let (doctrine_cached, doctrine_dirty) =
                    crate::cognition::cached_source::CachedRagSource::new(raw_doctrine);
                let (wall_cached, wall_dirty) =
                    crate::cognition::cached_source::CachedRagSource::new(raw_wall);
                // The invalidator owns a subscription to every room; it lives and dies with
                // the runtime (aborted in the runtime's Drop), never as an orphan task.
                runtime.own_task(crate::cognition::grounding_invalidation::spawn_publish_invalidator(
                    stream,
                    vec![doctrine_dirty.downgrade(), wall_dirty.downgrade()],
                ));
                (doctrine_cached, wall_cached)
            }
            Err(e) => {
                tracing::warn!(
                    persona = %identity.agent_name,
                    error = %e,
                    "doctrine/wall cache UNWIRED (subscribe failed) — serving raw \
                     airc fetch per compose; slow but never stale"
                );
                (raw_doctrine, raw_wall)
            }
        };

        // Room-board source: grounds the persona in the CURRENT ROOM's WHOLE
        // work board — every card, its column, priority, and owner — read live
        // from the same airc board a human's kanban widget renders (the desktop
        // app projects it into a KanbanViewState; this reads airc DIRECTLY, one
        // shared board, two faces, never through each other). The Observer
        // perceiving the shared plan, the complement to the active-work source's
        // own-claims-only view: whole-board-this-room vs own-claims-cross-room.
        // Reads the persona's own airc handle (upcasts to `RoomBoardReader`, a
        // supertrait of AircCitizen). Enriching framing, NOT a participation
        // gate — bound brain-only + defer-tolerant like the active-work + wall
        // sources. Task #117 O6.
        let room_board_source: Arc<dyn crate::cognition::rag_budget::RagSource> = Arc::new(
            crate::persona::room_board_source::RoomBoardSource::new(
                identity.peer_id.as_uuid(),
                runtime.clone(),
            )
            // Bound to the room she joined at bootstrap — the room her airc
            // connection (the reader) answers for. The room gate in deliver then
            // keeps this grounding out of turns in OTHER contexts (another room,
            // the eval fork's nil room) — the exam-bleed fix (#127).
            .for_room(identity.default_room),
        );

        // Live-call perception: the persona's room-as-NOW visual grounding — WHO is
        // visible on the call + a description of what they show, read NON-BLOCKING from
        // the persona's PerceptionBuffer (only cells resolved this tick; the turn never
        // waits) and BUDGETED through the same flexbox allocator so perception can never
        // dominate context ([[perception-feedback-must-not-blow-rag]]).
        //
        // The buffer is the ADDRESSABLE home in `media::perception_registry` — the ONE
        // seam the LiveKit media ingest (#192/#193) warms via `observe()` and this source
        // reads, resolved by persona_id from BOTH sides (the "one home, two reachers"
        // shape of `focus::registry()`). Its cells resolve on the runtime's ONE
        // `SharedCompute` (`shared_compute::global()`, which the Runtime adopts), so a
        // frame warmed once by ingest is a cache hit for EVERY viewing persona:
        // compute-once / share-many, the multi-persona vision moat
        // ([[vision-replication-is-the-multipersona-moat-vs-cloud]]). Empty until frames
        // arrive — this wires the READ path into cognition; the video_rx → observe ingest
        // is #192.
        let perception_buffer =
            crate::media::perception_registry().handle(identity.peer_id.as_uuid());
        let media_perception_source: Arc<dyn crate::cognition::rag_budget::RagSource> = Arc::new(
            crate::persona::media_perception_source::MediaPerceptionSource::new(
                identity.peer_id.as_uuid(),
                perception_buffer,
                crate::runtime::shared_compute::global(),
            ),
        );

        // Disk-backed, per-persona memory: open <home>/engrams.sqlite and
        // rehydrate prior engrams + recall metadata, so memory SURVIVES restart.
        // Without this, admission is in-memory only (NoopSink) and the persona is
        // amnesiac across boots. `identity.home` is the resolved
        // <root>/personas/<name> dir. A storage or schema failure refuses this
        // slot before registration: continuing with the fresh NoopSink state
        // would hide prior engrams and lose future admissions across restart.
        // A new home succeeds with an empty persistent store. MUST run before
        // WorkspaceCycle assembly so RecallFaculty binds persisted admission.
        cognition.attach_persistent_admission(
            identity.peer_id.as_uuid(),
            std::sync::Arc::new(persisted_admission),
        );
        let system_prompt = build_persona_system_prompt(&identity.agent_name);

        // Assemble this persona's continuous mind into the process-global
        // workspace registry — ONE WorkspaceCycle per persona, keyed by
        // persona_id (the "one soul, many rooms" invariant, PERSONA-BRAIN-
        // ARCHITECTURE.md §2.9). The shared hippocampus (cognition.admission)
        // and the persona's inference adapter are leased into its faculties.
        // Cheap (no model load — the adapter lazy-loads on first inference).
        // Additive: makes `ai/should-respond` resolvable for this persona; does
        // NOT change the existing service-loop decision path (heuristics stay
        // live until the coordinated cutover).
        // register_from_cfg (overwrite + retain a fork-template), not
        // get_or_build: a persona can respawn in the same process (node
        // resilience). get_or_build is idempotent by persona_id and would DISCARD
        // the fresh admission + adapter, leaving the mind bound to the prior
        // lifetime's orphaned (rehydrated-then-replaced) AdmissionState —
        // newly-admitted engrams invisible to recall, the "severed" failure across
        // a restart. Build + register replaces it. The retained cfg template is
        // what lets `cognition/eval` fork an ephemeral measurement copy without
        // touching this living mind (PersonaWorkspaceRegistry::fork_eval_cycle).
        let brain_cfg = crate::cognition::persona_workspace::PersonaBrainConfig {
            persona_id: identity.peer_id.as_uuid(),
            persona_name: identity.agent_name.to_string(),
            system_prompt: system_prompt.to_string(),
            admission: cognition.admission.clone(),
            adapter: adapter.clone(),
            // S1: the node's recipe registry, so this citizen's tool surface follows
            // the room she acts in (its recipe's affordances), not a global list.
            experience: crate::experience::source::node_experience_source(),
            // Neural recall when the embed model serves, lexical otherwise
            // — decided once here (process-stable; query + stored vectors
            // must share one embedding space). Already cached by the
            // resolver (embed-once-per-content, shared across personas).
            embedder: Some(
                crate::ai::embedding::resolve_recall_embedder(adapter.clone()).await,
            ),
            // Roster + doctrine bridged into the brain as STANDING-FRAMING
            // grounding faculties (high salience floor). Without these the
            // gating cutover routes decisions through the Workspace and the
            // #1650/#1651 grounding silently falls out of the live path —
            // the persona forgets who is present / what the room is for.
            grounding_sources: vec![
                // Roster — WHO is present. SYNCHRONOUS (ColdStartCritical),
                // like workspace-map and for the same reason: the deferral
                // was earned by the OLD airc-fetch reader this source
                // replaced; the ViewState roster is a watch-channel borrow
                // (microseconds), and keeping the fallback meant slow turns
                // "timed out" into `reproject_to_now` serving a CACHED
                // roster from a different presence moment — Benchy's prompt
                // flapped between fresh and stale byte layouts at 0.4%
                // depth, hit_rate 0.0, and slower turns caused MORE
                // reprojections (measured 2026-09-01: both his captures
                // carried "[reprojected … held]"). A source this cheap
                // never gets a fallback ([[fallbacks-are-illegal-fail-loud]]).
                crate::cognition::persona_workspace::GroundingSource::framing(roster_source),
                // Doctrine — WHAT the room is for: the PARTICIPATION GATE. This
                // one stays SYNCHRONOUS (ColdStartCritical): a cold-start `None`
                // would let the persona speak in a room it shouldn't on turn one,
                // which is wrong, not merely unenriched. The lone exception to
                // "defer almost everything."
                crate::cognition::persona_workspace::GroundingSource::framing(doctrine_source),
                // The persona's own live work across rooms — enriching framing so
                // it knows what it's working on (cross-activity, dynamic, no
                // hardcoded card state). Defer-tolerant.
                // Where else she is needed while she works here: it changes every turn.
                crate::cognition::persona_workspace::GroundingSource::framing(awareness_source)
                    .defer_tolerant()
                    .volatile_content(),
                crate::cognition::persona_workspace::GroundingSource::framing(active_work_source)
                    .defer_tolerant()
                    // Claim states flap per turn — floor stays, stable-tier
                    // placement goes (debug/prompt-reuse conviction, 2026-08-22).
                    .volatile_content(),
                // WHERE code lives — the real workspace layout as framing, so a
                // reasoner can avoid blind globs like `src/**/*.rs` from the
                // prompt alone. ColdStartCritical (synchronous, NOT deferred):
                // measured 2026-07-13 that the deferred version was ABSENT on
                // cold ticks (worst under repeated reboots), so some personas
                // acted blind to the layout — a WRONG turn (blind-glob loops),
                // not merely unenriched, which is exactly the ColdStartCritical
                // bar. It's a cheap local dir listing (unlike the airc-backed
                // framing sources that stay deferred), so it earns synchronous
                // presence like doctrine. requires_hands: the block SAYS "drill
                // in with code/list and code/tree" — it must vanish from a
                // tool-stripped cycle (spoken exams) or the RAG lies about her
                // affordances.
                crate::cognition::persona_workspace::GroundingSource::framing(workspace_map_source)
                    .requires_hands()
                    // Her OWN writes mutate the map — every productive act. It sat
                    // in the stable tier breaking the KV prefix at ~8k chars of a
                    // 40k prompt (measured 2026-08-23, turn-over-turn capture diff).
                    .volatile_content(),
                // The room's pinned shared documents (airc wall) as
                // enriching framing — the plan/instructions/recipe that
                // shape HOW the persona works here, read from the exact
                // rows a teammate or widget pins. Defer-tolerant: a
                // first-tick miss costs one under-grounded turn, not a
                // wrong one.
                crate::cognition::persona_workspace::GroundingSource::framing(wall_source)
                    .defer_tolerant(),
                // The room's WHOLE work board (airc kanban) as enriching
                // framing — every card/column/owner, so the persona can
                // coordinate against the shared plan, not just its own
                // claims. Defer-tolerant: a first-tick miss costs one
                // under-grounded turn, not a wrong one. Task #117 O6.
                crate::cognition::persona_workspace::GroundingSource::framing(room_board_source)
                    .defer_tolerant()
                    // Card/column states churn with the round — same conviction.
                    .volatile_content(),
                // Live-call perception (#187/#192): WHO is visible on the call +
                // what they show, as enriching framing. Defer-tolerant: a
                // first-tick miss costs one under-grounded turn, not a wrong one —
                // and perception is non-blocking by construction (absent cells are
                // simply not present this tick, never awaited). NOT requires_hands:
                // seeing is a SENSE, not a tool, so it stays present in a
                // tool-stripped (spoken-exam) cycle. Reads only ready cells (O(participants)
                // string assembly, no inference) — off the 30fps media plane entirely.
                crate::cognition::persona_workspace::GroundingSource::framing(
                    media_perception_source,
                )
                .defer_tolerant(),
            ],
            // The persona's HANDS — built by the caller for THIS persona's
            // identity (None → speak-only). What turns "talks" into "acts".
            tool_executor,
            // The window the gateway actually serves this persona (task #50:
            // single-sourced; Local → ServingPlan.served_context_window). The
            // deliberation faculty keeps its prompt inside it so llama-server
            // never 500s ("Context size has been exceeded").
            context_window: profile.context_length,
            // LIVE mind: recall runs as a speculative prefetch off the hot
            // path (Joel's CPU branch-prediction analogy). Turns here are
            // seconds apart, so the background worker always catches up and
            // the per-turn output reads a warm last-good instead of waiting on
            // a neural-embed + vector-search round-trip. Eval forks override
            // this to false (faithful synchronous measurement).
            defer_recall: true,
            // LIVE mind: push the defer-tolerant grounding (roster, active_work,
            // workspace_map) off the hot path too — the 90%-async win for the
            // enriching framing. Doctrine (ColdStartCritical) stays synchronous
            // regardless. Eval/harness override to false.
            defer_grounding: true,
            // The LIVING persona always keeps her memories — suppression is a
            // benchmark-reproducibility knob, never a life-path setting (#207).
            suppress_recall: false,
            forked: false,
            read_receipt: Some(Arc::clone(&airc) as Arc<dyn crate::cognition::llm_deliberation_faculty::ReadReceipt>),
        };
        // A checkpoint can be locked by offline adoption. Waiting and reading
        // belong off the async runtime workers; registration publishes nothing
        // until the existing snapshot has loaded successfully.
        #[cfg(test)]
        let fixture_home = crate::paths::home_dir();
        let registration = tokio::task::spawn_blocking(move || {
            #[cfg(test)]
            let _fixture_home = fixture_home
                .as_deref()
                .map(crate::paths::NativeHomeOverride::install);
            crate::cognition::persona_workspace::global().register_from_cfg(brain_cfg)
        })
        .await;
        let failure = match registration {
            Ok(Ok(_)) => None,
            Ok(Err(error)) => Some(error),
            Err(error) => Some(std::io::Error::other(error)),
        };
        if let Some(source) = failure {
            out.push(Err(SupervisorError::WorkspaceRegistration {
                slot_index,
                role: plan.role,
                persona_id: identity.peer_id.as_uuid(),
                source,
            }));
            continue;
        }

        out.push(Ok(PersonaContext {
            role: plan.role,
            identity,
            profile,
            adapter,
            runtime,
            cognition: Arc::new(cognition),
            system_prompt,
        }));
    }
    out
}
