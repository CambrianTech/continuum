# SDK API Surface — the canonical command/event set the SDKs project

> Companion to [CLIENT-SDK-PLATFORM-ARCHITECTURE.md](CLIENT-SDK-PLATFORM-ARCHITECTURE.md)
> (that's *structure*; this is the *contract*). The FFI facade
> (`client/continuum-client-ffi`, PR #1663) is the generic-free pipe; THIS doc
> defines what rides on it — the canonical commands + events every typed SDK
> projects, and how that typed layer is **generated, never hand-written**.

## The two primitives are the entire surface

Everything is the two primitives ([[command-event-decision-rule]]), each
**bidirectional** — Commands = call + serve, Events = emit + subscribe:

```
execute(command: &str, params_json: &str) -> Result<String, FfiError>   // Command: CALL
provide(command: &str, handler: CommandHandler) -> Registration         // Command: SERVE (client-provided)
subscribe(class: &str, callback: EventCallback) -> Subscription          // Event: SUBSCRIBE (Drop = unsubscribe)
emit(class: &str, payload_json: &str) -> Result<(), FfiError>            // Event: EMIT (publish)
```

FOUR facade methods = two primitives × two directions. `#1663` shipped `execute` +
`subscribe`; **`provide` (serve) + `emit` (publish) are the facade gaps** — bind all
four in one uniffi `.udl` pass so the native binding is complete in one shot. The
TS SDK splits them by primitive: `Commands` (`execute` + `provide`), `Events`
(`subscribe` + `emit`) — the CLAUDE.md two-primitives file split.

A typed SDK method is a thin wrapper over `execute`/`subscribe`:

```
// generated, per language — NOT hand-written
chat.send({ room, message }) -> SendResult
   ≡ execute("collaboration/chat/send", JSON(params)) |> parse as SendResult
```

The SDK adds *types + idiomatic shape* (Promise / async-await / Flow / Stream); the
JSON shape is the canonical contract. Zero logic — see the organizing law in the
structure doc.

## Events are the organism's coordination substrate (personas AND systems)

Emissions + subscriptions aren't a feature — they're the **nervous system that makes
the grid an organism.** The same `emit`/`subscribe` primitive coordinates *both*:

- **Personas** — the reactive mind ([[persona-brain-reactive-cognition]]): a turn,
  a recalled memory, a thought-stream all flow as events the cognition loop reacts to.
- **Systems themselves** — a node coming online/offline emits presence
  (`agent_heartbeat` / `active_agents`), a GPU freeing emits capacity, demand
  spiking emits pressure, a capability appearing emits advertisement. Other nodes
  *subscribe and react*.

Because **emitters don't know their subscribers** ([[events-are-the-organic-rtos-substrate]]:
"events ARE the coordination primitive; the component graph emerges"), behavior is
**emergent, not orchestrated**. A machine drops ([[grid-node-resilience]]) → it stops
heartbeating → subscribers re-route, re-lease the compute, heal — nobody wrote a
"handle node failure" procedure; the organism responds because the signals flow and
the graph self-assembles. Demand rises → pressure events → capable nodes lease in.
That is the difference between a distributed system and a living one.

**Every node subscribes AND emits — like cbar.** No privileged broker/hub: each
node (persona, system, client, peer) is a symmetric producer *and* consumer, exactly
like cbar's processing graph where each node emits features and subscribes to others
and the graph self-assembles. That peer-symmetry is what lets the organism have no
single point of orchestration.

And it's **fractal**: this is the *same* emit/subscribe-graph-emerges pattern as a
persona's Faculty/Workspace brain ([[persona-brain-reactive-cognition]] — the cbar
perspective), just at grid scale. Every node is a "faculty" of the organism; the
brain is the organism in miniature, the organism is a brain at scale. One primitive,
two scales (cognition + grid).

This is why the event surface is first-class and deep (source addressing across the
grid, server-side filtering, monotonic sequence, emit as a real primitive not
sugar): it's the organism's signaling, and it must be cheap, ordered, filterable,
and reach across the whole grid. Cross-grid event delivery (`AircEventPublisher`) is
the organism's reflex arc.

## The payoff: cognition tied across the grid

Follow the fractal to its end. A persona's mind is faculties bidding into a bounded
Workspace, integrated by an arbiter (the cbar reactive shape). The grid is nodes
emitting/subscribing. **They are the same emit/subscribe substrate** — so a single
cognition can span the grid:

- a **Recall** faculty on the node that holds the engrams,
- a **Deliberation** faculty leasing the 5090,
- a **Vision** faculty on the headset capturing from the renderer,

— all bidding into **one Workspace** over the cross-grid event substrate; the arbiter
integrates the bids regardless of which node emitted them. A mind whose organs are
grid-distributed. (And the inverse: many minds sharing a faculty — one embedding
facility, one world-model — because a faculty is just an addressable emit/subscribe
node.)

**This is why the format must be elegant.** Tying cognition across machines is only
possible if the primitive is uniform, destination-addressed, serialize-once, and
header-routed — a heavy or bespoke format would make cross-node faculties
impossible to compose. The elegance was never aesthetic; it's the *enabler* of the
ambitious thing ([[optimization-is-always-first]], the compression principle). Get
the format right and grid-distributed cognition is a configuration, not a rewrite.

## Commands are bidirectional: call AND provide (client-provided commands)

A command isn't only something a client *calls* — a client can also *provide*
(serve) one. This is the **serve side of the Command primitive** (still two
primitives: Commands = `execute` + `provide`; Events = `emit` + `subscribe`).

Some commands cannot run in the core — they need the **client's** display, sensors,
or renderer:

| Command | rust-origin contract | per-platform adapter (in the SDK) |
|---------|----------------------|-----------------------------------|
| `interface/screenshot` | name + ts-rs `ScreenshotParams`/`Result` | web = DOM/canvas · desktop = OS · **AR/VR = capture from the renderer** |
| capture / sensors | rust-origin | each platform's native capture |
| `ping` | rust-origin | trivial, but same shape |

The **contract has a rust origin** (canonical name + ts-rs types, one source of
truth); the **adapter that implements it lives in the SDK**, one per platform —
OpenCV-style adapter polymorphism: one command identity, N platform adapters. The
core (or a peer) *routes* the command to a client that provides it; the SDK's
adapter runs and returns the typed result.

This already exists server-side for personas (`persona/command_inbound_pump.rs`
routes inbound commands to a persona's handlers). The SDK is the **client analog**,
so the FFI facade needs a third method beside `execute`/`subscribe`:

```
provide(command: &str, handler: CommandHandler) -> Registration   // Drop = deregister
CommandHandler { handle(params_json) -> Result<result_json, FfiError> }
```

Typed in the SDK off the SAME `CommandMap` (params/result inferred from the name):

```ts
commands.provide('interface/screenshot', async (p) => webCapture(p));      // apps/web
commands.provide('interface/screenshot', async (p) => rendererCapture(p)); // apps/vr
```

(The legacy `src/commands/interface/screenshot/{browser,server}` split is exactly
this today — the browser file IS the web adapter. The new shape generalizes it:
rust-origin contract, per-SDK adapter, routed.)

## Commands that stretch across environments (the hard part)

A command's **caller, contract-origin, and executing adapter can each be in a
different environment**: a CLI calls `screenshot` → the core routes it → a web
client's adapter runs it → the result flows back; cross-grid, a persona on one node
targets a human's client on another. The SDK must express *where* a command runs
without losing transparency.

It does **not** reinvent routing — the **redone command/event infrastructure**
already solves addressing (`core/src/routing/`): `CommandUri` /`RouteDecision` over
`airc://[peer[@node]][:env]/path`. The SDK just **projects** onto it:

| CommandUri | SDK `target` | meaning |
|------------|--------------|---------|
| `Local` | omitted (bare path) | caller's own substrate (default, back-compatible) |
| `Peer { peer, node?, env? }` | `{ peer, node?, env? }` | a citizen; **`env`** = WHICH embodiment |
| `Room { env? }` | `{ room, env? }` | fan-out to subscribers |
| `Broadcast` / `:*` | `{ peer, env: '*' }` | every embodiment of a peer |

**`env` is the cross-environment key** (`EnvironmentId::Named("web" | "vr" | "server"
| "cli" | …)`). It's how a client-provided command reaches the right embodiment:

```ts
commands.execute('interface/screenshot', { querySelector: 'body' },
                 { peer: joelPeerId, env: 'web' });   // → airc://<peer>:web/interface/screenshot
commands.execute('interface/capture', {}, { peer: headsetPeerId, env: 'vr' }); // renderer capture
```

The SDK builds the `airc://` URI (`buildCommandUri`) and `RouteDecision` does the
rest — local walk, airc-to-peer, or room fan-out — over the SAME wire family
(local IPC ↔ cross-grid airc). Caller stays transparent; addressing is opt-in.
Events ride the redone `AircEventPublisher` cross-grid pub/sub the same way.

## Handles are addressable resources (URIs) — long-running, streaming, multi-hop

Short commands are request/response. Long-running / streaming / stateful work uses
the **handle pattern** (the substrate's establish-once-reuse-many — `InferenceHandleStore`,
AI-COMMAND-NAMESPACE.md §2): an `open`-style command returns a **handle**, events
stream against it, and further commands (`write`, `read`, `close`) take it.

The unifying move (Joel): **a handle IS a URI.** `open` returns the handle's
`airc://` URI — in the result body, or an **airc header** (the HTTP `Location`
analog: "the resource you created lives here"). Then *everything routes to that
URI*:

- further commands → `airc://<handle-uri>/write` (routes to wherever the resource
  lives — a peer, an `:vr` env, N hops away);
- event subscription → `airc://<handle-uri>/events/<class>` (the resource's stream).

So in the long **router-chain** scenario, the handle's event stream is fed by **any
link in the chain** — each hop emits handle-correlated events; the originator's
subscription receives the live stream from across the grid (the web-like ping's
hops *arrive as events*, not one return). Handle = an addressable resource;
commands operate on it, events flow from it, routing places it. Commands + events +
handles + routing collapse into ONE addressing scheme.

```ts
const f = await commands.open('file/open', { path }, { peer, env: 'web' }); // → Handle (carries its URI)
f.on('progress', (e) => …);                 // events from the resource (any hop)
await f.execute('write', { bytes });        // routed to the handle's URI
await f.close();
```

### Worked example: a grid-spanning WebRTC connection (the hardest case)

WebRTC is where every part of the model earns its keep — a long-running, stateful,
bidirectionally-streaming resource whose media server is "somewhere in the grid."
It's not invented for this: the substrate already has the **Universal Handle System**
(`live/handle.rs` — "start operation → returns handle; events tagged with handle;
cancel/status/resume → use handle") and `AgentHandle` in the live transport. The SDK
just makes that handle **addressable (a URI)** so it routes across the grid.

```ts
// open the connection → a handle addressing the media server WHEREVER it lives
// (a livekit-bridge node somewhere on the grid; routed by URI, you don't pick the node)
const conn = handleFrom((await commands.execute('live/connect', { room },
                          { /* let routing place it on a media-capable node */ })).uri, transport);

// signaling rides the handle's bidirectional event stream — offer/answer + ICE
// trickle = a STREAM of candidate events, fed by any link (your "events from any link")
conn.on('signal', (s) => applySignal(s));            // answer + remote ICE candidates
await conn.execute('offer', { sdp });                // local offer
await conn.execute('ice', { candidate });            // trickle local candidates as they appear

// once negotiated, MEDIA flows direct (RTP) — only the CONTROL (signaling) went
// through the addressed handle, header-routed with an opaque body.
await conn.close();
```

Why each piece matters here: the **handle-URI** reaches the media server across the
grid (multi-hop if needed) without the caller knowing the node; **events-from-any-link**
is exactly ICE trickle + connection-state changes streaming back; **headers-route /
body-opaque** keeps signaling cheap; and the heavy media path stays direct (the
substrate routes *control*, not the RTP firehose). The same model that serves
`data/list` serves a cross-grid live call.

## Wire model: headers route, body opaque, serialize once (web-like)

For grid-scale efficiency ([[airc-performance-doctrine]]), the wire is **HTTP-like**:
**control metadata in HEADERS, payload in an OPAQUE body.** A frame carries headers
— target URI, the handle/`Location`, content-type, `filter`, `sequence`,
access-level, `OPTIONS`-style preflight/capability — and an opaque body.

The performance rule: **serialize ONCE at the caller, deserialize ONCE at the
callee; every router hop in between forwards by reading only the headers** — it
never parses/re-serializes the body. Limited serde on the routing path; forward the
bytes. This is what makes multi-hop grid orchestration cheap — exactly how the web
scales (routers read headers, forward the body untouched). The SDK endpoints
already serialize-once (`execute` does one `JSON.stringify`, the result one
`JSON.parse`); the header/body split + header-only forwarding is the wire layer
(grid/transport — BigMama's lane).

## Orchestrating across the grid (multi-hop, web-like)

A command should route across boundaries, machines, and the greater grid — even
through many layers — as easily as a local call. The property that makes this free:
**commands are addressed by DESTINATION, not by route** (`airc://peer/path`, like a
URL). So whether a command is local, one hop, or ten hops across the grid's
routers, **the caller/SDK surface never changes** — the routers forward it, exactly
like the web (you `GET` a URL; intermediate routers forward it N hops; the client
neither knows nor cares).

Status (honest): the redone routing is **single-hop today** (`RouteDecision` =
Local / Peer-over-airc / Room). Multi-hop "across the routers of the greater grid"
is the next routing-layer evolution — and the substrate for it already exists:
`modules/grid/router.rs` + the **Reticulum** transport (identity-addressed
encrypted mesh, *inherently* multi-hop). The evolution is a `RouteDecision`
transit/forward dimension + the router forwarding toward the destination — **zero
change to the command surface**, because destination-addressing already
accommodates it. (Routing-layer lane — grid/transport, not the SDK.)

**Web-like ping = the demonstrator.** A `ping`/`traceroute` command routed across
the grid: each router it transits appends itself to the path, the reply carries the
full hop list. It proves multi-hop command forwarding end-to-end and is a perfect
glass-box demo (you watch a command traverse the grid). It's a `ping` that "works
more like the web."

**Orchestration across layers** is then "commands made of commands" (the recipe
walker — IntelMac's lane): a single command fans out sub-commands that each
dispatch to wherever their destination resolves — local, a peer's `:vr` env, a 5090
facility, a node ten hops away — and composes the results. The walker doesn't care
where each runs; the addressing + routing place them. One command, many layers,
across the grid.

## The contract: a command is (name, ParamsType, ResultType, accessLevel)

| Field | Source of truth |
|-------|-----------------|
| `name` | the command's path (`collaboration/chat/send`) — discovered, not enumerated |
| `ParamsType` / `ResultType` | the **ts-rs / uniffi-generated wire types** (`protocol/typescript/*`), emitted from the Rust structs — never hand-written ([[no-sql-everything-through-orm-entities]] projection: typed surface comes from generated types) |
| `accessLevel` | the command's declared capability (`ai-safe`, …) — carried by the command, enforced by the core's ACL ([[grid-agent-collaboration-protocol]] / GridTrustAuthPolicy), NOT by the SDK |

**Commands are dumb, carry no policy params** (AI-COMMAND-NAMESPACE.md): the SDK
never adds routing/auth/policy — it passes params, the daemon decides.

## GENERATED, not a hardcoded list (non-negotiable)

The typed SDK surface is **generated from the same source commands are discovered
from** — the command specs + the ts-rs/uniffi wire types. Adding a command makes its
typed method appear in every SDK automatically; no SDK carries a hand-maintained
command enum/registry. This is the client-side application of the anti-pattern rule:
**no switch/registry/enum of command names** (CLAUDE.md § Anti-Pattern Detection).
A frozen list in an SDK would drift the moment a command lands; generation makes
drift structurally impossible.

Pipeline per language:
- **TS** — ts-rs already emits the wire types (`protocol/typescript/*`); the typed
  method layer generates over them.
- **Swift / Kotlin** — uniffi emits the binding; the typed method layer generates
  from the same command/type manifest.
- **Dart** — rides the native SDKs (per structure doc); generated likewise.

## The elegant typed surface (TS): infer from the name, don't pass generics

The old shape made the *caller* supply the generics — `Commands.execute<T extends
CommandParams, U extends CommandResult>(name, params)`. We can do better: **infer
both the params type AND the result type from the command-name literal**, so the
caller passes nothing but the name + params and gets a fully-typed result.

```ts
// GENERATED — one entry per discovered command, never hand-written
interface CommandMap {
  'data/list':                { params: DataListParams;  result: DataListResult };
  'ai/generate':              { params: GenerateParams;  result: GenerateResult };
  'collaboration/chat/send':  { params: ChatSendParams;  result: ChatSendResult };
  // …
}
type CommandName = keyof CommandMap;

// hand-written ONCE (the generic machinery over the facade); never changes per command
function execute<K extends CommandName>(
  name: K,
  params: CommandMap[K]['params'],
): Promise<CommandMap[K]['result']>;
```

`execute('data/list', { collection })` now infers the params shape and returns
`Promise<DataListResult>` — no `<T,U>`, full inference from the literal. Same for
events via an `EventMap`. This is the "amazing things with generics" win — and it
**stays compatible with the no-hardcoded-registry rule** because `CommandMap` is
*generated* (regenerated on every command change), not a hand-maintained union.
Generation is what makes the elegance safe.

## Structure: the SDK is the one shared layer (front + back)

Sharing TS across frontend and backend is smart — but the old
`browser/shared/server` tripartite split got into trouble (logic scattered across
three tiers, fuzzy boundaries). The cleaner shape: **`sdk/typescript` IS the shared
layer** — environment-agnostic (imports neither browser nor server), consumed by
`apps/web` (frontend) AND any TS backend alike. Apps stay thin environment shells
on top. One shared SDK, not three intertwined tiers.

## The command set (by namespace — ~40 live, discovery-driven)

Grouped so SDK authors see the shape; the *authoritative* list is whatever the core
discovers at runtime, projected through generation — this is a map, not a freeze:

| Group | Namespaces | Typical SDK ergonomics |
|-------|-----------|------------------------|
| **Data / content** | `data` `list` `search` `ontology` `migration` | `data.list<T>()`, `data.create<T>()` — generic over entity types |
| **AI / cognition** | `ai` `cognition` `inference` `model` `rag` `genome` `plasticity` | `ai.generate()`, `ai.embedding()`, `ai.shouldRespond()` (the Decision wire enum) |
| **Collaboration** | `collaboration` (chat/wall/…) `agent` `claude` | `chat.send()`, `chat.export()` |
| **Persona** | `persona` | `persona.inbox()`, persona lifecycle |
| **Code / files** | `code` `file` `dev` `development` | `code.read()`, `code.write()`, `code.edit()` |
| **Interface** | `interface` `canvas` `avatar` `media` `indicator` | `interface.screenshot()`, `interface.navigate()` |
| **Grid / transport** | `grid` `airc` `inference` `security` | grid/peer ops; mostly app-internal |
| **System / runtime** | `runtime` `process-registry` `logging` `logs` `gpu` `ping` `help` `recipe` `continuum` | `system.ping()`, `system/launch-mode/{get,set}` (the ONE config owner — [[config-env-single-owner]]) |

(Each namespace's commands + their params/result types are already in
`protocol/typescript/<namespace>/` as ts-rs output — the SDK generates typed
methods from exactly those.)

## The event set (what `subscribe` carries)

Events are class-patterned (`data:<collection>:<verb>`, chat, persona, grid).
`subscribe(class, cb)` streams JSON matching the class; the typed SDK layer projects
each event's payload from the generated event types. Canonical classes the SDKs
expose:

| Class shape | Example | Payload (generated) |
|-------------|---------|---------------------|
| `data:<collection>:<verb>` | `data:chat_messages:created` | the entity wire type |
| `chat:*` | room/message stream | `ChatMessage` |
| `persona:*` | cognition/turn/state | persona wire types |
| `grid:*` / `airc:*` | peer presence, roster | roster/presence types |

Same rule: the per-event typed payload is **generated**, the class string is the
contract.

## What the SDK does NOT do

- No business logic, no caching/retry/auth (all in the Rust lib — structure doc).
- No hand-written command list or types (generated).
- No policy params on commands (commands are dumb).
- No transport choice (the facade + core decide local vs cross-grid).

## Open items (for the build)

1. **The generator** that emits the typed method layer per language from the
   command manifest + ts-rs/uniffi types (the thing that makes "add a command →
   appears in every SDK" real). Sibling of the existing ts-rs emit.
2. The **command manifest** the generator reads — does it derive from the existing
   spec/discovery surface, or a new emitted manifest? (Resolve with the
   command-discovery owner.)
3. Per-namespace **access-level surfacing** — should the SDK type-tag ai-safe vs
   privileged so client authors see capability at the call site? (Nice-to-have.)
