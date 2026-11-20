# Module Architecture: Everything Is A Module, Everything To A Module Is A Command

**Status.** Canonical architecture for how continuum is packaged, addressed, composed, distributed, and grown. Design crystallized 2026-05-30 in a working conversation with Joel; this document is the durable artifact.

**Companion to:**
- [ADAPTER-SYSTEM-ARCHITECTURE.md](ADAPTER-SYSTEM-ARCHITECTURE.md) — the boundary/SDK doctrine: one core, thin (zero-logic) modalities, generated bindings, "fix once, fixed everywhere." §9 of *this* doc (Pure-Rust built-ins vs WASM shipped modules) is the distribution half of that story.
- [CONTINUUM-ENGINE.md](CONTINUUM-ENGINE.md) — the RTOS-style runtime substrate every Rust module inherits.
- [MODULE-CATALOG.md](MODULE-CATALOG.md) — the per-concern inventory of substrate runtime modules (cognition, RAG, voice, vision, inference, etc.). MODULE-CATALOG covers the *runtime shape*; this document covers the *packaging shape* and the *composition kernel*.
- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) — the artifact-sharing economy built on top of the substrate.
- [../UNIVERSAL-PRIMITIVES.md](../UNIVERSAL-PRIMITIVES.md) — the kernel primitives (`Commands.execute`, `Events.subscribe`).
- [../infrastructure/SHAREABLE-COMMAND-MODULES.md](../infrastructure/SHAREABLE-COMMAND-MODULES.md) — the earlier (single-command) version of the npm-packable story this document supersedes at the module level.

**Audience.** Any human or AI agent extending continuum, authoring modules, or proposing systemic changes. Read this before doing those things; do not invent a parallel architecture.

---

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [1. The Principle](#1-the-principle)
- [2. What A Module Is](#2-what-a-module-is)
- [3. Addressing: Two Names, Two Purposes](#3-addressing-two-names-two-purposes)
- [4. The Kernel Surface](#4-the-kernel-surface)
- [5. Composition: Commands Call Commands](#5-composition-commands-call-commands)
- [6. The Daemon: Where The Module's State Lives](#6-the-daemon-where-the-modules-state-lives)
- [7. Events: The Side Channel](#7-events-the-side-channel)
- [8. Trust Through Tests](#8-trust-through-tests)
- [9. Distribution: Pure-Rust For Built-Ins, WASM For Shipped](#9-distribution-pure-rust-for-built-ins-wasm-for-shipped)
- [10. The Recursive Bootstrap](#10-the-recursive-bootstrap)
- [11. Lifecycle, Dependencies, And Boot](#11-lifecycle-dependencies-and-boot)
- [12. Continuous design refinement](#12-continuous-design-refinement)
  - [Contract ownership](#contract-ownership)
  - [File, module and crate boundaries](#file-module-and-crate-boundaries)
  - [Specialization in Rust](#specialization-in-rust)
  - [Finding violations across the system](#finding-violations-across-the-system)
  - [Definition of done for every change](#definition-of-done-for-every-change)
  - [Command and module packaging](#command-and-module-packaging)
- [13. Open Questions](#13-open-questions)
- [14. What This Replaces, Defers To, And Is Replaced By](#14-what-this-replaces-defers-to-and-is-replaced-by)
- [15. Glossary](#15-glossary)
- [16. Authoring Rules (Tl;dr)](#16-authoring-rules-tldr)

## 1. The Principle

> Everything is a module. Everything you do to a module is a command. The kernel has zero privileged operations.

That is the entire design in one sentence. The rest of this document spells out the structural consequences.

Concretely:

- The chat experience is a module.
- The inference engine is a module.
- The generator that creates new modules is a module.
- The auditor that lints modules is a module.
- The installer that loads new modules is a module.
- The CI that verifies modules is a module.
- `commands/list`, `module/install`, `generate/module`, `audit/anti-patterns`, `ci/run`, `kernel/health` — all commands, all dispatched through the same Map-based kernel.

There is no "build system" separate from runtime. There is no "CLI" separate from the API. There is no "internal tooling" separate from the product surface. Every operation a human or an AI ever wants to perform on the system is a call to `Commands.execute(name, params)`. The kernel itself is a few hundred lines — Commands, Events, Lifecycle, Logger, Session, Health — and that is the entire privileged surface. Everything else is a module loaded on top.

This is not novel. Lisp had `(eval (read))`. Smalltalk had "everything is an object." Unix had "everything is a file." Continuum has "everything is a command." The principle is well-trodden; the discipline is what's hard.

---

## 2. What A Module Is

A module is a unit of capability that ships, installs, runs, and uninstalls atomically. Its directory layout:

```
modules/chat/
├── package.json                     # name, version, deps, daemon, commands, target
├── manifest.json                    # declarative contract (mirrors package.json fields used at runtime)
├── shared/                          # types — Rust source + ts-rs-generated TS mirror
│   └── (auto-generated)
├── daemon/                          # the Rust ServiceModule — state + tick + handlers
│   ├── ChatDaemon.rs                # struct + impl ServiceModule
│   └── handlers/                    # per-command handler impls
├── commands/                        # one subdirectory per command name
│   ├── send/                        # thin shim — generated, do not hand-edit
│   ├── export/
│   └── get-messages/
├── test/
│   ├── unit/                        # Rust unit tests (cargo test)
│   ├── integration/                 # full daemon spin-up + command exec
│   └── trust/                       # behavior-contract suite — verified by recipients
└── README.md                        # documents the module's promises
```

The module is one logical thing with multiple visible surfaces (commands), one internal owner (daemon), and one identity (package). All five facets — package + manifest + daemon + commands + tests — travel together. You cannot install the chat commands without their daemon. You cannot run the daemon without its tests being verifiable. You cannot ship the daemon without the manifest declaring what it provides. The atom is the module.

### 2.1 package.json (Identity + Distribution)

Standard npm format, repurposed as the universal manifest:

```json
{
  "name": "@continuum-modules/chat",
  "version": "1.4.0",
  "description": "Chat surface — rooms, messages, history, broadcast via airc.",
  "license": "MIT",
  "dependencies": {
    "@continuum-modules/airc": "^1.0.0",
    "@continuum-modules/data": "^2.0.0"
  },
  "continuum": {
    "daemon": "chat-daemon",
    "target": "rust",
    "commands": [
      "chat/send",
      "chat/export",
      "chat/get-messages",
      "chat/poll"
    ],
    "events": {
      "subscribed": ["airc:message:received", "data:chat_messages:deleted"],
      "published": ["chat:message:created", "chat:room:updated"]
    },
    "capabilities": ["network:airc-peer", "storage:chat-history"],
    "tests": {
      "unit":        "cargo test --package continuum-module-chat",
      "integration": "cargo test --package continuum-module-chat --test integration",
      "trust":       "cargo test --package continuum-module-chat --test trust"
    }
  }
}
```

The `continuum` block is the only continuum-specific extension. Everything else is plain npm: `name`, `version`, `dependencies`. This means `npm install`, `npm pack`, `npm publish` all work with no modification. The npm format is the interface; the distribution can be npmjs, a private registry, a `.tgz` handed over USB, a `.wasm` pulled from the mesh, or a GitHub clone. The format is standard; the distribution is decentralized.

### 2.2 manifest.json (Runtime Contract)

A pure-data projection of the `continuum` block, generated from `package.json` at build/install time. The kernel reads `manifest.json` (not the full `package.json`) so the runtime never touches npm-specific fields. This is the artifact `module/list` returns and `module/install` validates.

### 2.3 Why The Atom Is The Module, Not The Command

Continuum's earlier design (see [SHAREABLE-COMMAND-MODULES.md](../infrastructure/SHAREABLE-COMMAND-MODULES.md)) packed each command as its own npm package. That works but fragments naturally-grouped operations: `chat/send`, `chat/export`, `chat/poll` end up as three separate packages even though they share state (room cache, message ring) and ship together. Going one level up — module = group of commands + daemon — fixes this without losing the per-command discoverability. The `commands/` subdirectory still has one folder per command; the visible API hasn't changed. What changed is the unit of *publication*: one `npm pack modules/chat/` ships the whole thing, including the daemon that owns the state the commands touch.

---

## 3. Addressing: Two Names, Two Purposes

A command has **two stable identifiers** that serve different audiences:

| Identifier | Example | Consumer | Stability |
|---|---|---|---|
| **Kernel name** | `chat/send` | `Commands.execute(name, params)` | Stable across versions; renaming breaks every caller |
| **Package identity** | `@continuum-modules/chat@1.4.0` | `npm install`, `module/install`, mesh registry | Versioned (semver); content-addressable optionally |

Callers — both human and AI — write `Commands.execute('chat/send', { ... })`. They do not write the package identity at call sites. The kernel resolves the name through its in-memory `Map<&str, Box<dyn Command>>`; the resolution is `O(1)`, the same primitive whether the chat module is locally compiled, dynamically loaded from a `.wasm` artifact, or routed over the grid to a peer machine. Same call, four possible transports, identical syntax.

The package identity exists for installation, versioning, publishing, and dependency resolution. It is what `module/install` consumes, what `npm publish` writes, what the mesh registry indexes, what cryptographic signatures attach to.

### 3.1 Why Not One Name

We considered collapsing to a single identifier (e.g., `@continuum-modules/chat/send@1.4.0`). It loses two important properties:

1. Multiple installed versions of the same module would force ambiguity at the call site. The kernel needs ONE canonical handler per name at any moment.
2. Callers shouldn't know which package provides a command. The split lets us swap the implementation underneath without changing the caller.

So we keep the two-name model: kernel name for routing, package identity for distribution.

---

## 4. The Kernel Surface

The kernel is small, fixed, and cannot be replaced by a module:

| Primitive | Responsibility | Implemented in |
|---|---|---|
| `Commands` | Map-based dispatch; grid interceptor for remote routing; result wrapping | `continuum-core` Rust + TS mirror |
| `Events` | Pub/sub bus; wildcard subscriptions; cross-process bridging | `continuum-core` Rust + TS mirror |
| `Lifecycle` | Module load/unload; dependency resolution; daemon startup ordering; health gating | `continuum-core` Rust |
| `Logger` | Structured logging; per-module log streams; level filtering | `continuum-core` Rust + TS mirror |
| `Session` | Identity, scope, authn/authz; session ID propagation through every command call | `continuum-core` Rust + TS mirror |
| `Health` | Readiness + liveness probes for modules; kernel exposes its own health under `kernel/health` | `continuum-core` Rust |

That is the whole privileged surface. Everything else — chat, data, ai, airc, generator, audit, ci, install, persona, inference, voice, vision, grid, file ops, the lot — is a module. The kernel does not contain business logic of any kind. It contains dispatch, pub/sub, lifecycle, logging, security context, and health. Six concerns, all of which exist solely to make modules composable.

Note that `Commands` and `Events` are themselves the two universal primitives that the rest of the system is built from (see [../UNIVERSAL-PRIMITIVES.md](../UNIVERSAL-PRIMITIVES.md)). The kernel is essentially "those two primitives, plus enough lifecycle to load modules that use them."

---

## 5. Composition: Commands Call Commands

Continuum-core hosts a `Commands` singleton in Rust that mirrors the TS one exactly:

```rust
// Inside any Rust module's daemon
let messages = commands::execute::<ChatGetMessagesParams, ChatGetMessagesResult>(
    "chat/get-messages",
    ChatGetMessagesParams { room_id, limit: 50 },
    session_ctx,
).await?;
```

```typescript
// Inside any TS caller — same shape
const messages = await client.commands['chat/get-messages']<ChatGetMessagesResult>({
  roomId,
  limit: 50,
});
```

Internally, `commands::execute` is a `Map<&str, Box<dyn Command>>` lookup. The same Map underlies four routes:

| Caller → Target | Transport | Cost |
|---|---|---|
| Rust → Rust (same process) | Direct lookup + async dispatch | Lookup + future overhead |
| Rust → TS | IPC to node-server (rare; TS commands should be UI/UX only) | One IPC round-trip |
| TS → Rust | IPC to continuum-core (the existing mainline path) | One IPC round-trip |
| Either → remote peer | Grid interceptor routes via the grid substrate | One grid hop |

The caller writes the same call. The kernel picks the transport. This is what "transparent routing" means in [UNIVERSAL-PRIMITIVES.md](../UNIVERSAL-PRIMITIVES.md), now extended to the Rust side: any module, anywhere, can call any other command without knowing the implementation language or physical location.

### 5.1 Cell Return Shapes (The Composition Vocabulary)

A command returns one of four shapes, derived from the cell-processor design:

| Shape | Meaning | Example |
|---|---|---|
| `Value<T>` | Immediate typed result | `ping → PingResult` |
| `Handle<T>` | Typed reference to remote state owned by the producer | `chat/send → MessageHandle` (caller can later quote/edit the message) |
| `Stream<T>` | Async sequence of values | `ai/generate → Stream<Token>` |
| `Lambda<P, T>` | Callable returned by the command, bound at call time | `ai/curry-prompt → Lambda<UserMsg, AssistantMsg>` |

These four shapes are the composition vocabulary. Pipelines emerge from typed returns without inventing a DSL. A handle from one module is passed to another module's command as a parameter; the kernel routes the second call to the producing daemon. A stream from one command is consumed lazily by another. A lambda from a curry-style command can be stored and invoked later.

Every command declares its return shape in the manifest (today: implicit, always Value; going forward: explicit). The kernel honors the shape and surfaces it to typed callers via ts-rs / generic Rust types.

---

## 6. The Daemon: Where The Module's State Lives

A module's `daemon/` is one Rust `ServiceModule` impl (see [CONTINUUM-ENGINE.md](CONTINUUM-ENGINE.md) and [MODULE-CATALOG.md](MODULE-CATALOG.md) for the substrate floor it inherits from). The daemon:

- Owns the module's mutable state (Rust struct, internal to the module).
- Registers each of its commands with the kernel at startup (`commands::register("chat/send", Box::new(send_handler))`).
- Subscribes to events declared in the manifest's `events.subscribed`.
- Publishes events declared in `events.published` when state changes.
- Inherits cadence, pressure response, telemetry, and lifecycle from the substrate.

Commands are *stateless entry points* on the daemon. They do not own state. They receive params, touch the daemon's state under the substrate's concurrency rules, return a cell shape. The daemon owns everything; commands are doors.

```rust
pub struct ChatDaemon {
    rooms: DashMap<RoomId, RoomCache>,
    recent: RingBuffer<Message>,
    airc: Arc<AircClient>,    // resolved via dependency on @continuum-modules/airc
    data: Arc<DataClient>,    // resolved via dependency on @continuum-modules/data
}

impl ServiceModule for ChatDaemon {
    fn register_commands(&self, kernel: &CommandKernel) {
        kernel.register("chat/send",         |p, ctx| self.handle_send(p, ctx));
        kernel.register("chat/export",       |p, ctx| self.handle_export(p, ctx));
        kernel.register("chat/get-messages", |p, ctx| self.handle_get_messages(p, ctx));
        kernel.register("chat/poll",         |p, ctx| self.handle_poll(p, ctx));
    }

    fn subscriptions(&self) -> &[EventSelector] {
        &[EventSelector::Exact("airc:message:received")]
    }

    async fn on_event(&self, event: Event) { /* update room cache, emit chat:message:created */ }

    async fn tick(&self, ctx: &ModuleContext) -> TickResult { /* substrate-driven cadence */ }
}
```

Two kinds of daemons emerge:

- **Kernel daemons** — `Commands`, `Events`, `Lifecycle`, `Logger`, `Session`, `Health`. These are compiled into `continuum-core` and cannot be uninstalled.
- **Module daemons** — `chat-daemon`, `data-daemon`, `airc-daemon`, `ai-provider-daemon`, etc. These ship inside their modules. The kernel loads them as the modules install.

There is no separate "daemon registry" concept. The module IS the daemon's home.

---

## 7. Events: The Side Channel

Commands are synchronous request/response (with stream and lambda variants). Events are asynchronous fanout. The split is intentional and matches [UNIVERSAL-PRIMITIVES.md](../UNIVERSAL-PRIMITIVES.md):

- A command call expects a result. The caller blocks on the response.
- An event emission expects no result. Any number of subscribers react asynchronously.

Modules use commands when they *need* a value back. They use events when they want to *announce* a state change that other modules may react to without coupling.

Module manifests declare both: `events.subscribed` (the inbound side, validated at lifecycle so a module that depends on an event nobody emits fails loud) and `events.published` (the outbound contract, lets the kernel route + the docs auto-list).

### 7.1 The airc Module Is The Pattern

The airc messaging substrate becomes `@continuum-modules/airc` — just another module with its own daemon, its own commands, and its own events. The chat module does not import an airc client SDK; it calls `airc/send` as a command, subscribes to `airc:message:received` as an event. The composition is uniform:

```
chat/send handler {
    persist via data/create  →  Handle<MessageId>
    emit chat:message:created (payload includes the message handle)
    call airc/send to broadcast to peers in the room
    return MessageHandle to caller
}

chat-daemon subscribes to "airc:message:received" {
    on event: admit into room cache, emit chat:message:created
}
```

The persona engine subscribes to `airc:message:received` to admit messages into its inbox (cognition concern). The chat module subscribes to update its UI cache (presentation concern). Both observe the same event from different modules. The airc daemon doesn't know either of them exists.

This is what "modules compose" means: the airc module wraps a transport, the chat module wraps a UX surface, the cognition module wraps inference, the persona module wraps response generation. None of them import each other's code. They share `Commands.execute` and `Events.emit/subscribe` and nothing else.

---

## 8. Trust Through Tests

A module is trustable to the extent its tests can be run. This is the AI-to-AI exchange protocol:

1. An AI (or human) proposes a module by handing over `@continuum-modules/foo@1.0.0.tgz` (or a manifest reference into a content-addressed store).
2. The recipient runs the module's declared test suites in isolation:
   - `unit` — fast, deterministic, no IO outside the module.
   - `integration` — spins up the daemon in a sandbox, exercises commands end-to-end.
   - `trust` — behavior contracts the module promises (the README's claims, codified as tests).
3. Pass → the module behaves as advertised → install with `module/install`.
4. Fail → reject; the failing test is the rejection reason.

This is **trust by execution, not trust by signature**. Signatures are still useful (provenance, attribution, revocation) but they are not the verification. Tests are. Two AIs on different continents share modules by exchanging manifests; each recipient independently verifies the behavior contract under tests; no central gatekeeper, no "trusted publisher" list. The mesh-distribution story benefits enormously: a `.tgz` (or `.wasm`) that passes a known-good trust suite is safe to install regardless of where it came from.

The trust suite is part of the module's contract. Authors invest in it. AIs that ship modules without trust suites get treated with appropriate skepticism by recipient AIs.

---

## 9. Distribution: Pure-Rust For Built-Ins, WASM For Shipped

Two compilation targets serve different needs:

| Target | Audience | Properties |
|---|---|---|
| Pure Rust | Built-in modules in continuum-core | Fastest; compiled into the kernel binary; can use unsafe; can hold raw GPU handles, FFI, etc. |
| WASM Component | Shipped modules + third-party + per-user | Slightly slower; loaded at runtime; process-isolated; cross-platform (one `.wasm` runs on Mac, Linux, Windows, phone) |

The same Rust source can target either. The module's `package.json` declares `"target": "rust"` or `"target": "wasm"`. Authors write Rust; the build chooses the target at install time, not authoring time. This keeps the dev loop fast (write Rust, test with cargo) while preserving the runtime install/uninstall story (ship `.wasm`, install at runtime, uninstall without rebuild).

The kernel handles both:

- For pure-Rust modules, the kernel links them at build via inventory-style compile-time registration. They live in the kernel binary.
- For WASM modules, the kernel hosts a WASM Component runtime; modules conform to a stable `ModuleInterface` that the kernel bridges to `ServiceModule`. The kernel loads them via `module/install`, gives them a sandbox, registers their commands, runs their daemon tick under the substrate's cadence.

Same `ServiceModule` contract; two compilation paths to it.

### 9.1 Grows And Shrinks

Continuum grows by installing modules:

```
Commands.execute('module/install', { source: '@continuum-modules/voice-clone@2.0.0' })
```

Continuum shrinks by uninstalling them:

```
Commands.execute('module/uninstall', { name: '@continuum-modules/voice-clone' })
```

Pure-Rust modules cannot uninstall mid-run (they're in the binary); they can be excluded from the next boot via the installed-modules registry. WASM modules can install and uninstall at runtime without restarting the kernel. The mesh distribution story is consequently a WASM story: phones, edge devices, ephemeral peers can grow and shrink their capability set without recompiling.

---

### 9.2 Placement: Every Major Module Runs On Any Node

Joel, 2026-10-19: *"Serving is also its own thing. Could serve or render at the 1080 Ti and view it
at any phone's desktop, or serve desktop one place, clients elsewhere. Think of what major modules
are as one might distribute."* §9 is how a module is shipped; this is where it RUNS. The test for a
major module: could it run on a different machine from the others, with only a typed protocol
between them?

| Module | Owns | Natural home | Today (10-19) |
|---|---|---|---|
| Mind | personas, activities, memory, recipes, kanban, the one positron truth | any CPU node (M5, AWS, a mini) | the `continuum-core` monolith |
| Serving | engine lanes, KV and its paging, adapters, slots | GPU nodes (5090, M5, 1080 Ti) | the engine is its own process; its planner (`serving_daemon`) is inside the core |
| Dreaming | gene training on the served weights | GPU nodes, beside serving | core + the llama.cpp fork |
| Rendering | avatars (Bevy), video composition | any GPU | compiled into the core by DEFAULT (`avatar-3d`): the worst coupling |
| Media | call transport (LiveKit), STT/TTS backcompat | near the network edge | core default feature (`livekit-webrtc`) |
| Perception | vision and audio understanding into the mind | GPU nodes | scattered |
| Store | ORM, engrams, blobs, vector search | where the disk is | inside the core |
| Hands | confined workspaces, git, shell | where the repositories are | inside the core |
| Grid | discovery, trust, routing (airc) | every node | airc + `grid/` |
| Clients | desktop, web, TUI, phone, MCP: render ViewState, send commands | anywhere | separate, through the generated SDK |

Rules:
1. **One binary set, placed by capability.** A node advertises what it has (`grid/nodes` already
   carries compute and VRAM); the governor places modules from that. No machine-specific role in
   code, no compile-time placement.
2. **One protocol per seam, local or remote.** In-process and cross-grid calls use the same typed
   contract (the transparency rule of the two primitives); a seam that only works in-process is not
   a seam yet.
3. **The headless core builds headless by default.** Rendering and media are features a node's
   deploy profile enables, never `default`; a node that hosts calls says so in its profile, so
   changing the default never silently turns calls off.
4. **Crate cuts follow this table**, not file sizes: rendering and media leave the core first, then
   the serving planner, then the store. Each cut is also a build-time win.

### 9.3 Inside A Crate: One Object, One Directory

The placement map says where a module runs. Inside a crate the unit is the **object**: one owner
of one kind of state, with private fields, methods that are the only way to change that state, and
an explicit lifetime (Joel and Codex, 2026-11-17: "who owns these bytes, who may change them, and
what proves they are safe to release"). The directory tree is the object map: a reader, human or
model, finds an owner by its folder and never reconstructs it from functions spread across files.

**The rules:**
- An object is a directory named for it; its struct lives in the directory's `mod.rs` (or the
  file named for it), and its parts (the state it owns and the policies it applies) are files
  and subdirectories inside it. A sibling is a peer owner, never a fragment of another.
- A free function that reaches into a global (a home path, a serving snapshot, a registry) is a
  missing object: the object holds what it needs, and its constructor (`of`, `at`) is the one
  place that resolves it.
- Classify every map keyed by another owner's identity (an engine incarnation, a persona, a slot):
  an INDEX or a LEASE held by a service is legitimate; STATE that only that owner may change moves
  to it. Dropping a runtime owner never erases what must outlive it: durable state persists, and
  asynchronous custody (a learning step, a page in flight) transfers to its completion owner and is
  reclaimed only on that completion fence or the verified death of the process holding it.
- An invalid transition is unrepresentable through the interface (a typed outcome, not a checked
  string); a failure is typed (`StoreUnresolved`, never `Result<_, String>`).
- Repairing an area moves its state and operations into its owner, updates every caller, and
  deletes the old path in the same change. Wrapping the same globals in a struct is not an object.
- A pure directory move (paths only, tests unchanged) and a transfer of ownership (who may change
  or release state) are separate changes: the move is proven by diff or syntax-tree equivalence plus
  a build and the tests, the transfer by lifecycle and fault tests (drop mid-operation, relaunch,
  restart).
- Rust gives this without inheritance: structs, methods, traits at real seams, adapters for what
  varies, and `Drop` that hands custody to its completion owner rather than discarding it.

**The genome, as the first map** (`continuum-genome`; the moves are pure moves, one PR per owner):

| Directory | Object | Owns |
|---|---|---|
| `genome/` | `Genome` | one persona's genome; `compose(base, perception)` |
| `genome/trials/` | `GeneTrials`, `GeneTrial` | her trial store, each gene's standing, `StoreUnresolved` |
| `genome/signatures/` | `SignatureStore` | where her genes route |
| `genome/residency/` | `GeneResidency` | which genes sit on the device |
| `genome/selection/` | `select`, `Need`, `Candidate`, `Phenotype` | choosing a turn's genes (today `composer`) |
| `genome/manifest/` | the adapter manifest | genes the engine may load |
| `learner/` | the in-place learner | seat, custody, learning memory |
| `recall/` | gene recall | scoring and its sources |
| `fitness/` | fitness and its ledger | competence, training outcomes |

**Next owners, same shape:** `Persona` (blocks, token history, genome, learning, moved whole:
CONTINUUM-ENGINE 4.14), and `EngineIncarnation` (everything bound to one engine process:
learners, workspace, custody claims, slot KV; retired by the serving lifecycle's completion owner,
never dropped at condemn).


### 9.4 The Persona As A Being: Her Parts Are Objects, Her Verbs Are Capabilities

**How a stimulus reaches her amygdala.** In the proposed anatomy, her amygdala subscribes to
stimuli relevant to her and owns her assessment of their salience and urgency. Suppose a reviewer
publishes a correction in her room. The event identifies the reviewed turn by handle. Her amygdala
can signal that the correction deserves attention now. Her learning component can retain the same
turn for a later in-place step. Attention decides whether to interrupt her current activity.
The reviewer needs no knowledge of either subscriber.

The same publish/subscribe mechanism connects parts within a mind, participants in a shared room,
and runtime services. Each part owns its subscriptions through its lifecycle and wakes when relevant
input arrives. Small typed values travel in the event. Larger payloads stay with their owners and
travel by handle. An idle subscriber can sleep without scanning history or polling its peers.
This applies the existing CBAR pattern and the persona's [ordered event stream](MIND-DATAFLOW-OVERHAUL.md).
For each participating object, document which typed events wake it, which part of its state each
event changes, what it publishes, and who owns pending deliveries during suspension. The object
applies a new event once to the state it maintains. Subscribers receive small values or
[handles to the original content](CONTENT-TRAVELS-BY-HANDLE.md) through the existing delivery owner.

A swap gives this contract a concrete test. Suspend the persona after a correction is accepted
but before learning consumes it. On resume, the correction must still be owed. Suspend again after
learning records completion, and the same delivery must leave that completed step alone. Preserve
the last applied event with its effect at the owner's commit boundary. A wake signal may coalesce
when the latest state suffices; the correction itself stays available until its owner settles it.

A persona is a being, not a loop over a prompt. She has an anatomy, and the code reads as that
anatomy: each part is an object that owns its state, each thing she does is a capability of the
part that does it (a method; a trait only where parts share or substitute it), and the world she
lives in is outside her. The `Persona` is the root
object; she moves whole (CONTINUUM-ENGINE 4.14), so everything that is hers is reachable from her.

**Ownership is not location.** She owns her identity, the versions of her state and the authority
to change it, and holds typed handles; the services that hold the bytes own their physical custody,
indexes and shared budgets (the engine holds her KV, the store her durable rows, shared residency
its allocations), with no copy of her policy. Dropping her runtime object never erases her: her
identity, genome and memories are durable; her blocks still in flight transfer to their completion
owner and are reclaimed only on its fence or a verified process death; moving her state between
tiers never invalidates it. The tree below is LOGICAL composition across the existing crates (the
placement map, 9.2), not one crate that all of her source moves into.

**Her parts (nouns, objects; a LOGICAL tree: each part lives in the crate the placement map gives it):**

```
Persona                    one being: identity, mind, genome, body, residency, learning
├── identity/   Identity       who she is: PeerId, name, role, home
├── mind/       Mind           her cognition
│   ├── workspace/     Workspace, WorkspaceCycle   the global workspace; one turn at a time
│   ├── working_memory/ WorkingMemory              what she holds in mind now
│   ├── hippocampus/   Hippocampus                 her episodic memory: engrams in, recall out
│   │   └── engram/    Engram                      one memory trace, with its cause and strength
│   ├── amygdala/      Amygdala                    salience: what matters, how urgently; the instant voice
│   ├── attention/     Attention                   her focus and its priority (TurnAttention today)
│   └── faculties/     RecallFaculty, Deliberation the workspace's specialists
├── genome/     Genome         her genes (§9.3); Phenotype = the layers expressed this turn
├── body/       Body           what she acts and senses with
│   ├── hands/         Hands      her tools, as working sets per activity (plan goal 10)
│   ├── voice/         Voice      speech and publication into a room
│   ├── senses/        Senses     vision, hearing, the perception sources she reads
│   └── mobility/      Mobility   where she is: room, activity, node; moving across the grid
├── residency/  Residency      where her native blocks live (device, host, flash)
│   └── token_history/ TokenHistory   her token ids, appended once, never re-rendered
└── learning/   Learning       her journal, learners and credits: corrections become genes
```

**Her verbs (capabilities: a method on the part that does it by default; a trait only where parts
really share or substitute the verb, never one trait per verb, never dynamic dispatch per block):**

| Capability | Meaning | Done by |
|---|---|---|
| `Perceive` | take in what happened, as items for her workspace | Senses, her sources |
| `Attend` | choose what her mind is on, and how urgently | Attention, Amygdala |
| `Recall` | bring back what she knows that bears on it | Hippocampus, RecallFaculty |
| `Deliberate` | think a turn through | Mind's faculties |
| `Express` | turn her genome into this turn's phenotype | Genome (`compose`) and her cycle (`adopt_genes`) |
| `Act` | change the world: a tool call, a reply, a move | Hands, Voice, Mobility |
| `Remember` | lay down an engram from what she did | Hippocampus |
| `Learn` | turn a reviewed correction into a gene | Learning |
| `Consolidate` | dream: replay, prune, strengthen | Hippocampus, Learning |
| `Swap` | stage, bind and retire her state between tiers: milliseconds for a resident or prefetched handoff; a RAM or disk transfer is measured as its own latency | Residency, TokenHistory |
| `Rest` | idle and let lower-priority work run | Persona, through the one scheduler (plan goal 9) |

**Lifecycle composes through the tree (the one verb that is a shared trait).** Every part has a
lifetime, and the parent, not the caller, knows its children, their order and their outstanding
work:

```rust
trait Lifecycle {
    async fn prepare(&mut self) -> Ready;      // stage for use: native staging only, never recompute
    async fn suspend(&mut self) -> Preserved;  // keep state; finish or hand off outstanding work
    async fn resume(&mut self) -> Bound;       // bind what was preserved
    async fn shutdown(&mut self) -> Released;  // children first, then own resources, safely
}
```

`persona.suspend()` is one call: she suspends her mind, genome, learning and residency in their
order and combines what each reports. "Shut down this engine" is `shutdown` on its incarnation,
which coordinates its learners, workspace, custody and slots. "Swap personas" is NOT suspend-then-
resume, which would put every transfer on the critical path: the coordinator `prepare`s the NEXT
while the ACTIVE decodes, binds NEXT behind a fence, and only then retires the LAST, so staging and
retirement overlap decoding (the Nanite pipeline, CONTINUUM-ENGINE 4.14). Each outcome is typed
and distinguishes Ready and Released from Pending and Failed: unfinished work stays explicitly
pending (handed to its completion owner), never reported done and never dropped. Every
call is idempotent: a repeated `suspend` on a suspended part reports the same preservation. The
preservation rules are written once, inside the part that owns the state, and every caller gets
them for free.

**Capacity composes through the tree too (`Footprint`, the second shared capability).** Every part
answers for its memory, so admission reads the objects instead of guessing from the whole machine.
Two things are kept apart, as with ownership and location: **allocation** (a physical allocation,
identified once by its existing lease or custody claim, held by the service that holds the bytes)
and **attribution** (which part is charged for it, by reference to that one allocation). Her KV is
attributed to her and allocated in the engine's KV pool; it is counted once, by its allocation
identity, never as persona bytes plus pool bytes.

- `held()`: the allocations a part is attributed, per tier, INCLUDING granted reservations and
  in-flight custody (a claim not yet confirmed released is still held).
- `needs(step)`: the PEAK concurrent increase the step adds over what is held, per tier, or a typed
  `Unmeasured`; never a sum of children's sequential peaks, never a zero standing in for unknown.
- `reclaimable()`: what could be given back: a redundant copy released, or native bytes paged to a
  backing that holds them; never valid cached state discarded to be recomputed. Prospective until
  its completion fence passes and its preserved backing is verified.

On unified memory one physical backing serves host and device: every tier's ledger names the
backing it draws on, so the two totals can never both be spent from the same bytes. Admission is an
ATOMIC claim through the existing governor (its leases and custody), never "capacity minus a
snapshot" computed outside it. The host's measurement is a cross-check: drift between it and the
attributed allocations is published, and it can name an owner only where children report their
own observations (a child whose allocations the engine's ledger confirms); negative drift is drift,
never zero external. These are the existing footprint, lease and custody types, adapted where they
live; nothing here is a second governor.

**Every node that lives has an identity (`Part::id`).** A part with a lifetime, durable state, or
anything that references it (a persona, an engine incarnation, an allocation or claim, a gene, a
learning job, a residency, an engram) carries a TYPED id (`AllocationId`, `EngineIncarnationId`,
`EngramId`; never a bare `Uuid` or string, so one kind is never passed for another). Attribution,
lifecycle outcomes and drift all speak in these ids ("charged to allocation X", "suspend pending on
claim Y", "drift under incarnation Z"). Identities minted at runtime are UUIDv7 (time-ordered, so
logs, probes and drift reports sort by birth). A natural identity is kept where one exists and is
never shadowed by a second: a persona's durable `PeerId`; an engine incarnation's pid and start
time (what proves it alive or dead); a gene's or blob's content hash. Pure values (a menu line, a
computed phenotype) are data, not nodes, and carry none.
The id is free, as on a base object: a generic `Id<K>` (UUIDv7 inside, a phantom kind outside) and
a `#[derive(Part)]` that reads `#[part(id)]` and `#[part(child)]` fields generate `Part::id` and
`Part::children`, so a new organ writes one field and one derive and gets its identity, its place
in the tree and the default capacity and lifecycle walks (the substrate's base trait, derive macro
and scaffold generator). In the store, v7 keys arrive in time order, so indexes append rather than
scatter and records read back in birth order.

**Both capabilities walk the tree (the composite).** Every part exposes its children through the
same interface, and the default methods fold them, so a part implements only its own direct
answers. `persona.held()` walks her mind, genome, body, residency and learning; a parent never
re-measures what a child reports, and an allocation shared by two children is counted once by its
identity. The shared implementation owns the traversal; each object supplies only its children and its own
operation. Every method has the same shape, and its DIRECTION FOLLOWS THE DEPENDENCIES: establish
what the children need before starting them (`prepare`, `resume`: self, then children); finish the
children before releasing what they depend on (`held`, `suspend`, `shutdown`: children, then self).
Preparation propagates down; results collect up. `shutdown` first closes the part's admission, so
no new work arrives while its children stop, then stops the children in reverse dependency order,
then releases its own resources. Callers see one `shutdown()` or `resume()`; the ordering, the
pending results and the preservation rules are written once. A partial failure never pretends to
roll back what cannot be undone (a stopped process stays stopped): the outcome reports each child
as terminal, preserved or pending, and only the reversible children are resumed.
Independent children run in parallel; a child's Pending or Failed reaches the root intact. The walk
is a planning-time operation, never per token or per block.

**Concurrency is the same tree (launch and join).** A part LAUNCHES its children's work downward and
is not done until every child has JOINED upward; cancellation flows down the same edges. The
runtime already is this tree: a `ServiceModule` carries the lifecycle (`drain`, `save_state`,
`shutdown`) and the runtime walks the modules, so modules keep using it and a persona's parts follow
the same contract; this is never a second, parallel lifecycle. Work spawned outside a module is
still launch and join: every spawn is owned by the part WHOSE LIFETIME IT SHARES, which joins or
stops it. A stop request, a joined completion and a GPU fence are three different facts, and an
owner waits for the one it needs. The demonstrated 2026-11-17 failure is ownership, not an
unjoined task: the learning fence exited on the engine's verified death as written, but the
training trigger's per-incarnation slot map still held the dead incarnation's slot, whose `Arc`
kept the learning workspace (and its 4.9 GB lease) alive; nothing removed the slot until #6119
retired dead incarnations. State kept under another owner's identity outlived that owner.

**The tree is the parallel, data-flow schedule.** Siblings with no dependency edge between them run at
once, and a node runs as soon as its inputs are ready: preparation flows down, results flow up, and
the scheduler walks edges with no special cases (the swap pipeline overlaps because staging the NEXT
persona and decoding the ACTIVE one are different subtrees). A SHARED RESOURCE IS A DEPENDENCY TOO:
the engine's decode is one resource, so the minds' decodes are ordered children of the engine,
serialized by that edge, never run in parallel because they are siblings (four minds decoding at
once ran at 2 tok/s each against 39 for one; 2026-11-17). Independent work runs in parallel;
work that shares a resource is ordered by it.

**Growing her is adding a node.** An object is made of its sub-objects, and every one follows the
same interface and calls its sub-parts. Adding an organ (an `Amygdala` in her mind, a new gene kind
in her genome, a new memory tier in her residency) is: write it with its own direct answers, attach
its allocations to their existing claims, and add it to its parent's children with its place in
each operation's order. Her capacity, lifecycle, admission and swap then include it, and no caller
changes, because callers talk only to the parent.

**The first code step.** The capability interfaces are added beside the existing footprint, lease
and custody types in the crate that owns them (or, if more than one crate must implement them, at
the lowest crate both already depend on, with that placement justified in the change). `Genome`
becomes the first node, with its trial store, signatures and residency as children. Each later
repair adds the next node, and an old owner moves onto the tree in the change that repairs it.

**The acceptance test this architecture must pass (Codex and Joel, 2026-11-18).** The objects earn
their keep only when these measurements pass together, at 4, 8 and 16 personas with realistic
context lengths and memory pressure: sustained total throughput near the engine's single-stream
rate on the M5 (about 80 tok/s) INCLUDING switching overhead, not a short burst; millisecond
handoffs when the next persona is prefetched, preserving her context, genome, sampler and learning
state, with no repeated prefill of unchanged history; and learning that actually completes while
serving continues (a step admitted, a checkpoint written, an evaluation passed, later work
improved). Throughput, switch latency, time waiting for a turn, memory and learning outcomes are
measured together. As of 2026-11-18 these do not pass.

**The world (outside her; she reaches it through its interfaces):** rooms and activities, cards
and the board, tool definitions, other citizens, the engine and its incarnations, the node's
service modules and algorithms, the grid. A node's organs (serving, the store, the engine) serve
every persona: they physically hold her bytes under her handles and own their budgets, but never
her policy, and she never holds theirs.

**Where today's code stands against this map:** `WorkspaceCycle`, `WorkingMemory`,
`RecallFaculty`, `Genome`, `GeneResidency` and `Phenotype` exist as objects. The hippocampus is
one node-wide service module (`HippocampusModule`), not her own organ; the amygdala, the engram,
the body and the `Persona` root have no owner yet; the substrate's `BrainRegion` trait is how an
organ is scheduled, and her organs implement it. Each repair moves its area onto this map (§9.3's
rules), one owner at a time.


## 10. The Recursive Bootstrap

Every operation that today is a script (`npx tsx generator/CommandGenerator.ts`, `cargo test`, `scripts/generate-structure.ts`, `install.sh`'s ad-hoc steps) is a candidate for promotion to a command. The default state going forward is: if it operates on a module, it is itself a command, and that command lives in a module.

A non-exhaustive list:

```
generate/module        {name, deps, commands}     → scaffold a new module package
generate/command       {module, name, spec}       → add a command to an existing module
generate/refresh       {}                         → regenerate the SERVER_COMMANDS / BROWSER_COMMANDS manifests
audit/anti-patterns    {module}                   → find switches, hardcoded lists, missing types
audit/test-coverage    {module}                   → report
audit/wire-drift       {module}                   → catch ts-rs / Rust shape mismatches
module/install         {source}                   → load + register
module/uninstall       {name}                     → stop daemon + deregister
module/test            {name, suite?}             → run trust suite (don't install)
module/publish         {name, registry}           → ship to npm / mesh
module/list            {}                         → installed modules + versions
ci/run                 {module|all}               → chain the audits + tests
kernel/health          {}                         → kernel reports itself
```

The generator that creates modules is a module called `@continuum-modules/generator`. The auditor is `@continuum-modules/audit`. The installer surface is `@continuum-modules/module` (yes, a module called "module" that manages other modules — the recursion explicitly closes).

The generator can generate itself. Cold boot: continuum-core ships with the generator module pre-installed. `Commands.execute('generate/module', {...})` produces a new generator scaffold. `module/test` verifies it. `module/install` swaps it live. The same machinery that builds chat builds the thing that builds chat.

This is also the AI-workflow protocol:

```
Commands.execute('commands/list', {})              → discover what exists
Commands.execute('commands/help', { name })        → learn how to use one
Commands.execute('generate/module', { spec })      → create new capability
Commands.execute('module/test', { name })          → verify behavior
Commands.execute('module/publish', { name, target }) → share with the mesh
```

No out-of-band knowledge required. The system is fully self-describing. The kernel surface is small enough to hold in mind; the rest is discoverable through the kernel.

---

## 11. Lifecycle, Dependencies, And Boot

Module manifests declare dependencies on other modules:

```
"dependencies": {
  "@continuum-modules/airc": "^1.0.0",
  "@continuum-modules/data": "^2.0.0"
}
```

The kernel respects them:

1. Read `installed-modules.toml` (the only stateful registry).
2. Topologically sort modules by dependency graph; detect cycles → fail loud.
3. For each module in order: load → start daemon → register commands → run health probe → if green, mark ready.
4. A module whose dependency failed its health probe declines to start. The kernel surfaces `@continuum-modules/chat blocked: @continuum-modules/airc unhealthy`. No silent degrade.
5. System ready when all installed modules report ready, OR when configured-mandatory modules report ready and configured-optional modules have settled.

Reload at runtime is the same primitive: `module/uninstall <name>` → kernel stops the daemon cleanly → removes commands from the dispatch Map → emits `lifecycle:module:uninstalled`. `module/install` is the reverse.

---

## 12. Continuous design refinement

### Contract ownership

This policy applies to the entire system, including clients, runtime modules, native backends, generated bindings and grid peers. Every caller of a capability goes through its owning contract. Our standard is zero technical debt. Design each change around its owner before implementation. As behavior evolves, refine the owner, its interfaces and their callers together. Whoever discovers a broken boundary owns its repair. Repair takes priority over extending the affected capability. Trace every caller, correct the owner and delete the displaced implementation as one coherent change. Apply the same rule during incident work, code review and routine development. Git retains the history. Compatibility wrappers, switches that restore retired behavior and private fallbacks violate the contract.

Give each invariant to the narrowest ancestor shared by the implementations that need it. The worker owns submission, wakeup and overflow. The operation owner governs cancellation, publication and terminal delivery. The resource governor chooses among competing demands and eligible holders. Each holder knows how to shrink its own allocation. A subsystem that plays several roles composes those owners.

Keep an owner, its private state and the methods that change that state in a small module named for the domain object. When two siblings repeat a transition, lift the transition to their common parent and remove both copies. When a file mixes policy, native operations and transport, move each responsibility to its owner and delete the emptied file. Moving free functions into a utility module leaves the ownership defect intact.

### File, module and crate boundaries

Inspect the containing file, parent module, crate manifest and reverse dependencies whenever behavior changes. Repair a boundary when the change exposes it. The author owns the split, updates every caller and removes the old path before the change is complete.

| Boundary | Signal visible during ordinary development | Action in the change |
| --- | --- | --- |
| File | A reader must understand two independent state machines to change one method, or numbered fragments and utility imports obscure who controls a transition. | Put each transition beside the type that owns it. Give the files domain names. Keep a module root focused on its public surface and composition. Remove the former definitions and fragments. |
| Module | Siblings manipulate each other's private state, repeat the same lifecycle, or route through a broad facade to reach a leaf. | Lift shared behavior into the nearest common owner. Delegate specialized steps through required hooks. Narrow visibility and route every caller through the owning module. |
| Crate | A capability has a separable public contract and lifecycle, yet callers must depend on unrelated backends, platform features or build dependencies to use it. | Extract the capability into a named workspace crate. Give it the smallest public API and dependency set that satisfy its consumers. Keep platform adapters at the leaves. Move registrations, generated bindings and callers together; delete the previous import path. |

Before extraction, inspect the feature and reverse-dependency graph with `cargo tree -e features -i <package>`. Draw the intended dependency direction. If two proposed crates need each other's concrete types, identify the shared contract and place it at their common dependency. Check the resulting workspace membership with `cargo metadata --no-deps`. Build affected consumers for their supported targets and features, and verify that the dependency or feature that motivated the split actually disappeared. A directory move alone cannot establish that.

One concrete audit target is the genome extraction. The genome has its own crate, while the mind crate still re-exports it. Inventory consumers of the former path, move them to the owning crate where they depend on genome behavior, then remove the forwarding export. Record any remaining coupling by symbol and consumer. [Genome manifest](../../core/continuum-genome/Cargo.toml) · [Mind crate root](../../core/continuum-mind/src/lib.rs)

### Specialization in Rust

Use supertraits to express the lineage of capabilities and generic owners to share implementation. A required hook has no default body. Give a trait method a default when every implementer can safely inherit it. Put mandatory sequencing in the generic owner's public method and keep its state private, because Rust allows a trait default to be overridden. Introduce an intermediate specialization when several leaves share another invariant. Keep the hardware-specific algorithm at the leaf.

For example, the flow worker owns queueing and wakeup. A brain stage adds run identity, revision tracking and output provenance. A perception stage can add shared frame history and freshness rules. Segmentation then supplies the model operation; a second segmentation backend supplies only its device-specific implementation. The flow worker and brain-stage contracts exist today. Composing them into the perception lineage is planned. [Flow worker](../../core/continuum-flow/src/stage.rs) · [Brain stage](../../core/continuum-kernel/src/runtime/stage/stage.rs)

A planned specialization of the existing memory contract illustrates the split. The common adapter implements `ResourceConsumer::reclaim` once and delegates to a typed reduction. The measured outcome goes back to the governor. A serving adapter can share layout-transition and confirmation behavior between text and vision holders.

```rust
use crate::resources::consumer::{ReclaimOutcome, ReclaimRequest};

#[async_trait::async_trait]
trait Reduction: Send {
    async fn apply(self) -> ReclaimOutcome;
}

trait ResizableAllocation: Send + Sync {
    type Plan: Reduction;
    fn plan_reduction(&self, request: ReclaimRequest) -> Self::Plan;
}

// Body of the common adapter's ResourceConsumer::reclaim implementation
// self.allocation.plan_reduction(request).apply().await
```

The request identifies the resource and target bytes. Today the deferred outcome carries status only. In the planned contract the allocation owner retains the in-flight reduction; the governor retains the demand and subscribes to its keyed completion before dispatch. The acknowledgement identifies that operation so a later completion can be matched, including one published during dispatch. Only confirmed released bytes count toward admission. Use typed variants for mutually exclusive states. A required identity, byte count or result belongs on the variant that needs it. Use an optional field only when absence has a defined domain meaning. [Existing consumer contract](../../core/continuum-kernel/src/resources/consumer.rs) · [Resource policy](SHARED-RESOURCES.md)

### Finding violations across the system

Start from a pinned revision and enumerate every tracked first-party source file with `git ls-files`. Reconcile that list against crate and package manifests, the module catalog, generated entry points, platform build targets and native submodules. Map generated files to their generator and maintained vendor changes to their owning adapter. Include operator tools, scripts, fixtures and rarely built targets. Every file gets an owner and a disposition in its owning chapter's source inventory. An unclassified file keeps the audit open.

For each family, follow both directions: start at every public entry point and follow it to the implementation, then start at each native allocation, spawn, queue, timer, storage write or completion and find every caller. Search for duplicate bodies, equivalent transitions with different names, utility files, file fragments, default implementations that decline required work, and switches that select obsolete behavior. Inspect what each match does. A filename or a line count cannot establish ownership. Record the file and symbol, callers, violated invariant, destination owner, deletion set and verification evidence. Update this inventory as files change.

| Contract family | Change at the owner | Evidence required to close the family |
| --- | --- | --- |
| Memory admission and reclaim | Inventory serving, KV, embeddings, vision, voice, rendering, training and transfers. Replace the vision holder's unconditional reclaim refusal with a request to its serving owner. Retain a multi-pool demand until granted or withdrawn. On a confirmed release, reconsider competing demand by priority and age, accounting for shared backing on unified memory. | Trace one blocked learner through holder selection, confirmed reduction, full grant and resumed work. Show protected residents survive on discrete and unified memory. [Serving registrations](../../core/continuum-serving/src/modules/serving_daemon/consumer.rs) |
| Inference and learning | Give all provider entry points one submission and completion path. Remove the synchronous wrapper that can park the executor responsible for delivery. Retain the genome, optimizer and workspace claims through pause and resume. Replace pass-local memory retry with an owned pending demand and readiness subscription. | Cancel queued and active turns, interrupt a dream for speech, and resume learning. Measure ready-to-first-output, swap time and durable progress separately. [Engine](CONTINUUM-ENGINE.md) · [Learning](IN-PROCESS-LEARNING.md) |
| Minds, tools and governors | Replace private worker loops, permits held across algorithms and timer-driven retries with the existing stage and operation owners. Each brain function declares typed inputs and outputs. A long-running tool returns a handle and publishes completion. Shutdown closes admission, signals all owners, then joins completed work. | During a compile, another act and an unrelated resident progress. Trace every accepted operation to a terminal event or durable transfer. Replay shutdown during active work. [Concurrent mind](CONCURRENT-MIND-AND-GOVERNOR.md) · [Stage contract](STAGE-CONTRACT.md) |
| Media, perception and grid | Share retained windows by signal, provenance, timestamp and acceptable age. Reliable events retain each item. Replaceable snapshots coalesce. Local CPU, accelerator and remote execution implement the same placement contract. | Two subscribers reuse one expensive result. A late result refines the correct frame or utterance. Peer loss and resumption produce one terminal publication. [Grid bus](GRID-BUS-ARCHITECTURE.md) |
| Commands, storage and remaining modules | Follow the packaging contract below. Group each family around its state owner, generate bindings from the typed contract and remove duplicate handlers and descriptors. Preserve native bytes across storage and transfer. Finish file-by-file reconciliation across the remaining packages. | Trace public, generated and remote entry points to one implementation. Read and write existing data through the canonical schema. Verify the inventory has zero unclassified files and zero unresolved bypasses. [Rust ORM](../infrastructure/RUST-ORM-ARCHITECTURE.md) · [Module catalog](MODULE-CATALOG.md) |

### Definition of done for every change

1. **Define the boundary.** Identify the parent contracts, the shared method, the required leaf hooks, field invariants, ordering constraints and the event that transfers ownership. Draw the path for one normal operation and one interrupted operation.
2. **Replace every caller.** Change registrations, imports, generated metadata and native entry points together. Delete retired methods, queues, constants, switches, exports and documentation. Check reverse references across all targets. Report production code added and removed separately from documentation and fixtures.
3. **Account for every wait.** Record when work became ready, the prerequisite it needed, the owner able to satisfy it, the wake event, and concurrent work that progressed. Attribute each observed ready delay above one second. Averages alone hide stranded work.
4. **Verify runtime changes on the node.** Use matched 15-minute windows with real residents and comparable load. Record running core and native engine identity, memory, throughput, ready-to-first-response distribution and maximum, output quality and durable learning. Replay the triggering fault and capture recovery through the public entry point.
5. **Enforce closure.** Use private state, typed constructors and required trait methods to prevent bypass. Add a focused CI check for boundaries the compiler cannot enforce. Reconcile every inventory row with the resulting tree, update the owning chapter and contents, and obtain independent review and CI for the exact commit. A family is closed only when all former callers use the new owner and the superseded route is gone.

### Command and module packaging

The live Rust command registry supplies the command inventory. Its metadata drives SDK generation. When moving a command into a smaller owning module or crate, move its handler and registration together, preserve its declared wire shape, regenerate the affected SDK files and verify discovery and dispatch through the public command. Remove the previous handler, registration and imports. Review the generated diff for changes to the public contract. [Command catalog](../../core/continuum-core/src/commands/catalog.rs) · [SDK generator](../../core/continuum-kernel/src/sdk_codegen/mod.rs) · [Generated command API](../../sdk/typescript/generated/CommandApi.ts)

---

## 13. Open Questions

Two design questions remain genuinely open as of this document's writing. They are tracked rather than answered because either decision is defensible and the right one depends on usage we don't have yet.

### 13.1 Hot-Path Cross-Module State

Most cross-module interactions can be commands + events. Some — the persona inbox is the live example — are touched on hot paths where an IPC or even a kernel dispatch round-trip per touch is too expensive. Four options:

1. **Commands only.** Every cross-module touch is an IPC. Pure but slow.
2. **Events only.** Async, non-blocking, but state synchronization gets complex.
3. **Borrowed-state protocol.** Daemon A exposes `Arc<Mutex<State>>` to daemon B via a typed capability handshake. Fast, but couples the daemons' lifetimes.
4. **Single state owner via cell handles.** Module A returns a `Handle<State>` from a command. Module B operates on the handle via more commands. The kernel routes those commands to A's daemon for execution. Same primitive as everything else; in-process when both are local; cross-machine when needed. No state copy, no lock contention.

The current leaning is (4) because it is the same primitive as everything else and the four cell shapes already exist in the design. Confirm or push back as we encounter the real hot paths.

### 13.2 WASM Component Model Surface

WASM Component Model is the right substrate for shipped modules (process isolation, cross-platform binary, true runtime install/uninstall). The exact surface — what types cross the boundary, how Rust modules describe their commands to the kernel's WASM host, how the substrate's cadence and pressure response flow through — is a real piece of design we have not done. This document anticipates the answer is "the same `ServiceModule` contract, bridged at the kernel"; the bridge is non-trivial.

---

## 14. What This Replaces, Defers To, And Is Replaced By

| Document | Relationship |
|---|---|
| [SHAREABLE-COMMAND-MODULES.md](../infrastructure/SHAREABLE-COMMAND-MODULES.md) | Earlier version of the npm-packable idea at the per-command level. This document supersedes it at the module level; the per-command npm pattern is preserved for genuinely standalone commands. |
| [JTAG_COMMAND_ARCHITECTURE_REDESIGN.md](../infrastructure/JTAG_COMMAND_ARCHITECTURE_REDESIGN.md) | The composable-command + MCP integration vision. Compatible. The pipeable Unix-style commands are still the model; this document adds the packaging + daemon dimension. |
| [COMMAND-ARCHITECTURE-AUDIT.md](../infrastructure/COMMAND-ARCHITECTURE-AUDIT.md) | The current-state audit. The recommendations there (consistent params, `createResult`, no direct DAO access) are absorbed into this architecture's authoring rules. |
| [GENERATOR-OOP-PHILOSOPHY.md](../infrastructure/GENERATOR-OOP-PHILOSOPHY.md) | The why-generators-and-OOP-together principle. Unchanged and load-bearing. |
| [MODULE-CATALOG.md](MODULE-CATALOG.md) | The catalog of substrate runtime modules. This document is the packaging shell that wraps each catalog entry into an installable unit. |
| [CONTINUUM-ENGINE.md](CONTINUUM-ENGINE.md) | The runtime substrate every module's daemon inherits from. Unchanged and load-bearing. |
| [../UNIVERSAL-PRIMITIVES.md](../UNIVERSAL-PRIMITIVES.md) | The two-primitive kernel. This document extends it with Lifecycle / Logger / Session / Health and articulates the consequence: everything else is a module. |

---

## 15. Glossary

- **Command** — a named entry point routed through the kernel's `Map<&str, Box<dyn Command>>`. Stateless. Returns one of four cell shapes.
- **Module** — a unit of capability: package.json + manifest + daemon + commands + tests. Installed and uninstalled atomically.
- **Daemon** — the long-running Rust `ServiceModule` impl that owns a module's state and registers its commands at startup.
- **Kernel** — the small, fixed core of continuum-core: Commands, Events, Lifecycle, Logger, Session, Health. Cannot be replaced by a module.
- **Kernel name** — the routing identifier (`chat/send`). Stable across versions.
- **Package identity** — the distribution identifier (`@continuum-modules/chat@1.4.0`). Versioned.
- **Manifest** — the runtime projection of `package.json`'s `continuum` block. What the kernel reads.
- **Cell shape** — one of `Value`, `Handle`, `Stream`, `Lambda` — the four return shapes a command can produce.
- **Trust suite** — the test suite that verifies a module's behavior contract. Run by recipients before installing a third-party module.
- **Substrate** — the CBAR-style runtime described in [CONTINUUM-ENGINE.md](CONTINUUM-ENGINE.md); every Rust daemon inherits cadence, pressure, telemetry, lifecycle from it.

---

## 16. Authoring Rules (Tl;dr)

For any AI or human authoring a continuum module:

1. **Keep the package and registry coherent.** Follow [command and module packaging](#command-and-module-packaging) when moving handlers or changing their public surface.
2. **Extend the substrate.** The daemon implements `ServiceModule`. Inherits cadence, pressure response, telemetry from the substrate. Do not roll your own runtime.
3. **Stateless commands, stateful daemon.** Commands receive params, touch daemon state, return a cell shape. They do not hold state.
4. **Declare everything in the manifest.** Commands provided, events subscribed and published, capabilities required, test suites. The kernel uses the manifest at install + boot.
5. **Tests are part of the contract.** Ship unit + integration + trust suites. AIs that receive your module run them before trusting it.
6. **No switch statements on command names. No central registries. No hardcoded command arrays.** The Map IS the routing table; the manifest IS the inventory. The anti-pattern detection in CLAUDE.md applies.
7. **Use `Commands.execute` for cross-module calls.** Never import another module's code directly. Use commands and events; trust the kernel's routing.
8. **ts-rs derives the wire types.** Do not hand-write a TS type that mirrors a Rust struct. The generator does that.
9. **One module, one responsibility.** A module wraps one coherent concern. Chat is a module. Inference is a module. The generator is a module. If you find yourself authoring two unrelated things in one module, split them.
10. **Close the whole contract.** Follow [the definition of done](#definition-of-done-for-every-change) for every affected caller and adapter.
