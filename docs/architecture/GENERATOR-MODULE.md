# `generator` module — Design

> **Status**: v1 shipped in PR #1487 (recursive bootstrap); v2 enriched scaffold in PR #1494 (matches Module Design Template).
>
> **File**: `src/workers/continuum-core/src/modules/generator/` (mod.rs + types.rs + templates.rs)
>
> **Canonical reference**: [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md)

## Role

**Commands** primitive, serving **architects + AI personas scaffolding new functionality**. Per Joel 2026-05-30:

> *"We developed a generator so we could manufacture these patterns for new commands modules etc, which itself was a command. Meta."*

The generator IS a module; the things it creates are modules; every operation it performs is a command. The system describes itself in its own terms — the recursive bootstrap.

After PR #1494 (v2), authoring a new ServiceModule means running ONE command:

```bash
./jtag generate/module --name "chat_analyze" --commands "..." --stateful
```

…then filling in handler bodies. All envelope wiring, typed Params/Result skeletons, concurrency test scaffold, DESIGN.md skeleton, per-resource lock pattern, and ts-rs annotations are emitted automatically.

## Command surface

| Command | Params type | Result type | Status |
|---|---|---|---|
| `generate/module` | `GenerateModuleParams` | `GenerateModuleResult` | ✅ Rust (PR #1487 + #1494) |
| `generate/command` (planned) | — | — | ❌ Not yet — add a new command to an existing module |
| `generate/refresh` (planned) | — | — | ❌ Not yet — re-scan modules tree + refresh manifests/barrels |

### `generate/module` spec

Params:
- `name: String` — lowercase ASCII identifier (validated; becomes Rust struct name + directory name)
- `description: String` — embedded in mod.rs docstring + README + DESIGN.md
- `commands: Vec<String>` — each becomes a dispatch arm + typed handler method + Params/Result type
- `events_subscribed: Vec<String>` — wired into `ModuleConfig::event_subscriptions`
- `events_published: Vec<String>` — documented in mod.rs docstring + DESIGN.md (no runtime wiring)
- `priority: PrioritySpec` — one of `Realtime` / `High` / `Normal` / `Background`
- `force: bool` — overwrite existing directory
- `stateful: bool` — opt in to per-resource lock scaffold (DashMap + tokio Mutex + helper + concurrency test)

Output (4 files per generation):
- `mod.rs` — ServiceModule impl with typed envelope dispatch + handler methods + concurrency test
- `types.rs` — `<Cmd>Params` / `<Cmd>Result` pair per declared command with `#[derive(TS)]`
- `DESIGN.md` — per-module design skeleton with required 8 sections
- `README.md` — author-facing summary + wire-up reminder

## Cross-module dependencies

**None.** Pure filesystem operations + template rendering. The generator is self-contained — it doesn't call any other module.

## State model

**Per-name locks** for the generation operation:

```rust
pub struct GeneratorModule {
    workspace_root: Option<PathBuf>,
    name_locks: DashMap<String, Arc<std::sync::Mutex<()>>>,
}
```

`std::sync::Mutex` (not `tokio::sync`) because the protected critical section is purely synchronous filesystem I/O — no `.await` inside the lock. Blocking the tokio worker for the brief mkdir + 4 file writes is correct and avoids cascading the API into async.

Lock entries are never evicted — module names are bounded (no unbounded production stream of unique names) and each entry is ~50 bytes. If memory ever matters, a TTL scan can be added without changing the protocol.

## Events emitted

**None.** Filesystem operations are the side effect.

## Concurrency contract

**Per-name lock** serializes concurrent same-name `generate/module` calls; different names stay fully parallel via DashMap's per-shard locking.

### Pinned invariants (multi-thread tests)

1. **`same_name_concurrent_generation_without_force_yields_one_winner`** — 8 racers, same name, no force; exactly ONE wins, 7 fail loud with "already exists" + escape hatch hint
2. **`same_name_concurrent_generation_with_force_produces_consistent_final_state`** — 8 racers, same name, force=true; both files (mod.rs + README.md) carry the SAME `MARKER-XX` proving they came from ONE generation round (no torn state)
3. **`different_names_concurrent_generation_runs_fully_parallel`** — 12 racers with distinct names, all succeed, each module's files distinct, lock map has 12 entries

All run `flavor = "multi_thread", worker_threads = 4`.

### Without the per-name lock (the bug it prevents)

Two parallel callers with the same name and different params would:
- Both call `target_dir.exists()` and see false
- Both call `create_dir_all` (idempotent — both succeed)
- Both write all 4 files in interleaved order
- Last write wins per file → on-disk state has mod.rs from caller A + README.md from caller B (silent torn state)

The friendly "already exists" error never fires; the corruption is silent.

## Migration notes

**No TS predecessor.** Designed fresh in Rust per the substrate doctrine. The generator's wire shape is the rethink — there was nothing to port.

### v1 → v2 (PR #1487 → PR #1494)

v1 produced 2 files (mod.rs + README.md) with raw-`Err` dispatch arms. Authors had to hand-author types.rs, the typed envelope wiring, the test module, the concurrency stress-test scaffold, and the DESIGN.md.

v2 produces 4 files matching [the Module Design Template](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md). Author fills in ONE line per command (the Err body) + adds typed fields to Params/Result + writes the DESIGN.md prose. That's it.

The v2 enrichment was driven by the substrate work in PRs #1485 (cell shapes) + #1486 (envelopes) + #1490–#1492 (concurrency doctrine). The generator now encodes those patterns automatically.

## Kinks found

1. **Same-name race silenced the friendly error.** Initial v1 impl had a race window between `exists()` check and `create_dir_all`. Two concurrent callers with the same name both passed the check, both created, both wrote — the "already exists" friendly error never fired. **Fix**: per-name `std::sync::Mutex` held across the entire exists/mkdir/write sequence (PR #1487 + concurrency test that caught it pre-merge).

2. **Same-name race with force=true could torn-write.** Even with force, two concurrent racers' files could interleave (mod.rs from A, README from B). **Fix**: same per-name lock; force-mode writes serialize to ONE complete generation round per caller, with the second caller's writes overwriting the first cleanly. Pinned by the MARKER test.

3. **v1's bare-`Err` dispatch carried no envelope wiring.** Every author writing a real handler had to convert raw `Err("not yet implemented")` arms into proper `CommandRequest::from_value` + typed handler + `CommandResponse::ok(...).into_command_result()`. **Fix in v2**: emit the envelope wiring + typed handler stubs directly — author only replaces the inner Err body.

### Substrate refinements not needed yet

The generator's surface is narrow (one command, four files emitted). It hasn't surfaced kinks that require new substrate primitives. If `generate/command` adds the "modify an existing module" pattern, AST-level parsing may surface design decisions (which Rust parser? `syn`? handwritten?) — flagged for then.

## References

- PR #1487 — v1 GeneratorModule (recursive bootstrap base + per-name lock fix)
- PR #1494 — v2 enriched scaffold (matches Module Design Template)
- PR #1493 — Field manual (the template v2 emits)
- [MODULE-ARCHITECTURE.md §10](MODULE-ARCHITECTURE.md) — recursive bootstrap doctrine
- [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md §3 + §6](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md) — Module Design Template + Generator usage
- Memory: `three-primitives-commands-events-persona`, `rethink-dont-port-commands-to-rust`
