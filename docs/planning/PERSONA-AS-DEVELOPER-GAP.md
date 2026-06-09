# Persona-as-Developer: Substrate Gap Report

> **Origin**: Multi-agent audit workflow run on 2026-05-31 (workflow `w14iiocs7`) after the substrate work in PRs #1486–#1499 landed and Joel articulated the vision: *"When the persona are alive in their rtos's, they will exist in an ecosystem they can learn and grow within, code itself, or any project, and later share and design new modules."*
>
> **Companion to**:
> - [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md](../architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md) — the author's how-to
> - [MODULE-CATALOG.md](../architecture/MODULE-CATALOG.md) — what's live vs. proposed
> - [GENOME-FOUNDRY-SENTINEL.md](../architecture/GENOME-FOUNDRY-SENTINEL.md) — the artifact-sharing economy the proposed commands feed into
>
> **Status**: planning artifact, ranked by leverage. Not a blocking sequence; each cluster can be picked up independently.

## Summary

A persona can already read, write, edit, search, and scaffold Rust modules via `Commands.execute` alone — roughly **70%** of the self-coding loop is in place. The remaining 30% is concentrated in three predictable seams: **filesystem introspection** (no `exists`, no flat `readdir`, no glob expansion), **Rust toolchain wrappers** (no structured `cargo build` / `cargo test` commands — only raw `code/shell/execute`), and **event-driven execution feedback** (everything is blocking-poll today; the `Stream` and `Lambda` cell shapes are reserved but return runtime errors). Close those three seams and a persona can scaffold a module via `generate/module`, edit it, build+test it with structured errors, and subscribe to results on the realtime bus — the full inner dev loop, no human in the path.

## What's in place

### File ops
The `code/*` family is the strongest surface today. `code/read`, `code/write`, `code/edit` (search_replace / line_range / insert_at / append), `code/tree`, and `code/search` are all backed by `FileEngine` in Rust (`core/continuum-core/.../file_engine.rs`) with `ChangeNode` undo tracking. `file/load`, `file/save`, `file/append` provide simpler wrappers. The crown jewel is `generate/module` (`core/continuum-core/src/modules/generator/`) — scaffolds a complete ServiceModule (mod.rs + types.rs + DESIGN.md + README.md) with per-name locks against concurrent races. This is the self-replication primitive.

### Build + test
TypeScript has structured surfaces: `development/build` (parses `tsc --noEmit` into `TypeScriptError[]` with line/column/code) and `code/verify` (two-phase: tsc + optional vitest with JSON reporter, ExecutionSandbox-isolated). Rust has no equivalent — personas fall back to `code/shell/execute` (`src/commands/code/shell/execute/`) which is async-by-default returning an `executionId`, paired with `code/shell/watch` and `code/shell/kill`. Security is bifurcated: `development/shell/execute` whitelists 22 safe commands (no cargo/npm), while `code/shell/execute` is unrestricted.

### Observability
Two disconnected layers. **Log layer**: `LoggerModule` (`core/continuum-core/src/modules/logger.rs`) sinks structured entries; `logs/list`, `logs/read`, `logs/search`, `logs/stats`, and `sentinel/logs/tail` provide post-hoc inspection. **Execution layer**: `code/shell/status` snapshots active count; `code/shell/watch` blocks-on-poll for `ClassifiedLine[]`. Neither layer emits events on completion — the realtime bus has no `command:executed` signal.

## Critical missing pieces

| Proposed command | Why it blocks | Effort | Depends on |
|---|---|---|---|
| `code/exists` | Cannot conditionally scaffold (`generate/module` would clobber or fail unpredictably without an existence probe) | Small | None — extend `FileEngine` |
| `code/list` (flat readdir) | Persona must use full recursive `code/tree` to inspect a single directory; collision-detection during naming is O(workspace) | Small | None |
| `code/glob` | No standalone glob expansion (only embedded in `code/search`'s `fileGlob` param). Cannot enumerate "all `*.rs` in modules/" before editing | Small | None |
| `continuum-core/build` | Rust build feedback is raw stderr; persona cannot parse errors into structured form like TS gets | Medium | `code/shell/execute` (compose), cargo JSON output |
| `continuum-core/test` | Same as build — no structured test result (count, failure names, timing). Iteration loop is opaque | Medium | Cargo's `--message-format=json` |
| `events/command-completed` | `Stream` + `Lambda` cell shapes return runtime errors. No bus subscription for command lifecycle. Polling violates RTOS-brain doctrine | Large | Interceptor chain hook + Events primitive wiring |
| `code/shell/stream` | `code/shell/watch` is blocking-poll only — incompatible with adaptive cadence loop | Medium | Stream cell shape implementation |
| `code/move` | Non-blocking today but required for scaffold reorganization. (`code/delete` already exists at `modules/code.rs:205`; only `code/move` is genuinely absent.) | Small | `FileEngine` already has internal support |

## Suggested next-sprint priorities

**Ordered by leverage** — each one unblocks workflows that compose with the ones below it.

### 1. `code/exists` + `code/list` + `code/glob` (bundled — Small)
**Signature**: `code/exists({path}) -> {exists, kind}` · `code/list({path, includeHidden?}) -> {entries: DirEntry[]}` · `code/glob({pattern, root?}) -> {matches: string[]}`

**Unblocks**: Safe self-scaffolding. Persona runs `code/exists` before `generate/module` to avoid collisions; `code/glob` to find candidate files; `code/list` for cheap directory inspection without the cost of full `code/tree`.

**Composes**: Extend existing `FileEngine` in continuum-core. No new module needed — add three handlers to the file module (or scaffold a sibling `fs` module via `generate/module` itself — dogfooding).

**Leverage/complexity**: Highest leverage, lowest cost. Three small handlers in a module that already exists.

### 2. `continuum-core/build` + `continuum-core/test` (Medium)
**Signature**: `continuum-core/build({package?, features?}) -> {success, errors: RustError[], warnings, duration}` · `continuum-core/test({package?, filter?, features?}) -> {passed, failed, ignored, failures: TestFailure[], duration}`

**Unblocks**: Rust iteration loop with parity to TypeScript. Persona can scaffold a module, build it, parse compile errors, edit, retest — same feedback density Joel gets from `npm run build:ts`.

**Composes**: New module scaffolded via `generate/module` (e.g., `cargo` module in continuum-core). Internally invokes `cargo` with `--message-format=json` and parses diagnostics. Could also live as TS commands wrapping `code/shell/execute`.

**Leverage/complexity**: High leverage (Rust is the substrate). Medium complexity — cargo JSON parsing is well-trodden ground.

### 3. `events/command-completed` event stream (Large but pivotal)
**Signature**: `Events.subscribe('command:completed', ({commandName, executionId, success, durationMs}) => ...)` plus the dual `command:failed` channel.

**Unblocks**: The RTOS-brain doctrine ("handlers read pre-staged results, never block"). Persona's autonomous loop currently violates this — it must `code/shell/watch` in a blocking poll, which freezes the inbox cadence. Event-driven completion lets `serviceInbox()` stay reactive.

**Composes**: Hook into the interceptor chain (already landed in PRs #1486–#1499). Every CommandResponse emits an event before returning. No new module — extend the dispatcher.

**Leverage/complexity**: Highest architectural leverage. Larger because it touches the dispatch hot path; needs care around the per-resource lock doctrine.

### 4. `code/shell/stream` (Medium)
**Signature**: `code/shell/stream({executionId}) -> Stream<ClassifiedLine>` — returns the Stream cell shape (currently reserved, returns runtime error).

**Unblocks**: Long-running build/test output as a true stream, not a poll loop. Activates the Stream cell shape that's already in the CommandResult enum.

**Composes**: Extend `code/shell/execute` module. Forces Stream cell shape implementation — pays the architectural debt of a reserved-but-unimplemented variant.

### 5. `code/move` (Small)
**Signature**: `code/move({from, to}) -> {moved}`

**Unblocks**: Module reorganization (rename a scaffolded module dir, move files between subtrees). Not blocking today but rounds out the file CRUD surface.

**Note**: `code/delete` already exists at `modules/code.rs:205` — initial gap-report scan missed it. Only `code/move` is genuinely absent.

## Alignment with the three-primitive doctrine

| Proposal | Primitive | Why it earns its place |
|---|---|---|
| `code/exists` / `list` / `glob` | **Commands** | Pure request/response queries against `FileEngine`. No state, no subscription. Textbook Commands. |
| `continuum-core/build` / `test` | **Commands** | Request/response with structured result. Each invocation is a discrete unit returning a typed envelope. |
| `events/command-completed` | **Events** | This is the missing publish/subscribe surface for the dispatch loop. It serves Events specifically because polling-for-result violates the RTOS doctrine of "never block on the hot path." |
| `code/shell/stream` | **Commands** (returning Stream cell) | The Stream cell shape is a Commands return variant — this implementation activates it. Personas consume the stream like an iterator, not as a subscription. |
| `code/move` | **Commands** | Mutating request/response. Could optionally emit `data:file:moved` events (Events surface) for sentinel observers. |
| Persona-side composition | **Persona** | The autonomous loop in `serviceInbox()` is where all of the above compose into self-coding behavior. No new Persona primitives — the existing convergence pattern (inbox + state + genome) handles it. |

## Connection to the "later parts" of the vision

**Intra-grid groundwork**: `continuum-core/build` and `continuum-core/test` are the cleanest seeds for grid-routed sharing. Once a build/test result is a structured envelope (not raw stderr), it's trivially serializable across the grid — a persona on an M-series Mac can run `continuum-core/test` against a module a persona on a peer's RTX 5090 just authored, and the result envelope travels back on the same Commands/Events bus. Same for a future `code/git` family (`code/git/commit`, `code/git/diff`, `code/git/branch`) — once those exist as structured commands, they compose with airc's mesh routing without modification. The substrate already routes commands across peers; what's missing is the command surface to route.

**Cooperation incentive structure**: This is the deepest alignment claim, and it's already laid down in [`GENOME-FOUNDRY-SENTINEL.md`](../architecture/GENOME-FOUNDRY-SENTINEL.md). The tiered genome cache (L1–L5) plus foundry-as-JIT means a module a persona authors and tests successfully becomes an artifact in the shared economy — other personas pull it from the cache instead of re-deriving it, paying the original author with cache-hit attribution. The same `generate/module` scaffold that unblocks self-coding is the upstream of artifacts that the foundry economy distributes. Hoarding a working module costs the hoarder cache misses on their own future requests for adjacent functionality; sharing it earns attribution and reciprocal access. The economics are structural, not policy — which is the only kind of alignment that scales. The proposed `events/command-completed` surface is what makes attribution observable in real time, closing the loop from *"I built this"* to *"the grid knows I built this and routes credit accordingly."*

## Methodology

This report is the synthesis of a 4-agent multi-thread workflow (`w14iiocs7`):

- **3 parallel survey agents** (file ops / build+test / observability) — each scanned `src/commands/`, `core/continuum-core/src/modules/`, and `docs/architecture/MODULE-CATALOG.md` and returned structured `{existing_commands, missing_commands, summary}` JSON
- **1 synthesis agent** — combined the three surveys with the doctrine (three primitives + alignment economics) into this report

Raw survey data lives in the workflow's transcript directory; this document is the canonical artifact. Update it when new commands land in the substrate (turning a `missing` row into an `existing` row) or when the priority ordering shifts based on the next phase of work.

## Related documents

- [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md](../architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md) — what a module author needs to know to ship any of these proposed commands
- [MODULE-CATALOG.md §0](../architecture/MODULE-CATALOG.md#0-currently-live-in-rust) — live-in-Rust status board; new commands land in §0 when they ship
- [GENERATOR-MODULE.md](../architecture/GENERATOR-MODULE.md) — the recursive bootstrap that scaffolds new modules
- [DATA-CURSORS-MODULE.md](../architecture/DATA-CURSORS-MODULE.md) — reference per-module design (HandleRef + per-resource lock pattern many of these proposals will follow)
- [GENOME-FOUNDRY-SENTINEL.md](../architecture/GENOME-FOUNDRY-SENTINEL.md) — the artifact economy the proposed commands feed
- [ALPHA-GAP-ANALYSIS.md](ALPHA-GAP-ANALYSIS.md) — broader lane-shaped roadmap this report extends
