# Adaptive genome: implementation handoff

Status: handoff snapshot, 2026-09-16. This is an implementation work order, not a claim that automatic paging, learning, or publication works end to end.

## Objective and canonical design

Make expertise follow ordinary activity demand automatically, while preserving a persona's ability to inspect, select, lock, and release their composition. The selection policy is an adapter chosen through a command. Record actual execution so colleagues can explain, replay, compare, learn from, and improve decisions.

Read the [adaptive genome lifecycle design](ADAPTIVE-GENOME-LIFECYCLE.md) first. Supporting documents:

- [Persona cognition pipeline](../../architecture/PERSONA-COGNITION-PIPELINE.md): existing mind and live inference path.
- [Concurrent substrate](../../architecture/CONCURRENCY-STYLE-GUIDE.md): ownership, snapshots, scheduling, and paging.
- [Continue/fork/mint](GENE-LIFECYCLE-CONTINUE-FORK-MINT.md): refinement and evidence for divergence.
- [HF repository architecture](../../architecture/GENOME-REPOSITORY-ON-HF.md) and [commons trust spine](../GENOME-COMMONS-TRUST-SPINE.md): publication and provenance.
- [Activity reconciler](../../architecture/ACTIVITY-RECONCILER.md): restart and resumption ownership.
- [Benchmarks are adapters](../../architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md): benchmarks use the ordinary activity pipeline.

Read AGENTS.md/CLAUDE.md before implementation. Coordinate the lane over AIRC before modifying shared owners. Extend existing code rather than introducing a parallel mind, pager, database pool, or benchmark runner.

## Landed, pending, and unproven

| Item | Receipt and limit |
| --- | --- |
| Lifecycle design | [PR #4119](https://github.com/CambrianTech/continuum/pull/4119), merged `20d754a49`. Design only. |
| Selection capture/replay | [PR #4122](https://github.com/CambrianTech/continuum/pull/4122), latest pushed head `d8281c379`, pending at handoff. Do not assume it is on canary. |
| Windows preparation replacement | [PR #4123](https://github.com/CambrianTech/continuum/pull/4123), merged `972994d32`, reviewed and CI green. Deployment recovery remains a separate gate. |
| Clippy debt | [Issue #4116](https://github.com/CambrianTech/continuum/issues/4116). Last strict run had 601 diagnostics, none in the replay PR's changed files. Repository lint is not green. |
| Storage handle contract | [Issue #4121](https://github.com/CambrianTech/continuum/issues/4121). Storage `handle` conflicts with the envelope's `HandleRef`; tests using the existing `dbPath` alias do not fix that API. Coordinate ownership. |

PR #4122 records candidates, query/context, weighted-policy configuration/revision, clock, and ranking result in the existing main ORM adapter. `genome/recall/replay` verifies exact ranking reproduction and evaluates counterfactual configurations against frozen inputs. Counterfactual results are unrecorded; they cannot impersonate original receipts. Capture access is restricted to the originating caller or node owner, including through generic data commands.

It does **not** implement general policy binding, automatic production selection, actual adapter admission, composition locking, inference replay, policy learning, or HF publication. Weighted-policy parameters configure one implementation; they are not the complete policy adapter contract. Selection receipts do not prove weights were loaded or used.

Validation: 207 native recall tests plus one Owner ACL regression passed. Hosted CI then reported 8,116 passes and two failures, both for missing documentation on `traceId`. Head `d8281c379` adds the description and regenerated TypeScript; nine native conformance tests and the binding-export test pass. Hosted checks must pass on this revised head before merge. The earlier failure was real, not infrastructure noise.

## First pickup: finish existing gates

1. Read #4122's current diff, reviews, and checks; merge the reviewed green head. Preserve capture/access regressions and generated bindings.
2. Establish an identified deployment before drawing live conclusions. The prior Windows preparation compiled successfully but failed replacing its receipt; #4123 fixes that exact failure. Retry the supported public installer, verify a new successful preparation, then use its supported resume path. Never consume an older receipt to bypass failed preparation.
3. Verify the running core revision, actual engine executable/revision, serving readiness, and useful resumed activity. Installed files or a ready roster are insufficient. Coordinate with peers so old-binary behavior is not diagnosed as a fresh bug.
4. Claim the bounded slice below in the shared board/room, with an owner and reviewer. Attach this document and the implementation PR; preserve continuation state outside chat history.

## Next implementation PR: policy binding and composition authority

Implement the composition contract before speculative live prefetch. Audit existing owners first and extend their commands/state. Exact command names must follow the current registry; the following operations are semantic requirements.

- Inspect effective policy binding, composition, auto/locked mode, actor/scope, generation, and unresolved or unservable reasons.
- Select a versioned policy adapter through a typed command with configuration and expected-generation checks. Start with the existing weighted implementation. Support static, learned, external-agent, and persona implementations without requiring every policy to be a LoRA.
- Select a resolved composition, lock it, and release it back to automatic selection. Use the same authority/compatibility path for automatic and manual proposals. Persist state across restart and rehoming.
- Keep policy binding, composition lock, and publication head separate. Changing policy does not release a lock; an HF update does not silently alter a locked composition.
- Reject stale proposals atomically. A policy evaluation preceding a manual lock/binding change cannot overwrite newer intent. Attribute decisions to the revision that made them.
- Reuse verified caller/delegation authority. A name or transport arrival is not permission to modify a colleague.
- A lock promises composition, not permanent GPU residency. Report unavailable artifacts or capacity according to the activity contract instead of silently substituting.

Acceptance: real command registration/shared persistence; competing manual/automatic updates; restart restoration; unauthorized mutation; policy replacement during evaluation; and real persona/owner inspection. Extend existing fixtures. Serialization-only tests do not establish behavior.

## Following slice: demand to real paging

Use existing perception watch snapshots as demand. The concurrent owner coalesces superseded demand and invokes policy outside inference's critical path. Delegation and context changes naturally change demand; no magic skill-name prompt is required.

Concrete seams from the design audit, to recheck against the current tree:

- `genome/recall_impl.rs` and `commands/genome_recall.rs`: ranking/capture. Share indexed retrieval instead of reading manifest/signature files on every inference.
- `genome/residency.rs`: existing module/working-set owner. Test registration and persona-UUID overlay placeholders do not prove production artifact resolution.
- `cognition/persona_workspace.rs` and `WorkspaceCycle::page_in`: shared genome handle and admission path. Read the cognition pipeline before editing.
- `inference/slots.rs::ActivityKey`: composition revision belongs in the existing KV ownership identity, not a second competing check.
- Serving planner, `PagedResourcePool`, and `TrackedDir`: planned headroom, eviction, and bounded artifact storage.

Gate candidates on authority and base/tokenizer/backend compatibility before ranking. Resolve real artifact revisions. Prefetch within planned weight/KV/lane reserves, cancel obsolete demand, and lease one coherent composition at admission. Never change adapters mid-generation. Explicitly test backends with global adapter state for cross-persona contamination.

Required evidence: production registration ratchet; live `genome.page_in {persona, gene, reason}` probe; observed admitted revisions; concurrent distinct compositions; valid KV reuse and required invalidation; cancellation, missing artifacts, pressure, and peer reconnection. Measure selection-to-admission p50/p95, loading, cache behavior, memory, and useful throughput against the same baseline. Derive budgets from recipes and measured hardware, not arbitrary constants or the smallest model tier.

## Replay, learning, and publication

Extend existing turn/RAG recorders and artifact identities to connect selection to the actual request, admitted composition, base/tokenizer/backend, executing node, tool results, output, costs, and outcomes. Capture required evidence before dispatch or fail visibly under the activity contract. Ranking replay is not inference replay; report missing retained artifacts and stochastic/backend limits honestly.

Feed attributable ordinary work and benchmark outcomes into existing academy/dream/training owners. Evaluate retained and new capabilities on held-out tasks. Traces alone prove neither improvement nor causation. Use matched comparisons before promoting a phenotype or learned policy.

Publishing is a durable adapter job: promoted weights, card, signature, parent references, and Forge Alloy evidence form one coherent revision of an owned repository. Advance its recommended head with an expected-parent check. Retry interruption idempotently; racing updates require reevaluation rather than overwriting another contributor. Fork for evidenced useful divergence, not every failed training run. Deduplicate artifacts; retain those needed for leases, rollback, and supported replay. Private context is not public evidence by default.

Final acceptance: a second ordinary installation discovers, verifies, adopts, and independently measures a published gain, including offline and restart behavior. No developer paths or privileged root organization belong in the mechanism.

## Completion evidence

Each implementation PR names its existing owner/seam, commands changed, actual validation, performance impact, deployment receipt, and remaining unproven behavior. Demonstrate real implementation/review/planning work through the normal activity pipeline with selection, admission, and outcome receipts. Compare the same tasks and budgets without the change. Keep held-out answers out of learning.

The handoff succeeds when the receiving engineer can identify the running build, reproduce current behavior, and own the next bounded card. The feature succeeds when useful peers select, use, learn, and share expertise through this ordinary pipeline with measured gains and reliable recovery.
