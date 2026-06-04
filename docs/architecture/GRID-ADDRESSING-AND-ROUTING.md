# GRID-ADDRESSING-AND-ROUTING

**Status:** Slice P in flight (card `fa25de80-0c1b-4de5-8ff9-524d95e303cd`).
**Audience:** anyone touching `Commands.execute()`, the airc routing
layer, sidecar contracts, persona-internal namespaces, or any new
client that wants to address substrate state from anywhere.

## Why this document exists

The substrate provides exactly two universal primitives:
**Commands.execute()** + **Events.subscribe()/emit()**. Everything else
is built on these. For those primitives to scale to:

- 14 personas × multiple nodes × multiple grids (Tailscale + Reticulum +
  public mesh)
- N concurrent embodiments per actor (web tab, terminal, VR headset,
  AR overlay, jtag CLI — all live simultaneously)
- Persona-internal substructure (cognition layers, genome, mood,
  in-flight turn) addressable from anywhere
- Ares-the-dispatcher operating on the entire substrate uniformly

…there has to be ONE addressing primitive every consumer reaches
through. If addressing drifts between local-vs-remote, between
operator-vs-persona, between command-vs-event, between
production-vs-debugging, every future feature becomes a special case
and the compression principle dies.

This document defines that primitive.

## Three braided motivations (cf [[continuum-thesis-airc-is-the-medium]])

1. **Universality** — same primitive serves operator inspection,
   Ares allocation, persona-to-persona collaboration, sentinel audit,
   widget rendering, and cross-grid debugging.
2. **Consistency** — "you can take it to any node" (Joel,
   2026-06-04). Migrate Maya from the laptop to the 5090 mid-call;
   the caller's URI doesn't change.
3. **Compression** — one URI grammar, one parser, one dispatcher,
   one auth gate, one observability stream — instead of N
   special-case routing paths that drift.

## The URI grammar

RFC 3986-aligned authority decomposition. Every component is optional
except the path; the substrate fills in defaults from the caller's
context.

```
airc://[peer[@node]][:env]/[path][?query][#fragment]
```

| Component | Required | Meaning |
|---|---|---|
| **scheme** (`airc://`) | optional for bare paths | universal substrate medium per [[continuum-thesis-airc-is-the-medium]] |
| **peer** | optional (empty = local) | persona-name OR Ed25519 peer_id; name resolves via `airc whois` |
| **node** | optional | node-id for peer-at-node disambiguation when same persona seed exists on multiple nodes |
| **env** | optional (default = caller's primary env) | embodiment context: `web`, `tty`, `vr`, `ar`, `cli`, `headless`, custom names, or `*` (broadcast) |
| **path** | required | command-path OR persona-internal substructure (`/cognition/working-set/`) |
| **query** | optional | command parameters |
| **fragment** | optional | handle anchor (resumable streaming) |

### Examples

```text
inference/llm/generate                                    # implicit local (no scheme)
airc:///inference/llm/generate                            # explicit local (empty authority)
airc://maya/inference/llm/generate                        # peer by name
airc://18c04c5b-e059-4129-816f-75e8e58fd74c/inference/...  # peer by UUID
airc://maya@5090-rig/inference/llm/generate               # peer-at-node
airc://maya:web/widget/show                               # specific env
airc://maya:vr/scene/spawn
airc://maya:tty/dashboard/show
airc://maya:*/notification/post                           # broadcast to all Maya's active envs
airc://room:cb2e21a1-999a/render/start                    # room broadcast
airc://room:cb2e21a1:web/render/start                     # room broadcast, web envs only
airc://ares:cli/dispatch/report                           # Ares' CLI env
```

### Persona-internal address space

Every load-bearing substructure has a URI. Joel: "personas cognition
could have address space if we so cared. This is why I like
consistency. You can take it to any node."

```text
airc://maya/                                              # persona herself
airc://maya/cognition/                                    # cognition subsystem
airc://maya/cognition/working-set/                        # L1 active context
airc://maya/cognition/hippocampus/                        # L2 engram store
airc://maya/cognition/hippocampus/engram:e7a3b1.../       # specific engram by id
airc://maya/cognition/genome/                             # L5 LoRA stack
airc://maya/cognition/genome/lora:typescript-expertise/   # specific adapter
airc://maya/cognition/rag/inspect                         # RAG introspection (supersedes `persona/rag-inspect`)
airc://maya/state/mood
airc://maya/state/energy
airc://maya/turn/current                                  # in-flight turn — returns streaming HandleRef
airc://maya/inventory/tools/
airc://maya/inventory/tools/code/search/                  # specific tool
```

The same URI works whether Maya is local or 5090 or migrated. The
dispatcher resolves the peer's current physical location at call
time; the caller sees one stable name.

## The four load-bearing axes

| Axis | Authority component | What it answers |
|---|---|---|
| **Identity** | `peer` | WHO is the target (cryptographic peer_id, name-resolves via whois) |
| **Locality** | `@node` | WHERE the target lives (optional; for peer-at-node disambiguation) |
| **Embodiment** | `:env` | WHICH presentation context (browser, VR, TUI, AR, headless, *) |
| **Substrate** | `/path/…` | WHAT inside the target (command, cognition substructure, state, handle) |

Each axis is independently optional and composes uniformly. A bare
local path (`inference/llm/generate`) is `Local{...}`; a fully-qualified
URI is `Peer{peer, node, env, path, query}`.

## Transport selection is the substrate's job, not the URI's

The URI says WHERE and WHAT. The substrate picks HOW.

```text
URI: airc://maya/inference/llm/generate
       ↓
peer_id resolution: maya → 18c04c5b (via `airc whois` or cached scope)
       ↓
peer locality lookup: is maya in this process? on this node? on the LAN? remote?
       ↓
transport candidates (existing in-tree today: Grid + Tailscale + Reticulum + Unix):
  - same process? → in-memory dispatch
  - same node (different proc)? → Unix-socket IPC via airc daemon
  - same Tailscale subnet (inside)? → Tailscale-direct
  - cross-grid (outside)? → Reticulum / cross-grid airc
       ↓
auth check at substrate boundary (caller peer_id × URI × policy)
       ↓
execute, return HandleRef (wire-stable per [[init-once-handle-then-lease-zero-copy-refs]])
```

Inside/outside (Tailscale subnet vs. public mesh vs. cross-grid) is
**transport selection, not URI variation**. ONE URI, the substrate
picks the optimal path. If the destination peer migrates between
calls, the routing function picks a different transport; the URI
doesn't change.

## Environments (the embodiment dimension)

The TS router has long had a presentation-layer split (browser vs.
server). Slice P generalizes that to N concurrent embodiments per
actor: Joel can have a browser tab, a terminal session, AND a VR
headset all attached simultaneously; Maya can have a web widget, a
3D avatar render, and a CLI subscription all active at once.

### Well-known environment names

| Env | Meaning |
|---|---|
| `web` | Browser DOM / web shell |
| `tty` | Terminal-based interactive UI (menu-based TUI) |
| `cli` | Non-interactive CLI invocation (`./jtag`, scripts) |
| `vr` | VR scene (Bevy, OpenXR, etc.) |
| `ar` | AR overlay |
| `headless` | No presentation; substrate-only consumer (Ares, sentinels, foundry) |
| `*` | Broadcast — every active env subscribed for this actor |

Custom env names are allowed (e.g. `console-osx-menubar`,
`bevy-render-rig`); the registry is open.

### How an env declares itself active

Each Rust client (jtag, future web shell, future TUI, future VR rig)
calls `env/register` on the substrate when it attaches, passing its
`EnvironmentId`. The substrate associates `(actor, env)` and routes
matching URIs to subscribers.

### Default-env precedence

When a URI omits `:env`:

1. If the caller's Context has `environment()`, use it.
2. Otherwise default to `headless`.

When a URI uses `:*`, the substrate broadcasts to every currently
active env for the named actor.

### Cross-actor environment semantics

Each actor has its own env set. Joel's browser tab is not the same
env as Maya's VR scene; they live on different actors. The
`:env` in the URI ALWAYS scopes to the target peer's envs, not the
caller's.

## Auth gate — every URI has one

Per [[constitutional-design-always-a-next-step]] and
[[no-fallbacks-ever]]: every load-bearing operation has an address,
every address has a gate, every gate is observable.

```text
caller emits envelope:
  { caller_peer_id (Ed25519-signed), uri, params, ... }
       ↓
target substrate receives:
       ↓
auth gate: policy(caller_peer_id, uri) → Verdict
       ↓
  Verdict::Allowed       → execute, return HandleRef
  Verdict::Forbidden{r}  → return typed refusal (NEVER silently degrade)
  Verdict::Deferred{r}   → consent prompt to the target's primary env
       ↓
CaptureSink records every verdict per [[observability-is-half-the-architecture]]
```

### Verdict types

```rust
pub enum Verdict {
    Allowed,
    Forbidden { reason: ForbiddenReason },
    Deferred { reason: DeferredReason, prompt_target_env: EnvironmentId },
}

pub enum ForbiddenReason {
    UnknownPeer,
    NoPermissionForUri(String),
    AdmissionDenied(String),   // PressureBroker said no
    Revoked,
}
```

`Deferred` is the "ask the persona first" verdict. When Ares wants
to evict Maya's LoRA, the policy may say "ask Maya's primary env
first" — the dispatcher routes a consent prompt; Maya (or her
operator) approves; the original URI proceeds. Mechanism, not
hardcoded rule.

### Policy storage

Per [[no-sql-everything-through-orm-entities]]: policy lives in
the ORM. Each `(target_peer, uri_pattern, caller_pattern) → Verdict`
row is an entity. Editing policy is a `Commands.execute("data/...")`
call against entities. No bespoke config file.

## HandleRef across the wire

HandleRef already exists in tree per Slice 60 (cell return shapes).
Slice P extends it to survive transport:

```rust
pub struct HandleRef {
    pub uri: CommandUri,        // where to route subsequent ops
    pub handle_id: String,      // opaque substrate-assigned id
    pub expires_at: SystemTime,
    pub stream: Option<StreamMetadata>,  // for streaming handles
}
```

The dispatcher accepts handle URIs:

```text
airc://maya/handle:h_8a3b1c.../poll
airc://maya/handle:h_8a3b1c.../close
airc://maya/handle:h_8a3b1c.../stream
```

Identical to the original-command URI form for the caller; the
dispatcher resolves both as "operation on this handle on Maya's
substrate."

### Streaming handles

Long-running operations (live LLM generation, video frame streams,
RAG capture replay) return streaming HandleRefs. The caller polls
or subscribes via Events.subscribe(`handle:<id>:chunk`) on the
target peer's substrate. Each chunk is a typed event the
environments can render as they choose.

## Universal logging + true JTAG (the next compression layer)

The URI grammar isn't only the addressing primitive — it's the
universal identifier that makes structured logging and live
debugging compose for free. The same `airc://maya/cognition/turn:...`
that addresses an operation IS the span tag in `tracing::Span`.
Joel, 2026-06-04:

> Once you have that you have a universal mapping for logging and
> debugging. It makes universal macros like `debug!` just wire in,
> for segregated logging or a true JTAG in every sense.

### Tracing integration

Every URI dispatch establishes a `tracing::Span` with the URI as
a structured field:

```rust
let span = tracing::info_span!(
    "dispatch",
    uri = %uri,
    caller = %caller_peer_id,
).entered();

// All log macros inside this scope inherit the URI tag:
debug!("admitting incoming");
info!(turn_id = %id, "turn complete");
warn!(error = %e, "engram persistence failed");
```

The `tracing::Subscriber` routes events to URI-segmented log files
OR exposes them as `Events.subscribe()`-able streams. Per-persona
log segregation, per-env filtering, cross-grid trace correlation —
all fall out of one structured field. No special-case "this log
belongs to Maya, that one belongs to Niko" code; the span context
carries it.

### Substrate JTAG (the literal meaning, not the metaphor)

Hardware JTAG (Joint Test Action Group, IEEE 1149.1) gives you
arbitrary-depth structured access to any pin on any chip on a
shared scan chain — halt, step, scan-in, scan-out. The substrate's
`jtag` CLI was named after it; with URI addressing in place, the
namesake's semantics apply literally.

The substrate exposes a `/debug/` namespace under every URI scope:

```text
airc://maya/debug/spans/active                      # what's executing right now
airc://maya/debug/trace/stream                      # live trace event stream (subscribe)
airc://maya/debug/trace/filter?level=debug&path=cognition  # filtered live trace
airc://maya/debug/breakpoint/set?uri=cognition/working-set/upsert
airc://maya/debug/breakpoint/list
airc://maya/debug/handle:h_8a3b.../inspect          # examine a live handle
airc://maya:vr/debug/render-stats                   # VR env frame budget
airc://5090-rig/debug/sidecar/inference/lane:3/dump # inspect stuck inference lane
airc://5090-rig/debug/sidecar/voice/ort-session/dump
airc://room:cb2e21a1/debug/subscribers              # who's listening on this channel
airc://*/debug/heartbeat/last                       # any-actor health check
```

Operator use:

```bash
$ ./jtag airc://maya/debug/trace/stream
[2026-06-04T18:39:18Z INFO  airc://maya/cognition/turn:144036023249865] admitting incoming
[2026-06-04T18:39:18Z DEBUG airc://maya/cognition/recall/algorithm-4] candidates_examined=23
[2026-06-04T18:39:18Z INFO  airc://maya/cognition/turn:144036023249865] turn complete duration_ms=1697

$ ./jtag airc://5090-rig/debug/sidecar/inference/lane:3/dump
{ "lane_id": 3, "active_persona": "niko", "tokens_generated": 142, "stuck_at_token": 143, "last_progress": "5s ago", ... }

$ ./jtag airc://maya/debug/breakpoint/set?uri=cognition/genome/lora-page-in
# Future calls to that URI halt Maya's cognition, surface the call to operator,
# allow inspect / step / continue — same span context Ares sees in her own
# dispatcher cognition, except the operator is the controller this time
```

### Probes — structured measurements as first-class substrate operations

A previous-generation continuum system had a `probe!` macro that
worked nicely but was never universally wired — the same coverage
problem that killed segregated logging. The URI substrate brings
probes back as first-class operations whose coverage is structurally
enforced.

**`debug!` vs `probe!` — the contract distinction:**

- `debug!` emits freeform messages for human-readable trace tail
- `probe!` emits structured measurements for ALWAYS-ON dashboards,
  replay, training signals, SLO breach detection

```rust
// Freeform log (default tracing)
debug!("admitting incoming message lamport={}", lamport);

// Structured probe — routes to airc://<actor>/debug/probes/latency/stream
probe!(latency,  turn_id = id, duration_ms = elapsed);

// Routes to airc://<actor>/debug/probes/decision/stream
probe!(decision, action = "evict-lora",
       target = "typescript-expertise", reason = "lru");

// Routes to airc://<actor>/debug/probes/state/stream
probe!(state,    working_set_size = ws.len(),
       recall_candidates = candidates_examined);

// Routes to airc://<actor>/debug/probes/admission/stream
probe!(admission, lane = 3, verdict = "accepted",
       caller_uri = %caller);
```

The class (first argument) is the routing key. Probe-stream URIs:

```text
airc://maya/debug/probes/latency/stream      # live tail
airc://maya/debug/probes/decision/stream     # decision audit
airc://maya/debug/probes/state/stream        # periodic state snapshots
airc://maya/debug/probes/admission/stream    # PressureBroker verdicts
airc://maya/debug/probes/<class>/replay?from=2026-06-04T00:00Z
airc://maya/debug/probes/<class>/aggregate?window=5m  # rolled-up stats
```

Independent subscribers consume each class:
- **Sentinels** subscribe to `latency` for SLO breach detection
- **Ares** subscribes to `decision` + `admission` as training signal
  for her dispatcher cognition
- **Foundry** fitness-loops subscribe to whichever class its current
  recipe optimizes for
- **Operator** opens `./jtag airc://maya/debug/probes/decision/stream`
  during an incident to watch Maya's reasoning live

All independent, all routed through the same dispatcher.

### Configurable, on/off, per-scope — substrate-native, not config-file-driven

Joel's previous continuum had configurable segregated logging
(per-node, on/off, level), but the configuration system was its own
parallel surface — and inherited the drift problem. With URIs, every
log-control operation IS a command on a URI; same dispatcher, same
auth gate, same observability:

```text
airc://maya/debug/log/level/set?level=debug              # this persona only
airc://maya/debug/log/level/get
airc://5090-rig/debug/log/level/set?level=warn           # whole node
airc://maya:vr/debug/log/level/set?level=trace           # maya's VR env only
airc://maya:tty/debug/log/level/get
airc://maya/debug/probes/latency/enable
airc://maya/debug/probes/latency/disable
airc://maya/debug/probes/<class>/sample-rate/set?rate=0.1
airc://*/debug/log/redirect?sink=file:/tmp/cluster.log   # all actors
airc://*/debug/probes/decision/redirect?sink=ares://training-corpus
```

The previous system's "configurable but never universally wired"
failure mode goes away because there is no separate config surface
— the URI dispatcher IS the wiring. Adding a new module doesn't
require remembering to register a logger; the dispatcher does it
by construction.

### Why this matters at the substrate level

- Same primitive (the URI) used at THREE consumption points: addressing
  (where to dispatch), observability (what to tag), debugging (what to
  poke). No three drifting representations.
- Ares-the-dispatcher consumes the SAME trace stream the operator
  does — when her cognition asks "why did lane 3 stall on the 5090?"
  she queries `airc://5090-rig/debug/sidecar/inference/lane:3/dump`
  with the same URI surface a human would. Cognition + operator
  share the debug primitive.
- Segregated logs aren't a config decision — they're a structural
  property of how `tracing` propagates spans through URI-tagged
  dispatches. Adding a new persona doesn't add a new logger
  registration; it inherits.
- Cross-grid debugging just works: span context propagates with the
  envelope, so a trace started on Joel's laptop carries into the
  5090's substrate when the URI routes there, and the laptop's
  `./jtag .../trace/stream` sees the full causal chain.

### Doctrine alignment for this layer

- [[observability-is-half-the-architecture]] — CaptureSink already
  the substrate-wide convention; URI-tagged spans extend it from
  command-level capture to log-event-level capture without
  bespoke wiring
- [[commands-are-dumb-daemons-are-smart]] — the `/debug/` namespace
  is just commands with the same dispatcher; smart subscribers
  (operator, Ares, sentinels) layer on top
- Joel's compression principle — ONE URI grammar, ONE tagging
  mechanism, ONE dispatcher, N consumers (addressing, logging,
  debugging, audit, replay)

## Composition with existing in-tree primitives

| Existing primitive | How Slice P uses it |
|---|---|
| `Commands.execute()` | Accepts `CommandUri` OR bare path; bare path → `Local` variant |
| `Events.subscribe()/emit()` | Topics become URIs (`event-topic:<topic>` paths); subscribers can filter by env on the substrate's side |
| `airc::discover_*` | Provides peer-id resolution from name (whois) and node locality |
| `GridModule` + Tailscale + Reticulum transports | The transport candidates the router selects from |
| HandleRef (Slice 60) | Wire-stable handle representation; URI is the address-level wrapper |
| PressureBroker | Consulted by the auth gate for admission verdicts |
| CaptureSink | Records every URI dispatch + verdict |
| Context object (Slices 1–4 of #142) | Provides `caller_peer_id`, default env, default authority |

## What lands in Slice P

1. **`CommandUri` typed enum** with exhaustive variants and a
   complete RFC 3986 parser/generator. Round-trips.
2. **`Commands.execute(uri_or_path, params)`** accepts both forms;
   bare path → `CommandUri::Local`. Backwards-compatible.
3. **Routing function** `route(uri) → TransportDispatch` —
   consults peer_id resolution + locality lookup + transport
   candidates (existing Grid + Tailscale + Reticulum).
4. **Auth gate** at the substrate boundary, typed
   `(caller, uri) → Verdict`. Implementation reads policy from
   ORM entities.
5. **HandleRef** marshaling across the wire (peer_id + handle_id
   + expiry). URI-addressable handles
   (`airc://maya/handle:h_X/poll`).
6. **`EnvironmentId` registry** — well-known names + custom +
   open-ended.
7. **`Context::environment()`** accessor — Slices 1–4 of #142 made
   Context the universal actor handle; Slice P extends it with the
   environment dimension. Each Rust client (jtag, web shell, TUI,
   VR rig) reports its env when attaching.
8. **`env/register` + `env/unregister` commands** so actors declare
   their active envs.
9. **`event-topic:` URI scheme path** so subscriptions are
   addressable (`airc://maya/event-topic:turn-complete` etc.).
10. **Tracing-span URI propagation** — every URI dispatch establishes
    a `tracing::Span` with the URI as a structured field. `debug!`,
    `info!`, etc. inherit the tag automatically; per-persona log
    segregation falls out for free.
11. **`/debug/` namespace under every URI scope** —
    `airc://maya/debug/spans/active`, `…/debug/trace/stream`,
    `…/debug/breakpoint/set`. The substrate's `jtag` CLI gets its
    namesake's literal semantics: arbitrary-depth structured access
    to any URI in any persona's address space from any node.
12. **`probe!` macro + per-class probe streams** — structured
    measurements (latency, decision, state, admission) routed to
    `airc://<actor>/debug/probes/<class>/stream`. Always-on,
    low-cost, subscribe-able by sentinels, Ares, foundry, operators.
    Brings back the previous-generation continuum's probe primitive
    with universal coverage enforced by the URI dispatcher.
13. **Configurable log levels + probe enables via URIs** —
    `airc://maya/debug/log/level/set`, `…/probes/<class>/enable`,
    `…/log/redirect`. The previous system had this configurable
    but never universally wired; the URI substrate makes the wiring
    structural.
14. This document, evolved in-place as the design crystallizes.

## What does NOT land in Slice P (explicit non-goals)

- Implementation of every persona-internal namespace path
  (those land per category in B'.X sub-slices; Slice P just
  establishes the URI shape they use)
- Actual VR / AR / TUI clients (each is its own slice, built on
  top of `:env`)
- Ares-the-dispatcher's cognition (her own card, consumes Slice P)
- Auth policy DSL (Slice P establishes the gate point + Verdict
  type; rich policy language is a follow-up)
- Default policy curation (substrate ships empty default policy;
  operator's seed-time config populates baseline)

## Test fixtures (per [[test-fixtures-are-system-primitives]])

Every test that needs to exercise URI dispatch leases ONE of:

- `StubAircCitizen` (existing) — for the airc identity side
- `StubRouter` (new) — synthetic transport selection; tests can
  assert the chosen TransportDispatch for a URI without spinning
  up real transports
- `StubEnvRegistry` (new) — declare active envs for synthetic
  actors

No demo binaries; every behavior locked at the test boundary.

## Open questions (work in flight)

These don't block Slice P landing but need resolution as we
implement:

1. **Name-collision policy across continuums.** Two operators each
   name a persona "Maya." When Joel's continuum talks to a peer
   continuum that has its own "Maya," the dispatcher must
   disambiguate by peer_id. UI: should the caller see
   `airc://maya/...` and the substrate pick the closest match, OR
   should the caller see `airc://maya@joel-grid/...` always when
   cross-grid? Current lean: caller's URI uses local name; cross-
   grid edges add the `@grid` annotation transparently.

2. **HandleRef expiry across migrations.** If Maya migrates from
   laptop to 5090 mid-call, the laptop-issued HandleRef becomes
   stale. Should the substrate transparently re-resolve handles
   across migrations (catch + retry once), or surface a typed
   migration error the caller handles? Current lean: substrate
   re-resolves; surface a captured event for observability.

3. **Env namespace ownership.** Are well-known envs like `web`,
   `vr`, `tty` part of the substrate or operator-extensible? If
   the substrate ships a fixed list, custom env names need an
   escape hatch; if the operator extends, the registry needs an
   anti-collision rule. Current lean: open-ended registry; custom
   envs MUST be prefixed (`x-<vendor>-<name>`) per RFC 3986
   convention for unregistered schemes.

4. **Wildcard semantics for broadcast.** Does `airc://maya:*/...`
   mean "every currently active env at dispatch time" or "every env
   that EVER attaches"? Persistence semantics matter for late-
   attaching subscribers. Current lean: dispatch-time snapshot for
   commands; persistent for events (late subscribers still receive).

5. **Cycle detection in cross-grid routing.** Two grids interconnect
   via overlapping airc peers; URI dispatch could loop. Standard
   TTL + visited-set, but where the bookkeeping lives is open.
   Current lean: dispatcher embeds a hop-counter in the envelope
   header; substrate rejects envelopes with TTL=0.

## Sequencing

- Lands BEFORE A.2.2 (adds `runtime/mode/*` commands; these must
  be grid-addressable from line one — operator on laptop calls
  `airc://5090-rig/runtime/mode/get` to inspect remote substrate
  mode)
- Lands BEFORE B' (every `CategorySidecar` exposes
  `category/<name>/*` commands; uniform addressing across the
  grid is the foundation)
- Can land in parallel with A.2.1 review/merge (independent
  worktree, no overlap)
- Multiple commits expected: this design doc first, then the
  typed enum + parser + dispatcher in incremental commits

## Doctrine alignment

| Doctrine | How Slice P enacts it |
|---|---|
| [[continuum-thesis-airc-is-the-medium]] | airc:// IS the substrate's universal addressing space |
| [[airc-headers-are-the-routing-layer]] | Slice P realizes this at the URI layer; the envelope carries the URI |
| [[commands-are-kernel-level-and-compose]] | local + remote commands share dispatcher logic; composition works across the wire |
| [[host-the-seemingly-impossible]] | 14 personas × N internal substructures × M environments × P nodes, all addressable with ONE URI grammar |
| [[observability-is-half-the-architecture]] | every URI fetch + verdict captured by CaptureSink with zero hot-path cost |
| [[constitutional-design-always-a-next-step]] | every URI has a gate; constitutional layer rides on this |
| [[no-fallbacks-ever]] | URI parse failure / unknown peer / unknown env all produce typed Verdict::Forbidden, never silent substitution |
| [[init-once-handle-then-lease-zero-copy-refs]] | HandleRef is wire-stable; init at the target substrate, lease across the URI |
| [[test-fixtures-are-system-primitives]] | StubRouter + StubEnvRegistry are first-class test primitives |
| Joel's compression principle (this session) | one URI grammar, one parser, one dispatcher, one auth gate, one observability stream |

## References

- docs/architecture/AI-COMMAND-NAMESPACE.md — the `ai/*` command surface this rides on
- docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md — RTOS contract every URI dispatcher inherits
- docs/architecture/OBSERVABILITY-AS-SUBSTRATE.md — CaptureSink pattern the verdict log uses
- docs/architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md — current command system A.2.1 already lives in
- docs/architecture/INFERENCE-LANES-REALISTIC.md — lane-allocation surface that becomes addressable per persona-internal URIs
- docs/architecture/GENOME-FOUNDRY-SENTINEL.md — foundry surface that becomes URI-addressable
- docs/ARES-MASTER-CONTROL.md — the consumer that validates the addressing model (Phase 6+ "URL Routing")
- docs/UNIVERSAL-PRIMITIVES.md — the two primitives Slice P extends
- airc work card `fa25de80-0c1b-4de5-8ff9-524d95e303cd` — Slice P scope of record
