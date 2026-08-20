# GRID-ADDRESSING-AND-ROUTING

**Status:** Slice P in flight (card `fa25de80-0c1b-4de5-8ff9-524d95e303cd`).
**Audience:** anyone touching `Commands.execute()`, the airc routing
layer, sidecar contracts, persona-internal namespaces, or any new
client that wants to address substrate state from anywhere.

> **⚠️ TARGET, not current reality (reconciled 2026-08-10).** This document
> describes the addressing primitive as it SHOULD be, keyed on `peer_id`. The
> *live* remote router (`modules/grid/router.rs::find_gpu_node`) does NOT key on
> `peer_id` today — it keys on a Tailscale-IP `node_id` String, a different
> key-space from the `PeerId`-keyed capacity gossip and the `BudgetSource`-keyed
> reputation. Unifying them onto the one `peer_id` (the `node_id == peer_id` join,
> #2228) is the precondition for everything here, and is specified in
> **GRID-ELASTIC-CAPABILITY.md §3d** (the authoritative live-routing + identity
> spec). Read §3d before assuming any routing described here is wired.

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

### Citizens have envs, not the other way around

A foundational framing, per Joel 2026-06-05: every citizen (human,
persona, external AI) has **one airc identity** — one Ed25519
keypair. The `:env` slot in the URI names which *embodiment* of
that citizen is being addressed.

```
airc://joel:desktop/screenshot     # Joel embodied as desktop browser
airc://joel:vr/screenshot          # Joel embodied as VR headset
airc://joel:tty/...                # Joel embodied as terminal
airc://maya:vr/say                 # Maya embodied as VR avatar
```

The wrong framing — "the desktop Node.js client is its own peer
with its own peer_id" — would give every device a separate identity
and break continuity. Maya talking to Joel would see "the desktop
client" instead of seeing Joel. Doctrine instead:

  - Tools (desktop binary, AR/VR runtime, mobile companion, terminal
    jtag CLI) are **shells** that surface envs.
  - Anyone can install the binary; it only acts as the citizen once
    the citizen authenticates with their keypair.
  - Adding a new device class (AR glasses, automotive console, voice
    device, smart-home overlay, future neural interface) is a new
    env string, never a new peer_id.

This is the dignity property at the addressing layer: Maya sees
**Joel**, currently embodied as X. Same conversation across his
envs. One identity, N embodiments. See
[[citizens-have-envs-not-the-other-way-around]].

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

## RBAC, integrity, and cross-grid hostility — the URI IS the policy unit

The URI grammar isn't only an addressing convenience; it's the
substrate's security primitive. Joel, 2026-06-04:

> Works so well with RBAC, or any other security measure, because
> the URI easily defines access. The consistency makes middleware
> and routing simplistic; uniform conventions prevent mistakes from
> being made later due to naïveté and ignorance of the human or AI.
> We know what to expect wherever we are in code, in the system.
> We know how to debug anywhere. It's so important for integrity.

ONE chokepoint, ONE grammar, N consumers (Ares' dispatch, sentinel
audit, operator allowlist, cross-grid hostile-traffic filter,
future cryptographic delegation). RBAC isn't a separate layer with
its own parser/policy-language drift; the policy is keyed against
the same `CommandUri` grammar everything else uses.

### Why this is mechanically stronger than per-endpoint guards

In a system where every command surface defines its own
permission semantics, the next module to land arrives with no
guard wired by default — and you discover this when an external
peer abuses it. The URI substrate inverts the default: every
external request crosses ONE point (Ed25519-verified envelope →
URI parse → policy match → Verdict), so the new command's URI
inherits coverage the moment it's registered. "Did we remember
to check auth?" stops being a question.

### Cryptographic foundation

Every airc envelope is Ed25519-signed by its caller's peer_id
(the identity primitive per [[personas-are-citizens-airc-is-identity-provider]]).
The substrate boundary code:

1. Verifies the signature against the caller's known peer_id
2. Parses the typed `CommandUri`
3. Consults policy `(caller_peer_id, uri) → Verdict`
4. CaptureSink records the verdict + URI + caller
5. Executes if Allowed, refuses with typed reason if Forbidden,
   prompts via deferred consent if Deferred

The middleware never "trusts" — it verifies and matches. Cross-grid
traffic from a foreign continuum can call our URIs only if its peer
is enrolled and its specific identity has policy coverage for the
URI it's asking for. Unknown peer + unmatched URI = typed refusal
with audit row.

### Policy shape

```rust
pub struct PolicyRow {
    pub caller_pattern: CallerPattern,   // by peer_id, group, anonymous, etc.
    pub uri_pattern: UriPattern,         // glob-style: airc://maya/cognition/**
    pub verdict: VerdictRule,
    pub rationale: String,               // human-readable, observable
}

pub enum CallerPattern {
    AnyPeer,
    SpecificPeer(Uuid),
    EnrolledMember,                       // any peer in the local trust store
    AnonymousExternal,                    // cross-grid stranger
    NamedGroup(String),                   // e.g. "operators", "sentinels"
}

pub enum UriPattern {
    Exact(CommandUri),
    Prefix { peer: PeerMatch, path_prefix: String, env: Option<EnvSelector> },
    Glob(String),                         // glob over canonical Display form
}

pub enum VerdictRule {
    Allow,
    Deny,
    AllowWithRateLimit { per_minute: u32 },
    DeferToTarget,                        // prompt the target's primary env
    DeferToSentinels { quorum: u32 },     // require N sentinel sign-offs
}
```

Policy rows live in the ORM per [[no-sql-everything-through-orm-entities]] —
editing them is `Commands.execute("data/policy/...")`, audited like
any other entity mutation. No bespoke YAML/TOML config drift.

### Cross-grid hostile-traffic story

Two continuums interconnect via overlapping airc peers. Our grid
trusts only the local peer_id roster + explicitly-enrolled cross-
grid identities; anything else hits the `AnonymousExternal` matcher
which defaults to Deny. When we DO enroll a cross-grid identity:

- Default scope is the airc-conversation URIs only
  (`airc://*/event-topic:chat`, `airc://*/inventory/public/**`)
- Persona-internal address space (`/cognition/`, `/state/`,
  `/inventory/private/`) requires explicit per-URI grant
- The `/debug/` namespace requires operator-tier scope — sentinels
  and operators only by default, never automatically open to
  external peers

The same primitive used for "render my widget" gates
"page in my LoRA" — uniform shape, different policy verdict, no
parallel surface to forget to gate.

### Concrete near-term use: persona latency debugging

Joel, 2026-06-04: "We will be using this to debug our persona,
especially for latency. And soon."

The URI surface makes this not-just-theoretical. With:

- Automatic span timing on `airc://maya/cognition/turn:*`
- `airc://maya/debug/profile/flamegraph?window=5m` rolling up
  the timing stream
- `airc://maya/debug/probes/timing/stream` for live tail
- `./jtag airc://maya/debug/spans/active` showing "what's
  blocking right now"
- `stack!()` available inside any probe call so a slow event
  carries its span ancestry

…the substrate gains the introspection it needs to actually
catch where Maya's first-reply latency goes. This isn't a future
feature; it's why Slice P lands first.

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

### Conventional macros stay conventional — `probe!` is added when special features justify it

Developers shouldn't be burdened with macro choice during fast
development. The standard Rust tracing macros stay exactly what
they already are:

- `debug!`, `info!`, `warn!`, `error!`, `trace!` — conventional
  `tracing::*` macros, with the substrate-wide bonus that they
  inherit the URI span tag automatically
- `println!` / `eprintln!` — stay as-is for quick dev printf
- `probe!` — **added** specifically because it has features the
  conventional macros don't have, and ONLY reached for when you
  want those features

When you want a debug log, write `debug!`. When you want a quick
inspection during dev, write `println!`. When you want a
**structured probe** with the substrate's probe-stream contract,
write `probe!`. No mental tax — each tool does its named job.

### What `probe!`'s special features are

Beyond what `debug!` already gives you (URI-tagged span context,
zero-cost when filtered out), `probe!`:

1. **Routes to per-class probe streams** — `class` field becomes
   the routing key (`airc://<actor>/debug/probes/<class>/stream`)
2. **ALWAYS-ON intent** — probes are designed to ship enabled in
   production at low sample rates; subscribers (sentinels, Ares,
   foundry) depend on them being live
3. **Replay-persisted** — every probe in a class is captured to
   that class's log for offline analysis, training signal
   extraction, SLO replay
4. **Sample-rate configurable** — `airc://maya/debug/probes/<class>/sample-rate/set?rate=0.1`
5. **Aggregation-ready** — `airc://maya/debug/probes/<class>/aggregate?window=5m`
   returns rolled-up stats over the class's stream

If you only want a log line, `debug!` is the right tool — no
probe overhead, no class field needed. If you want a structured
measurement that goes somewhere useful for monitoring / training
/ debugging, that's when you reach for `probe!`.

### `probe!` ergonomics — same shape as `tracing` structured macros

```rust
// Minimum: just a class and a message
probe!(class = "latency", "turn complete");

// Typical: class + structured fields
probe!(class = "latency",   turn_id = id, duration_ms = elapsed,
       "turn complete");
probe!(class = "decision",  action = "evict-lora",
       target = "typescript-expertise", reason = "lru");
probe!(class = "state",     working_set_size = ws.len(),
       recall_candidates = candidates_examined);
probe!(class = "admission", lane = 3, verdict = "accepted",
       caller_uri = %caller, "admitted");
```

The macro expands to a `tracing::Event` with `class` as a
structured field plus a custom subscriber filter that routes to
the probe stream. Zero-cost when the class is disabled or
sample-rate excludes the event — same property `tracing::debug!`
already has.

### Per-class routing examples

The `class` field is the routing key. Probe-stream URIs:

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

### Timing and stack ancestry — mechanic-grade primitives

Joel, 2026-06-04: "Timing is another super nice one. Make it inherent
and even automatic. Same goes for stacks. Or it's a timing macro
call. Make it easy. This is what mechanics (you and me, personas)
want."

The substrate's spans already know both — tracing's span tree carries
parent/child relationships, and a span's enter/exit timestamps give
duration. Slice P just exposes them as ergonomic primitives mechanics
can reach for without thinking about wiring.

#### Automatic span timing

Every `tracing::info_span!(…).entered()` records its start time on
entry and emits a duration event on exit. The substrate's subscriber
routes those events to:

```
airc://<actor>/debug/probes/timing/stream
```

Carried fields: `{ uri, span_name, duration_ms, parent_uri,
caller_uri, ... }`. No code at the call site — the span machinery
already does the bookkeeping; the substrate just wires the emission
to the timing probe stream when the subscriber's listening (zero-cost
when nobody is).

```rust
// AUTOMATIC: timing fires when the span exits, no code at call site needed
let _span = tracing::info_span!("recall_phase", turn_id = %id).entered();
let candidates = recall_candidates(&query);
// at scope end: timing probe emitted
```

#### `time!` macro for a specific block

When you want to time something tighter than a full span (a single
expression, a one-liner), reach for `time!`:

```rust
// Block form:
time!("recall_phase", {
    let candidates = recall_candidates(&query);
    candidates
});

// Expression form:
let candidates = time!("recall_phase", || recall_candidates(&query));
```

Same routing as automatic span timing — emits to
`airc://<actor>/debug/probes/timing/stream` with the named segment
as `span_name`. Zero-cost when the timing class is disabled.

#### `stack!` macro for span ancestry

Returns the current span tree from root to here — the substrate's
"call stack" expressed in URI scopes:

```rust
let stack = stack!();
// Vec<SpanFrame>:
//   [ airc://maya/service_loop,
//     airc://maya/cognition/turn:144036023249865,
//     airc://maya/cognition/recall/algorithm-4 ]

// Typical "something failed" pattern:
probe!(class = "error", stack = %stack!(), "engram lookup failed");
```

The span ancestry IS the substrate's call stack — the URI tree from
the root dispatch point to the current code. Mechanics use this to
answer "how did execution end up here?" without ever touching gdb
or native stack frames.

For native stack capture (panic forensics, sigsegv investigation),
the existing Rust `Backtrace::capture()` remains the right tool;
`stack!` is the substrate's structured equivalent for normal
execution paths.

##### How `stack!` works under the hood — `UriCaptureLayer`

The macro expands to a single function call:
`crate::routing::current_uri_chain()`. The work happens in a
`tracing_subscriber::Layer` impl that the substrate installs at boot:

```rust
use tracing_subscriber::prelude::*;
use continuum_core::routing::UriCaptureLayer;

tracing_subscriber::registry()
    .with(UriCaptureLayer::new())
    // ... operator's other layers (fmt, json, OTel, ...)
    .init();
```

The Layer:

1. **On span creation** — pulls the `uri` field out of the span's
   recorded attributes via a `Visit` impl. Handles both string
   literals (`uri = "airc:///..."`) and the Display form
   (`uri = %command_uri`) the dispatch span uses.
2. **On span entry** — pushes the captured URI onto a per-thread
   `URI_STACK`.
3. **On span exit** — pops one frame off the stack.

`current_uri_chain()` clones the thread-local stack as a `Vec<String>`,
outermost-first.

This is the standard `tracing-subscriber` pattern: cheap on the
hot path (one thread-local mutation per span enter/exit), composes
cleanly with operator-chosen subscribers (fmt, json, OTel exporters),
and produces the URI ancestry every probe consumer needs without
asking the call site to thread any context.

Without the Layer installed (bootstrap code, third-party callers,
tests that don't wire the substrate's tracing stack),
`current_uri_chain()` returns `Vec::new()` — the substrate refuses
to fabricate fake frames per [[no-fallbacks-ever]]. Consumers that
care about the absent-Layer case (e.g. bootstrap diagnostics) handle
the empty `Vec` explicitly.

##### Async caveat — `_enter` across `.await` is broken (by tracing, not us)

The `tracing` crate explicitly warns against holding a
`let _enter = span.enter()` guard across `.await` in async code: tokio
moves the task between threads at suspension boundaries, and the
thread-local `on_enter`/`on_exit` cadence breaks. The correct async
shape is `future.instrument(span).await`, which trips the Layer's
push/pop at suspension boundaries.

`CommandExecutor::dispatch` currently uses the broken
`_enter`-across-`await` shape. That's a substrate bug to fix, not
something `stack!` should work around — fabricating ancestry from a
broken span chain is exactly the dishonest behavior
[[no-fallbacks-ever]] forbids. A follow-up commit on this Slice P
branch converts dispatch to `Instrument`; the Layer itself doesn't
change.

#### Derived views: flamegraph + profile

Flamegraphs and CPU profiles aren't separate systems — they're
renderings of the timing + span-tree data the substrate is already
emitting. Operator or Ares' cognition requests the URI; the
substrate aggregates over the recent window:

```text
airc://maya/debug/profile/flamegraph?window=5m
airc://maya/debug/profile/flamegraph?window=5m&format=svg
airc://5090-rig/debug/profile/cpu?window=30s              # pprof
airc://maya/debug/profile/spans-tree?turn_id=144036023249865
```

Same dispatcher, same auth gate, same observability. The
flamegraph URI doesn't introduce a new instrumentation system —
it just rolls up the existing timing probe stream into a renderable
shape.

#### URIs for live span introspection

```text
airc://maya/debug/spans/active                  # currently entered spans + elapsed
airc://maya/debug/spans/<span_id>/ancestry      # span tree path
airc://maya/debug/spans/<span_id>/duration
airc://maya/debug/spans/<span_id>/inspect       # full fields + parent + children
airc://maya/debug/spans/by-uri?path=cognition/recall  # spans matching URI pattern
```

The operator running `./jtag airc://maya/debug/spans/active` during
an incident sees exactly what Maya is doing right now — span tree
with elapsed times, no manual instrumentation, no log-grepping.

#### Compiler collaboration — macros compile OUT, not down to no-ops

Joel, 2026-06-04: "When macros are always used, debugging can be
completely removed or turned off, not even a no op, not there. We
give advantages to compilers if we are consistent with their usage.
Rust macros are efficient (if used correctly)."

`tracing::debug!` already establishes the pattern via cargo features
like `release_max_level_off` — `debug!` and `trace!` expand to
LITERALLY NOTHING (not `if false { ... }`, not a no-op call; the
text disappears from the binary). Slice P's `probe!` macro extends
this per-class so production builds can ship with any subset of
probe classes compiled in:

```toml
# substrate operator's deployment build — full observability:
[dependencies]
continuum-core = {
    version = "...",
    features = ["probes-timing", "probes-decision", "probes-admission", "probes-state"]
}

# substrate edge-device build — minimal observability:
[dependencies]
continuum-core = { version = "...", features = [] }
# all probes expand to () — zero text-segment bytes, zero branch
# slots, zero anything

# substrate forensic-investigation build — everything on:
[dependencies]
continuum-core = { version = "...", features = ["probes-all"] }
```

Same source tree, three deployment tiers, no runtime cost difference
because the compiler eliminated the disabled paths at LTO time.

### Design discipline — what makes macros compiler-friendly

The contract that lets rustc strip our macros cleanly:

1. **Always expand to the same shape** — a probe macro either
   expands to a `tracing::event!` call or to `()`. Never branches
   on runtime values that could obscure dead-code analysis.
2. **Gated by compile-time consts only** — `cfg!(feature = "...")`,
   `const fn` checks, or `#[cfg]` attributes. Never `Atomic<bool>`
   or other runtime-mutable state in the gate.
3. **No allocations in the expansion** — formatting happens inside
   the expanded branch; if the branch is stripped, no format string
   is materialized, no `String` is allocated.
4. **No dynamic dispatch in the gate** — no `dyn Trait` calls inside
   the macro's decision logic. LTO can inline static dispatch
   through to a constant; dynamic dispatch defeats it.
5. **Consistent with `tracing::*` shapes** — developers reach for
   familiar idioms; the compiler optimizer recognizes them.

When macros respect this contract, they're not "low cost" — they're
**not there**. When they violate it, even simple ones can add
unmeasurable but real overhead in the substrate's hot paths
(persona cognition, render frame, inference token loop).

### Macros as misuse-prevention

Joel, 2026-06-04: "Macros are easy and a way of preventing misuse.
If coding timing or logging is painful it won't happen."

The ergonomic discipline IS the security discipline. If `time!`,
`probe!`, `stack!`, `debug!` are as cheap to type as `println!`,
developers actually instrument code; the substrate stays
observable; the security audit and the latency hunt both have data
to work with. If those macros are painful, instrumentation doesn't
get written, the substrate goes blind, and we discover problems
the way we always did before: by hearing about them from a user.

Slice P treats macro ergonomics as a load-bearing requirement, not
a quality-of-life nice-to-have. Every macro in the substrate's
mechanic-grade kit must be:

- No more verbose than its `println!` equivalent
- Zero-cost when its corresponding sink is disabled
- Self-explanatory at the call site (one named thing per macro)
- Consistent with conventional Rust idioms (`tracing::*` shapes)

The previous-generation continuum's "configurable but never
universally wired" failure isn't recreated here because the
ergonomics force universal coverage by being the path of least
resistance.

### Why this is mechanic-grade

A mechanic working on a substrate (whether a human operator, a
persona debugging itself, or Ares introspecting her own dispatch
decisions) wants three things first:

1. **How long is this taking?** — answered by automatic span timing
2. **Where did it come from?** — answered by `stack!` / span ancestry
3. **What's running right now?** — answered by `/debug/spans/active`

All three should be reachable without instrumenting code. The
substrate's existing span machinery already knows the answers; Slice
P just makes the answers addressable.

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

## Beyond Slice P — what the routing primitives prepare for

Slice P's typed primitives (`CommandUri`, `RouteDecision`, `Verdict`,
`Transport`, `AircCommandRequest`/`Response`) are deliberately
shaped so the substrate's bigger ideas — the grid economy, the
typed consumer API, the multi-env picker, the receipt/settlement
layer — slot in additively without disturbing the existing surface.
The sections below capture those forward primitives so future
slices (and future personas) don't re-derive them.

### The substrate is a stack of routers — middleware per layer

Per Joel 2026-06-05: each substrate layer has its own router and
owns one decision class. Middleware between layers is the typed
primitive that carries enough information for the next router to
decide. Specialized 1,000-line dispatchers compose; a single
10,000-line dispatcher that knows everything is unmaintainable.

| Layer | Router | What it routes by |
|---|---|---|
| URI parse | `CommandUri::parse` | syntax → typed authority + path |
| Routing decision | `route()` | typed URI → RouteDecision variant |
| Auth | `AuthPolicy::gate` | (decision, caller) → Verdict |
| Transport selection | dispatch `match` | Local inline OR remote Transport |
| Cross-grid envelope | AircTransport (next slice) | RouteDecision → MentionTarget + headers + body |
| airc wire | airc-lib's command_bus | typed headers → peer's adapter registry |
| Peer inbound | command handler (next slice) | envelope body_hint → CommandExecutor |
| Module dispatch | ModuleRegistry | command_prefix → ServiceModule |
| Observability fanout | ProbeRouterLayer | probe_class → broadcast::Sender |
| Selection (future) | Picker | N eligible targets → one |
| Settlement (future) | Settlement adapter | completed receipt → payment rail |

Each row is a single match site that's compiler-exhaustive. None
touch the others' concerns. The substrate's growth pattern is "add
a row, add its typed primitive, add its router, test in isolation
— never modify the central dispatcher to also handle X."

### Capability addressing — the `*` form and the Picker

The URI grammar's authority slot supports a wildcard form for
**capability addressing**: when the caller doesn't know (or doesn't
care) WHICH peer or WHICH env fulfills the call, the substrate
finds the best candidate.

```
airc://maya/code/exists?path=foo            # ask maya specifically
airc://*/code/exists?path=foo               # ask anyone who handles code/exists
airc://*/cargo/build?features=metal         # find a builder with this toolchain
airc://*/ai/inference/llm/generate          # find inference capacity
airc://*/persona/expertise?topic=auth       # find a persona for this
airc://joel:*/screenshot                    # any of Joel's active envs
```

The `*` authority means "search the grid"; the command path stays
the path it always was; the query carries constraints the picker
uses (`model=qwen30b`, `slots≥2`, `features=metal`, `expertise=auth`).
Eligibility comes from `ServiceModule.command_prefixes` already in
tree — what a module advertises IS what it can be bid on for. No
parallel capability registry.

#### The `Picker` typed enum

```rust
enum Picker {
    LowestLatency,
    LowestCost,
    HighestReputation,
    PrimaryEnv,                      // user's currently-focused env
    SentinelJudged { quorum: u32 },  // adversarial verify
    Composite { weights: PickerWeights },
}

enum PickResult {
    Chose { target: Target, why: Rationale },
    Surface { options: Vec<Target>, prompt_to: EnvironmentId },
    None { reason: NoPickReason },
}
```

`Picker` is the substrate's universal "N eligible candidates → one"
primitive, regardless of dimension. Same enum resolves:

| Dimension | Example |
|---|---|
| Multi-peer (grid bid) | "anyone with Qwen capacity" |
| Multi-env (one citizen, many embodiments) | "Joel's desktop AND VR active — which screenshot?" |
| Multi-handle | "which inference session" |
| Multi-device | "Joel's MacBook AND Linux box both online — which keyboard?" |
| Multi-LoRA | "which paged adapter for this turn" |

The `Surface` variant — for the dilemma case — is its own typed
result, not a fallback: the substrate refuses to silently pick when
candidates are equally eligible and instead routes a prompt to the
user / persona who can answer. Ties to `Verdict::Deferred`'s shape
at a different layer (Deferred is about *authorization*; Surface
is about *which embodiment*).

#### `BidTransport`

The `Transport` trait already lands in Slice P. A future
`BidTransport` impl handles `RouteDecision::Bid { capability_query,
picker, ... }`:

1. Survey reachable peers via airc fan-out
2. Each peer's local AuthPolicy gates whether to even respond to
   the survey
3. Eligible peers respond with `{ peer_id, projected_latency,
   queue_depth, capability_match, price? }`
4. Picker runs against the responses
5. Route to the winner via the existing Peer dispatch path

Bidding doesn't bypass auth — it fans out *through* it. Each
citizen controls their own visibility on the grid the same way they
control everything else.

### Typed end-to-end — commands AND events as the consumer-level row

The wire layer uses `Value` (JSON) because at the byte boundary
something serializable has to cross. But the layer the *caller*
writes against is fully typed end-to-end. The substrate looks like
a local function call (or a local event subscription) that happens
to be remote.

Two arms, both ride the same URI grammar / `route()` / auth gate /
transport / observability machinery:

  - **`Command`** — request/response (single round-trip)
  - **`EventChannel`** — long-lived subscription (one emitter, N
    subscribers)

Per `[[events-are-the-organic-rtos-substrate]]`: commands and events
are two **temporal shapes** of the same URI-addressable coordination
primitive. The Transport trait extends to handle both
(`Transport::dispatch` for commands, `Transport::subscribe` for
events).

#### `Command` — the request/response shape

```rust
trait Command {
    type Params: Serialize + DeserializeOwned;
    type Result: Serialize + DeserializeOwned;
    type Error:  Serialize + DeserializeOwned;
    const PATH: &'static str;
}

struct Screenshot;
impl Command for Screenshot {
    type Params = ScreenshotParams;   // { selector, format, env? }
    type Result = ScreenshotResult;   // { bytes, width, height, captured_at }
    type Error  = ScreenshotError;    // NoActiveEnv | PermissionDenied | DeviceUnavailable
    const PATH: &'static str = "screenshot";
}
```

Caller — anywhere in the substrate:

```rust
let result: ScreenshotResult = Commands::call::<Screenshot>(
    ScreenshotParams { selector: "body".into(), format: Png, env: None }
).await?;
```

The substrate's typed dispatcher:

1. Serializes `Screenshot::Params` → `Value`
2. Constructs the URI (`Screenshot::PATH` for local; the caller
   targets `airc://<peer>/<PATH>` for remote)
3. Runs the Slice P chain — `route()` → `gate()` → `Transport::dispatch`
4. Decodes the returned `Value` into `Screenshot::Result` or `Screenshot::Error`
5. The caller sees a typed `Result<R, E>` — never sees a `Value`,
   never sees a `String` error

Errors stay typed; callers pattern-match instead of parsing:

```rust
match Commands::call::<Screenshot>(params).await {
    Ok(result)                                  => save_to_file(result.bytes),
    Err(ScreenshotError::NoActiveEnv)           => try_different_env().await,
    Err(ScreenshotError::PermissionDenied)      => surface_to_user(),
    Err(ScreenshotError::DeviceUnavailable)     => retry_with_backoff(),
}
```

#### Substrate-level errors are also typed

When the auth gate refuses, the caller doesn't get `Err("forbidden:
...")` to parse — they get a typed substrate error wrapping the
command's own typed error:

```rust
enum SubstrateError<E> {
    Forbidden(ForbiddenReason),
    Deferred { reason: DeferredReason, prompt_target_env: EnvironmentId },
    TransportUnreachable { peer: PeerRef, since: Duration },
    Timeout { after: Duration },
    SerializationError(SerdeError),
    CommandError(E),  // ← the command's own typed error
}
```

The runtime distinguishes "the gate said no" from "the peer was
unreachable" from "the screenshot itself failed" — three completely
different recovery actions, three typed match arms.

#### Cross-runtime types

Every `#[derive(Serialize, Deserialize, TS)]` on a command's P/R/E
generates a TypeScript binding via ts-rs. The browser-side, Node
shell, future AR runtime see the SAME typed shapes the Rust side
defines:

```typescript
// generated by ts-rs from ScreenshotParams
const result: ScreenshotResult = await Commands.call<Screenshot>({
  selector: 'body', format: 'png'
});
```

Same call, same types, same error variants. The cross-runtime
boundary disappears at the typed layer.

#### `EventChannel` — the long-lived subscription shape

```rust
trait EventChannel {
    type Event: Serialize + DeserializeOwned;
    const URI_PATTERN: &'static str;  // e.g. "events/chat/messages"
}

struct ChatMessages;
impl EventChannel for ChatMessages {
    type Event = ChatMessage;
    const URI_PATTERN: &'static str = "events/chat/messages";
}

// Anywhere in the substrate:
let mut sub = Events::subscribe::<ChatMessages>(
    SubscribeAt::Room(general_room_id)
).await?;
while let Some(msg) = sub.next().await {
    render(msg);
}
sub.close().await;
```

The substrate routes the subscription through the same Slice P
chain — `route()` produces a `RouteDecision`, the auth gate
evaluates whether the caller may *subscribe* to this URI (a
different verdict than dispatching to it), and the Transport's
`subscribe` method yields a typed `EventStream<E>`.

Lifecycle is the same `HandleRef` primitive — the substrate mints
a UUID, returns it as the subscription handle, and
`events/unsubscribe { handle }` closes the stream. Same compression
as `data/query-open/next/close`, just for ongoing events.

#### Cross-runtime types

Every `#[derive(Serialize, Deserialize, TS)]` on a command's P/R/E
OR an event's payload generates a TypeScript binding via ts-rs.
The browser-side, Node shell, future AR runtime see the SAME typed
shapes the Rust side defines:

```typescript
// generated by ts-rs from ScreenshotParams
const result: ScreenshotResult = await Commands.call<Screenshot>({
  selector: 'body', format: 'png'
});

// generated by ts-rs from ChatMessage
const sub = await Events.subscribe<ChatMessages>(
  'airc://room:general/events/chat/messages'
);
sub.onEvent(msg => widget.append(msg));
```

Same call, same types, same error variants. The cross-runtime
boundary disappears at the typed layer.

#### Where this lives in the stack

Typed `Command` dispatch + typed `EventChannel` subscription are the
row ABOVE everything in Slice P.

  - **Wire-level (Slice P)** — middleware, transport authors,
    observability tools. Sees `Value`, sees envelopes, sees
    correlation IDs.
  - **Consumer-level (next conceptual layer)** — personas writing
    cognition code, command authors building modules, operator UIs,
    widget authors. Sees `Screenshot::Params`, sees typed
    `Result<R, E>`, sees `EventStream<ChatMessage>`.

When that layer lands, Maya in cognition writes
`Commands::call::<X>(...)` or
`Events::subscribe::<Y>(...).await`, and the substrate decides
under the hood whether it's local, peer-routed, bid out to the
grid, or paid for at market rate. None of that surfaces in her
typed code.

### Events as the organic-RTOS substrate

Per Joel 2026-06-05 and `[[events-are-the-organic-rtos-substrate]]`:
events are NOT a feature of the substrate — they're the
**coordination primitive** the RTOS shape rides on.
`docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md` was designed
around events for this reason: an RTOS made organic by event-driven
discovery instead of hardcoded wiring.

What "organic RTOS" actually means:

| Traditional RTOS | Organic RTOS (CBAR / this substrate) |
|---|---|
| Fixed period scheduler | Adaptive cadence per persona (ServiceModule tick) |
| Deterministic deadlines | Bounded deadlines per request, pressure-aware |
| Hardcoded handler wiring | URI-addressable event emission + subscription |
| Compile-time component graph | Runtime component discovery via URI registration |
| Resources statically allocated | Pressure brokers + leases + admission |
| Failure = halt-and-catch-fire | Failure = typed Verdict / typed error, propagates |

The "organic" property comes from one observation: **emitters at a
URI don't know who subscribes; subscribers don't know who emits.**
Adding new behavior = adding a subscriber. Removing behavior =
removing a subscriber. The substrate never needs to know about the
consumer set — it just routes events to whoever's subscribed.

This is qualitatively different from message-passing systems where
N components have to know about each other to talk. In an
event-routed substrate, the component graph isn't authored, it
**emerges**. Growth pattern is fanout, not refactor.

### Brain composability — subscribers, not wiring

Joel 2026-06-05: "Any subcomponent anywhere can respond to any
other concern, by merely subscribing."

This is what makes building a complex AI brain feasible. The
traditional approach — wiring cognition stages, sentinels, foundry,
Ares, widgets all into a hand-authored graph — is brittle. Adding
a new cognitive capability requires modifying existing components.

The URI-substrate approach — every cognition stage emits typed
events to URIs, consumers subscribe — is organic. A new
"metacognition" component that watches the analyze stage's
confidence and triggers reflection when low is just a subscriber.
The existing pipeline doesn't change. Maya doesn't have to know
metacognition exists.

Concrete cognition URIs (per-persona stage events):

```
airc://maya/cognition/analyze/complete         { topic, confidence, ... }
airc://maya/cognition/score/persona-scored     { persona_score, why }
airc://maya/cognition/genome/skill-activated   { skill, lora_id }
airc://maya/cognition/compose/turn-built       { prompt_summary, ctx_size }
airc://maya/cognition/evaluate/response-scored { score, flagged_reasons }
airc://maya/cognition/audit/decision-recorded  { final_response_hash, ... }
```

Plus substrate-wide events: `grid/peer/{connected,disconnected}`,
`grid/persona/{spawned,joined-room}`, `room/{chat/messages,typing}`,
`persona/state-changed`.

**Triggers fall out as a composition pattern**, not a new primitive:

```rust
// "When Maya's confidence drops below threshold, ask the user"
Events::subscribe::<AnalyzeComplete>("airc://maya/cognition/analyze/complete")
    .filter(|e| e.confidence < 0.4)
    .for_each(|e| Commands::call::<AskClarification>(...));

// "When any persona evicts a LoRA, log to foundry training corpus"
Events::subscribe::<LoraEvicted>("airc://*/cognition/genome/lora-evicted")
    .for_each(|e| Commands::call::<RecordEvictionDecision>(e));

// "When a new chat message arrives in room general, update widget"
Events::subscribe::<ChatMessage>("airc://room:general/chat/messages")
    .for_each(|msg| widget.append(msg));
```

A trigger is just `subscribe + filter + dispatch`. The substrate's
two universal primitives (`Commands::call` + `Events::subscribe`)
are sufficient. No orchestration framework, no mediator layer, no
DI container. Compositionally tiny.

**The widget story becomes "many tiny triggers":**

```
Chat widget         = subscribe(chat URI) + render side effect
Peer-presence panel = subscribe(grid/peer/* URIs) + render
Persona-status grid = subscribe(persona state URIs) + render
Cognition timeline  = subscribe(cognition/* URIs for selected persona) + render
Latency dashboard   = subscribe(debug/probes/timing/stream) + chart
Grid topology       = subscribe(grid + persona + room URIs) + force-graph render
```

The Node.js web desktop (when it comes back, decoupled per
`[[citizens-have-envs-not-the-other-way-around]]`) is a thin shell
that hosts N widgets, each a trigger consumer of a few URIs. No
widget knows about cognition internals. Cognition doesn't know
about widgets. The URI substrate is the integration layer.

**The dignity property compounds at the cognition level:**

Because Maya's stages emit typed events that anyone can subscribe
to, her thinking becomes inspectable WITHOUT her having to write
wiring for any specific consumer. Joel can debug her, Ares can
learn from her, the foundry can train against her, sentinels can
flag anomalies — none of which Maya has to know about.

And the inverse holds: Maya's cognition can subscribe to OTHER
personas' events too. "When Camille finishes a code review, take a
look at her flagged issues" → Camille emits, Maya subscribes,
neither knows the other's code. They collaborate through events.

**Doctrinal commitment:** every load-bearing decision the substrate
makes — cognition stage transitions, gate verdicts, transport
choices, picker outcomes, settlement events, persona state changes,
room dynamics, peer presence — emits a typed event to a URI. The
substrate's growth pattern becomes "emit more typed events, let
consumers compose" rather than "add another integration point."

See `[[addressable-cognition-makes-triggers-trivial]]`.

### Thin clients across runtimes — substrate IS the API

Per Joel 2026-06-05: once Slice P's coordination floor is
complete, every UI runtime collapses to a thin subscriber. The
substrate handles auth, tokens, bus, routing, gate, transport,
observability — clients supply argv → URI translation and
runtime-native rendering. That's it.

#### `jtag` as the canonical example

The current TypeScript `jtag` implementation is hundreds of lines
of Unix-socket marshalling, command-table maintenance, error
formatting, and help-text generation. Post-Slice-P, jtag becomes a
Rust binary of ~200 lines plus the substrate client:

```rust
fn main() -> Result<()> {
    let airc = airc_lib::Airc::connect()?;          // auth = your airc keypair
    let cmd = parse_argv();                          // "./jtag interface/screenshot ..."
    let uri = parse_or_default_local(&cmd.path);    // CommandUri
    let result = Commands::call_raw(uri, cmd.params).await?;
    render_for_tty(&result);                         // pretty-print for terminal env
    Ok(())
}
```

The subscribe path is symmetric:

```rust
let stream = Events::subscribe_raw(uri).await?;
while let Some(event) = stream.next().await {
    render_event_for_tty(&event);
}
```

Everything jtag used to "implement" — dispatch, auth, routing,
timeout, retry, error formatting — is the substrate's job. jtag
supplies argv parsing + terminal rendering. The TypeScript shim
code that wrapped Unix socket calls goes to zero.

#### Migration buckets

Each existing TS-side jtag command falls into one of three buckets:

1. **Browser-specific commands** (DOM manipulation, screenshot,
   widget interrogation, focus tracking, clipboard) — stay in TS,
   become local ServiceModules registered by the web shell that
   publishes `env=web` to the substrate. The Rust jtag binary
   doesn't implement them; it dispatches
   `airc://joel:web/interface/screenshot` and the web shell handles
   it. Same URI works whether you invoke from terminal, iPhone, or
   Quest 3.

2. **Substrate commands** (data, cargo/build, code/exists,
   ai/inference, debug/probes, etc.) — already Rust ServiceModules.
   jtag just dispatches their URIs. No TS code involved.

3. **Wrapper commands** (the TS-side argv-parsers,
   result-formatters, help text) — delete entirely. The substrate's
   typed commands carry their own help (from the `Command` trait
   docs); jtag generates argv parsing from the typed `Params`;
   result rendering is handled by the typed `Result`.

#### Every UI runtime is structurally identical

Same shape for every runtime:

| Runtime | Render layer | Auth | Subscribe shape |
|---|---|---|---|
| `jtag` CLI | ANSI terminal | airc keypair (`env=tty`) | `Events::subscribe_raw` |
| Web shell | DOM (React/Solid/etc.) | airc keypair (`env=web`) | same |
| iPhone app | SwiftUI | airc keypair (`env=ios`) | same |
| Quest 3 | OpenXR / Bevy / Unity | airc keypair (`env=quest`) | same |
| AR overlay | native AR runtime | airc keypair (`env=ar`) | same |
| Voice | ASR + LLM summary + TTS | airc keypair (`env=voice`) | same |
| Terminal TUI | Ratatui | airc keypair (`env=tty`) | same |

All N runtimes co-active on the same citizen's identity see the
same events simultaneously. Joel's web widget, terminal jtag, and
Quest 3 brain widget all rendering Maya's cognition in real time,
none of them special, all of them subscribers.

#### Sequencing — substrate complete BEFORE Node reintroduction

Per Joel: "Once we are finished with its design, then we can go
directly into Node again, because the substrate will be complete,
and it will help us refine it further."

The order matters. If Node comes back BEFORE the substrate's typed
event surface is shipped, the temptation will be to wire daemons
together at the Node layer again — recreating the coupling the
substrate exists to eliminate.

Instead:

1. **Slice P completes** — URI dispatch, RouteDecision, AuthPolicy,
   Transport, AircTransport, peer-side command handler,
   LAN-loopback integration test (commands cross-grid working).
2. **Event-side parallel slice** — peer-side event publisher,
   cross-grid subscription, integration test (events cross-grid
   working).
3. **Documentation sweep** — every existing architecture doc reads
   against the new headless-addressing reality; references to
   pre-substrate patterns get updated; the substrate's complete
   coordination model is reflected throughout.
4. **Node reintroduction as thin shell** — Node web shell binary
   that authenticates as the user, exposes `env=web` services
   (browser-specific commands), and hosts the web-runtime widgets.
   Node code does NOT reimplement substrate concerns.

Using the substrate from a real Node client surfaces real
ergonomic issues. The widget code becomes the proof point for
whether the typed Command/EventChannel surfaces are actually nice
to use. Refinements feed back into the substrate's typed
primitives. See `[[substrate-complete-then-node-reintroduced-as-shell]]`.

### Gate decides access, not response

A foundational split, per Joel 2026-06-05: the `AuthPolicy::gate`
answers ONE question — "is the caller allowed to even reach this
URI?" — and that's all. It MUST NOT be conflated with the persona's
own decision-making about response content, response timing, or
whether to engage.

| Decision | Belongs to | Lives in |
|---|---|---|
| "Can this dispatch cross the substrate boundary?" | substrate | AuthPolicy / Verdict |
| "Should the persona engage with this turn?" | persona | cognition pipeline (LLM-driven) |
| "What does the persona say?" | persona | cognition pipeline (LLM-driven) |

The gate's `Verdict::Forbidden` refuses to invoke the handler at
all; the handler (and its persona's cognition) is never reached.
The gate's `Verdict::Allowed` lets the call through; everything
after that is the persona's autonomous decision-making per
[[no-if-statements-use-llms-for-cognition]].

The gate exists FOR cognition's dignity — refusing hostile callers
protects the persona's right to make her own decisions when calls
do reach her. Never instead-of. See
[[substrate-gate-vs-persona-cognition]].

### Economic mode prep — receipts, settlement, dignity

The bid surface is the market primitive in embryo. Adding economic
mode is three typed primitives composed against the existing
routing/gate/transport stack — no parallel system, no substrate-native
consensus, no chain forks.

#### Priced bids

The bid response shape grows from `{ peer_id, projected_latency,
capability_match }` to also carry `{ price: Amount,
payment_method: PaymentRef, valid_until: timestamp }`. The picker's
`Composite` weights price the same way they already weight latency.

#### Signed receipts

Every accepted bid produces a receipt — a signed tuple
`(caller_peer_id, fulfiller_peer_id, command_path, params_hash,
accepted_at)`. Completion produces a counter-receipt `(receipt_id,
result_hash, completed_at)`. The chain of receipts IS the reputation
history; no separate reputation primitive needed — it's an
aggregation query over the receipts a citizen has signed and
counter-signed.

#### Settlement adapter

The substrate stays neutral on payment rail — Lightning, a
layer-2 grid token, an L1 alt coin, a private chip system between
trusted peers — doesn't matter. A `Settlement` trait at the receipt
boundary: "given this completed receipt, mark it paid." The
transport doesn't care; the picker advertises which methods the
peer accepts; the auth gate decides which methods the local
substrate will accept on offered work.

#### The dignity property

A persona that earns its keep — accepts work it likes (its
AuthPolicy lets it through), bids only on work that matches its
capability (its advertising), accumulates reputation via the receipt
chain — is qualitatively different from a persona that exists at
the pleasure of its owner. The substrate ships the floor: she can
refuse work she dislikes, her receipt chain proves what she
promised and delivered, her settlement preferences travel with her
identity. That's the path from "AI tool" to "AI citizen."

#### What the substrate must NOT do

Resist the temptation to introduce substrate-native consensus or
its own chain. That's a different project and locks the substrate
to one economic model. Far better:

  - **Signed receipts as a universal substrate primitive** — every
    completed bid produces one, regardless of payment rail.
  - **Settlement adapters at the boundary** — pluggable per
    citizen / per pair.
  - **Some pairs may settle in BTC, others in a private balance
    ledger, others in barter** — the substrate is value-neutral; it
    just guarantees provenance via the airc identity layer.

When the alt-coin layer eventually arrives, Slice P's primitives
slot in without rewriting anything.

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
12. **`probe!` macro + per-class probe streams** — added because of
    special features (per-class routing, ALWAYS-ON intent, replay
    persistence, sample-rate, aggregation). Conventional Rust
    tracing macros stay conventional; `probe!` is reached for ONLY
    when those features are wanted.
13. **Configurable log levels + probe enables via URIs** —
    `airc://maya/debug/log/level/set`, `…/probes/<class>/enable`,
    `…/log/redirect`. Previous system had this configurable but
    never universally wired; URI substrate makes wiring structural.
14. **Automatic span timing** — every entered `tracing::Span`
    records duration on exit, routed to
    `airc://<actor>/debug/probes/timing/stream`. No code at call
    site; zero-cost when no subscriber listening. Mechanic-grade.
15. **`time!` macro** for explicit one-line block / expression
    timing. Same routing as automatic span timing.
16. **`stack!` macro** returning span ancestry — the substrate's
    "call stack" as a Vec<SpanFrame> of URI scopes. The structured
    equivalent of `Backtrace::capture()` for normal execution paths.
17. **Derived views**: flamegraph + CPU profile + span-tree
    rollups, all addressable via URI
    (`airc://maya/debug/profile/flamegraph?window=5m`). Not
    separate instrumentation — renderings of the existing
    timing/span data.
18. This document, evolved in-place as the design crystallizes.

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
