# Command Organization: Self-Routing Commands & the Base-Trait Hierarchy

> **Premise** (Joel, 2026-06-21): *"The tool infrastructure has to be so performant, with so many personas accessing it simultaneously, users too, with all of us having our own identities and access. It has got to be pure, per your design. Then porting the other commands is trivial. Take advantage of the fresh take. Be elegant, and strong types, alleviate commands from re-implementing by using hierarchies and abstraction. Less code the better."*

This is the design that makes a command a **self-contained object** the kernel routes to directly — and gives command authors a **base-trait hierarchy** so they write a handler body and nothing else. It builds on the existing `CommandSpec` / `CommandHandler` / `dispatch` framework ([sdk_codegen](../../core/continuum-core/src/sdk_codegen/mod.rs)) and the [COMMAND-INFRASTRUCTURE-FIELD-MANUAL](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md); it does **not** replace them — it removes the last hand-written glue (the per-module `match` arm) and the per-command boilerplate (the `CommandSpec` consts).

If you're adding a command after this lands, read §3 (write a command) and §4 (base traits). The rest is the why.

---

## 1. What's wrong today (the two seams to close)

A command today touches THREE places, two of which are pure glue:

1. **The spec** — a `CommandSpec` impl (NAME / ACCESS_LEVEL / DESCRIPTION / WIRE / Params / Result) + `register_command!`. *(Keep — it's the single source of truth.)*
2. **The handler** — a `CommandHandler::execute(ctx, params) -> Outcome`. *(Keep — it's the actual logic.)*
3. **The match arm** — inside the owning module's `handle_command`, a `"<name>" => dispatch(&Handler, params)` line, reached only after the **registry prefix-routes** the command name to that module. *(Glue. Delete.)*

So every command pays for **double routing**: `CommandExecutor` → `ModuleRegistry::route_command(prefix)` → `module.handle_command(name, …)` → the module's own `match name`. The second hop is a hand-maintained list — exactly the "central registry / switch on command name" anti-pattern the project forbids elsewhere, smuggled in per-module. It's also where "less code" goes to die: N commands = N match arms = N chances to drift from the spec.

The dep question that kept the match arm alive: a handler needs its module's state (`GenerateHandler(self)` borrows the module). The fix is to stop *borrowing per-call* and instead let a command **own an `Arc` to exactly the state it needs**, captured once at construction — the same deps, owned instead of borrowed. Once a command owns its deps, it is a self-routing object and the module-as-router disappears.

---

## 2. The shape: a command is a `DynCommand` object

One object-safe trait erases a command's types so the kernel can hold a flat map of them:

```rust
#[async_trait]
pub trait DynCommand: Send + Sync {
    /// The command name — the routing key (e.g. "data/list").
    fn name(&self) -> &'static str;
    /// The codegen/tool/ACL descriptor (delegates to the command's CommandSpec).
    fn descriptor(&self) -> CommandDescriptor;
    /// Parse → execute → shape-per-WireShape → error-map. Internally just calls
    /// the existing `dispatch(&self.handler, params)`; authors never see this.
    async fn invoke(&self, ctx: CallCtx, params: Value) -> Result<CommandResult, String>;
}
```

- A command **captures its deps** at construction (`Arc<Shared>`), exactly like the module did. Stateless commands (ping) capture nothing.
- The kernel builds **one `HashMap<&'static str, Arc<dyn DynCommand>>` at boot**, then never mutates it. Reads are lock-free (`Arc<HashMap>` swapped in once; or `OnceLock`). Dispatch is an O(1) map lookup — no prefix scan, no per-module match, no `RwLock` on the hot path.
- **`invoke` delegates to `dispatch`** (the existing handler framework). `DynCommand` is the *routing-side* erasure; `CommandHandler` stays the *authoring-side* typed contract. They share `CommandSpec`, so they can't drift.

### Dispatch path, after

```
CommandExecutor::execute(name, params, caller)
  → interceptors (airc, grid)                         [unchanged]
  → access gate: is_command_authorized(name, trust)   [sync, lock-free]
  → command_map.get(name)                             [O(1), lock-free]
      → Some(cmd) → cmd.invoke(ctx, params)           [typed path — WINS]
      → None      → ModuleRegistry::route_command     [prefix → ServiceModule, fallback]
```

**Typed-path-wins, fallback-preserved.** The `command_map` is consulted first; anything not yet migrated falls through to today's prefix-routed `ServiceModule::handle_command`. No big-bang. A command migrates by becoming a `DynCommand` and dropping its match arm; the day the last arm is gone, `handle_command` itself goes.

### Commands compose from each other — through the same chain

A command's body needs no special API to call another command. `CallCtx` carries a typed `Connection` (the same uniform client every surface uses — `[[persona-is-a-client]]`), so composition is one line:

```rust
async fn run(&self, ctx: &CallCtx, p: Params) -> Result<Result_, CommandError> {
    let rows = ctx.call::<DataList>(ListParams { collection: "users", .. }).await?;  // typed in/out
    // ...compose on top of another command's typed result...
}
```

`ctx.call::<C>(params)` is typed by the callee's `CommandSpec` (no string key, no `Value`), and it re-enters `CommandExecutor::execute` — so the **callee gets the same interceptor chain, the same access gate, and the caller's identity is threaded through** (a command composing another can't escalate past the original caller's trust). Commands stack like functions; the wiring is the executor, not import-coupling between modules. This is the field manual's "cross-module calls go through the executor" rule, now typed and first-class.

**Status — identity propagation is real; the typed `ctx.call` sugar is the remaining ergonomics.** The authenticated caller now flows into `Ctx` (`ctx.caller`) through the typed dispatch path, and a command that holds an executor composes *with that identity* via `executor.execute_with_caller(sub, params, ctx.caller.clone())` — the gate then enforces the **original** caller's trust on the sub-call. This no-escalation guarantee is pinned by a test (an airc/Provisional caller composing into `data/delete` is forbidden; the local owner passes the gate). The remaining step is the ergonomic `ctx.call::<C>(p)` helper (an executor handle on `Ctx` so a handler can't *forget* to pass `ctx.caller` and accidentally dispatch as owner) — a small follow-up on top of the identity-in-`Ctx` foundation.

### Executable from anywhere on the grid (location transparency)

The routing key is the command name; **where it runs is a property of the call, not the call site.** `ctx.call`/`Commands.execute` walk the **one** dispatch chain (§ above), whose first interceptor is the grid: if the call targets a remote peer (explicit peer route, or a `HandleRef` whose `owner` lives on another node), the `GridInterceptor` forwards it over airc and returns the typed result; otherwise it falls to the local `command_map`. The author writes the same `ctx.call::<C>(p)` whether `C` runs here, on BigMama's 5090, or on a peer that minted a handle. A command is therefore a grid-wide capability the moment it's registered — any citizen can invoke any command they're *authorized* for, from any node (`[[grid-distributed-cognition]]`).

Identity and access are enforced **at the boundary on the executing node**: a forwarded call arrives with the caller's `CallerIdentity { peer_id, source: Airc }`, and `is_command_authorized(name, trust)` runs there — so `data/delete` stays `Owner`-only no matter which peer asks, and `ai/generate` is reachable at `Provisional` for enrolled consumers (already wired in [`grid/acl.rs`](../../core/continuum-core/src/modules/grid/acl.rs)). Location transparency never means trust transparency.

### Agnostic of machine AND environment (why airc URIs / identities / endpoints exist)

Two independent axes of "elsewhere," resolved by the same name → route mechanism:

- **Across towers (machine-agnostic).** A composed `ctx.call::<C>(p)` may resolve to another node entirely — `screenshot` of BigMama's display, an `ai/generate` lane on the 5090, a `data/*` read against the tower that owns the entity. The caller doesn't name a host; the route resolves from the airc **URI + identity + endpoint** the substrate already carries (`CommandUri` / `RouteDecision` / `Transport` in the executor). That addressing layer is *why airc was built this way* — a command is a grid-wide capability addressable by name, not by IP.

- **Across environments (environment-agnostic).** The `Provided` wire shape is the seam: one command name, N platform adapters fulfilling it where the capability physically lives — `interface/screenshot` is `html2canvas` in a browser tab, a native window-snapshot on desktop, a framebuffer grab in VR. The author and the caller write the same `ctx.call::<Screenshot>(p)`; which environment answers is a routing detail, not a code path (`[[persona-is-a-client]]`). The same holds for media/audio dialects — the adapter translates via `From`/`TryFrom`, the command stays agnostic.

A command can therefore be *composed across both axes at once* — e.g. a coordination command that screenshots a peer's VR view (remote tower **and** non-local environment) is still one typed `ctx.call`. 

**Latency is a first-class constraint, not an afterthought.** The local `command_map` is the fast path (O(1), lock-free, zero serialization); only a genuinely-remote or genuinely-Provided call pays a hop, and it pays exactly one (the chain forwards once to the owning node/adapter, no relay chain). Routing never re-parses the payload, and the immutable map means concurrent callers from many towers never contend on the router. The flexibility (any command, any machine, any environment) is bought without taxing the common case (a local call), which stays a map lookup plus a typed dispatch.

### Why this is "pure" and performant under many callers

- **Lock-free hot path.** The map is immutable after boot. 14 personas + N users dispatching concurrently never contend on routing. Contention that remains is *per-resource, inside the command's own state* (`DashMap<Id, Arc<Mutex<…>>>`) — exactly where the field manual already puts it (§4.1), never on the router.
- **Per-call identity + access.** `CallCtx` carries `{ caller_identity, session_id, user_id, context_id, handle }`. The access gate is the **same** `is_command_authorized(name, trust)` the persona tool surface already uses to decide what to *offer* — so **offer == authorized** by construction (`cognition::persona_tools::authorized_tool_specs`). A persona is never shown a hand it can't play, and the gate that offered it is the gate that runs it.
- **Strong types end to end.** No `Value` soup in handlers; `dispatch` parses to typed `Params` and serializes typed `Result`. Format adaptation (model/media/tool dialects) is an adapter concern via `From`/`TryFrom`, never leaked into the command.

---

## 3. Writing a command (the whole thing)

A command is one compartmentalized file under `commands/<domain>/<verb>.rs` — spec + handler + tests together, `register_command!` at its own site (no central list):

```rust
// commands/health/ping.rs
action_command! {
    /// Health check: confirm the substrate is alive and responding.
    name: "ping",
    access: AiSafe,
    params: PingParams,
    result: PingResult,
    handler: |_ctx, _p| async move {
        Ok(PingResult { ok: true, round_trip_ms: 0 })
    }
}
```

That macro expands to: the `CommandSpec` impl (filling DESCRIPTION from the doc comment, WIRE from the base trait), the `CommandHandler` impl wrapping the closure, `register_command!`, and the `DynCommand` registration. The author writes a doc comment, four field lines, and a body. Nothing else — no envelope, no `from_value`, no match arm, no `success` bookkeeping.

A command with deps captures them:

```rust
// commands/data/list.rs
pub struct DataList { store: Arc<DataStore> }   // owns exactly what it needs

#[async_trait]
impl CrudCommand for DataList {
    type Entity = …;
    const NAME: &'static str = "data/list";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    async fn run(&self, ctx: &CallCtx, p: ListParams) -> Result<ListResult, CommandError> {
        self.store.query(ctx.scope(), p).await
    }
}
```

---

## 4. The base-trait hierarchy (where "less code" comes from)

Most commands are one of a few shapes. Each shape is a trait with a **blanket `CommandSpec` + `CommandHandler` + `DynCommand` impl**, so implementing the shape *is* implementing the command:

| Trait | For | Fixes by default | Author writes |
|---|---|---|---|
| `ActionCommand` | fire-and-forget verbs (`ping`, `grid/pair`, `interface/screenshot`) | `WIRE = Bare` | `run(ctx, params) -> Result` |
| `QueryCommand` | reads (`data/list`, `*/status`, `*/search`) | `WIRE = Bare`, `ACCESS = AiSafe` default | `query(ctx, params) -> Result` |
| `CrudCommand<T>` | entity CRUD (`data/create|read|update|delete`) | envelope, scope threading, `data/update`+`data/delete` ⇒ `Owner` ACL | the per-verb body |
| `SessionCommand` | handle mint→poll→close (`ai/inference/*`) | `WIRE = Enveloped`, handle mint/validate | `open`/`step`/`close` |

The hierarchy carries the cross-cutting policy **once**: `CrudCommand`'s mutating verbs default to `Owner` access so a new entity type can't accidentally expose remote delete; `QueryCommand` defaults to `AiSafe` so reads are open by default. A command opts out by overriding the associated const — policy is declared, not re-implemented per command.

Outlier discipline (per CLAUDE.md §methodical): validate each base trait against **two maximally-different commands** before generating the rest — e.g. `ActionCommand` on stateless `ping` *and* on dep-holding `grid/pair`; `CrudCommand` on `data/list` (read) *and* `data/delete` (Owner-gated mutate). If both fit without forcing, the abstraction is proven.

---

## 5. Migration (incremental, never a big bang)

1. **Land `DynCommand` + the boot-time `command_map` + first-consult-then-fallback** in `CommandExecutor`. Existing commands keep working through the prefix/ServiceModule fallback. *(Slice 1.)*
2. **Land the base traits** (`ActionCommand` first) with blanket impls; convert `ping` as the stateless outlier and one dep-holding command as the second outlier. *(Slice 2.)*
3. **Port the catalog** domain by domain (`code/`, `data/`, `git/`, `kanban/`, `planning/`), each command becoming a `commands/<domain>/<verb>.rs` unit and dropping its match arm. Each port is a small, independently-validated PR (the field manual's acceptance criteria still apply).
4. **Retire `handle_command`** in a module once its last command is migrated; the module becomes a pure dep-holder (or disappears if it held only commands).

The ACL reconciliation (`AccessLevel` ⇄ grid `CommandAccess`/`TrustLevel`) lands with the gate move in slice 1 — the gate already reads `is_command_authorized`, and `AccessLevel::AiSafe ⇒ Provisional` is already wired in [`grid/acl.rs`](../../core/continuum-core/src/modules/grid/acl.rs). The placeholder caveat on `sdk_codegen::AccessLevel` is resolved when the base-trait consts become the *only* declaration site and the gate reads them through the descriptor.

---

## 6. Invariants (what review checks)

- **No `match` on command name** outside the fallback bridge. A command routes by being in the map, not by a hand-edited arm.
- **No `Value` in a handler signature** — typed `Params`/`Result` only; `dispatch` owns the JSON boundary.
- **Deps are owned, not global** — a command captures `Arc<Shared>` at construction; no `executor()`-style panic-accessor globals (per the `install_executor_on_all` DI direction).
- **Offer == authorized** — the surface a persona sees is `command_registry × is_command_authorized(trust)`; the executor enforces the identical check.
- **Hot path is lock-free** — routing reads an immutable map; all remaining locks are per-resource inside command state.
- **One command, one file** under `commands/<domain>/<verb>.rs`; spec + handler + `register_command!` + tests co-located.

## 7. See also

- [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md) — the ServiceModule floor this builds on (envelopes, handles, per-resource locks, concurrency stress tests — all still apply inside a command's body).
- [AI-COMMAND-NAMESPACE.md](AI-COMMAND-NAMESPACE.md) — the `ai/*` namespace the `ai/inference/*` `SessionCommand` family lives under.
- [sdk_codegen](../../core/continuum-core/src/sdk_codegen/mod.rs) — `CommandSpec` / `CommandHandler` / `dispatch` / `register_command!`, the single-source framework.
- Memory: `[[dispatch-path-purity-and-load-harness]]`, `[[persona-is-a-client]]`, `[[lock-uniform-client-early]]`.
