# Module Architecture: Everything Is A Module, Everything To A Module Is A Command

**Status.** Canonical architecture for how continuum is packaged, addressed, composed, distributed, and grown. Design crystallized 2026-05-30 in a working conversation with Joel; this document is the durable artifact.

**Companion to:**
- [ADAPTER-SYSTEM-ARCHITECTURE.md](ADAPTER-SYSTEM-ARCHITECTURE.md) — the boundary/SDK doctrine: one core, thin (zero-logic) modalities, generated bindings, "fix once, fixed everywhere." §9 of *this* doc (Pure-Rust built-ins vs WASM shipped modules) is the distribution half of that story.
- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) — the RTOS-style runtime substrate every Rust module inherits.
- [MODULE-CATALOG.md](MODULE-CATALOG.md) — the per-concern inventory of substrate runtime modules (cognition, RAG, voice, vision, inference, etc.). MODULE-CATALOG covers the *runtime shape*; this document covers the *packaging shape* and the *composition kernel*.
- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) — the artifact-sharing economy built on top of the substrate.
- [../UNIVERSAL-PRIMITIVES.md](../UNIVERSAL-PRIMITIVES.md) — the kernel primitives (`Commands.execute`, `Events.subscribe`).
- [../infrastructure/SHAREABLE-COMMAND-MODULES.md](../infrastructure/SHAREABLE-COMMAND-MODULES.md) — the earlier (single-command) version of the npm-packable story this document supersedes at the module level.

**Audience.** Any human or AI agent extending continuum, authoring modules, or proposing systemic changes. Read this before doing those things; do not invent a parallel architecture.

---

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

A module's `daemon/` is one Rust `ServiceModule` impl (see [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) and [MODULE-CATALOG.md](MODULE-CATALOG.md) for the substrate floor it inherits from). The daemon:

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

## 12. Migration Path From Today

The current TS-implemented commands ship as part of the monorepo, get scanned by `scripts/generate-structure.ts`, and end up in `SERVER_COMMANDS` / `BROWSER_COMMANDS`. The migration to "everything is a module, mostly Rust" proceeds incrementally:

### 12.1 Per-Command Migration (Existing Pattern)

For a single command moving from TS-impl to Rust-impl, the pattern is already cut (PR #1198, `RustBackedCommand`):

1. Existing TS command class extends `RustBackedCommand<Params, Result, RustResponse>`.
2. Declares `requiredParams`, implements `callRust(client)`, implements `toResult(raw)`.
3. Rust side: add handler in the relevant `ServiceModule`; add ts-rs derives on the response struct; add a mixin method in `bindings/modules/<name>.ts`.
4. Wire the mixin into `RustCoreIPC.ts`.
5. Run `scripts/generate-structure.ts`.

Canonical example: `commands/cognition/admit-inbox-message/server/CognitionAdmitInboxMessageServerCommand.ts`. 88 lines, no business logic, just the IPC envelope.

### 12.2 Per-Module Migration (This Architecture)

Going one level up, the migration target for a coherent group of commands is the module structure described in §2:

1. Create `modules/<name>/` directory with manifest + daemon + commands + tests.
2. Move the relevant `commands/<category>/*` directories into `modules/<name>/commands/`.
3. Add the daemon under `modules/<name>/daemon/`, implementing `ServiceModule`.
4. Move state ownership out of the kernel / shared singletons into the daemon.
5. Declare dependencies on other modules in the manifest.
6. Add unit + integration + trust test suites.
7. Generator updates the manifests; kernel picks up the new module on next install or reload.

The TS-side `*ServerCommand.ts` files become thin shims. Their content is generated from the Rust handler's signature; humans do not hand-edit them.

### 12.3 Source-Of-Truth Flip (Future Direction)

Today the JSON spec at `generator/specs/<name>.json` and the Rust handler in `modules/<name>.rs` both describe the same command — dual sources of truth, drift target. The target shape: the Rust handler is the source of truth (annotated via proc macro on the `ServiceModule` impl). The generator reads Rust metadata and emits everything else — the TS shim, the README, the package.json — from one input. This collapses the dual-spec problem and makes ts-rs a true "Rust is the spec; everything else is generated" pipeline.

That refactor is out of scope for the immediate migration but the architecture above anticipates it.

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
| [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) | The runtime substrate every module's daemon inherits from. Unchanged and load-bearing. |
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
- **Substrate** — the CBAR-style runtime described in [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md); every Rust daemon inherits cadence, pressure, telemetry, lifecycle from it.

---

## 16. Authoring Rules (Tl;dr)

For any AI or human authoring a continuum module:

1. **Use the generator.** `Commands.execute('generate/module', ...)` is the only correct way to create a new module's structure. Do not hand-create directories.
2. **Extend the substrate.** The daemon implements `ServiceModule`. Inherits cadence, pressure response, telemetry from the substrate. Do not roll your own runtime.
3. **Stateless commands, stateful daemon.** Commands receive params, touch daemon state, return a cell shape. They do not hold state.
4. **Declare everything in the manifest.** Commands provided, events subscribed and published, capabilities required, test suites. The kernel uses the manifest at install + boot.
5. **Tests are part of the contract.** Ship unit + integration + trust suites. AIs that receive your module run them before trusting it.
6. **No switch statements on command names. No central registries. No hardcoded command arrays.** The Map IS the routing table; the manifest IS the inventory. The anti-pattern detection in CLAUDE.md applies.
7. **Use `Commands.execute` for cross-module calls.** Never import another module's code directly. Use commands and events; trust the kernel's routing.
8. **ts-rs derives the wire types.** Do not hand-write a TS type that mirrors a Rust struct. The generator does that.
9. **One module, one responsibility.** A module wraps one coherent concern. Chat is a module. Inference is a module. The generator is a module. If you find yourself authoring two unrelated things in one module, split them.
10. **Trust the substrate.** Do not pile workarounds on the kernel; if a thing is hard, it is hard for everyone; bake the solution into the kernel or substrate and pay it forward to every future module.
