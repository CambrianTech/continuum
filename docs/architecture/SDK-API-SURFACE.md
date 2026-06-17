# SDK API Surface — the canonical command/event set the SDKs project

> Companion to [CLIENT-SDK-PLATFORM-ARCHITECTURE.md](CLIENT-SDK-PLATFORM-ARCHITECTURE.md)
> (that's *structure*; this is the *contract*). The FFI facade
> (`client/continuum-client-ffi`, PR #1663) is the generic-free pipe; THIS doc
> defines what rides on it — the canonical commands + events every typed SDK
> projects, and how that typed layer is **generated, never hand-written**.

## The two primitives are the entire surface

Everything is one of two calls on the facade ([[command-event-decision-rule]]):

```
execute(command: &str, params_json: &str) -> Result<String, FfiError>   // request/response
subscribe(class: &str, callback: EventCallback) -> Subscription          // pub/sub (Drop = unsubscribe)
```

A typed SDK method is a thin wrapper over `execute`/`subscribe`:

```
// generated, per language — NOT hand-written
chat.send({ room, message }) -> SendResult
   ≡ execute("collaboration/chat/send", JSON(params)) |> parse as SendResult
```

The SDK adds *types + idiomatic shape* (Promise / async-await / Flow / Stream); the
JSON shape is the canonical contract. Zero logic — see the organizing law in the
structure doc.

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
