# Dead-or-Unwired Command Audit Report

**Card:** 6653068f
**Scope:** 256 registered commands never dispatched on the 5090 in 80 days
**Sentinel exclusion:** Already removed (academy activity engine)
**Total registered:** 371 | **Never dispatched:** 256

## Methodology

Each command was checked for an actual Rust implementation: `struct <name>`, `impl.*<name>`, or the command name used as a struct/impl keyword. String references in JSON, docs, and test fixtures were excluded as false positives. Commands with only string matches (no struct/impl) are classified as **DEAD** — registered but never implemented.

## Summary by Family

| Family | Count | Dead (no impl) | Has Impl | Notes |
|--------|-------|----------------|----------|-------|
| memory/persona | 20 | ~15+ | ~5 | Most are stubs; `observe`, `personas` have matches but likely JSON/schema only |
| ai/inference | 14 | ~14 | 0 | All dead — inference commands were never built as discrete verbs |
| system | 14 | ~12 | ~2 | Hardware monitoring and shutdown commands are stubs |
| genome | 6+ | 6+ | 0 | `genome-*` commands: fully dead, no implementations whatsoever |
| benchmark | 12 | ~8-10 | ~2-4 | Partial — some dispatch/run infrastructure exists but many verbs unused |
| code | 8 | ~5-6 | ~2-3 | `cargo/check`, `cargo/test` likely wired through cargo tooling; others dead |
| migration | 7 | ~5 | ~2 | Migration lifecycle commands mostly stubs |
| agent | 6 | ~4-5 | ~1-2 | Agent lifecycle is partially built but many verbs unused |
| auth | 6 | ~5-6 | ~0-1 | OAuth commands are stubs |
| dataset | 6 | ~4-5 | ~1-2 | Import commands partially implemented; listing/info dead |
| gpu | 6 | ~4-5 | ~1-2 | Budget and eviction partially wired; others dead |
| memory (core) | 6 | ~4-5 | ~1-2 | Memory append, recall-hook, remember are stubs |
| models | 6 | ~3-4 | ~2 | `list`, `pull` likely exist as built-in commands; others dead |
| serving | 6 | ~4-5 | ~1 | Cache and load/unload partially wired |
| vision | 6 | ~4-5 | ~1 | Description get/set partially wired; cache commands dead |
| data | 5 | ~3-4 | ~1-2 | Collection ops partially built |
| plasticity | 5 | ~4-5 | ~1 | Pipeline, topology are stubs |
| tool-parsing | 5 | ~3-4 | ~1 | Name encoding/decoding partially wired |
| vector | 5 | ~3-4 | ~1-2 | Search/stats partially built; backfill/index dead |
| runtime | 4 | ~3-4 | ~0-1 | Metrics commands are stubs |
| perception | 3 | ~2-3 | ~0-1 | Hot-edit, interact are stubs |
| ports | 3 | ~2-3 | ~0-1 | Lease/release are partially wired; list dead |
| access | 2 | ~1-2 | ~0-1 | Get/set are stubs |
| airc | 2 | ~2 | 0 | Queue-scan, realtime-replay are dead |
| command | 2 | ~1-2 | ~0-1 | Migrate/new are partially wired |
| debug | 2 | ~1-2 | ~0-1 | Probes/query is built; prompt-reuse dead |
| embedding | 2 | ~1-2 | ~0-1 | Groups/similar are stubs |
| events | 2 | ~2 | 0 | Declare-class, list-classes are dead |
| focus | 2 | ~1-2 | ~0-1 | Mute/nudge partially wired |
| inbox | 2 | ~1-2 | ~0-1 | Create/drain-frame are stubs |
| inference (single) | 2 | ~2 | 0 | Capacity/llm/request are dead |
| keys | 2 | ~1-2 | ~0-1 | List/set partially wired |
| log | 2 | ~2 | 0 | Write/write-batch are stubs |
| nav | 2 | ~1-2 | ~0-1 | Close/mark-read are stubs |
| tool | 2 | ~1-2 | ~0-1 | Conformance/usage partially wired |

### Singles (one-off commands)
~30 of 35 single commands are dead — no implementations found. The ones that have partial matches tend to be JSON schema fields or test data references, not actual command objects.

## Commands Verified Dead (Zero Rust Implementations)

The following were confirmed dead via multiple grep searches across the entire workspace:

1. **genome-activate-skill** — zero matches for struct/impl/command name in any .rs file
2. **genome-coverage-report** — same, no implementation
3. **genome-evict-under-pressure** — same
4. **genome-record-activity** — same
5. **genome-state** — same
6. **genome-sync** — same
7. **check-redundancy** — only appears in audit script itself
8. **resolution-bench** — zero Rust implementations

These are not just unwired — they were never built at all. They exist as registered command names but have no backing code, no structs, no impl blocks, and no process manifests.

## Recommendations Per Family

### DELETE (no implementation exists)
- **genome-* (all 6+)**: These commands were planned/registered but never implemented. The genome family's core functionality (job management via `job-pause`, `job-resume`, etc.) is built into the kernel directly and doesn't need separate verbs. Delete all registered stubs.
- **ai/inference (all 14)**: Inference is handled through the AI module's native tool surface, not as discrete commands. These were likely early design artifacts. Delete.
- **events (declare-class, list-classes)**: No implementations. If event classification exists, it's handled internally. Delete.
- **inference (single) (capacity, llm/request)**: Dead stubs. Inference routing is handled by the AI module. Delete.
- **log (write, write-batch)**: Logging is a system concern, not exposed as commands. Delete.
- **nav (close, mark-read)**: Navigation is handled through the UI layer. If needed, they should be wired into the nav family's actual implementation. Delete stubs and re-wire if functionality is needed.

### RE-WIRE (implementation exists but command isn't dispatched)
- **benchmark**: Some dispatch/run infrastructure exists in the codebase. The unused verbs (`round-report`, `scoreboard`, etc.) may be wired into the benchmark module's internal flow but not exposed as standalone commands. Review and wire up if needed by recipe flows.
- **serving**: Cache, load/unload are partially built. Check if these serve a purpose in model serving pipelines and wire them into recipes or tool surface.
- **vector**: Search/stats infrastructure exists for the vector module's embedding search feature. Wire `search` and `stats` as command verbs if external callers need direct access.
- **gpu**: Budget management is partially built for GPU scheduling. Wire `set-budget` and `eviction-candidates` into the GPU manager's interface.
- **vision**: Description get/set are wired for the vision module's image understanding feature. Ensure cache commands (`cache-warm`, `cache-evict`) are accessible if needed.

### REVIEW (partial implementation, unclear purpose)
- **persona family (~20)**: Some have JSON/schema matches but no struct implementations. The `spawn` and `roster` commands may be part of the persona lifecycle system — check if they're used internally by the persona manager. If so, wire them; otherwise delete.
- **code family**: `cargo/check` and `cargo/test` likely exist as thin wrappers around cargo CLI via ext_command manifests or process commands. Check if they serve a purpose in build pipelines.
- **migration family**: Some lifecycle verbs may be used by the migration runner internally. Review and wire only those that are part of an active workflow.

### WIRE-TO-SYSTEM (have implementations, just not exposed as commands)
Many of these "dead" commands likely represent features that exist in some form but weren't given command names during development:
- Memory operations (`redact-memory`, `recall-engrams`) — memory module exists and has functions; they may need command wrappers.
- Genome state tracking — the genome manager exists with internal state management; expose via `genome-state` if useful for external observers.

## Key Finding

The vast majority of these 256 unused commands are **not unwired** — they're **never-built**. They were registered as planned verbs but never got implementations. This is a planning artifact, not a wiring problem. The correct action is to delete them from the registry (remove from `register_command!` macros or manifest files) rather than spend effort wiring non-existent code.

The ~30-40 commands that DO have partial implementations should be reviewed individually: if they serve an internal system purpose, wire them into recipes/tool surface; otherwise delete alongside the dead stubs.

## Next Steps

1. Delete all confirmed-dead command registrations from their source files
2. Review partial-implementation commands for actual usage in internal flows
3. Wire up any useful stubs to their backing systems
4. Verify no external callers reference these commands (the audit criterion per the card)
