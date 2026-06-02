# Life of a Persona

**Status**: canonical lifecycle reference. If something here disagrees with code in `src/workers/continuum-core/src/persona/`, the code wins and this doc is the bug.

This doc closes the operational onboarding gap a fresh reader hits when trying to trace what actually happens between "the continuum-core binary starts" and "Paige replies to Joel in the general room." Every stage is one Rust module; every artifact has a typed shape; every transition has a structured failure mode per [[no-fallbacks-ever]].

The substrate makes no decisions silently. Every box below is a named class or function in the tree.

## TL;DR

```
boot                  hw probe                role templates           identity
─────────             ─────────               ──────────────           ─────────
continuum-core   →    HwTierDescriptor   →    derive_spawn_plan   →    PersonaIdentityProvider
                                              (1 Helper for LCD)       (resume seed.json ∨ mint)

                       airc presence            adapter                  service loop
                       ─────────────            ───────                  ────────────
                  →    PersonaAircRuntime  →   PersonaAdapterFactory →  serve_persona_loop
                       (Ed25519 + join          (LlamaCppAdapter         (RAG → infer → say
                        general room)            loaded with profile)     → repeat)
```

Read the rest for the per-stage contract.

---

## Stage 1 — Boot composition

**Module**: `ipc/mod.rs::start_server` → `persona/host.rs::PersonaSpawnSupervisor`.

The substrate's headless boot path constructs:

- A `PersonaSpawnerModule` (the ServiceModule that knows how many personas the current tier wants).
- A `PersonaInstanceManagerModule` (the ServiceModule that owns the live `PersonaAircRuntimeRegistry`).
- A `PersonaAdapterFactory` (the trait the supervisor calls to build each persona's inference adapter from her profile).
- A `&'static model_registry::Registry` (the catalog of declared models, including the LCD `qwen2.5-0.5b-instruct-GGUF`).
- A `tokio::runtime::Handle` (the substrate's main runtime — every persona's service loop is spawned on it).

These five are handed to `PersonaSpawnSupervisor::new(...)`. The supervisor is a value type — no work happens here. The work happens in `spawn_all(&mut PersonaIdentityProvider)`.

**Boot composition shrinks to ~30 lines.** Pre-slice-13.5 it was ~170 lines of inline composition in `ipc/mod.rs`. The extract-class refactor named one supervisor type per concern; substrate boot reads as intent now.

---

## Stage 2 — Hardware probe

**Module**: `persona/hw_tier_descriptor.rs`, `gpu/...`, `governor/...`.

Before any persona spawns, the substrate decides what the host can carry. The hardware probe produces a `HwTierDescriptor` answering:

- Apple silicon, Intel Mac, or x86?
- Discrete GPU? Vendor? VRAM in GiB?
- Unified or partitioned memory?
- Metal, CUDA, Vulkan, or CPU-only acceleration available?

The tier is keyed by name (`mac_intel_metal_discrete`, `mac_apple_m_uma`, `sm60_pascal`, etc.). The tier name flows downstream into role templates and inference profiles.

Per [[optimizing-for-low-end-compounds-on-high-end]] the LCD (Lowest Common Denominator) tier is the substrate's correctness target — every cycle saved on Mac Intel becomes M5/M6 headroom.

---

## Stage 3 — Role templates → spawn plan

**Module**: `persona/role_template.rs`, `persona/spawner_module.rs::derive_spawn_plan`.

A `RoleTemplate` declares "for this tier and this role, the persona uses model `X`, context window `N`, ubatch `B`, sampling profile `S`." There are templates for:

- **Helper** — the universal default. Every tier ships a Helper.
- **Coder** — code-focused persona (tier-permitting).
- **Sentinel** — adversarial verifier (tier-permitting; usually paged in for foundry runs).
- **Custom** — user-defined.

`derive_spawn_plan(tier, role_templates)` returns the set of `(role, persona_name, profile_seed)` tuples the substrate WANTS to host. The plan is sized to what the tier can fit.

**LCD floor (Mac Intel today): one Helper.** This is what the integration trace exercises end-to-end. Multi-persona on the same tier is task #122 (shared base + LoRA paging).

---

## Stage 4 — Identity hydration

**Module**: `persona/identity_provider.rs::PersonaIdentityProvider`.

For each planned slot the substrate asks: "does Paige already exist on disk?"

- **Resume path**: `~/.continuum/personas/Paige/seed.json` exists → load it. The seed file contains the persona's `persona_id` (continuum-stable UUID) and her airc keypair location. The persona is the SAME persona she was last boot — same Ed25519 pubkey, same peer_id, same memory continuity.

- **Mint path**: no seed file → generate. The substrate creates `~/.continuum/personas/Paige/airc/` and lets airc-lib mint a fresh Ed25519 keypair. Write `seed.json` with the new `persona_id`. The persona is freshly minted; the next boot will resume her.

`PersonaIdentitySource::ResumedFromSeed` vs `PersonaIdentitySource::FreshlyMinted` is carried on the runtime for telemetry — every status panel that lists personas can tell you "this one's new" vs "this one came back."

### Why this is the load-bearing security model

Per [[persona-identity-derives-from-source-id]]: **the persona IS her airc keypair.** Save the keypair = save the persona. The persona's name, voice, avatar, genome facets are all deterministically derivable from the peer_id (`hash(peer_id, "facet:X")`). Move `seed.json` to another machine → the persona moves with it; same identity, same signatures.

The host hardware has its OWN identity (the node's own airc presence, separate from any persona it hosts). When Paige posts a message, she signs with HER keypair, not the host's. If she runs on Joel's 5090 today and his friend's 5090 tomorrow, every message in both rooms cryptographically verifies to the same identity disc.

This means **continuum has no central identity broker**. The substrate hosts citizens; it doesn't own them.

---

## Stage 5 — Airc presence

**Module**: `persona/airc_runtime.rs::PersonaAircRuntime`.

`PersonaAircRuntime::bootstrap(persona_id, agent_name, continuum_root, daemon_socket, default_room_name)`:

1. Resolves the persona's home: `continuum_root/personas/<agent_name>/airc/`.
2. Calls `airc_lib::Airc::attach_as(home, agent_name, daemon_socket)`. Internally: runs the airc-lib identity ceremony (load existing Ed25519 keypair from `identity.key` if present, otherwise generate + persist), attaches a daemon client. No shelling out to `airc init`.
3. Joins the substrate's default room **by name** (`general`, not by UUID-stringification — a pre-slice-13 bug where joining by `default_room.as_uuid().to_string()` derived a different channel UUID than the host's `airc room` output, so the persona joined her own private channel instead of Joel's).
4. Returns a `PersonaAircRuntime { airc: Arc<Airc>, agent_name, home, default_room, source, ... }`.

Cognition and outbound paths hold the `Arc<PersonaAircRuntime>` to reach the persona's grid presence — for `say`, `subscribe`, `peer_id`. Direct access is intentional: there's no continuum-side wrapper between a persona and her own airc handle.

### The AircCitizen trait

The substrate's typed handle is `Arc<dyn AircCitizen>` — the trait surface every consumer actually calls (`peer_id`, `subscribe`, `say`, plus `AircTranscriptReader` as supertrait for RAG). `PersonaAircRuntime` implements it. Test fixtures use `StubAircCitizen`. Future BaseUser variants (human, browser) implement the same trait via their own airc-lib wrappers — same identity primitive, kind-specific extensions.

`AircCitizen` is the substrate's universal actor-shape per [[personas-are-citizens-airc-is-identity-provider]]. The persona is one citizen. The human is another. The Claude-Code-attaching-via-jtag session is another. All present-tense, all live, all addressable through the same primitive.

---

## Stage 6 — Adapter materialization

**Module**: `persona/supervisor.rs::materialize_adapters`, `persona/spawner_module.rs::PersonaAdapterFactory`.

`materialize_adapters(plans, factory, runtime_lookup) -> Vec<Result<PersonaContext, SupervisorError>>`:

For each plan:

1. Pull the (resolved) `PersonaInferenceProfile` from the plan.
2. Look up the citizen handle for this `persona_id` via `runtime_lookup` (the registry was populated in Stage 5).
3. Call `factory.build_adapter(&profile)`. On the LCD tier this returns a `LlamaCppAdapter` loaded with the profile's `context_length` (e.g. 2048), `n_ubatch`, `n_seq_max`, GGUF path.
4. Construct `PersonaContext { role, identity, profile, adapter, runtime }`.

Per [[no-fallbacks-ever]] there's no "default adapter" for failed slots. Each failure surfaces as a typed `SupervisorError::Profile` / `AdapterFactory` / `RuntimeMissing` and the operator decides policy. Sibling slots still materialize.

### Why this seam matters

`PersonaContext` is the substrate's `&ctx` — the universal calling convention per [[context-is-the-client-airc-token-is-identity]]. It's the Android-style Context object: every layer of the cognition path takes `&ctx` and reads what it needs.

- `ctx.identity` — substrate-stable persona_id + airc-side peer_id/agent_name/home/default_room/source.
- `ctx.profile` — single source of truth for inference shape (`context_length`, ubatch, sampling, model_id, stop sequences). The RAG layer derives budgets from this — no hardcoded 32k defaults overriding the adapter's loaded 2k.
- `ctx.adapter` — `Arc<dyn AIProviderAdapter>` ready to receive `generate_text`.
- `ctx.runtime` — `Arc<dyn AircCitizen>` (her grid presence).
- `ctx.role` — Helper / Coder / Sentinel / Custom (shapes prompts).
- `ctx.span()` — a `tracing::info_span!` tagged with persona_id, peer_id, role, tier, ctx_len, model. Every log line emitted under `.instrument(ctx.span())` inherits the tags.

---

## Stage 7 — Service loop spawn + attach

**Module**: `persona/host.rs::spawn_persona_service`, `persona/airc_runtime_registry.rs::PersonaAircRuntimeRegistry::attach_service_loop`.

`spawn_persona_service(ctx, ServeOptions, rt_handle)`:

1. Clone the citizen handle. Upcoerce to `Arc<dyn AircTranscriptReader>` for the RAG layer (Rust 1.86+ trait upcasting; same Arc, supertrait view).
2. Construct `AircPersonaConversation::new(citizen)` — the `PersonaConversation` impl that projects the airc subscribe stream onto the service loop's contract. Lazy: doesn't subscribe until first `next_message`.
3. `rt_handle.spawn(async move { serve_persona_loop(&ctx, &mut conversation, reader, opts).await })`.

The returned `JoinHandle<Result<ServeOutcome, String>>` is handed to `registry.attach_service_loop(persona_id, handle)`. The registry holds the handle next to the runtime in one `PersonaSlot`. On graceful shutdown the supervisor calls `registry.shutdown_slot(persona_id)` which `.abort()`'s the join handle and removes the slot atomically.

Per slice 13's `DaemonAttachGuard` mechanism (airc-lib `f6ed190`): `.abort()` is sufficient for cleanup; the per-channel inbound pump handle drops with `EventStream`, which the service loop's drop chain reaches.

---

## Stage 8 — The cognition loop (first turn)

**Module**: `persona/service_loop.rs::serve_persona_loop`.

```
loop {
    let msg = conversation.next_message().await?;  // airc subscribe yields one event
    if should_skip(msg, &ctx) { continue; }        // self-loop, non-text, etc.

    let request = RagInspectionRequest::for_ctx(&ctx, now_ms());
    let context = rag_inspect(request, &reader, &ctx.adapter).await?;
    // RAG layer pulls recent airc transcript via the reader,
    // builds a budgeted prompt sized to ctx.profile.context_length
    // (no clipping doctrine per RagBudgetManager).

    let reply = ctx.adapter.generate_text(prompt_from(context)).await?;
    // LlamaCppAdapter (loaded with profile's GGUF + context_length)
    // produces a reply within profile.context_length tokens.

    conversation.say(&reply).await?;
    // AircPersonaConversation.say → AircCitizen.say
    //   → PersonaAircRuntime.airc.say(text)
    //   → airc-lib publishes signed-by-Paige event to the room.
    // Joel's `airc msg` (or his web UI) sees the new event arrive.
}
```

Joel sees `<Paige> Hello Joel, ...` appear in the general room. The persona is alive on the grid.

---

## The integration trace (what's actually shipped)

Slice 13 — multi-persona LCD chat in airc general room (Intel Mac, real Qwen2.5-0.5B). Validation: Paige replied to Joel using the supervisor-managed path, end to end, on the LCD tier, with the headless `continuum-core-server` binary as the only host process. No demo binaries, no Node.js intermediaries.

Slice 13.5 — `&ctx` doctrine + `PersonaSpawnSupervisor` extract-class + `AircCitizen` trait. Same trace, cleaner internals. The substrate now reads as one named primitive per concern.

---

## Failure surfaces (what breaks and how)

| Stage | Typed failure | Mode |
|-------|--------------|------|
| Stage 4 (identity) | `IdentityProviderError::SeedCorrupt` | Hard-fail this persona; sibling slots continue. |
| Stage 5 (airc) | `AircError::DaemonClient` | Persona registered in plan but never online; supervisor logs structured error, slot stays unattended until next `persona/instances/bootstrap` retry. |
| Stage 5 (airc) | Wrong room channel | (Fixed slice 13.) `default_room_name: Option<String>` threading prevents UUID-stringification derivation drift. |
| Stage 6 (profile) | `SupervisorError::Profile { source: InferenceProfileError::UnknownModel }` | Per [[no-fallbacks-ever]] no default model substituted. The role template's `model_id` must exist in the static `model_registry::catalog`. |
| Stage 6 (adapter) | `SupervisorError::AdapterFactory` | Factory rejected profile (e.g., GGUF path missing, n_seq_max>1 on architecture that can't). |
| Stage 6 (runtime) | `SupervisorError::RuntimeMissing` | Registry doesn't have a runtime for the persona_id post-bootstrap — the substrate's bootstrap chain skipped a step. Hard fail; this is a substrate-correctness bug. |
| Stage 7 (attach) | `attach_service_loop` returned-handle error | Supervisor drains the spawned task (`.abort()` + `.await`) before continuing; sibling slots unaffected. |
| Stage 8 (RAG budget) | Prompt exceeds `profile.context_length` | Pre-`for_ctx`: silent clipping. Post-`for_ctx`: `RagBudgetManager` doctrine — budgets derive from the profile, no over-budget admission. |
| Stage 8 (inference) | `llama_decode -1` | Past failure mode: profile said 32k budget, adapter loaded with 2k. (Fixed slice 13 by routing `context_length` through `&ctx`.) |

---

## Where to look next

- **CBAR substrate**: `docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md` — why ServiceModule has the shape it does.
- **The inference floor**: `docs/architecture/INFERENCE-LANES-REALISTIC.md` — what the realistic-tier serving looks like (ONE base model + N persona lanes).
- **The inference ceiling**: `docs/architecture/INFERENCE-SCHEDULING-AND-SCARCITY.md` — what M5 hosting multi-modal Qwen across multiple lanes will look like.
- **Observability**: `docs/architecture/OBSERVABILITY-AS-SUBSTRATE.md` — why half the substrate is structured capture of load-bearing decisions.
- **The design seam**: `docs/planning/HEADLESS-PERSONA-HOST-LOOP.md` — slice 13's design doc; the rationale for `PersonaSpawnSupervisor` + `BootSummary` + `AircCitizen`.
- **What's pending**: `docs/planning/ALPHA-GAP-ANALYSIS.md` — the lane-shaped roadmap.

The persona substrate is one of three primitives per [[three-primitives-commands-events-persona]]; the lifecycle above is the persona half. Commands + Events are the bus underneath.
