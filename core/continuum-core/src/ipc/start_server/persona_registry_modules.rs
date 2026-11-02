//! Boot phase 11a: the persona registry and every module that rides it
//! (grid capacity, work, room, activity, benchmark grade, governor, instance
//! manager, grant issuance, rag-inspect).

use super::*;

pub(super) fn register_persona_registry_modules(
    runtime: &Arc<Runtime>,
    resource_daemon: &Arc<crate::resources::ResourceDaemon>,
    memory_manager: &Arc<crate::memory::PersonaMemoryManager>,
    pressure_monitor: &Arc<crate::system_resources::MemoryPressureMonitor>,
    daemon_socket: std::path::PathBuf,
    discovered_default_room: Option<airc_core::RoomId>,
) -> Arc<crate::persona::instance_manager::PersonaInstanceManagerModule> {
    // Grid capacity gossip (#56 step 4): this node offers its live capacity to
    // the grid on the module tick and hears every peer's offers (its own echo
    // included — the loopback proof) via inbound_attach → gossip::global_ledger.
    // Rides the DISCOVERED default room — the ONE consumer in this block that
    // genuinely needs it (#432: the room is a gossip channel, not a birth dep;
    // citizens must come online whether or not a default room was discovered).
    match discovered_default_room {
        Some(default_room) => {
            runtime.register(Arc::new(
                crate::modules::grid_capacity::GridCapacityModule::new(
                    resource_daemon.clone(),
                    default_room,
                ),
            ));
        }
        None => {
            tracing::warn!(
                "no default room discovered — grid capacity gossip is NOT \
                     registered this run; citizens still boot (#432)"
            );
        }
    }
    let continuum_root = crate::paths::continuum_root();
    let daemon_socket_for_rag_inspect = daemon_socket.clone();
    let registry = crate::persona::PersonaAircRuntimeRegistry::new();
    // Publish the live roster process-globally so host-independent callers — the
    // detached cognition/eval body — can acquire an eval-preemption lease over
    // the fleet without a threaded handle. First writer wins (this boot path).
    crate::persona::PersonaAircRuntimeRegistry::set_global(registry.clone());
    // Publish THE bus process-globally too (same first-writer-wins shape) so
    // host-independent bodies — the detached eval's `eval:progress` scoreboard —
    // can emit events widgets/observers subscribe to (#123/#141).
    crate::runtime::MessageBus::set_global(runtime.bus_arc());
    crate::resources::ResourceDaemon::set_global(resource_daemon.clone());
    // Native airc kanban tools — personas claim/create/release cards on the
    // shared board as THEIR OWN airc key, delegating to airc's work API. Shares
    // the SAME registry (cheap Clone over an inner Arc) so a work command can
    // resolve the calling persona's live airc runtime. Registered before the
    // executor is built so its typed commands land on the one registry.
    runtime.register(Arc::new(crate::modules::work_seams::work_module(
        registry.clone(),
        memory_manager.embedding_provider(),
    )));
    runtime.register(Arc::new(crate::modules::room::RoomModule::new(
        registry.clone(),
    )));
    // activity/* (#274) — the verb that turns a recipe into a room. Same
    // registry: creating a room acts as the CALLER's own airc identity, so the
    // creator is a real peer rather than the substrate acting anonymously.
    runtime.register(Arc::new(crate::modules::activity::ActivityModule::new(
        registry.clone(),
    )));
    // Event-driven SWE grade-on-done — the REACT half of the benchmark adapter,
    // and the verb that actually CLOSES the kanban benchmark loop. Subscribes to
    // work.card.state_changed (emitted by work/state) and, when a benchmark SWE
    // card reaches a terminal state, grades the citizen's workspace diff against
    // the HELD-OUT oracle in a fresh clone at base_commit (launder-proof) and posts
    // the verdict into the room. No commands, no tick — a pure event subscriber
    // ([[the-whole-system-is-event-based-not-polling]]). Registered here, at the
    // real boot site, because `impl ServiceModule` registers nothing: until this
    // line, the module existed but never reached dispatch, so grade-on-done never
    // fired in production.
    runtime.register(Arc::new(
        crate::modules::benchmark_grade::BenchmarkGradeModule::new(registry.clone()),
    ));
    // CITIZEN HEALTH IS A SUBSTRATE RECEIPT (2026-09-14): the hour's acts, writes,
    // lane grants and settles beside residency and lanes, judged and said in the
    // org room by the core itself — never a hand read again.
    runtime.register(Arc::new(
        crate::modules::citizen_health::CitizenHealthModule::new(),
    ));
    // CARD CARRY: one owner keeps residents' committed card work on origin (2026-10-17).
    runtime.register(Arc::new(crate::modules::card_carry::CardCarryModule::new()));
    // SubstrateGovernor — the deterministic cognitive-region scheduler daemon.
    // Schedules the ChannelDigestRegion: per live persona it pre-stages the
    // persona's current-channel digest into the SHARED digest buffer
    // (channel_substrate globals) that AircRagSource peeks. Flood-safe — building
    // a digest runs NO inference (element embeddings are lazy). Slice 2C live.
    let digest_region: Arc<dyn crate::runtime::BrainRegion> = Arc::new(
        crate::cognition::channel_digest_region::ChannelDigestRegion::with_buffer(
            crate::cognition::channel_substrate::global_channel_digest_builder(),
            Arc::new(registry.clone())
                as Arc<dyn crate::cognition::channel_digest_region::PersonaChannelReader>,
            crate::cognition::channel_substrate::global_channel_digest_buffer(),
        ),
    );
    // Every mind's resident result bodies answer to the one memory owner: they stay in RAM
    // until the monitor sheds, never a size cap of ours (Joel, 2026-10-29).
    // The shared channel-element cache is rebuildable (airc is the record): RAM sizes it, it sheds
    // least recently used first under pressure, never an entry cap of ours.
    crate::cognition::channel_substrate::global_channel_element_cache().register(pressure_monitor);
    let mind_results = Arc::new(crate::cognition::persona_workspace::MindResultsReporter::new(
        crate::cognition::persona_workspace::global(),
        pressure_monitor.subscribe(),
    ));
    // One overrun is shared across its priority's holders (Cormac on #5697): it joins the group.
    let holder: Arc<dyn crate::system_resources::memory_pressure::Holder> = mind_results.clone();
    crate::system_resources::memory_pressure::register_holder(Arc::downgrade(&holder));
    pressure_monitor.add_reporter(mind_results);
    // The dream/consolidation region goes LIVE (#145 slice B): per live persona,
    // on a material-driven cadence (dreams only when undigested episodic
    // experience accrues, `CadenceHint::Sleep` otherwise), it distills episodic
    // clusters into durable Semantic facts and leaves ONE `[thought:historian]`
    // SelfReflection per dreaming tick — the first mind-wanderer. Hippocampus +
    // adapter resolve per tick from the live workspace registry (re-home-safe),
    // never a parallel persona→adapter map.
    let dream_region_concrete = Arc::new(
        crate::cognition::dream_consolidation::DreamConsolidationRegion::new(
            crate::cognition::persona_workspace::global()
                as Arc<dyn crate::cognition::dream_consolidation::PersonaReflectionSource>,
        ),
    );
    // Install the flywheel handle: `cognition/dream-now` drives THIS region
    // on demand (factory mode) — same instance the governor ticks, so the
    // per-persona single-flight guard holds across both drivers.
    crate::cognition::dream_consolidation::install_global(dream_region_concrete.clone());
    let dream_region: Arc<dyn crate::runtime::BrainRegion> = dream_region_concrete;
    // Wire the live memory-pressure feed (R4 slice 3): each pass sizes its slice
    // budget to the host's current memory band so a society of inference-bearing
    // background regions can't stampede the model backend under load. A homeostatic
    // protection, not cognition steering — and on a healthy host the band is Normal
    // → budget None → behavior is identical to the uncapped default the digest
    // region runs under today (the digest runs no inference; the consolidation
    // region is the first InferenceHeavy tenant under this floor).
    // A reviewed credit the training trigger has not accepted is settled again by this region,
    // per resident, whether or not she acts (card 99b2773a): its store is hers, as her identity.
    let credit_registry = registry.clone();
    let credit_region: Arc<dyn crate::runtime::BrainRegion> = Arc::new(
        crate::persona::training_producer::CreditAcceptanceRegion::new(Arc::new(move |persona| {
            let name = credit_registry.get(persona)?.agent_name().to_string();
            let conn = crate::persona::training_producer::reviewed::credit_connection(persona).ok()?;
            Some((name, conn))
        })),
    );
    runtime.register(Arc::new(
        crate::persona::substrate_governor::SubstrateGovernor::new(vec![digest_region, dream_region, credit_region], registry.clone())
            .with_pressure_gate(pressure_monitor.subscribe()),
    ));
    let instance_manager = Arc::new(
        crate::persona::instance_manager::PersonaInstanceManagerModule::new(
            registry,
            daemon_socket,
            continuum_root,
        ),
    );
    // Task #222 + R1/R2 BLOCK on PR #1568: the executor lookup
    // happens LAZILY inside `bootstrap_one` (in PIM), so no
    // ordering coupling with `init_executor_with_interceptors`
    // further down in `start_server`. The pre-fix shape
    // eagerly fetched the global here and panicked because
    // `init_executor` hadn't run yet.
    runtime.register(Arc::new(crate::modules::persona_instance_manager::PersonaInstanceManagerService(
        instance_manager.clone(),
    )));
    log_info!(
        "ipc",
        "server",
        "PersonaInstanceManagerModule registered — citizens can be spawned via \
             `persona/spawn`"
    );

    // `grid/grant/issue`: the owner mints capability grants signed by a running
    // persona's airc identity (sharing the same runtime registry). Owner-gated
    // by ACL (not in the cross-grid allow-list), so only the local operator can
    // sell its personas' compute. Closes the contracted-grid loop with an
    // operator surface over `routing::grant_issuance::issue_grant`.
    runtime.register(Arc::new(
        crate::modules::grant_issuance::GrantIssuanceModule::new(
            instance_manager.registry().clone(),
        ),
    ));
    log_info!(
        "ipc",
        "server",
        "GrantIssuanceModule registered — owner can mint grants via `grid/grant/issue`"
    );

    // ── persona/rag-inspect — RAG introspection callable from any AI ──
    //
    // FilesystemPersonaResolver reads the persona's seed.json + attaches
    // via airc_lib::Airc::attach_as using the same continuum_root +
    // daemon_socket the instance manager just used. The module exposes
    // the `persona/rag-inspect` command so sentinel personas, Claude,
    // and any other AI can `Commands.execute('persona/rag-inspect', {
    // persona: 'Paige' })` to honestly see what Paige's RAG layer would
    // surface right now. Per [[observability-is-half-the-architecture]].
    //
    // chain_inference path stays RAG-only here (default_adapter=None).
    //
    // The old reason cited on this line — "AdapterRegistry is Box-based
    // + can't hand out Arcs" — is a STALE LIE (same class as #77): #162
    // already moved the registry to `Arc<dyn AIProviderAdapter>` and
    // added `get_arc`, and `global_registry()` hands out Arcs today.
    //
    // The REAL blocker is per-persona model resolution. The canonical
    // live generation path (`cognition/generate_response.rs`) selects
    // via `registry.select(DEFAULT_GENERATE_PROVIDER, Some(&session.model),
    // Auto)` — it needs the persona's SESSION MODEL to honor the
    // no-fallbacks `select()` guard (which refuses no-specifier
    // auto-discovery). The FilesystemPersonaResolver only reads
    // seed.json (persona_id + name); it has no session/model, and the
    // substrate "doesn't yet model per-persona adapter preferences"
    // (see FilesystemPersonaResolver::with_default_adapter). Threading
    // the persona's live model in is the real follow-up — NOT an Arc
    // refactor. Until then the chained variant is exercised by unit
    // tests with an explicit adapter; RAG-only is the honest default
    // for production callers (the recent deliveries already carry the
    // persona's own last generations, which is what an inspector reads).
    let rag_inspect_resolver = std::sync::Arc::new(
        crate::modules::persona_rag_inspect_filesystem::FilesystemPersonaResolver::new(
            crate::paths::continuum_root(),
            daemon_socket_for_rag_inspect,
        ),
    );
    let rag_inspect_module = std::sync::Arc::new(
        crate::modules::persona_rag_inspect::PersonaRagInspectModule::new(rag_inspect_resolver),
    );
    runtime.register(rag_inspect_module);
    log_info!(
        "ipc",
        "server",
        "PersonaRagInspectModule registered — `persona/rag-inspect` available"
    );
    instance_manager
}
