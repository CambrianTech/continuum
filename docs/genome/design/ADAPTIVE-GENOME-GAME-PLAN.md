# Adaptive genome: the game plan (2026-09-16)

Status: the plan the M5 (Fable) and the Intel Mac (Cormac) follow to implement Astra's
[handoff work order](ADAPTIVE-GENOME-HANDOFF.md) against the
[lifecycle design](ADAPTIVE-GENOME-LIFECYCLE.md). Joel, 2026-09-16: "It's a natural
transition because the genome and its sharing supercharge this work, and allow for
speciation. If you guys seem ready and follow a game plan I believe in you."

## Order, and what gates each step

Nothing here starts before the standing work is green, in this order:

1. **Stability + benchmark.** The frozen control round (verified, seed 916, no reboots for
   its length) settles and is read; the render budget (#4124) is deployed and measured on
   both tiers (`cognition.budget.prefill_bound`, `delib.context.render` totals).
2. **Academy.** The echo loop is gone on the M5 after #4112 deploys; d41b6fc1 (a citizen
   speaking under another's name) and c9bf2949 (unlisted substrate notices) are closed or
   owned.
3. **Astra's work merged.** #4122 (selection capture + replay) and #4125 (handoff) on
   canary; #4121 (storage handle contract) owned.

Then the slices below, one PR each, each merged before the next starts.

## Division of labour

| Who | Owns | Why |
|---|---|---|
| M5 (Fable) | every implementation slice | the M5 builds continuum-core in ~9 min; the serving/KV/pager seams the slices touch are its lane |
| Intel Mac (Cormac) | review at exact heads; the structural tests; the measurements on the small tier; the learning-lane cards (081674a6 family) | a 20-minute build and 1h46m llama rebuild cannot carry an edit-build-test loop; his reviews caught two of tonight's three real defects before they shipped |
| citizens | the work the slices are measured on — ordinary project cards through the normal pipeline | the acceptance is useful work with receipts, never a loaded adapter |

## Slices, with the seam and the receipt

### S1 — composition authority (Astra's "next implementation PR")
- Seams: `commands/genome_recall.rs` (extend the registered verbs), the persona's durable
  state (the same owner that persists overrides/rehoming), `routing::grid_trust_policy`
  for authority.
- Adds: inspect (effective policy binding, composition, auto/locked, actor, generation,
  unresolved reasons); select-policy (versioned adapter + expected generation; the
  weighted implementation first); select/lock/release a composition through the ONE
  validation path automatic selection will use; stale-proposal rejection by generation.
- Receipt: real command registration; competing manual/automatic updates; restoration
  after restart and rehoming; unauthorized mutation refused; policy replacement during an
  evaluation. Existing fixtures extended; no serialization-only tests.

### S2 — demand → real paging
- Seams: the perception `watch` snapshot as demand; `genome/residency.rs` registered on the
  production boot path (Cormac's boot-registration ratchet lands first: every
  ServiceModule/BrainRegion registered or explicitly dormant with a card, the
  `every_cache_class_has_a_decided_eviction_story` shape); `WorkspaceCycle::page_in` with a
  production caller; `inference/slots.rs::ActivityKey` carries the composition revision so
  KV ownership is by construction; prefetch spends the serving planner's PLANNED headroom
  (the `sidecar_planned_headroom_bytes` shape) as a `PagedResourcePool` with a real
  `evict_at_least` and a `TrackedDir` row.
- Receipt: `genome.page_in {persona, gene, reason}` on a live turn; observed admitted
  revisions; two personas with different compositions concurrently and no adapter/KV
  contamination (the backend's global-adapter behaviour tested explicitly); cancellation,
  missing artifact, pressure and peer reconnection each a named outcome; selection-to-
  admission p50/p95 and useful throughput against the same baseline.

### S3 — measure on real work
- A project card worked twice by the same citizen: base composition vs. selected
  composition, same task and budget; the Positron/glass-box view shows demand, reason,
  candidate hash, prefetch cost, activation, lock owner, cache reuse.

### S4 — learn
- Outcomes into the existing dream/training owners with attribution to the exact
  composition; a parent continued, a harmful update rejected, one justified fork —
  through matched comparisons on held-out activities. Learning credits are the receipt
  (the health line already counts them).

### S5 — share
- One owned HF skill repository updated by the publishing adapter (weights + card +
  signature + parent + Alloy evidence as one revision, expected-parent check); a second
  ordinary installation discovers, verifies and measures it; interrupted publication and
  racing updates recover; bounded artifact retention with an eviction owner.

## Rules that hold throughout
- Extend the existing owner; never a parallel mind, pager, pool or runner.
- Every slice ships with the probe that proves it on a live turn and the test that fails
  when the wiring is absent — the 9/14 flywheel and tonight's `GenomeResidencyModule`
  (registered only in its own tests) are the class this plan exists to end.
- Budgets are derived from recipes and measured hardware (#4124's shape), never from the
  smallest tier.
- No developer path, machine name or privileged org in the mechanism.
- Merge cadence: a peer word on the exact head + green; deploy from canary tip; read the
  receipt before the next slice.
