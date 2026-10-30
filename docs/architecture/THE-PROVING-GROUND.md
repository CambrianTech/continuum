# The proving ground: a system that proves the system, from one mind to the grid

Status: design for review, 2026-10-30. No code until this is agreed.

Joel, 2026-10-30: "You need to simulate all aspects of this. You made some dyno before too. We keep
forgetting and regressing into amateur designs. Let's also design the system to prove the system itself."
And: "Yes, and the same for the grid itself."

Companions: [HOW-A-MIND-GETS-WHAT-SHE-NEEDS](HOW-A-MIND-GETS-WHAT-SHE-NEEDS.md) (the requirements and
scenarios this proves), [ONE-BUDGET](ONE-BUDGET.md), [GRID-MARKET-CLEARING](GRID-MARKET-CLEARING.md) (whose
slices S1 to S4 call for "the sim as the gym"), [ADAPTIVE-GRID-INTELLIGENCE](ADAPTIVE-GRID-INTELLIGENCE.md),
[SERVING-ENGINE](SERVING-ENGINE.md) (S0, `serving-replay`).

## 0. Why: we keep regressing because nothing proves a change before a resident does

Every regression of 2026-10-30 reached the M5 untested against the behaviour that broke:
- the reply cap that starved her view;
- its replacement that cut Aris three times at 963 tokens;
- the 1,024-token think budget larger than the grant;
- the window replayed on one machine.

Each would have failed a scenario run before deploy, had one existed. The pieces of a proving ground exist
already, built at different times by different agents, and forgotten between sessions (inventory,
2026-10-30, `origin/canary`):

| Piece | Gives us | Missing |
|---|---|---|
| Context dyno (`continuum-context/examples/replay*`) | real `View` and `levels` on real captures; turnovers, re-reads, reuse | re-models the core's fit rules (a copy that drifts); one mind; no engine, time or memory; asserts nothing; not in CI |
| `contend` (`continuum-engine/.../planning/contention.rs`) | minds on one engine under admission policies, virtual clock, measured rates required | analytic engine; no view, no memory, no paging; aggregated workload |
| `continuum-flow::sim` (branch `fable/continuum-flow`, off canary) | the real flow scheduler on an engine calibrated to the M5; reproduced a live gate change | off canary; one calibration; no memory model |
| `serving-replay` (SERVING-ENGINE S0) | a real `llama-server` with page-in, page-out, slot save and restore | serial acts; generates a fixed count, not her reply; paging policy copied from core; not in CI |
| `capacity/sim.rs` ("the gym") | the production `AllocationPolicy` through capacity timelines (a game eats the GPU) | synthetic; no time, latency, KV or paging; scenarios in code, not data |
| Grid: `capacity/{grid,gossip,grid_budget,market,settlement}.rs`, `run_symmetric_grid` | production placement and gossip on synthetic snapshots; partitions, joins, churn | no prices wired, no time, no transfer cost; `placement-decisions.jsonl` (176K recorded) never replayed; GRID-MARKET-CLEARING's sim scenarios unbuilt |
| Wire tests (`TwoAircLoopback`, `airc_remote_inference_*`) | two real airc peers, real executor | two peers, fake compute; partly not in CI |
| Measured rates (`~/.continuum/state/{decode-knee,prefill-rate,serving-rates}.json`) | per-model prefill and decode on this node | no node-profile type; nothing replays them |
| Captures (`fixtures/prompt-captures` 560M, probes 38M + 128M archive, evidence 530M) | every resident's real requests, responses, timings since September | used by one dyno at a time |

Five of these are on CI's `pending` list ("tests exist but no job selects them"). Nothing joins view,
admission, memory and time; nothing runs more than two nodes with time; and nothing checks that any of them
predicts reality.

## 1. Principles

- **P1. Only the world is simulated; the system is real.** The proving ground drives the production policy
  code through its own seams: the allocator, the view, admission, residency, placement, the emission
  classifier. It models only what is outside our control: hardware, the engine's physics, the network,
  people, other apps, and the minds' choices. A copy of a production rule inside a simulator is forbidden
  (the dyno's fit rules and `serving-replay`'s paging copy are migrated to call production).
- **P2. Every world model is calibrated from measurements, and its error is published.** Node profiles come
  from measured rates and capacities; the engine model's curves come from the node's own runs; the network
  model comes from airc's measurements. Each model carries its calibration source, date and error bound.
- **P3. Workloads come from what minds actually did.** Captures and probes are the demand. Synthetic
  workloads exist only for shapes no capture has yet (a new activity), and are labelled as such.
- **P4. The prover is itself proven** (section 6): it must predict live reads within its published error, and
  it must fail every known-bad policy we have actually shipped.
- **P5. Scenarios are data, not code.** Each scenario (S1 to S15 and every incident) is a file: workload,
  world, events, pass criteria. A live incident becomes a scenario file the same day, so it can never return.
- **P6. The same receipts as live.** Every simulated decision emits the receipt the live system emits
  (HOW-A-MIND R11), so the same analysis reads both, and sim-versus-live is a diff of like with like.
- **P7. Fast by default, faithful when needed:** fidelity tiers (section 3), with the fast tier running in
  seconds on any machine and in CI.
- **P8. Deterministic.** A seed, a world, a workload and a policy revision give byte-identical receipts.

## 2. What is simulated

**The world model, per node:**
- **Hardware profile:** memory (device, unified, RAM), disk, CPU, measured; from a node's own state files
  and runs, or from a declared shape for a machine we do not have (marked unmeasured).
- **Engine physics:**
  - prefill and decode rates as functions of concurrency (the decode knee);
  - bytes of state per token;
  - page-in and page-out costs by tier;
  - checkpoint costs;
  - prefix reuse as exact token-prefix matching.

  Starts from `continuum-flow::sim`'s calibrated engine and `contend`'s replay, merged into one.
- **The person and their apps:** a timeline of external memory and CPU use (a game, Photoshop, a build),
  a person's messages and their patience (how long they wait before it hurts).
- **The minds' choices:** replayed from captures (section 4), with a behaviour model for counterfactuals.

**The world model, for the grid:**
- N simulated nodes, each a node world above, with heterogeneous profiles;
- links with measured latency, bandwidth and loss (from airc measurements), partitions and heals;
- gossip of offers and prices on its real cadence, with staleness;
- nodes joining, leaving, sleeping, and dying mid-work;
- leased minds whose state lives on one node and whose person is on another.

**The system under test** is production code: the allocator and its tiers, the view and its loops, the
emission classifier, admission, residency, placement (`GridPlacementPolicy`, market clearing), gossip.

## 3. Fidelity tiers

| Tier | Engine | Speed | Where it runs | Used for |
|---|---|---|---|---|
| T0 Analytic | calibrated curves, virtual clock | seconds for hours of simulated time | any machine; every PR in CI for the affected crates | every policy change; sweeps; the scenario suite |
| T1 Real policy, modelled engine, real tokenizer | as T0 plus exact token counts and the real codec | minutes | any machine | before merge of a context, admission or placement change |
| T2 Real engine, proxy model | a real `llama-server` with a small model (`serving-replay`, made concurrent) | tens of minutes | a non-serving machine or a serving-down window | engine verbs (reconfigure, paging), KV behaviour |
| T3 Live read | the node itself | 15 minutes | the live node after deploy | the final receipt, compared against T0 and T1's prediction |

A change must pass T0 and T1 before merge. T2 is required for engine changes. T3 is the deploy's receipt and
the prover's own check.

## 4. Workloads, and the counterfactual problem

A replayed capture says what she did under the view she had. A new policy gives her a different view, and
she might act differently. Three tools address this, each labelled in its results:

1. **Demand replay (exact):** the requests she made, with their sizes and times, replayed open loop. Exact
   for time, memory, admission and placement questions that do not change her choices.
2. **Behaviour model (estimated):** measured conditional rates from captures. These include:
   - the probability she re-reads an item when it was folded versus whole (the fault model);
   - her output size distribution per activity;
   - her think length;
   - her gaps between acts.

   The model replays her choices under the new view. Its error is checked against T3 (section 6).
3. **Model in the loop (faithful, slow):** at T2, a real model answers the new prompts, so her choices are
   generated, not assumed. Used to validate the behaviour model, not for every run.

Workload sources:
- prompt captures, per mind;
- `delib.generate.cache` probes;
- engine logs;
- `placement-decisions.jsonl` for the grid;
- evidence directories for incidents.

Cormac owns turning these into the scenario suite (HOW-A-MIND section 10).

## 5. What it measures and what passes

Every run reports the same metrics as the live health line and the 15-minute read, from the same receipts.

| Group | Metrics |
|---|---|
| Per mind | acts, writes, act seconds (p50, p90), prefill and decode seconds per act, prefix reuse, faults, repeats, cuts by stop class, starvation time |
| Per loop | reversals |
| Per node | relaunches, memory returned to the person's apps and how fast |
| Per grid | placements, re-homes, partition behaviour, transfer bytes and seconds |

Each scenario file states its pass criteria in these terms (HOW-A-MIND section 6). A policy change reports
before and after on the whole suite, across the hardware sweep:
- the M5;
- the 5090;
- a 24 GB GPU;
- a 32 GB unified laptop;
- randomly drawn unknown machines within declared ranges.

A change that improves one scenario and regresses another shows both; it is never summarised to one number.

## 6. Proving the prover

The proving ground is trusted only as far as it is shown to be right.

1. **It must fail what we actually shipped broken.** A permanent "known-bad" suite replays our real
   incidents with the policy that caused them, and the proving ground must flag each:
   - the 12,288 reply cap (2026-10-30 read loop);
   - the 963-token grant with the 1,024 think budget (Aris, three identical cuts);
   - the fixed four slots under one hot writer;
   - the 49,152 window on a different machine;
   - the Kimi 100,704-token turn;
   - the live-prefill-rate window that moved 48k to 32k (74 turnovers in 37 minutes).

   A simulator that passes a known-bad policy is broken, and is fixed before anything else is trusted
   (mutation testing at the policy level).
2. **It must predict live within its published error.** Every deploy's T3 read is compared with the T0 and
   T1 prediction made before the deploy, on the same metrics. The error is recorded per metric and per node.
   When it exceeds the published bound, the world model is recalibrated, and the deploy's conclusion is
   labelled as unproven until it is.
3. **Its calibration drifts visibly.** Each world model carries its calibration date and source; a model
   older than the node's latest measured change (a new engine build, a new model) is flagged stale.
4. **Determinism is tested:** the same inputs twice give byte-identical receipts.

## 7. CI and the development loop

- **T0 runs in PR CI** for any crate whose policy the suite exercises, through the affected-crate selector
  (the same selector that now builds touched examples, #5613). The `pending` entries for
  `continuum-context`, `serving-replay`, `tool-working-set`, `expert-pager-policy` and `airc-test-fixtures`
  are paid off as their parts join the proving ground.
- **A PR changing a policy posts its before/after table.** Reviewers read the table, not a claim.
- **Incidents become scenario files** the day they happen; the known-bad suite only grows.
- **Before any live deploy,** the T0/T1 prediction for the 15-minute read is written down, so the read
  tests the prediction as well as the change.

## 8. Build order: join what exists, copy nothing

1. **One world model for one node:** merge `continuum-flow::sim`'s calibrated engine and `contend`'s replay
   into one engine model, add memory and paging costs, and load node profiles from the measured state files.
   Bring `continuum-flow` to canary.
2. **The system on it:** drive the production view (replacing the dyno's re-modelled fit rules), the emission
   classifier, admission and `capacity/sim`'s `AllocationPolicy` through their seams. Scenarios become data
   files (the deferred serde loading in `capacity/sim`).
3. **The known-bad suite** from today's incidents, and S1, S2, S7 and S15 from captures. The proving ground
   must fail the known-bad policies before anything else is built on it.
4. **The grid:** extend `run_symmetric_grid` with time, links and prices; drive the production placement,
   gossip and market clearing; replay `placement-decisions.jsonl`; GRID-MARKET-CLEARING's S1 (byte-identical
   replay) and S3 (n=3 contention, the sim predicts the winner).
5. **T2:** `serving-replay` made concurrent and calling production paging; it validates the engine verbs
   (SERVING-ENGINE S1 to S3) and the behaviour model.
6. **Prediction versus live** wired into every deploy's read.

## 9. Ownership (proposed)

| Part | Owner |
|---|---|
| Node world model, engine model, T0/T1 harness, view and emission seams | Fable |
| Grid world model, admission, residency, placement and market seams | Codex |
| Workloads, behaviour model, scenario suite, known-bad suite, outcome metrics | Cormac |

## 10. Questions for review

1. Is the counterfactual handling (demand replay exact; behaviour model estimated and checked against live;
   model in the loop for validation) sufficient, or does any scenario need T2 by default?
2. What error bound must the prover hold against live before a deploy decision may rest on it alone?
3. Which unknown-machine ranges should the hardware sweep draw from?
4. Should the known-bad suite block merge (a PR may not make any known-bad policy pass), in addition to the
   scenario suite?
