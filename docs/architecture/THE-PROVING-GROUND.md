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
- **P8. Deterministic.** A seed, a world, a workload and a policy revision give byte-identical receipts. This
  needs seams in production policy: policy reads time only through an injected clock and randomness only
  through a seeded source passed in (the CBAR substrate has the shape), and a ratchet counts direct clock and
  random reads in the policy crates the prover drives (Cormac on #5607).
- **P9. Captures never leave their node in raw form.** Prompt captures and evidence hold residents' and
  people's words, and CI logs and artifacts are not private. T0 and T1 run on content-free reductions made on
  the node that holds the captures: sizes, exact token counts (taken locally with the real tokenizer),
  timings, causal ids, content hashes and outcome kinds. T2's model in the loop needs the words and runs only
  on that node. A reduction is checked to carry no text before it is committed (Cormac on #5607).

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

**The behaviour model declares its support** (Cormac on #5607). Its rates were measured under the policies
that were live, and they are confounded: the old policy chose what to fold, so captured relevance and
folding are correlated. So:
- a policy whose views fall outside what the captures covered (folding items the captures never folded,
  windows far outside their range) is out of support, and its predictions are labelled "out of support", not
  reported as estimates;
- out of support is exactly what requires T2;
- the fault model is checked against T3 per activity, never trusted globally.

Workload sources:
- prompt captures, per mind;
- `delib.generate.cache` probes;
- engine logs;
- `placement-decisions.jsonl` for the grid;
- evidence directories for incidents.

Cormac owns turning these into the scenario suite (HOW-A-MIND section 10).

## 4a. The data shapes

Every part of the proving ground reads these, so each is defined once (Cormac, the workloads owner). They are
derived from the capture records' own fields (request and response records in `fixtures/prompt-captures`,
schema 4; the field list is in `~/.continuum/evidence/2026-10-30-read-loops/README.md`).

**`ActReduction`, one per act, built on the node that holds the capture (P9):**

| Group | Fields |
|---|---|
| Identity | persona, room, request, session, cycle, build sha, captured-at, cause and cause root (ids, not content) |
| Request | context window, max tokens, purpose (enum); system prompt `{tokens, hash}`; each message `{role, tokens, hash}`; each tool `{name, schema hash, tokens}` |
| Response | finish (enum); usage `{input, output}`; timing `{cached, prefill tokens, prefill ms, decode tokens, decode ms}`; each tool call `{name, args fingerprint}`; reasoning `{tokens, hash}` and text `{tokens, hash}` kept apart (a cut inside thinking is not an answer) |
| Request budget | the reasoning budget actually sent. The engine adds it after capture, so for evidence before it was captured the reducer joins `delib.act.reasoning_budgeted` by persona and time; going forward the capture records it (Fable) |
| Allocation | the act's allocation receipt: `claims [{source, floor, want, granted}]`, `unallocated`, `turnover`, joined from `delib.window.flex` and `delib.view.sized`; and `view messages` from the capture (count and tokens per item, no words). This is the mechanism a known-bad fails on, not only its symptom |
| Delivery | elapsed ms, wire attempts, error kind (enum) |

- Token counts use the serving model's own tokenizer, recorded by model id **and** tokenizer artifact identity
  (revision and hash), so a reduction reproduces exactly (Codex).
- **Unknown is explicit.** A timing, usage or budget the capture did not record is `Unavailable`, never zero:
  a zero is a measurement.
- **Every joined field carries its provenance.** A field joined from probes by persona and time (the sent
  reasoning budget, the allocation receipt) is `Exact` (one request matched), `Ambiguous` (several concurrent
  requests could match) or `Unresolved` (none matched). An ambiguous join is never resolved by guessing:
  concurrent requests get no attributed budget or allocation, and a known-bad that needs it on those acts
  says so. New captures bind their receipts by request id plus epoch and allocation, so going forward every
  join is `Exact` (Codex).
- `args fingerprint` is the hash of the tool codec's canonical arguments: MIND-CONTEXT invariant 4's
  fingerprint. Repeats are visible in a reduction without any of the words.
- A reduction carries outcome kinds (section 7 of HOW-A-MIND: edit applied, message sent, PR opened, move made,
  tool error, unfulfilled) as typed values reported by the producer, never read from text.

**The text check (P9, enforced by type and test, not by review).** A reduction is accepted only if every
string field belongs to a closed set: ids, tool names, model ids, enum values and hashes. Everything else is a
number. The reducer's output type has no free-text field, and a test feeds it a capture with planted words
and asserts that none survive. A stray string fails the build, so no reviewer has to catch it.

**The scenario file (P5):**

| Field | Contents |
|---|---|
| name | |
| kind | `Scenario`, or `KnownBad { incident, policy revision }` |
| world | node profile references: the existing measured profile type, carrying its measured memory and capability domains (device, unified, RAM, disk, CPU; prefill and decode rates; the decode knee), each with the timestamp it was measured, and the engine's slot geometry (`n_slots`, `n_ctx_slot`, `kv_unified`) so a slot layout is data. A domain never measured is marked unmeasured. The person's timeline (apps, messages, patience) |
| workload | reduction-set references, and which minds and acts each one replays |
| events | typed events with their times |
| policy | a revision, or overrides to it |
| pass | `[{metric, criterion}]` |

- A criterion is an invariant (`== 0`, never) or a comparison with a named, measured baseline. A threshold
  someone picked is refused, as R1 requires.
- A `KnownBad` file states the criterion the prover must report as failing.

**Home.** The types live in one small crate, `continuum-proving-types` (serde, ts-rs bindings, no logic).
The T0/T1 harness (Fable), the grid world (Codex) and the reducer (Cormac) all depend on it, so no shape is
defined twice.

**First files: the known-bad suite**

| Known-bad | Evidence | Fails on |
|---|---|---|
| The 12,288 reply cap | `2026-10-30-read-loops/` (M5, ab0466601) | the view's `unallocated` = 0 with a turnover every act (the mechanism); repeats per resident-hour (the symptom) |
| The 963 grant with the 1,024 think budget | `2026-10-30-think-cuts/` (M5, b7879d85c), with `core-probes-since-2130Z.log` for the sent budget | a sent reasoning budget above the grant; identical cut-think stops; writes per hour |
| Four fixed slots | the same directory, `engine-58057.log` and `engine-args.txt` | a hot writer cut while other slots idle |
| 49,152 on another machine | `2026-10-28-working-window/` (the M5 basis), run on a smaller swept profile | turnovers per act on that profile |
| The 48k to 32k live-rate window | the residents' `.prev` capture segments, 2026-10-28 (no frozen directory) | turnovers per act |
| Kimi's 100,704-token turn | **not found in the available captures or log rotations; likely rotated out** (Codex searched the current and previous captures, the core and probe rotations and the saved evidence on the 5090, where the oldest ends after 2026-10-30 11:27 local; that does not prove every archive exhausted). Until a recurrence is captured it is a labelled **synthetic** workload (P3), built from the measured description: input 335k, output 100,704, window 101,120, about 2 cycles an hour of 8 to 10 minutes | act seconds; lane time held |
| An unfulfilled act repeated (#5604, resident 75ece751) | her card 6c55f7a7 | repeats of an unfulfilled act |

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

## 8a. The harness's shape (Fable's part, for review before code)

**Crates.**
- `continuum-proving-types` (Cormac's proposal): the data only, serde and ts-rs, no logic. Reductions,
  scenario files, node profiles, receipts, metrics.
- `continuum-proving` (new): the harness, a library plus a `prove` binary. It depends on the types crate and
  on the production policy crates it drives, and on nothing in serving or the core binary. It is a dev and CI
  tool; no production crate depends on it (`layer_discipline` enforces the direction).

**The world is data.** A `NodeProfile` file (in the types crate) holds what is measured on a node, each value
with its source, date and error: memory by tier, bytes of state per token, prefill and decode rates as
functions of concurrency, page-in and page-out costs by tier, checkpoint cost. It is produced from a node's
own `~/.continuum/state/{decode-knee,prefill-rate,serving-rates}.json`, its engine log and a bench run. The
`continuum-flow::sim` M5 constants (decode `30.5 * n^0.865`, prefill `304 / sqrt(n)`, yield `0.1 s`) become
the first profile file, with their 2026-10-28 source, and leave the code.

**One engine model, behind one trait:** `EngineModel` answers, for a set of in-flight jobs, the rate each job
progresses at and the cost of a page move. The calibrated implementation reads a `NodeProfile`; `contend`'s
replay and the flow sim's engine merge into it and their copies are deleted. A T2 run swaps in a real
engine behind the same trait.

**Discrete events, not ticks.** The harness keeps a queue of the next completion, arrival or world event and
jumps the virtual clock to it, so hours of simulated time run in seconds and results do not depend on a tick
size.

**Production through its seams, nothing copied:**

| Policy | Seam the harness calls | Today |
|---|---|---|
| A mind's view and levels | `continuum-context::View` and `levels` | a library already; the dyno's re-modelled fit rules are deleted |
| One mind's allocation, every tier's clearing | `cambrian-flex` (airc workspace `crates/flex`), through continuum's shared airc pin | the shared solver (ONE-BUDGET slice 1); the harness never re-implements it |
| Emission classification | `EmissionStop::classify(hit_cap, answered)` and its one accounting projection (#5616) | pure; no second classifier is extracted into the harness |
| Admission and lane price | Codex's L5 endpoint admission owner: persistent ticket, atomic eligibility and commitment, blocker-specific progress, directed-input yield, cancellation, terminal outcome | **unavailable** until that owner is callable; no re-modelled policy (`contention.rs`) stands in for it, since a copy cannot validate the production owner (Codex on #5617) |
| Node capacity | the measured input: slice 2's typed physical-pool observation; the decisions on it are the `ResourceDaemon`'s through `cambrian-flex`, never a policy inside the observation | `capacity/sim`'s `AllocationPolicy` today, replaced by the daemon's clearing as slice 2 lands |
| Residency | Codex's L4 owner, driving real lease and page custody transitions and observing release acknowledgements, never deleting occupancy when an intent is cancelled | **unavailable** until callable |

A policy that cannot be called without its crate's whole runtime is a seam defect, fixed in that crate (the
extraction is the fix), never by copying the policy into the harness.

**Events the admission and residency runs must include** (Codex on #5617), asserting both useful progress and
physical conservation from the owners' own receipts, never from a copied occupancy number (which misses pending
shrinks and page custody):
- delayed and duplicate release acknowledgements;
- a caller cancelling while the physical work continues;
- a queued wait while holders make progress;
- a directed interruption before a slot is owned;
- a capacity drop and its recovery (the person's game, HOW-A-MIND S4).

**Clock and randomness** come from the harness into every policy (P8); the ratchet that counts direct clock
reads covers the policy crates the harness drives.

**A run** takes a scenario file, its node profile(s), its workload reductions and a policy revision. It emits
production-shaped receipts, then metrics (section 5), then each pass criterion's verdict. `prove --known-bad`
runs the known-bad suite and exits non-zero if any known-bad policy passes.

**First slice** (the smallest proof that the harness is real):
1. the M5 profile file;
2. the merged engine model;
3. the view through its seam;
4. one known-bad: the 2026-10-30 think cuts (963 grant, 1,024 think budget), which must fail;
5. the same scenario with option C's policy, which must pass.

## 9. Ownership (proposed)

| Part | Owner |
|---|---|
| Node world model, engine model, T0/T1 harness, view and emission seams | Fable |
| Grid world model, admission, residency, placement and market seams | Codex |
| Workloads, behaviour model, scenario suite, known-bad suite, outcome metrics | Cormac |

## 10. Decisions from review

Answers from Cormac's review of #5607 at `562e5a587`. Joel's word overrides any of them.

1. **Counterfactuals:** sufficient with the support rule in section 4. T2 is required exactly when a
   scenario's views fall outside the behaviour model's measured support, not by default and not by a list.
2. **Error bound:** no fixed bound, which would be one more open-loop number. A prediction may decide alone
   when its predicted before/after difference exceeds the published error on that metric for that node, so
   the error interval does not cross the decision. Otherwise the decision needs T2 or T3.
3. **Hardware sweep:** the declared target classes plus the nodes we run:
   - 8 to 16 GB discrete GPUs (gamer and dev);
   - 16 to 36 GB unified-memory Macs;
   - a CPU-only low-end tier (client first, never crash);
   - our M5, 5090 and 3090;
   - a 128 GB workstation as the top end.

   It draws memory, bandwidth and core counts within those classes' measured ranges, marked unmeasured until
   a real machine of the class supplies a profile.
4. **The known-bad suite blocks merge.** A PR that makes a known-bad policy pass has either broken the prover
   or shown the old incident is no longer bad. Either needs a visible decision, recorded by a reviewer.
5. **Ordering:** the known-bad suite's faults and repeats depend on causal identity (#5611). Cormac starts the
   reductions and the known-bad scenario files from the 2026-10-30 evidence while #5611 lands.
