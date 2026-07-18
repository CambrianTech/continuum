# Eval-Preemption Lease — quiesce the autonomic fleet for a clean measurement

**Status:** designed (all seams scouted 2026-07-14), slice 1 (registry flag + RAII guard) in build.
**Task:** new — "Eval-preemption lease: benchmark/eval acquires a fleet-quiesce lease; the
autonomic self-tick suspends for the duration, restores on drop." Ties #56 (ResourceGovernor
lease model), #59 (humane snapshot-eval), #126 (governor-arbitrated scaling).
**Memory:** [[benchmark-is-a-governor-preemption-lease]], [[benchmark-numbers-carry-gpu-provenance]],
[[first-class-citizens-even-during-benchmarks]].

## Why

A single-GPU box running live personas + a benchmark cannot be made honest by static
config (proven this session: `set-sleep-mode` only changes *attention*, so a persona with
a directed task keeps generating; despawn is heavy and breaks `--persona_id`; and the
autonomic loop interleaves with an eval on the same cognition, contaminating it). The
RTOS-correct answer is **priority preemption**: the eval acquires an exclusive lease and
the governor reactively quiesces the lower-priority consumer (the autonomic self-tick),
then restores it. This is the one primitive that makes *every* future measurement clean
and reproducible — the enabling seam for both the genome A/B and the agentic gym.

`quiesce` ≠ `despawn`: the instance stays alive (answers explicit/forked eval work), it
just stops *initiating* autonomic self-directed turns. Humane by construction — a
suspend-with-restore, never a kill.

## The seams (scouted)

| Concern | File / symbol |
|---|---|
| Autonomic turn is initiated here | `persona/service_loop.rs` — `Wake::Tick` arm → `run_self_cycle(...)` (~L390) |
| The one live-instance registry | `persona/airc_runtime_registry.rs` — `PersonaSlot { runtime, service_loop }` |
| Despawn (the hard sibling) aborts the loop JoinHandle | `commands/persona/instances/despawn.rs` |
| ctx the loop reads | `persona/supervisor.rs` — `PersonaContext` (= `HostedPersona`); `.runtime: Arc<dyn AircCitizen>` fetched via `materialize_adapters`'s `runtime_lookup` (same object as the registry runtime, upcast) |
| ctx construction sites (all must stay consistent) | `supervisor.rs:873`, `cognition_io.rs:376` (test), `modules/cognition.rs:756` |
| Eval entry that should hold the lease | `cognition/eval.rs` — `run_eval(...)` |

## Design (compression: ONE registry, ONE flag, RAII lease)

1. **Flag on the registry slot.** `PersonaSlot` gains `quiesced: Arc<AtomicBool>`
   (`airc_runtime_registry.rs`). The registry is the single source of "who is online";
   "who is quiesced" is the same keyspace — no parallel registry (the doc-comment there
   already refuses that).
2. **Registry API.** `set_quiesced(id, bool)`, `is_quiesced(id) -> bool`, and
   `quiesce_all() -> QuiesceLease` where `QuiesceLease` is an RAII guard that sets every
   slot's flag on construct and clears them on `Drop` (so a panicking eval can never leave
   the fleet frozen — the #59 discipline, guaranteed restore).
3. **Loop honors it.** In the `Wake::Tick` arm, before `run_self_cycle`, read the flag for
   this persona (threaded into the loop as an `Arc<AtomicBool>` clone via `ServeOptions`,
   sourced from the slot at spawn in `host.rs`/`ipc` — explicit handle, no global). If
   quiesced: drift `next_beat` toward `rest_cap` and `continue` (skip the self-cycle;
   still handle `Wake::Msg` so an explicit turn is never dropped).
4. **Eval acquires the lease.** `run_eval` opens with `let _lease = registry.quiesce_all()`
   (or `quiesce_all_except(target)` if the target's live lane is reused). Held for the run;
   dropped on return → fleet resumes. This is the "benchmark requests an exclusive-GPU
   lease and the governor quiesces the persona consumer" from the memory.
5. **Manual verbs (optional, cheap).** `persona/instances/pause` / `resume` → the same
   registry setter, for hand-driven measurement and debugging.

## Slices

- **S1 (safe, self-contained — building now):** `QuiesceRegistry`-side flag + `set/is_quiesced`
  + `QuiesceLease` RAII guard + unit test (guard sets on construct, clears on drop, clears
  even on panic). Compiles + tests independently of the loop wiring.
- **S2:** thread the flag into `serve_persona_loop` via `ServeOptions`; add the tick gate;
  unit test (quiesced → self-cycle skipped; not-quiesced → runs).
- **S3:** `run_eval` holds the lease; live-validate the genome A/B on the gene's OWN
  held-out set (register the 39-task set as a benchmark) — clean, no interference.
- **S4:** governor integration — the lease becomes a `ResourceGovernor` lease so exclusive
  GPU is arbitrated, not just persona-quiesced (#56 convergence).

## Non-goals / guardrails

- Not a global singleton — the flag is an explicit `Arc` threaded from the registry.
- Not `despawn` — the instance stays alive; only the self-tick suspends.
- No locks across await in the tick (CONCURRENCY-STYLE-GUIDE) — a single `AtomicBool::load`.
- The lease MUST restore on drop/panic — a frozen fleet is worse than a contended eval.
