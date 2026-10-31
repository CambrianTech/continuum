# Focus: one allocation, adaptive attention

Status: normative design for implementation and review, 2026-10-28. Full compliance
and a learned attention policy are NOT yet demonstrated. Joel's direction is to
use the existing generalized Positron/RAG flexbox machinery throughout the mind.
This is the canonical focus contract; implementation inventories and receipts link
here rather than defining another budgeting policy.

## Purpose and boundaries

A persistent mind can pursue several activities while giving its current work a
compact, relevant view. After reading code, the file, intended change, results and
review feedback deserve detail. Other activities remain reachable at cheaper
levels. Switching activities changes their claims, without erasing their context.
Recency alone is not relevance: goals, dependencies, uncertainty and commitments
also matter. Focus is adaptive allocation of detail, not a claim of consciousness.

The existing cognition pipeline owns assembly. The context/engine design owns
physical capacity and KV continuity. The concurrent-mind design owns scheduling.
This contract joins their boundaries; it creates no second allocator or scheduler.
It supersedes conflicting local prompt-sizing recipes, not durable-state,
authorization, ownership or lifecycle contracts.

## Enforced allocation contract

1. One logical allocation covers a generation's framing, tools, required input,
   conversation, action/results, grounding, recall, working memory and reply.
   Sources supply claims and levels; they do not independently size or clip views.
2. A claim identifies its source and activity, stable content/provenance handle,
   whole-level want, mandatory floor, available representations and their costs,
   and attention priority. Required dependencies must remain representable.
   This is a semantic contract, not a requirement to duplicate existing types.
3. Representations can include whole, summary and pointer. A cheaper level must
   retain a resolvable path to its source. A pointer is not compliance if retrieval
   fails on the receiving node. Missing content is explicit, never silently lost.
4. Required current input and the newest result needed for the next action stay
   whole. If mandatory claims plus reply cannot fit, return an explicit capacity
   outcome for scheduling/replanning; do not cut their heads or silently omit them.
5. Wants, grants and actual sent costs are distinct. Full demand is the sum of
   whole claims plus framing/reply accounting, with no double counting. Changing
   focus priorities or grants must not change that demand for identical claims.
   Serving must never infer need solely from an already-focused prompt.
6. Allocation uses an explicit capacity snapshot and policy revision. A changed
   capacity invalidates or revalidates the allocation before send; source-local
   cache credits and spawn-time fractions cannot silently change its window.
7. RAM/disk retention limits are named storage budgets. They do not decide what
   the mind perceives. Storage eviction preserves the durable retrieval contract.
8. Capture records the exact fitted request at send and its actual response.
   Learning uses those lived calls, not the unallocated wants or an imagined view.
   Bind that capture to its allocation receipt: capacity and policy revision,
   claim identities, wants, selected levels and grants. Preserve the linkage to
   subsequent feedback so attention learning can attribute outcomes to decisions.

An allocation may use internal solver iterations. The forbidden duplication is a
source making its own competing allocation before the common owner sees its claims.
Token estimates must be reconciled with the target tokenizer and serialized wire
request; multimodal costs require the backend's applicable accounting.

## Learnable attention policy

The allocator enforces feasibility and preservation. A separately versioned policy
assigns priorities or utility to levels using activity intent, causal dependencies,
recent results, feedback, uncertainty, interruption obligations and time waiting.
No permanent percentages for coding, conversation or any other activity are part
of this contract. New modalities and activities implement the same claim interface.

An initial explicit policy is a baseline, not evidence of learned attention.
A learned policy may improve priority/level preferences but cannot waive floors,
access rules, retrieval, capacity or capture. Its decision receipt names its input
features and version sufficiently for replay. Avoid a new large-model call on every
allocation merely to choose priorities; measure policy cost as part of latency.

Train/evaluate against useful task outcomes and feedback, alongside latency,
prefill, retrieval mistakes and neglected commitments. Fewer tokens alone is not
success. Compare with a frozen baseline on held-out activities, including switches
and interruptions; test unrelated work, restart retention and subsequent use.
Keep the focus-policy acceptance distinct from the gene-learning acceptance gate.

## Node and scheduling contract

Every node composing or executing a view must honor the same semantics. Carry or
reconstruct claim/representation provenance and full demand across handoff; validate
the target's actual capacity and retrieval access. A remote boundary must not add
another silent crop or reinterpret a grant as a want.

Conformance requires behavioral evidence, not matching installed versions. Record
installed and running revisions separately. An incompatible node cannot claim the
focus capability; route to a compliant owner or give an explicit incompatibility.
Mixed-version operation must preserve existing jobs and identities during rollout.

Focus chooses detail within an activity; scheduling chooses when activities run.
Both consume shared demand and measured resources. A bounded, priced training
reservation may temporarily reduce concurrent warm slots after draining active
work and retaining their KV/state. It must release/restore admission on completion,
cancellation, failure and restart. Unknown memory price is not an admission grant.
Do not implement permanent lane reduction from a stale training-demand record.

## Division of work and review gates

| Boundary | Coordinating owner | Deliverable |
|---|---|---|
| Single allocation and sources; Positron/RAG semantics | Fable | Source migration, private sizing APIs and `self_sized` ratchet at zero |
| Serving, inference and grid | Codex | Wants/grants consumer audit, capacity and handoff conformance, Windows running receipts |
| Lived capture, Dreaming and genes | Cormac | Capture-after-allocation invariant, durable demand/recovery review and learning receipts |
| Modality/provider integrations | Relevant existing module owners, coordinated by Fable | Same levels/cost/retrieval contract; no independent prompt budget |

These are coordination boundaries, not permission to overwrite another owner's
checkout. Exact review heads and implementation receipts belong in the work board
and PRs; this document must not become an ever-growing status log.

Required executable validators before all-node compliance is claimed:

- Every production source enters the single allocation; ratchet zero cannot be
  achieved by renaming sizing helpers or excluding production paths.
- Identical claims under two focus policies retain identical full demand; grants
  change, mandatory content and reply remain valid, serialized request fits.
- Focus and activity switching preserve all source handles, including cross-node
  retrieval, unavailable-source handling and newest required results.
- Storage pressure changes retention location without independently cropping view.
- Capacity changes, interruptions, restart and mixed-version handoff fail explicitly
  or replan without losing accepted work, identity, KV continuity or demand.
- Recorder output equals the actual request/response; training lineage identifies
  that capture and cannot substitute whole wants for what the model experienced.
- Each node publishes its exact tested/running receipt. A coordinated 15-minute
  live read measures four residents, useful acts, prefill, full-act latency and
  retrieval failures against baseline. Preserve the existing rollback rule.

## Documentation map

- [Cognition pipeline](PERSONA-COGNITION-PIPELINE.md): assembly and source ownership.
- [Context and engine](MIND-CONTEXT-AND-ENGINE-CONFIG.md): capacity, KV and placement.
- [Concurrent mind](CONCURRENT-MIND-AND-GOVERNOR.md): activity scheduling and continuity.
- Flow implementation plan, PR #5560 section 2d: migration order and measured
  non-compliance inventory. It implements this contract; its ratchet is enforcement.

Proposed field/type changes remain implementation-review decisions. This contract
does not assert that the learned policy, all-node rollout or canonical gene proof
already exists.
