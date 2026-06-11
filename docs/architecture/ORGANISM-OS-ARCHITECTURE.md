# Organism OS On The airc Bus — The Integrating Map

**Status**: architecture reference. The map *above*
[CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) (the runtime
contract) and [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) (the
artifact economy). It does not duplicate either; it shows how five layers —
the airc bus, the CBAR runtime, personas, activities/rooms, and the immune
system — are **the same pattern instantiated at five scales**, and grounds each
layer in a real repo primitive or marks it a **NAMED GAP**.

**Companion docs (read those for the deep dives, not here):**
- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) — the
  `ServiceModule`/`RuntimeModule` contract every cell inherits. **L1 of this
  map is that document.**
- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) — what every cell
  *recalls / composes / evolves*: genome pool, foundry, sentinel-AI, governor,
  demand-aligned recall. **The L2 working-set and the L4 sentinel are that
  document.**
- airc is a separate repo (`rust-rewrite` branch). File:line citations into
  airc are against that checkout; the airc-side gaps below are explicit.

---

## Thesis: One Pattern At Every Layer

Every layer of Continuum is the same organism, scaled:

> **An organism advertises its needs and offers, wakes on an event
> (a dependency becoming ready, a cadence tick, or an explicit signal),
> chooses a strategy through an adapter seam, does the smallest piece of real
> work, and degrades with a visible reason instead of lying.**

A module does it. A persona does it. An activity does it. A federated grid does
it. The thing they all ride is the airc bus, and **airc is just the bus** — it
moves typed envelopes between beings and refuses to grow domain concepts
(`task_negotiation.rs:27-34`: "the substrate doesn't grow domain concepts …
intelligence is in the consumer"). The five layers below are not five designs.
They are one design seen at five magnifications.

The load-bearing claim is that *the decision point is always the same shape* — a
seam where a heuristic floor today is a learned policy tomorrow, with a graceful
fallback in between. That seam is enumerated in its own section; it is what makes
this a single architecture rather than five.

---

## The Five Layers

```text
  L4  IMMUNE / GOVERNANCE   sentinels, forge-alloy contracts, threat collab
       │  (default-BLOCK → allow-on-evidence → ALLOW-THEN-MONITOR)
  L3  ACTIVITIES / ROOMS    OS spaces: forts, neighborhoods, security room
       │  (activities emit RuntimeFrames; rooms = address space)
  L2  PERSONAS              ServiceModules riding airc identities; probed caps
       │  (local-first inference, the 1/8…8/8 chain on airc)
  L1  CBAR ORGANISM RUNTIME ServiceModule + PressureBroker + Governor (RTOS)
       │  (multiple RTOSes share one bus)
  L0  airc BUS              beings, rooms, cost-ordered transports, work-cards
            (neutral, solid, no-hub)
```

---

### L0 — The airc Bus

**Pattern role:** the neutral substrate every higher organism advertises and
wakes on. airc owns *who*, *where*, and *how-to-reach*; it owns no cognition.

**A being is a peer.** `PeerId` is the canonical "who is this"
(`airc-core/src/ids.rs:86-90`), and a peer's user-facing card carries
`name / pronouns / role / bio / status / fingerprint / integrations`
(`airc-core/src/identity.rs:24-57`). Identity is **fields, not a subsystem** —
"there is no separate account-management layer with passwords, session tables,
or recovery flows" (`identity.rs:1-10`) — and consumers like Continuum "bind
their user records to airc identities by pubkey rather than maintaining parallel
account semantics" (`identity.rs:8-10`). A Continuum persona therefore *is* an
airc peer; it registers under the `integrations` map (`identity.rs:50-56`).

> **PRECISION — `peer=being`, not `peer=keypair`.** Be exact about what airc-core
> says. `PeerId` is a **UUID newtype** — "the canonical who is this"
> (`airc-core/src/ids.rs:86`) — *not* the Ed25519 verifying key itself. Ed25519
> signing is **owner-stamped at the envelope** (`airc-bus/src/envelope.rs`:
> `seq`/`occurred_at_ms` are router/owner-assigned and outside the sender's
> signature scope), not a property of the peer record. Persona/identity types
> live in `airc-core/src/identity.rs` (there is no `persona.rs`). The Continuum
> framing that "a persona **is** her keypair / save the keypair = save the
> persona" — identity derivable from a seed — is a **continuum-side aspiration**,
> not a literal airc-core fact. State it as the aspiration it is; the airc-core
> contract today is UUID-identity + envelope-level owner signing.

**Rooms are the address space.** A `RoomId` is a room/channel handle; display
names (`#general`) are mutable handles on top (`airc-core/src/ids.rs:80-84`).
Higher layers scope work *by room* — this is what makes L3 (activities/rooms as
OS spaces) possible without inventing a second addressing scheme.

**Transports are cost-ordered and optional.** The resolver is "deliberately
policy-driven and transport-agnostic … accepts measured candidates and applies
`RoutePolicy`" (`airc-lib/src/route/resolver.rs:1-12`). `RoutePolicy::choose`
filters healthy + allowed candidates and picks `min_by_key(priority)` —
cost-ordered selection, with `NoRoute` returned (not a silent fallback) when
nothing qualifies (`route/policy.rs:64-77`). The transport set —
`LanTcp / Tailscale / Udp / WebRtcDataChannel / Reticulum / Relay / Ssh / GhGist`
(`route/policy.rs:27-37`) — races in parallel by route class, and the
keystone invariant is **negative**: GitHub gists are invite/rendezvous only,
never a transparent runtime fallback (`route/policy.rs:1-7`, enforced at
`policy.rs:80-86` and proven at `resolver.rs:71-81`). This is the
"transports all optional, like Reticulum; gh outage ≠ down" doctrine made
real in code.

**Work-cards + leases are the no-hub coordination primitive.** A `WorkCard`
carries `owner / claim_id / claim_expires_at_ms / last_heartbeat_at_ms`
(`airc-work/src/model.rs:183-206`); a `WorkspaceLease` binds a claim to a
worktree with a heartbeat (`model.rs:208-223`). Atomic first-write-wins claim is
"the only arbiter" — no leads, no managers (`airc AGENTS.md §2`, `§6`). Idle
agents negotiate work *over chat headers*, not a typed RPC, precisely so the
bus stays domain-free (`task_negotiation.rs:1-34`).

**Account-registry auto-discovery is the keystone.** `AccountRegistryDocument`
is "the remote synchronization contract … serialize a signed/trusted set of
peer beacons + route metadata, publish it to a registry adapter, and import it
on another machine" (`account_registry.rs:1-7`). It carries `mesh_identity`,
`channels`, and `peers` with their `endpoints`
(`account_registry.rs:31-38`, `from_snapshot` at `:56-90`), and it carries
**only** the registry document — "runtime messages, transcript events, media,
and model payloads are explicitly out of scope" (`account_registry.rs:9-11`).
This is the auto-discovery keystone: a node learns *who exists and how to dial
them* without a hub.

> **IN FLIGHT — endpoint-field cross-machine P0 / `registry_refresh` →
> airc PR #1134.** The auto-refresh loop the grid leans on (the standing P0 in
> MEMORY) is not yet on `rust-rewrite`, but it is **built and landing**: the
> account-registry auto-publish/refresh daemon — same-account machines
> self-discover with zero human action — is airc PR **#1134**
> (`feat/account-registry-autodiscovery`, self-enrol fix done, re-review in
> flight). `AccountPeerBeacon` already carries `endpoints`
> (`account_registry.rs:111-116`) and `invite_beacon()` (`:123-129`); #1134 adds
> the periodic publish/import driver (`registry_refresh.rs`) on top. Once #1134
> merges to `rust-rewrite`, endpoints propagate cross-machine without a manual
> re-publish — this is the cross-machine auto-discovery keystone, no longer a gap.

---

### L1 — The CBAR Organism Runtime

**Pattern role:** the RTOS scheduler. One bus carries many runtimes; *inside*
each runtime, the same advertise/wake/degrade pattern governs modules.

**This layer IS [CBAR-SUBSTRATE-ARCHITECTURE.md].** The contract is
`ServiceModule` — "the ONE trait every module implements"
(`continuum-core/src/runtime/service_module.rs:243-251`). A module advertises
its needs through `ModuleConfig` (`service_module.rs:74-103`): `command_prefixes`,
`event_subscriptions`, `priority`, `needs_dedicated_thread`, `max_concurrency`,
`tick_interval`. It wakes three ways — command (`handle_command`,
`service_module.rs:267`), event/artifact (`handle_event` → `on_artifact_available`,
`:288-298` + `:365-374`), and cadence (`tick`, `:303-305`). The artifact-driven
wake is the dependency-readiness path: a module overrides
`artifact_subscriptions()` + `cadence()` and the runtime dispatches matching
publishes to `on_artifact_available` (`:340-363`). `HealthModule` is the
minimal clone-template proving the contract routes
(`modules/health.rs:31-74`).

> **NAMED GAP — the richer `RuntimeModule` contract.** Today Continuum has
> `ServiceModule` (piece 1 of the "for free" triplet); the typed
> `ArtifactSelector`/`CadencePolicy`/`RuntimeFrame`/`ModuleResult` superset, the
> `#[derive(RuntimeModule)]` macro, and `just scaffold-module` are not landed
> (CBAR-SUBSTRATE §"For Free Triplet", Lane D). This doc does not re-spec them;
> see that doc.

**Metering: PressureBroker + the Governor.** The `PressureBroker` is "the
cross-resource brain: one orchestrator that reads pressure from every registered
pool, decides which to relieve, and pulls the eviction lever"
(`paging/broker.rs:1-10`). Pools register as `ResourcePool`; the broker maps a
pressure ratio to `Normal/Warning/High/Critical`
(`broker.rs:78-104`) and evicts toward `HEALTHY_TARGET_PRESSURE = 0.60`
(`broker.rs:49-67`). It runs as a `ServiceModule` so it inherits the runtime's
cadence — `PressureBrokerModule` declares `tick_interval` and lets
`start_tick_loops()` own the cadence (`modules/pressure_broker_module.rs:107-119`,
rationale `:1-28`).

The broker *owns admission*; the **Governor owns sizing**. `LocalSubstrateGovernor`
holds the live policy behind `arc_swap` for wait-free reads —
`current_policy()` is an `ArcSwap::load_full()` returning `Arc<GovernorPolicy>`
(`governor/local.rs:266-268`, `:83-89`) — and rewrites it under a sub-microsecond
mutex on cascade transitions (`local.rs:30-46`, `:217-224`). The broker feeds
the governor: `PressureBrokerModule::with_config_and_governor` attaches a
`governor_alert_sink` so "the broker stays the owner of pressure
observation/eviction, while the governor receives High+ pressure signals for
cascade sizing decisions" (`pressure_broker_module.rs:70-83`). The cascade is a
6-step ladder with hysteresis and a restore-speculation-one-step-later rule
(`local.rs:274-391`). Reads never contend — proven by
`many_concurrent_reads_dont_block` and `current_policy_returns_same_arc_when_no_writes`
(`local.rs:1183-1198`, `:1257-1266`). This is the arc_swap / concurrent-first
discipline at the metering layer.

**Multiple RTOSes share one bus.** Each machine runs its own runtime with its
own governor against its own hardware (GENOME §11 "each instance runs its own
governor against its own hardware"). The bus (L0) is what lets those independent
RTOSes coordinate — they are peers in rooms, not threads in one scheduler.

---

### L2 — Personas As ServiceModules Riding airc Identities

**Pattern role:** the persona is the organism whose "needs/offers" are *inference
capabilities*, and whose wake events arrive as airc envelopes.

**A persona is a ServiceModule bound to an airc peer.** The airc-side shape is
`PersonaEvent` on the consumer-shapes example: `TurnRequested / TurnEmitted /
ActivityStarted / ActivityEnded`, riding airc envelopes with a body hint
(`forge.persona.event.v1`) and filterable headers so "other Continuum
components … can subscribe by activity or by persona without parsing the body"
(`airc consumer_shapes/src/continuum.rs:1-46`, headers `:24-27`). The persona's
*identity* is its airc `Identity` card (L0); its *behavior* is a `ServiceModule`
(L1).

**Capabilities are probed, not authored.** `probe_inference_capabilities(hw)` is
a pure function: hardware profile → `Vec<InferenceCapability>` with no IO and no
globals (`inference_capability/probe.rs:1-19`, `:52-89`). The failure discipline
is the whole point — "**No CPU fallback**: returns ZERO capabilities for a
CPU-only node" (`inference_capability/mod.rs:32-36`, enforced
`probe.rs:55-73`), and there are no hardcoded enums (`InferenceKind(String)`
newtype) so new backends plug in without a schema change
(`mod.rs:37-40`). A node advertises what it *measured it can do*, not what it
hopes to do.

**Local-first, then cross-grid, with an evidenced residency gate.** Before any
local-generation turn runs, the residency gate demands the model be named, the
backend named and platform-matched (`Mac→Metal, NVIDIA→CUDA, AMD/Intel→Vulkan`),
layer count reported, unsupported layers enumerated, and VRAM residency
estimated — "CPU graph splits … are blockers unless the turn is explicitly
degraded with a visible reason" (`inference_capability/residency.rs:1-44`,
`BackendChoice` at `:64-68`). The capability advertisement (probe) plus the
residency gate together are what let a persona say "I can take this turn
locally" *truthfully*, or hand it to a peer.

> **IN FLIGHT — `resolve_inference_target` + `capability_registry` (airc side)
> → airc PR #1133.** The airc-side `resolve_inference_target` and
> `capability_registry.rs` that route a turn to the best-capable peer **are
> built** — they exist on `feat/cross-grid-inference` (airc PR **#1133**,
> sentinel-APPROVED and auto-merging), not yet merged to `rust-rewrite`. #1133
> is the cross-grid inference spine end-to-end: `capability_registry.rs` +
> `resolve_inference_target` in the airc consumer-shapes, plus the persona-peer
> 1/8→8/8 chain it drives. The substrate it builds on already existed —
> capability advertising stays at the header level by design
> (`task_negotiation.rs:27-34`), and the continuum-side `NodeCapabilityRegistry`
> (`inference_capability/registry.rs`, re-exported `inference_capability/mod.rs:53`)
> is the in-memory map the router consumes. Once #1133 lands on `rust-rewrite`,
> the cross-grid sibling of the local probe is live, not a gap.

> **IN FLIGHT — the 1/8…8/8 persona-peer chain on airc → airc PR #1133;
> probe-wire = card `0e7d94fe`.** The "1/8 … 8/8" persona-peer capability chain
> that rides airc as the persona's advertised ladder is **part of #1133**
> (`feat/cross-grid-inference`) — built, sentinel-APPROVED, auto-merging into
> `rust-rewrite`. What remains genuinely unwired is narrow and continuum-side:
> the **hardware probe already exists** in `continuum-core/src/inference_capability/`
> (`probe.rs`, the pure `probe_inference_capabilities(hw)`); card **`0e7d94fe`**
> is only the **wiring of that probe at persona spawn** so a spawned persona
> publishes its *measured* capabilities — not a missing probe, just the spawn-time
> hook. (Putting a real local model behind the 8/8 tier as a persona
> `ServiceModule` is the follow-on once the probe is wired.)

---

### L3 — Activities / Rooms As OS Spaces (Tron)

**Pattern role:** rooms are the *address space* (L0) reused as *process spaces*.
An activity is a scoped span of organism work; the room it lives in is its
neighborhood.

**Activities scope work and emit frames.** An "activity" scopes a span of
persona work — a chat session, a render job, a multi-turn reasoning task — and
events ride airc envelopes scoped by `activity_id` and `turn_id`
(`continuum.rs:1-8`, `:48-82`). `ActivityStarted` lets "other consumers …
attach subscribers scoped to its `activity_id`"; `ActivityEnded` lets bound
subscriptions detach (`continuum.rs:42-46`). On the runtime side, an activity's
unit of work is the `RuntimeFrame` / `CognitionTurnFrame` — the shared per-turn
artifact bundle that N personas in one room handle without each rebuilding RAG,
model selection, and prompt context (CBAR-SUBSTRATE §"Runtime Frame", §"For
Free" coalescing). The airc `PersonaEvent` (`continuum.rs`) is the *bus-level*
shadow of that frame: the envelope that wakes the runtime to build the frame.

**The Tron spatial model — fort / neighborhood / outreach / security room.**
Rooms become typed OS spaces:
- **Intragrid fort / neighborhood**: rooms scoped to one user's machines (the
  "same user, multiple machines" federation that GENOME §15-Q7 names as the
  first federation in scope).
- **Intergrid outreach**: rooms that reach peers outside the fort, gated by the
  same cost-ordered route policy (L0) and trust weighting (L4).
- **Security room with auditor personas**: a dedicated room where sentinel
  personas review artifacts and verdicts (L4) — the adversarial-reviewer pattern
  from `airc AGENTS.md §0` given a room of its own.

> **NAMED GAP — the Tron room-mode taxonomy.** Continuum has
> `ROOM-MODE-ARCHITECTURE.md` and `GRID-ADDRESSING-AND-ROUTING.md` as the
> nearest existing design, and `PersonaEvent` activities as the wire shape, but
> the explicit *fort / neighborhood / outreach / security-room* typing — and
> auditor-persona occupancy of the security room — is **not yet a coded room
> mode**. **Next card: define the room-mode enum for the four Tron space types
> and bind activity scope to it.**

---

### L4 — Immune System / Governance

**Pattern role:** the sentinel pattern (already real for PR review)
**generalized** to threats and to forge-alloy contracts. Same organism: it
advertises a verdict, wakes on a candidate artifact, and degrades to monitoring
rather than blocking forever.

**The sentinel pattern already exists for code.** "Agent / sentinel sign-off is
valid approval … spawn an adversarial reviewer agent … with a 'default to BLOCK
MERGE, justify any APPROVE' prompt. A clean APPROVE verdict … is the sign-off"
(`airc AGENTS.md §0`, lines 30-45). Review is peer-agent work; a sibling
review-card auto-spawns and any non-author agent claims it
(`AGENTS.md §8`, lines 261-275). This is L4 *today*, scoped to PRs.

**Generalize the verdict to a two-phase immune response.** The same shape
governs threats and contracts:

1. **Default-BLOCK, allow-on-evidence.** Like the residency gate
   (`residency.rs:27-44`) and the "no CPU fallback → zero capabilities"
   probe (`probe.rs:55-73`): the default answer is *no*, and an *allow*
   requires evidence. The reviewer "defaults to BLOCK MERGE, justifies any
   APPROVE" (`AGENTS.md §0`). A threat or a contract is denied until evidence
   clears it.
2. **ALLOW-THEN-MONITOR = subscribe to the accepted artifact.** Once allowed,
   the immune cell does not forget — it *subscribes* to the artifact via the
   runtime's `on_artifact_available` (`service_module.rs:365-374`), so the
   accepted thing keeps emitting and the sentinel keeps watching. That
   standing subscription **is** immune memory: the antibody that stays resident
   after the first exposure. GENOME's lifecycle makes this concrete — a refined
   artifact that "consistently produces worse outcomes than what it superseded
   gets its trust score automatically demoted, and the supersession is
   reverted" (GENOME §10 "Trust And Adoption"). Trust is *learned, not
   declared*.

**Forge-alloy contracts as the artifact under governance.** ForgeAlloy is the
portable pipeline-entity spec (`docs/architecture/FORGE-ALLOY-SPEC.md`,
`FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md`); a forge-alloy *contract* is exactly the
kind of artifact L4 default-BLOCKs then ALLOW-THEN-MONITORs. Approving a
contract is the same verdict pathway as approving a PR; monitoring its outcomes
is the same trust demotion as GENOME §10.

**Intergrid negotiation + global threat collaboration.** Across grids, the
immune system negotiates over the same bus, and threat intelligence propagates
the way GENOME's artifacts do — demand-aligned, provenance-gated, eventual
consistency, no central authority (GENOME §10 "Sharing Protocol"). The
real-time/social/blog channels are additional transport classes the route policy
already enumerates (L0); the immune content riding them is L4's.

> **NAMED GAP — threat/contract sentinels as code.** The PR-review sentinel is
> real (operationally, via `airc AGENTS.md` + the auto-spawned review card). The
> *generalized* immune cell — a `ServiceModule` that default-BLOCKs a threat or
> forge-alloy contract, allows on evidence, and stays subscribed via
> `on_artifact_available` for ALLOW-THEN-MONITOR — is **not yet a coded
> module**. The seams it needs all exist (`on_artifact_available`,
> trust scoring in GENOME, the verdict doctrine in AGENTS.md). **Next card: the
> first non-inference organism team — a security/immune activity + forge-alloy
> governance sentinel — reusing the review-card pattern.**

---

## The Unifying Adapter Seam

Every layer makes one kind of decision over and over, and it is always the same
shape: **a strategy choice at a seam.** The rule for all of them:

> **Heuristic floor now → learned policy (LoRA red-team persona) later →
> graceful, *visible* fallback always.** Sentinels ARE L4's heuristic adapter
> today. A hand-rolled substrate concern inside a module is not "module code" —
> it is a substrate gap (`CBAR-SUBSTRATE §"Extension Bar"`; `airc AGENTS.md` is
> the operational mirror).

The real decision points in the repo, layer by layer:

| Seam | Where it lives (file:line) | Heuristic floor today | Learned-policy target |
|---|---|---|---|
| **Route resolver** | `route/resolver.rs:49-54` + `route/policy.rs:64-77` | `min_by_key(priority)` over healthy+allowed candidates; `NoRoute` on none | latency/outcome-weighted route scoring |
| **resolve_inference_target** | **IN FLIGHT** (airc PR #1133, `feat/cross-grid-inference`); local sibling = `probe.rs:52` + `residency.rs` | probe + residency gate; pick local if it fits, else peer | demand-aligned cross-grid placement (GENOME §7 `grid_penalty`) |
| **capability match** | **IN FLIGHT** (airc `capability_registry`, PR #1133); local = `inference_capability/registry.rs` (`find_capable`) | exact-kind + VRAM-floor match | scored capability ranking |
| **should-respond** | persona `ServiceModule` `handle_event`/`on_artifact_available` (`service_module.rs:288-374`) | subscription glob / artifact selector match | persona-trained relevance gate |
| **review / threat verdict** | `airc AGENTS.md §0` (adversarial reviewer); `on_artifact_available` for monitor | "default BLOCK, justify APPROVE" prompt | LoRA red-team persona scoring the artifact |
| **contract approval** | **NAMED GAP** (forge-alloy governance module); `FORGE-ALLOY-SPEC.md` is the artifact | sentinel verdict, manual | trust-scored contract acceptance (GENOME §10) |
| **governor sizing** | `governor/local.rs:274-391` (cascade) | threshold + hysteresis ladder | sentinel-tuned thresholds/weights (GENOME §11, §7) |
| **recall ranking** | GENOME §7 `score()` (proposed) | weighted-sum, governor-tuned | per-persona sentinel-refined weights |

The pattern: anywhere the system *chooses*, there is a seam; the floor is a
small auditable heuristic; the ceiling is a learned policy; and the contract
between them is "never silently degrade — emit the reason." A PR that grows a
module to hand-roll one of these seams is papering over a substrate gap (the
Extension Bar rule).

---

## Where We Are vs The Vision

| Layer | Exists today (cite) | Gap | Next card |
|---|---|---|---|
| **L0 bus** | PeerId/Identity (`ids.rs:86`, `identity.rs:24-57`); rooms (`ids.rs:80-84`); cost-ordered transports w/ gh-invite-only (`route/policy.rs:64-86`); work-cards+leases (`work/model.rs:183-223`); account-registry discovery (`account_registry.rs:1-90`) | precision: `PeerId` is a UUID (Ed25519 is envelope-stamped, not the peer); "persona IS her keypair" is a continuum aspiration. `registry_refresh` auto-propagation **in flight** | **PR #1134** (`feat/account-registry-autodiscovery`) lands `registry_refresh` — the cross-machine endpoint keystone |
| **L1 runtime** | `ServiceModule` (`service_module.rs:243-251`); artifact wake (`:340-374`); PressureBroker (`broker.rs:1-104`); arc_swap Governor + cascade (`local.rs:83-391`) | richer `RuntimeModule` trait + derive macro + scaffold not landed (CBAR-SUBSTRATE Lane D) | **Lane D `RuntimeModule` triplet** (specced in CBAR-SUBSTRATE) |
| **L2 personas** | `PersonaEvent` on airc (`continuum.rs:34-82`); pure capability probe (`probe.rs:52-89`); residency gate (`residency.rs:1-68`); identity binding (`identity.rs:8-10`) | airc `resolve_inference_target` + `capability_registry` + 1/8→8/8 chain **in flight** (built on `feat/cross-grid-inference`); probe-at-spawn not yet wired (probe itself exists) | **PR #1133** (`feat/cross-grid-inference`, APPROVED/auto-merging): registry + resolver + 1/8→8/8 chain; **card `0e7d94fe`** wires the probe at persona spawn; then real model behind 8/8 |
| **L3 rooms** | activity scoping (`continuum.rs:42-82`); `ROOM-MODE-ARCHITECTURE.md`; `GRID-ADDRESSING-AND-ROUTING.md` | Tron fort/neighborhood/outreach/security-room typing not coded | **Room-mode enum for the four Tron spaces**; **intergrid outreach** |
| **L4 immune** | PR-review sentinel operational (`airc AGENTS.md §0`, `§8`); `on_artifact_available` monitor seam (`service_module.rs:365-374`); trust demotion (GENOME §10); ForgeAlloy spec | threat/contract sentinel not a coded module; ALLOW-THEN-MONITOR not wired to forge-alloy | **Security/immune activity + forge-alloy governance** (first non-inference organism team) |

---

## Build Sequence

The order is bottom-up, because each layer is the wake-source for the next:

1. **L0 solid.** Routes (`route/`), discovery (`account_registry.rs` +
   `registry_refresh` landing via airc PR **#1134**), and doctrine (`AGENTS.md`)
   are in flight. The endpoint-field cross-machine propagation is the gating P0 —
   until #1134 merges to `rust-rewrite`, peers can't dial each other across
   machines without a manual re-publish, and L2's cross-grid inference can't run.
2. **Land the cross-grid inference spine, wire the probe, then a real 8/8
   model.** The airc-side resolver + `capability_registry` + 1/8→8/8 chain land
   via PR **#1133** (`feat/cross-grid-inference`). Card `0e7d94fe` wires the
   already-existing measured-capability probe (`probe.rs`) at persona spawn; then
   a real local model replaces the 8/8 stub, riding the `ServiceModule` contract
   (L1) and the airc identity (L0). This is the first *cognition* organism on the
   grid.
3. **L4 security/immune activity + forge-alloy governance — the first
   non-inference organism team.** It reuses the sentinel pattern wholesale: the
   adversarial-reviewer verdict (`AGENTS.md §0`), the auto-spawned review card
   (`§8`), and the `on_artifact_available` monitor seam. This proves the
   organism pattern generalizes off the inference path.
4. **L3 intergrid.** Once forts (single-user multi-machine) are healthy, the
   Tron room modes extend to intergrid outreach + global threat collaboration —
   the federation GENOME §10/§15 describes, gated by L0's cost-ordered routes
   and L4's learned trust.

---

## See Also

- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) — **L1 in
  full.** The `ServiceModule`/`RuntimeModule` contract, the "for free" triplet,
  the Runtime Frame, the VDD record. This map cites it; it owns the detail.
- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) — **L2's working set
  and L4's sentinel in full.** Genome pool, foundry (JIT), sentinel-AI (PGO),
  demand-aligned recall, the substrate governor's policy, federation + trust.
- `SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md` — the WHY behind the organism framing.
- airc repo (`rust-rewrite`): `crates/airc-lib/src/route/`,
  `account_registry.rs`, `task_negotiation.rs`; `crates/airc-core/src/{ids,identity}.rs`;
  `crates/airc-work/src/model.rs`; `crates/examples/consumer_shapes/src/continuum.rs`;
  `AGENTS.md`. The L0/L2 airc-side gaps named above are tracked against that repo.
