# Unified Generator Architecture

**Date**: 2026-03-12
**Status**: Design — approved for implementation
**Branch**: `feature/unified-generator-architecture`
**Priority**: P1B in [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md)

---

## Vision

**One spec, full vertical, both languages.** A single JSON spec produces the complete
Rust + TypeScript stack for any system module — from trait implementation to browser command.

This is not just a build tool. It's an **architectural compiler** — it encodes the rules
of the system so that every module is born correct, discoverable, health-aware, and
pressure-responsive. Generators are the missing element in AI-assisted development:
they save tokens (AIs don't rewrite boilerplate), prevent code rot (patterns can't drift
from spec), and enable delegation (AIs generate specs, the compiler enforces correctness).

**Long-term vision**: This becomes a general strategy for AIs coding any project —
a framework where AI agents generate module specs and the generator produces
correct, tested, integrated code in any language the project uses.

---

## Why This Matters

### The Problem (Yesterday's 35GB Leak)

```
PersonaUser.ts (TypeScript, 2213 lines)
  └── activateRegisteredAdapters()     ← hand-written, no health check
      └── loads 14 × 2GB LoRA adapters ← no pressure gate, no trait enforcement
          └── 35GB RSS, machine unusable
```

TypeScript made a resource decision that Rust should have gated. No trait required
`activateRegisteredAdapters()` to check health or pressure. The method existed because
a human wrote it without constraints.

### The Fix (Architectural Enforcement)

```
CommandSpec v2 (JSON)
  └── generates Rust ManagedService trait  ← MUST implement health(), pressure_response()
      └── generates ts-rs wire types       ← #[derive(TS)], zero hand-written types
          └── generates TS IPC mixin       ← thin wrapper, no business logic
              └── generates TS command     ← static accessor, fully typed
```

With this architecture, adapter loading goes through a `ManagedService` that checks
`health()` and `pressure_response()` before touching GPU memory. The trait is enforced
at compile time — you can't forget to implement it.

---

## Architecture

### The Full Vertical

```
                    ┌─────────────────┐
                    │  CommandSpec v2  │  ← Single source of truth (JSON)
                    │  (module.json)  │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              ▼              ▼              ▼
    ┌─────────────┐  ┌────────────┐  ┌───────────┐
    │ Rust Traits  │  │ Wire Types │  │ TS Layer  │
    │             │  │            │  │           │
    │ ServiceModule│  │ #[derive(  │  │ IPC Mixin │
    │ ManagedSvc  │  │   TS)]     │  │ Command   │
    │ Health probe│  │            │  │ Static    │
    │ Pressure    │  │ Params     │  │ accessor  │
    │ Lifecycle   │  │ Result     │  │ Tests     │
    └─────────────┘  └────────────┘  └───────────┘
```

### Generator Adapters

The generator uses an adapter pattern to support different module shapes:

| Adapter | Rust Output | TS Output | Use Case |
|---------|-------------|-----------|----------|
| `RustIPCAdapter` | ServiceModule + handler + ts-rs types | IPC mixin + command + static accessor | Most commands (gpu/stats, system/pressure) |
| `ServerOnlyAdapter` | — | Server command + static accessor | Pure TS commands (chat/send, data/list) |
| `BrowserOnlyAdapter` | — | Browser command + static accessor | Browser-only commands (screenshot, interface/*) |
| `FullStackAdapter` | ServiceModule + ts-rs | Both server + browser commands | Cross-environment commands |
| `TraitOnlyAdapter` | Trait definition + ts-rs | TS interface (no command) | Cross-cutting concerns (health, pressure) |

### Reverse Engineer Mode

```bash
# Read existing hand-written command, generate spec from it
npx tsx generator/CommandGenerator.ts --reverse commands/sentinel/run

# Output: generator/specs/sentinel-run.json (inferred from code)
# Shows warnings for missing types, any casts, pattern violations
```

Critical for migrating 266 hand-written commands without rewriting them manually.

### Audit Mode

```bash
# Scan all commands, report conformance
npx tsx generator/CommandGenerator.ts --audit

# Output:
#   273/312 commands conform to generator patterns
#   39 commands missing static accessors
#   23 commands with any casts in types
#   5 raw Commands.execute() calls found
#
#   Top violations:
#     sentinel/* (8 commands) — no static accessors, any casts
#     agent/* (4 commands) — no static accessors
#     ...
```

### Enforcement Hook

```bash
# Precommit check: new command dirs must have generator spec
# Added to .husky/pre-commit or equivalent

generator/enforce.ts:
  - Scan commands/ for directories without matching spec in generator/specs/
  - FAIL if any new (untracked by git) command dir has no spec
  - WARN if existing command dir has no spec (migration in progress)
```

---

## Rust Traits: The Self-Managing Service Contract

### Core Trait: `ManagedService`

Every system module that manages resources implements this trait. The generator
produces the scaffolding; the developer fills in the logic.

```rust
/// Core contract for self-managing services.
///
/// Every module that owns resources (GPU memory, connections, caches, file handles)
/// MUST implement this trait. The system calls these methods automatically —
/// modules don't need to know about each other.
pub trait ManagedService: Send + Sync {
    /// Human-readable name for logging and diagnostics
    fn name(&self) -> &str;

    /// Current health status — called by the health monitor every N seconds.
    /// Return Degraded/Unhealthy with a reason string for diagnostics.
    fn health(&self) -> HealthStatus;

    /// React to system memory pressure.
    /// Called when PressureLevel changes. Module decides how to respond.
    /// Normal: full operation
    /// Warning: reduce non-essential work
    /// High: shed optional load
    /// Critical: emergency minimum — keep only what's needed to not crash
    fn pressure_response(&self, level: PressureLevel);

    /// Emergency load shedding — drop everything non-essential NOW.
    /// Called when pressure_response(Critical) isn't enough.
    /// This is the "pull the fire alarm" method.
    fn shed_load(&self);

    /// Attempt self-repair after a failure.
    /// Return true if healed, false if external intervention needed.
    /// Examples: reconnect dropped IPC socket, re-index corrupted cache,
    /// re-download missing model file.
    fn heal(&self) -> bool;

    /// Graceful shutdown — release all resources, flush buffers.
    /// Called once during system shutdown. After this, the module is dead.
    fn shutdown(&self);
}

/// Health status with graduated severity
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
}
```

### Trait Implementations (Existing Modules)

| Module | `health()` | `pressure_response()` | `shed_load()` | `heal()` |
|--------|-----------|---------------------|-------------|---------|
| **Inference (Candle)** | Check model loaded, GPU accessible | Warning: reject new loads. High: unload idle adapters. Critical: gate closed | Unload all non-active models | Re-download model, rebuild adapter stack |
| **Live (Bevy)** | Check render loop alive, GPU context valid | Warning: idle_cadence=2. High: idle_cadence=4, resize to Tiny. Critical: unload idle slots | Unload all idle avatar scenes | Recreate GPU context, re-load active models |
| **Audio (LiveKit)** | Check WebRTC connected, audio flowing | Warning: reduce broadcast targets. High: mono only. Critical: mute all | Drop all non-active audio tracks | Reconnect WebRTC, re-join room |
| **ORM (SQLite/Postgres)** | Check connections alive, recent query success | Warning: disable WAL checkpoints. High: close idle handles. Critical: read-only mode | Close all non-default DB handles | Reconnect, run schema evolution |
| **RAG (Embeddings)** | Check index exists, embedding model loaded | Warning: skip re-indexing. High: cache-only. Critical: disable entirely | Drop embedding model from memory | Re-load model, rebuild index |

### Cross-Cutting Traits (Generated, Not Hand-Written)

```rust
/// Lifecycle management — modules that need init/shutdown coordination
pub trait Lifecycle: Send + Sync {
    async fn initialize(&mut self) -> Result<(), String>;
    async fn shutdown(&mut self);
    fn is_initialized(&self) -> bool;
}

/// Metrics emission — modules that report telemetry
pub trait MetricsEmitter: Send + Sync {
    fn emit_metrics(&self) -> serde_json::Value;
    fn metric_prefix(&self) -> &str;
}

/// Configurable — modules with runtime-adjustable parameters
pub trait Configurable: Send + Sync {
    fn get_param(&self, name: &str) -> Option<serde_json::Value>;
    fn set_param(&mut self, name: &str, value: serde_json::Value) -> Result<(), String>;
    fn list_params(&self) -> Vec<(&str, &str)>; // (name, description)
}
```

---

## CommandSpec v2

### Current Spec (v1)

```json
{
  "name": "gpu/stats",
  "description": "Query GPU memory stats",
  "params": [{ "name": "subsystem", "type": "string", "optional": true }],
  "results": [{ "name": "gpuName", "type": "string" }],
  "accessLevel": "ai-safe"
}
```

**Problems**: Flat param/result tuples. No support for complex types, imports, enums,
nested interfaces. No Rust awareness. No trait declarations.

### Spec v2

```json
{
  "version": 2,
  "name": "gpu/stats",
  "description": "Query GPU memory manager stats",

  "environment": "server",
  "adapter": "rust-ipc",

  "rust": {
    "module": "modules/gpu",
    "traits": ["ManagedService", "MetricsEmitter"],
    "command_prefix": "gpu/",
    "imports": ["crate::system::pressure::PressureLevel"]
  },

  "types": {
    "SubsystemInfo": {
      "fields": {
        "budgetMb": { "type": "number", "description": "Budget in MB" },
        "usedMb": { "type": "number", "description": "Used in MB" },
        "consumers": { "type": "number", "description": "Active consumers" }
      }
    }
  },

  "params": [
    {
      "name": "subsystem",
      "type": "'inference' | 'tts' | 'rendering'",
      "optional": true,
      "description": "Filter to specific subsystem"
    }
  ],

  "results": [
    { "name": "gpuName", "type": "string" },
    { "name": "totalVramMb", "type": "number" },
    { "name": "pressure", "type": "number" },
    { "name": "rendering", "type": "SubsystemInfo" },
    { "name": "inference", "type": "SubsystemInfo" },
    { "name": "tts", "type": "SubsystemInfo" }
  ],

  "examples": [
    {
      "description": "Get full GPU stats",
      "command": "./jtag gpu/stats"
    }
  ],

  "accessLevel": "ai-safe"
}
```

**New fields**:
- `version`: Spec version (for migration)
- `adapter`: Which generator adapter to use
- `rust`: Rust-specific config (module path, traits to implement, imports)
- `types`: Inline type definitions (generates both Rust structs + TS interfaces)

---

## Implementation Phases

### Phase A: Generator Redesign (TypeScript)

Redesign the generator architecture itself. Even though it only generates TS at first,
the design accounts for Rust from day one.

1. **CommandSpec v2 schema** — richer type system, adapter field, rust config
2. **Adapter pattern** — `GeneratorAdapter` interface with `ServerOnlyAdapter`, `RustIPCAdapter`, etc.
3. **Reverse engineer mode** — `--reverse` reads existing command, produces spec
4. **Audit mode** — `--audit` scans all commands, reports conformance
5. **Enforcement script** — precommit check for new commands without specs
6. **Migrate top 39** — generate specs for the 39 commands missing static accessors

**Output**: Generator produces correct TS for all command shapes. All 312 commands
have specs. New commands must use generator.

### Phase B: Rust ManagedService Trait

Add the core self-managing service contract to the Rust worker.

1. **Define `ManagedService` trait** in `workers/continuum-core/src/system/traits.rs`
2. **Implement for existing modules**:
   - `modules/gpu.rs` → GPU memory management
   - `live/mod.rs` → Bevy renderer + audio
   - `modules/rag.rs` → RAG embeddings
   - `orm/mod.rs` → Database connections
3. **Health monitor** — periodic `health()` poll, emit `system:health:snapshot` events
4. **Pressure dispatcher** — when `PressureLevel` changes, call `pressure_response()` on all modules
5. **Self-heal loop** — on Unhealthy, call `heal()`, log result, escalate if failed

**Output**: Every Rust module self-manages under pressure. No TypeScript band-aids.

### Phase C: Full Vertical Generation

Generator produces both Rust and TypeScript from a single spec.

1. **Rust template adapter** — generates `ServiceModule` impl with `ManagedService` trait stubs
2. **ts-rs integration** — generates `#[derive(TS)]` structs for all spec types
3. **IPC mixin generation** — generates typed TypeScript IPC wrappers from Rust types
4. **End-to-end validation** — spec → Rust build → ts-rs → TS build → integration test

**Output**: Adding a new Rust-backed command is one JSON file + filling in the logic.
Everything else (types, IPC, command, tests, docs) is generated.

---

## Current Generator Problems (Detailed)

### 1. Monolithic String Templating
`TokenReplacer` does `{{TOKEN}}` replacement. No AST awareness, no composition,
no ability to conditionally include sections based on spec properties.

**Fix**: Adapter pattern. Each adapter knows its output structure. Templates are
per-adapter, not one-size-fits-all.

### 2. CommandSpec is Too Flat
`params: [{name, type, description}]` can't express:
- Union types: `'inference' | 'tts' | 'rendering'`
- Nested types: `SubsystemInfo` as a typed object, not `any`
- Enums: Pressure levels, health statuses
- Imports: When a param uses a type from another module

**Fix**: Spec v2 `types` block for inline definitions. `type` field supports
TS syntax directly. Generator resolves imports.

### 3. TokenBuilder is a God Class
337 lines of static string-building methods. Mixes naming, formatting,
documentation, and type generation in one file.

**Fix**: Split into focused classes:
- `NamingConventions` — PascalCase, camelCase, path resolution
- `TypeRenderer` — interface fields, factory functions, generics
- `DocRenderer` — README, examples, parameter docs
- `TestRenderer` — unit test scaffolding, integration test scaffolding

### 4. No Validation
Spec says `type: "SubsystemInfo"` but generator doesn't check if that type
exists. Generated code may not compile.

**Fix**: Type registry. Generator tracks all known types (from spec `types` block,
from existing commands, from Rust ts-rs exports). Warns on unknown types.

### 5. No Reverse Engineering
266 hand-written commands can't be migrated without manually writing specs.

**Fix**: `--reverse` mode reads a command's Types file, extracts params/results
interfaces, generates a spec JSON. Human reviews and adjusts.

### 6. No Enforcement
Nothing prevents a developer from creating `commands/foo/` by hand.

**Fix**: Precommit script scans for new command directories without specs.
CI check validates all commands have conforming types.

---

## File Structure (Post-Redesign)

```
generator/
├── CommandGenerator.ts          # Orchestrator (slim — delegates to adapters)
├── adapters/
│   ├── GeneratorAdapter.ts      # Interface
│   ├── ServerOnlyAdapter.ts     # Pure TS server command
│   ├── BrowserOnlyAdapter.ts    # Pure TS browser command
│   ├── FullStackAdapter.ts      # Both environments
│   ├── RustIPCAdapter.ts        # Rust ServiceModule + IPC mixin + TS command
│   └── TraitOnlyAdapter.ts      # Rust trait definition + TS interface
├── renderers/
│   ├── NamingConventions.ts     # Case conversion, path resolution
│   ├── TypeRenderer.ts          # Interface fields, generics, factory functions
│   ├── DocRenderer.ts           # README, examples, parameter docs
│   └── TestRenderer.ts          # Test scaffolding
├── validators/
│   ├── SpecValidator.ts         # Validate spec against schema
│   ├── TypeRegistry.ts          # Track known types, warn on unknown
│   └── ConformanceChecker.ts    # Audit existing commands vs spec patterns
├── reverse/
│   └── ReverseEngineer.ts       # Read existing command → generate spec
├── enforce/
│   └── PrecommitCheck.ts        # Verify new commands have specs
├── templates/
│   ├── command/                  # TS command templates (existing, improved)
│   ├── rust/                     # Rust ServiceModule templates (new)
│   ├── ipc-mixin/               # IPC mixin templates (new)
│   └── trait/                    # Trait definition templates (new)
├── specs/                        # All command specs (47 → 312)
│   ├── gpu-stats.json
│   ├── sentinel-run.json         # ← generated via --reverse
│   └── ...
├── types/
│   ├── CommandSpec.ts            # Spec v2 interface
│   └── AdapterConfig.ts         # Per-adapter configuration
└── core/
    ├── ModuleGenerator.ts        # Base class (existing, cleaned up)
    ├── TemplateEngine.ts         # Replaces TokenReplacer (richer)
    └── FileWriter.ts             # File I/O with backup/force
```

---

## Success Metrics

| Metric | Current | Target |
|--------|---------|--------|
| Commands with generator specs | 47/312 (15%) | 312/312 (100%) |
| Commands with static accessors | 273/312 (88%) | 312/312 (100%) |
| `any` casts in command types | 23 | 0 |
| Raw `Commands.execute()` calls | 5 | 0 |
| Rust modules with `ManagedService` | 0 | All resource-owning modules |
| Time to add new Rust-backed command | ~2 hours manual | ~10 minutes (spec + fill logic) |
| Self-healing coverage | 0% | All modules implement `heal()` |

---

## Broader Impact: AI-Assisted Development

This generator architecture is not specific to Continuum. It's a general pattern
for how AI agents should interact with codebases:

1. **Specs are token-efficient** — An AI generates a 30-line JSON spec instead of
   500 lines of boilerplate across 8 files. This is a 15x token reduction.

2. **Generators encode institutional knowledge** — Architecture rules, naming
   conventions, trait requirements, test patterns — all baked into the generator.
   An AI doesn't need to learn these rules; it just produces a spec.

3. **Validation catches mistakes at generation time** — Not at code review, not
   in production. The generator refuses to produce code that violates the architecture.

4. **Reverse engineering enables migration** — Existing codebases aren't rewritten.
   The generator learns the existing patterns and produces specs that encode them.

5. **Enforcement prevents regression** — Once a codebase is generator-managed,
   it stays managed. No drift, no rot, no "someone hand-wrote this and forgot the types."

This is potentially a publishable contribution: **generators as architectural compilers
for AI-assisted software development**. The insight is that the right abstraction for
AI coding isn't "generate code" — it's "generate specs, compile to code."

---

## References

- [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md) — P1B priority
- [GENERATOR-OOP-PHILOSOPHY.md](GENERATOR-OOP-PHILOSOPHY.md) — Original generator philosophy
- [GENERATOR-ROADMAP.md](GENERATOR-ROADMAP.md) — Previous generator improvement plans
- [ARCHITECTURE-RULES.md](../ARCHITECTURE-RULES.md) — Rules the generator must enforce
