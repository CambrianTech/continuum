# Automatic genome selection and an evolving shared skill repository

Status: implementation design, 2026-09-16. This specifies missing connections;
it does not claim that automatic paging or publication already works end to end.

Extends [continue/fork/mint](GENE-LIFECYCLE-CONTINUE-FORK-MINT.md),
[HF repositories](../../architecture/GENOME-REPOSITORY-ON-HF.md), and the
[commons trust spine](../GENOME-COMMONS-TRUST-SPINE.md). Use the existing
[concurrent substrate](../../architecture/CONCURRENCY-STYLE-GUIDE.md).

## Two independent controls

A persona's composition and a publisher's skill repository are different objects.

* **Composition:** automatic by default. A persona or authorized colleague can
  select and lock a composition, introspect it, and release that lock. The lock
  belongs to the persona's execution state. It is not an HF publishing mode.
* **Skill repository:** one durable skill identity with an evolving recommended
  revision. Continued learning updates its weights, signature, evidence and model
  card together. Revision hashes identify evaluated bytes and ancestry; they do
  not require publishing or retaining every training checkpoint forever.

Owners govern updates to their namespaces. CambrianTech is the initial owner of
its own published skills, not a special authority embedded in the runtime. Every
installation can search permitted providers, evaluate candidates and publish to
its own configured destination. Receiving an artifact grants no upstream write
permission. Another team's improvement can be proposed upstream or maintained
as a derivative with its original ancestry intact.

## What exists and what remains unproven

Source audit on 2026-09-16:

| Existing seam | Reuse and missing connection |
|---|---|
| `genome/signature.rs`, recall sources and scoring | Embedder-versioned similarity, fitness and provenance inputs exist. Reuse these for demand selection, not keyword role tables. |
| `commands/genome_recall.rs` | Manifest/signature/fitness retrieval and candidate ranking exist. Move reusable work behind the shared owner; do not repeatedly read files on each inference. |
| `genome/residency.rs` | Existing `ServiceModule` and working-set manager; constructor registration found only in tests. Its persona-UUID overlay placeholder is not a resolved adapter artifact. Registering it alone would not complete paging. |
| `cognition/persona_workspace.rs` | Live cycle and deliberation share an atomic genome handle, initialized empty. |
| `WorkspaceCycle::page_in` | Publishes adapter requests to that handle; discovered callers are evaluation/tests. Automatic production selection is not established. |
| Genome training, fitness and Forge Alloy surfaces | Reuse for training and receipts; prove promotion, publication and consumer adoption separately. |

Older Node/TypeScript daemon documents are historical implementation references,
not a requirement to add another process. The owner should be an independently
scheduled existing Rust module/faculty. Use `ServiceModule`, typed events,
`watch` snapshots and the existing pager. A dedicated OS process is a deployment
choice only where isolation or a backend requires it.

## One command contract, automatic policy as a caller

Extend existing registered commands where possible. The following are semantic
operations, not assertions that these exact CLI names already exist:

| Operation | Contract |
|---|---|
| Inspect/search | Explain current composition, mode, candidates, compatibility, evidence, costs and selection reasons. Search does not activate. |
| Select policy | Bind a versioned policy adapter to a persona/activity through the command surface; inspect or replace that binding using its expected generation. |
| Select | Propose a typed composition for a persona, optionally locking it. Same validation and activation path as automatic selection. |
| Lock/release | Hold an exact resolved composition or return to automatic selection. Durable across restart, with actor, scope and generation. |
| Prefetch | Obtain verified artifacts within budget without changing active cognition. |
| Propose refinement | Submit attributable experience to existing dream/training machinery. |
| Evaluate/promote/publish | Separate evidence gathering, local recommendation and owner-authorized repository update. |

In-process callers use typed references/handles. Serialization happens at actual
IPC/network/persistence boundaries. No command-specific second paging pipeline.
Use expected-generation checks so a stale automatic proposal cannot overwrite
a newer lock or manual decision. Changes to another persona require that persona's
existing delegation/authorization policy. A lock prevents automatic replacement;
it does not guarantee permanent GPU residency. If it cannot be served, expose the
reason rather than silently changing its composition.

## The selection policy is an adapter

Policy is an explicit, replaceable adapter contract, not a hidden fixed scoring
algorithm inside the pager. A command selects it. Implementations may be a static
rule, a learned model/LoRA, a training or evaluation controller, an external agent,
or the persona herself. A policy adapter need not be a weight file. These providers
receive the same typed demand/candidate/budget snapshot and return attributable
selection proposals through the same command path.

The binding records adapter identity/revision, configuration, serving tier, scope,
actor and generation. The demand snapshot comes from the existing perception
`watch::Sender<Snapshot>` seam; its representation scales with the served model.
A small tier must not impose its context or policy limits on a frontier tier. Policy selection is distinct from both composition locking and artifact
publication. Selecting a different policy does not unlock a held composition.
Training/testing mode is explicit: shadow proposals can be evaluated and recorded
without changing the live composition. An experimental policy acts live only under
the activity's authorized mode and budgets.

The concurrent owner invokes the selected policy outside inference's critical path.
Local policies use typed calls; a remote agent uses the existing AIRC command/reply
contract with cancellation, deadlines and generation matching. Slow, failed or stale
proposals cannot hold the serving lane or overwrite newer intent. The pager enforces
compatibility, capacity and authority independently of the policy implementation;
the policy proposes choices rather than bypassing those constraints.

Bootstrap uses an explicitly configured policy binding. Evaluating that binding
does not recursively select another policy. A policy can propose its own replacement,
but replacement is a recorded command decision under existing authority, validated
and activated between evaluations. The incumbent revision owns its in-flight
decisions; it cannot rewrite their attribution after learning.

## Record actual execution, then learn from it

Capture the model-visible request before dispatch, referencing existing immutable
artifact identities rather than copying payloads. Correlate policy input and output,
candidate set, selected and rejected revisions, lock/mode, composition actually
admitted by the backend, base/tokenizer identity, layer scales, KV identity and
executing node. Link tool results, model outputs, latency/memory, outcome evaluations
and subsequent training to the same causal chain. A proposed composition is not an
execution receipt; a ready roster is not a capability result.

Use the existing recorder, artifact store and activity projections. Define capture
durability as part of the activity contract; a failed required capture must be visible
before dispatch, not silently described later as a replayable run. Local private
context stays access-controlled; public HF/Alloy evidence exposes only permitted
records. Replay requires retained referenced artifacts and records stochastic/backend
limits rather than promising bit-identical regeneration.

These records train both specialist skills and the policy adapter: which expertise
helped, at what cost, on which tasks and hardware. Preserve alternatives and selection
probabilities when the policy exposes them, so selection bias is measurable. A trace
alone is not counterfactual evidence; validate policy improvements on held-out real
activities before promotion. The same provenance and publishing machinery applies to
a learned policy, without forcing static or agent-backed implementations into LoRA.

## Predict before inference; activate coherently

1. Existing perception/working-memory events publish a versioned demand snapshot:
   persona, activity, task intent, retrieved context handles, model binding and
   budgets. Delegation changes this demand naturally. Never require a persona to
   utter a magic skill name.
2. The concurrent owner coalesces superseded demand and queries the existing
   signature/recall index. Similarity is valid only within a compatible embedding
   space. Unsupported spaces require explicit re-embedding, not comparison of
   unrelated vectors.
3. Compatibility and authorization are gates before ranking: base revision,
   architecture, tokenizer/adapter format, supported composition, provenance and
   available resources. Rank eligible candidates using measured task lift,
   similarity, local costs and uncertainty. Popularity is at most a prior.
4. Prefetch promising candidates asynchronously through existing artifact handles
   and working-set/paging budgets. Bound speculative work and cancel obsolete
   demand. Keep useful resident expertise through related turns to avoid churn;
   calibrate switching cost from measured loading and cache loss. Admission spends
   planned headroom (weights, served-window KV times lanes, and reserves), not a
   transient free-RAM reading; reuse the serving planner
   (`serving_daemon::sidecar_planned_headroom_bytes`). Prefetch participates in the
   existing `PagedResourcePool` with a real `evict_at_least`; checkpoint/prefetch
   directories are `TrackedDir` resources with an explicit eviction policy.
   Measured decode cost and the resulting lane knee are selection inputs.
5. At an inference admission boundary, resolve one compatible ready composition
   and lease it for the request. Atomically publish its identity and adapter set.
   Never mutate weights midway through a generation or silently combine mutually
   incompatible layers. Missing expertise is visible; continue with the current
   valid composition only when the activity permits it, otherwise expose a wait
   or placement failure.
6. KV ownership includes base/tokenizer identity, ordered adapter revisions and
   scales, and relevant backend semantics. A changed composition cannot reuse
   incompatible KV. Put the composition revision inside `inference/slots.rs::ActivityKey`
   so `KvSlotPool` and its page directories retain one ownership check. Do not add
   a second sidecar KV identity gate. A hash alone is not proof that the backend honors isolation:
   validate slot behavior, especially a backend with global adapter controls.
7. Outcomes feed existing experience and fitness owners. Attribute observations
   to the exact composition and task, rather than assuming every success proves
   an adapter helped. Use controlled comparisons for promotion claims.

Predicted context is the early signal. Struggle or uncertainty is an additional
signal for reconsideration, model placement or colleague help. It is not required
before appropriate expertise becomes available. Switching activities preserves
persona identity, history and responsibility.

## Discover, refine, diverge, consolidate

Similarity and provenance supply the search space; behavioral evidence makes the
decision. Provenance establishes origin, not quality. Distance alone cannot tell
whether a regression is a useful new specialty or simply a bad update.

* **Reuse:** search local cache, fleet and configured public indexes for compatible
  skills before training. Cache metadata and incrementally refresh changed heads;
  fetch weights only for selected candidates. Public search works without requiring
  an upload account. Offline installations continue from their local index.
* **Continue:** cluster attributable new experience and train a candidate from the
  nearest suitable parent. Require improvement on new demand and retention of the
  parent's declared capabilities on held-out evidence before moving its head.
* **Reject/retry:** a failed retention gate first identifies a failed assimilation.
  Check training validity and reproducibility. Do not publish a fork merely because
  an optimization step regressed or one noisy test failed.
* **Fork:** when repeatable evidence establishes valuable new behavior that conflicts
  with the parent's niche, the existing learning policy proposes divergence under
  owner rules. Record the parent revision, conflicting evaluation outcomes and new
  niche signature. The parent repository continues evolving; it is not frozen.
* **Mint:** absence of a suitable candidate permits a local experiment. Distance
  alone does not authorize a public repository. Require useful distinct behavior
  and a nonredundancy check before publication.
* **Consolidate:** periodically test whether overlapping skills can be compressed
  or distilled into a smaller, equally capable replacement. Compression is a
  candidate transformation with retention gates, not an automatic quality claim.

No fixed capability threshold invented for this design. Existing recipes and owner
policy specify evaluation budgets, tolerances and evidence requirements; retain
their versions in receipts. Insufficient evidence means undecided, not divergence.
Training and selection can learn from ordinary work and benchmark activities through
the same experience stream, without exposing held-out answers to training.

## Update the existing repository, not a checkpoint landfill

The publishing adapter commits promoted weights, model card, signature, parent
revision references and Forge Alloy evidence as one coherent revision. Update the
same skill's recommended head after successful publication. HF is the first
destination; other repositories/mirrors implement the same storage contract.
Do not rely on a hosting site's human-facing family tree to encode the complete
composition DAG: Alloy retains explicit parent hashes and locations.

Publication is an idempotent durable job. Stage artifacts, verify them, commit the
revision and advance the head with an expected-parent check. A concurrent upstream
update triggers re-evaluation against the new head or a review proposal, not a
last-writer-wins overwrite of another team's learning. Report partial failures and
retry safely without generating duplicate skill repositories.

Ownership, room/persona sharing policy and training-data rights gate publication.
Private raw context is not uploaded with the weights. Extraction tests and local
evaluations reduce risk but are not mathematical proof of privacy or harmlessness.
Signing proves authorship/integrity, not truth of a capability claim.

Keep failed/intermediate checkpoints local under bounded retention. Content-address
and deduplicate promoted artifacts. Retain versions referenced by active compositions,
benchmark receipts, rollback policy and supported lineage dependencies. Garbage
collection distinguishes metadata references from a promise to retain reconstructible
weights, uses a grace period and never deletes an active lease. Old receipts may
remain as compact evidence while explicitly marking deleted weights unavailable.
External hosting may retain old objects under its own rules; a local deletion is
not a promise of remote erasure. No publish per turn or per training step.

## Implementation and acceptance order

1. **Composition contract:** expose inspect/select/lock/release through existing
   command registration, with durable modes and atomic generation checks. Prove
   automatic selection cannot undo a lock, including after restart and rehoming.
2. **Live paging:** connect demand, existing recall and pager to the shared genome
   handle. Prove real artifact resolution, compatible activation and request lease;
   tests that only exchange persona UUIDs do not establish adapter loading. Require
   a production registration ratchet (registered or explicitly dormant with a card)
   and a live `genome.page_in {persona, gene, reason}` probe. A test-only module
   registration cannot satisfy this step.
3. **Measure:** on a real project, change from implementation to review/planning
   work, show automatic demand and selection receipts, then compare useful outputs
   and latency with the base/current composition. Exercise concurrent personas with
   different compositions and prove no adapter/KV cross-contamination.
4. **Learn:** use real work outcomes to continue a parent, reject a harmful update,
   and demonstrate a justified fork. Validate compression against retained behavior.
5. **Share:** automatically update one owned HF skill repository and its card through
   the adapter. A second ordinary installation discovers, verifies and evaluates it.
   Demonstrate offline recovery, interrupted publication, racing updates and bounded
   artifact retention. No machine names, developer paths or privileged root org baked
   into the mechanism.

Expose these state transitions through the existing activity/Positron view and glass
box: demand generation, selection reason, candidate hash, prefetch cost, activation,
lock owner, cache reuse/invalidation, training lineage, evaluation and publication
receipt. Preserve exact composition attribution in replay. Acceptance is useful
work with measured gains and recovery, not just a loaded adapter or a passing mock.
