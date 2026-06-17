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
subscribe(class: &str, callback: EventCallback) -> Subscription          // Event: subscribe (Drop = unsubscribe)
```

(`#1663` shipped `execute` + `subscribe`; `provide` is the serve-side primitive the
facade still needs — see "Commands are bidirectional" below.)

A typed SDK method is a thin wrapper over `execute`/`subscribe`:

```
// generated, per language — NOT hand-written
chat.send({ room, message }) -> SendResult
   ≡ execute("collaboration/chat/send", JSON(params)) |> parse as SendResult
```

The SDK adds *types + idiomatic shape* (Promise / async-await / Flow / Stream); the
JSON shape is the canonical contract. Zero logic — see the organizing law in the
structure doc.

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
