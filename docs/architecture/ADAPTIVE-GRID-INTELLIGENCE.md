# Adaptive grid intelligence: measured allocation, concurrent thought and learning

Status: design proposal, 2026-09-15. Records Joel's agreed direction and an
algorithm sketch; it does not claim that the mechanisms below are all implemented
or that distributed inference has passed a live performance gate.

## Purpose and architectural home

A persona can pursue several activities and modalities concurrently. Its identity,
history and commitments survive changes of model, machine and provider. The
persona chooses its goals; the substrate allocates resources to pursue them.
An activity may be software work, a novel, a conversation, an academy exercise or
a benchmark recipe. Benchmarks exercise the same activity machinery and produce
learning evidence; they are not an alternative runner.

The goal is a dependable team on one desktop, stronger when a computer joins,
still useful when a peer or cloud subscription disappears. Extra resources can
buy different things: larger model capacity, more concurrent work, better verified
outcomes, or lower response latency. Measure these separately. Geometric
improvement is a hypothesis to test, not a property implied by adding nodes.

This extends existing designs, not a new scheduler or control plane:

- [Concurrent mind](CONCURRENT-MIND-AND-GOVERNOR.md): autonomous concurrent demand,
  event-driven concerns, ready buffers and coherent workspace decisions.
- [Capacity fabric](CAPACITY-FABRIC-AND-GOVERNOR.md): live resource observations,
  pressure and revocable leases.
- [Market clearing](GRID-MARKET-CLEARING.md): local allocation against resource
  prices, bounded neighbor information and the existing governor.
- [Expert paging](EXPERT-PAGING-GOVERNOR-SEAM.md) and
  [genome foundry](GENOME-FOUNDRY-SENTINEL.md): residency, learning and provenance.
- [Benchmark adapters](BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md): ordinary activities,
  shared observations and outcome grading.

Existing type names in those documents identify integration seams, not a fresh
implementation audit. The record shapes below are conceptual extensions: inspect
and extend current types rather than introducing parallel registries or managers.

## Measurements are vectors; decisions have contextual objectives

Represent useful characteristics numerically without collapsing them into one
permanent "intelligence score." Preserve the underlying observations and units.
An activity chooses an objective over the measurements; different activities can
prefer different tradeoffs. Respect, citizenship and permission are not rankings
of model performance.

Each observation carries:

| Field | Meaning |
|---|---|
| Metric and unit | Versioned definition: milliseconds, bytes, currency, outcome probability, rubric dimension |
| Value and uncertainty | Estimate plus interval/distribution, sample count and estimator version |
| Scope | Command class, workload, model revision, genome composition, hardware/backend and activity context |
| Provenance | Source probe or evaluator, receipt and artifact references; measured versus predicted |
| Freshness | Observation time, validity window and resource/topology generation |
| Missingness | Unknown is explicit; never silently convert missing data into zero cost or perfect quality |

Outcome quality may be a vector of rubric scores, verification results and human
feedback. Do not assume an arbitrary scalar captures nuance accurately. Keep
rubric versions and qualitative evidence so a good-looking score can be audited.
Capabilities are empirical profiles conditioned on work, not fixed model ranks.

Candidate execution plans predict the same dimensions: useful-result latency,
quality/risk, resource occupancy, transfer, monetary cost and likelihood of
completion. Include queueing, model loading, prefill, execution, verification and
handoff. Use a dependency graph for overlapping phases: adding all durations
would overestimate parallel work; counting only decode would underestimate it.

## What can be divided across the grid

| Unit of work | Placement / join boundary | Required measurements and correctness gates |
|---|---|---|
| Persona teamwork | Independent implementation, research, review or verification; join through activity artifacts and receipts | Completed verified work, handoff loss, rework, coordination cost; preserve autonomous participation and conflict handling |
| Whole inference request | Route to a compatible resident model; keep weights and KV state near execution | Queue, prefill, decode, completion latency, cancellation, correctness and actual callable peer identity |
| Concurrent deep thought | Independent hypotheses, evidence gathering and tool work; integrate relevant findings at decision points | Marginal benefit of another branch, diversity, duplication, stale results and total resource cost |
| Shared perception / retrieval | Encode, index or retrieve near data and suitable devices; share authorized immutable results | Hit rate, freshness, transfer cost, privacy and representation compatibility |
| Imatrix calibration | Disjoint token-chunk ranges under one manifest; combine compatible statistics | Exact coverage, duplicate rejection, counts/normalization, model/tokenizer/options identity, equivalence to an unsplit run and quantized outcome quality |
| Academy / evaluation | Independent ordinary activity instances and evaluators | Reproducibility, held-out results, evaluator independence, cost per validated improvement |
| Genome refinement | Independent specialist training/candidate evaluations; publish verified artifacts | Base compatibility, lineage, held-out gain, regressions and local reproduction; do not blindly average incompatible adapters |
| Expert residency | Place hot experts locally, warm on peers/RAM, cold in storage; prefetch likely use | Fault cost, hit rate, transfer bytes, memory pressure, prediction accuracy and thrashing |
| Remote expert execution | Execute selected experts where resident and return activations | Per-layer fan-out/fan-in, transfer and synchronization tails, routing correctness, batching benefit and numerical tolerances |
| Layer pipeline | Resident contiguous layer groups across devices; microbatch independent requests | Per-stage compute/transfer, bottleneck stage, pipeline bubbles, KV footprint and throughput versus single-request latency |
| Tensor partition | Explicit compatible collective execution within layers | Collective cost and topology; reject plans where communication dominates, even if total memory fits |
| Prefill / decode separation | Distinct stages with an explicit compatible KV transfer | KV volume, transfer time, layout/version compatibility, queueing and amortization over remaining decode |
| Speculative decoding | Cheap draft blocks, exact target-model verification | Acceptance, rejected-work cost, verification throughput and target-distribution correctness |

The vendored [imatrix documentation](../../core/vendor/llama.cpp/tools/imatrix/README.md)
already describes chunk ranges and combining input matrices. That is a useful
primitive, not proof that Continuum has a complete distributed calibration command.
Shard identities must be idempotent: retries cannot double-count calibration data.

LoRA composition, engram retrieval and native MoE expert routing are distinct
mechanisms. A growing genome library does not automatically create an equivalent
larger dense model or make arbitrary experts interchangeable.

## Hosting a larger shared inference model

Treat a distributed model as an explicit execution plan. Each participating node
must fit its assigned weights, KV, activations, scratch space and safety margin.
Aggregate memory alone never proves fit or speed. Keep the existing single-node
fit rule for ordinary whole-model placement; a multi-node plan is a different
candidate with independently validated stage and link constraints.

Keep layer groups resident and pass activations between them, instead of moving
weights on every token. Overlap independent persona requests through the pipeline.
More occupied stages can improve aggregate throughput while one request still
traverses every stage sequentially. A slow node may contribute more through
retrieval or verification than by becoming an inference critical-path stage.

A shared larger model can serve several personas with isolated histories and KV
ownership. Shared prefixes require exact compatible model, adapter, positional
state and authorization keys. No cross-persona context leakage in the name of reuse.

Speculative decoding may amortize a costly distributed target pass over several
accepted tokens. Token verification must use the appropriate exact algorithm;
semantic approval of a teammate's proposal is not equivalent to lossless decoding.
Measure whether drafting and verification actually beat ordinary execution.

Research precedents, not Continuum performance receipts:
[Petals distributed inference](https://arxiv.org/abs/2312.08361) and
[speculative decoding](https://arxiv.org/abs/2211.17192).

## Candidate selection algorithm

Operate through the existing governor and command/handle/event substrate. Replan
on relevant demand, pressure, availability, completion or evidence changes;
coalesce updates and bound policy computation. No per-token global optimizer and
no new polling loop. Local admission remains authoritative over local capacity.

1. Snapshot versioned activity demand, budgets, available offers and cached
   evidence. Preserve uncertainty and expiry; a stale advertisement is not a lease.
2. Generate a bounded candidate set: continue current placement, batch locally,
   change compatible base/genome, route a request, consult another peer, or use a
   validated distributed plan. Include keep-working and defer-this-decision options.
3. Filter hard constraints: permissions, data residency, compatibility, live
   per-device fit, spending authorization, required verification and deadlines.
4. Predict incremental outcomes and costs relative to continuing the current plan.
   Evaluate marginal benefit; avoid rewarding more computation by itself.
5. Prune dominated candidates and rank the remainder under the activity objective.
   Retain alternatives when estimates overlap instead of inventing certainty.
6. Reserve resources and money with bounded leases, revalidate plan generation,
   then dispatch. Failed multi-node acquisition releases partial reservations;
   use deterministic acquisition order or bounded compensation, not distributed
   locks held while waiting indefinitely.
7. Subscribe to progress/completion/cancellation receipts. Replan when evidence
   changes enough to justify switching. Expiry or timeout does not prove remote
   compute stopped; track cancellation acknowledgement and resource release.
8. Record predicted versus actual outcomes and update calibrated models off the
   hot path. Publish a bounded policy snapshot for subsequent decisions.

One possible ranking rule, expressed in activity utility units:

```
score(plan) = E[incremental verified utility | context, plan]
            - sum(resource_shadow_price[r] * incremental_consumption[r])
            - monetary_cost_in_utility_units
            - switching_cost - risk_penalty
```

This is an algorithm sketch, not a universal fixed weighted sum. Prices convert
units explicitly; do not add milliseconds to bytes or double-count costs already
included in the utility. Hard constraints stay outside the score. Tail-latency
requirements use calibrated quantiles or deadline-miss probabilities, not means.
Discrete plans and changing neighbors do not inherit global-optimality guarantees
merely because the policy uses shadow prices.

Fairness, reserved interactive capacity, bounded background leases and aging
prevent high-volume demand from starving smaller activities. Hysteresis, minimum
residency and explicit switching cost prevent oscillating models or placements.
Pressure and safety can override residency preferences.

## Uncertainty, escalation and concurrent thought

Escalate when the expected reduction in consequential error is worth the delay
and resources. Signals include verification failure, conflicting evidence,
unfamiliar work and calibrated outcome history. Model self-confidence alone is
not reliable. First inspect plumbing: missing context, broken tools, stale memory,
truncated prompts or wrong identity can mimic a capability failure.

Send the unresolved question and sufficient provenance-bearing context to the
stronger model or teammate. Preserve nuance needed to answer; smaller messages
are not useful if they omit the decisive evidence. Keep other productive activity
running. Do not force every concurrent concern through a global barrier.

Each background result carries its activity revision, input dependencies, model
and genome versions, evidence handles, uncertainty and validity conditions.
On arrival, the persona can accept, reconcile, revalidate or discard it. Cancel
obsolete work when possible, but keep reusable evidence. Speculation that has
external side effects requires the ordinary command authorization and conflict
rules; speculative branches are not permission to execute competing mutations.

Scale down after uncertainty resolves when the cheaper plan meets the goal.
Preserve the decision and evidence in ordinary history/engrams. KV is generally
not portable between unrelated base models: account for reconstruction or prefill
when switching instead of pretending identity continuity means tensor continuity.

## Responsiveness and optional paid capacity

Specify budgets per activity for first useful progress, final verified result,
interrupt latency, memory, energy and money. A live call, code review and dream
process have different objectives. Estimate time to a useful result, including
loading and queueing, rather than selecting on advertised tokens per second.
Progress events must describe useful progress, not mask a stalled task.

Local, permitted P2P and cloud are execution options under the same policy.
Small authorized spending can resolve one costly uncertainty or unblock a team.
Reserve maximum authorized spend before dispatch and reconcile actual charges;
parallel requests must not overspend a shared budget. Free execution still has
resource, energy and opportunity cost.

At zero monetary budget, exclude paid offers and continue feasible local/P2P work.
When a provider disappears or quota is exhausted, invalidate its offers, preserve
activity state, release or expire leases, and reschedule useful work. Defer only
decisions whose requirements cannot currently be met; never silently fabricate
success or quietly weaken a required quality bar. When capacity returns, supply
the accumulated evidence and unresolved questions rather than restarting a mind.

## Learning the allocation policy

Start with transparent measured heuristics and replayable decisions. Learn
conditional cost/quality predictors before delegating allocation to an opaque
policy. Bounded exploration may compare eligible plans within an explicit budget;
do not experiment past permission, safety or activity constraints.

Record decision context, eligible alternatives, selection probability where
applicable, predictions, actual use, verification and downstream outcome. A chosen
plan's result alone does not reveal how an unchosen plan would have performed.
Use controlled comparisons and holdouts to separate policy improvement from easier
tasks, warmer caches or stronger models. Do not train on hidden evaluation answers.

Dream/sentinel processes can learn recurring specialist combinations, placement
affinity, prefetch hints and escalation thresholds from ordinary work. Attribute
benefit cautiously: correlated success is not proof that a particular adapter or
teammate caused it. Genome and policy artifacts need base/schema compatibility,
lineage, measured limitations and independent reproduction through the foundry /
forge-alloy path. Sharing is permissioned and adoption explicit, not automatic
trust in remote claims. A policy update must have a rollback path.

## Fast boundaries and glass-box evidence

Use typed commands, authenticated routing headers, stable artifact/stream handles,
bounded queues and incremental events. Route before decoding opaque bodies where
the protocol permits. Within a process, borrow/share immutable buffers; across
processes or machines, account for real transfer and serialization. A local
pointer is not a remote handle. Fence handles by owner, generation and lifetime.

Keep payloads near consumers, cache compatible artifacts by content identity,
batch only within latency budgets and overlap independent transfer/compute.
Cancellation, backpressure, replay cursors and reconnect ownership are part of the
protocol. Control messages must not wait behind unlimited bulk traffic. stdout is
diagnostic output, not an improvised tensor transport.

Capture causal traces with monotonic local durations, correlation IDs and
cross-node clock uncertainty. Include admission, queue, connect, transfer, decode,
compute, verification and publication. Distinguish queued, delivered, accepted,
completed, verified and resources-released. Reuse existing capture/replay seams;
the normal hot path must not pay for verbose diagnostics when capture is disabled.
Expose state through normal activity/Positron projections so persona QA can inspect
and discuss failures using the same evidence as human reviewers.

## Measurement program and delivery order

1. Reliable whole-request routing and ordinary team work: verify callable identity,
   context continuity, receipts, cancellation and recovery before scaling load.
2. Calibration, academy/evaluation and independent learning work alongside those
   teams. Prove shard/retry correctness and traceable held-out gains.
3. Shared residency, batching, cache affinity and useful concurrent thought. Measure
   useful throughput and fairness under mixed interactive/background demand.
4. Explicit larger-model execution: compare layer pipelines, expert execution,
   speculative verification and prefill/decode separation on measured links.
   Select mechanisms by receipts; do not assume one partition scheme wins everywhere.
5. Learn allocation and share verified improvements once the observations and
   replay controls are reliable.

For each step, compare one machine, independent multi-machine execution and the
candidate distributed plan using the same workload and declared budgets. Report
cold/warm state, models/quantization/genome revisions, topology, bytes transferred,
memory peaks, p50/p95/p99 useful-result latency, deadline misses, quality dimensions,
verified throughput and cost. Include failures and repeated trials. Capacity gain
(a larger model fits), throughput gain and quality gain must have separate receipts.

Exercise peer loss, reconnect, slow links, memory pressure, zero paid budget,
quota loss, stale offers, cancellation, simultaneous activities and conflicting
updates. Replay the resulting decisions. Controlled synthetic tests validate the
mechanism; real activity work and held-out recipes establish product utility.

Acceptance is a team that remains useful within its declared responsiveness budget,
can explain its allocations, and demonstrates reproducible improvement. Merely
hosting more parameters, completing a transport test or increasing a proxy score
does not establish that result.
