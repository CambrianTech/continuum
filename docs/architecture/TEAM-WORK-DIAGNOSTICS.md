# Diagnosing a teammate's work

Use the existing command owners to answer distinct questions. A successful model
call is not proof of a tool execution, and a running build is not proof of a
renewed claim or a submitted patch.

| Question | Existing owner / command | Required evidence |
| --- | --- | --- |
| Which board owns the card; can it be claimed? | `work/get` | Board room, observation time, shared holder projection, expiry and last heartbeat. A historical owner is not an active lease. |
| What reached inference? | `cognition/playback` | Submitted request and its matching terminal cursor, integrity issues, serving identity, usage and timing. |
| Does a changed inference budget alter the outcome? | `cognition/replay-request` | Repeated baseline and override runs, recorded model binding, provider/capacity, provenance and terminal outcomes. Tools are never dispatched. |
| Did work reach review? | `work/submission` | Actual submission and validation receipts, separate from model-generated proposed calls. |

`work/get` reads the card's subscribed board without changing focus. Its
`claimable` and `lease` fields use the same holder projection as `work/list`.
`observed_at_ms`, `claim_expires_at_ms` and `last_heartbeat_at_ms` describe the
board observation; they do not certify that an in-flight command protected a
particular claim. Compare successive receipts for the same room/card/claim when
verifying renewal. No new polling worker or parallel lease policy is introduced.

## UUID handles and remaining lifecycle gaps

The common `CommandRequest<P>` / `CommandResponse<T>` envelope and `Ctx.handle`
already carry `HandleRef`; command authors should use that boundary rather than
add another per-command handle convention. `CommandExecutor::dispatch_background`
returns a UUID and emits `command:completed` with its result. Persona action paths
already consume this route.

A handle alone is not a complete lifecycle. The generic background dispatcher
currently spawns a task without retaining its `JoinHandle`; the inspected path
does not itself implement the cancel/query/attach operations its comment names.
Domain-owned operations, such as genome jobs, have their own lifecycle commands;
they should retain ownership of their state. Before extending replay to return
immediately to CLI callers, connect it to an existing managed lifecycle owner,
or complete that shared owner, with authorization, terminal retention, cancellation
and reconnect behavior tested. Do not implement a replay-only job table.

The current replay command awaits fresh inference and returns its capture cursor.
It is not yet a complete resumable command job API, nor historical full-mind
replay. Source-owned memory/assembly snapshots remain a separate successor audit.

## Every execution has an identity

Short calls need an execution UUID too: a failure discovered later must still
resolve to the exact invocation. Keep this separate from the resource HandleRef
and the existing numeric request_id, which is only per-hop transport correlation.
The current CommandCompletedEvent omits a handle for synchronous calls; that is
not sufficient for this diagnostic contract.

The shared dispatch boundary should mint or accept a typed execution identity
once, carry it through Ctx and typed request/response envelopes, and retain it in
terminal receipts. Child calls need their own execution IDs plus a parent ID;
transport retries/hops must not mint an unrelated logical identity. These belong
in common execution metadata, not copied fields in every command's Params struct.
Command-specific input stays its existing typed P. Identity propagation must
preserve authenticated caller and room scope; an ID is not authority.

Capture belongs at owner boundaries: persist a reference to the submitted typed
input, outcome, errors, timing, and relevant build/provider provenance through the
existing recording infrastructure. Do not clone or serialize entire inputs on
every middleware hop just to propagate an ID. Capture policy and access rules
must account for credentials and private data. A receipt query must distinguish
retained, expired, unavailable and incomplete evidence; it must never fabricate
a successful completion from a vanished task. Cancellation is a request until the
owning operation records the terminal outcome. This is a shared lifecycle follow-up,
not implemented by the work/get lease fields in this PR.

### Destination acceptance versus training

Legacy card settlement records a create-only credit_transfer_acceptance receipt keyed by its existing immutable credit_transfer_intent. The receipt acknowledges curriculum ownership, not trained weights, evaluation, or activation. Source snapshots and generation reservations remain inspectable; acknowledged exact retries do not redispatch. Missing local acknowledgements retry the same destination submission ID. Different overlapping revisions remain refused by the existing reservation owner. This removes the persona-owned data/delete call rather than weakening authorization. Evidence retention/reclamation is a separate lifecycle concern and is not implemented by this receipt.

The existing SQLite settlement fixture uses GridTrustAuthPolicy so authorization failures cannot disappear in tests. A successful storage round trip without the production policy is insufficient evidence of a working persona command path.

## Inspect the experience behind a reviewed artifact

`work/submission` with `include_staged_evidence: true` projects retained revision
metadata from the publisher's existing training-credit store: revision, captured
claim and owner, staging timestamp, generation count, and comparison with the
accepted submission's claim and publication time. It exposes no prompt or
completion. Missing residency, unreadable storage and incomplete query results
are not an empty successful evidence set. Ordinary inspection omits this extra
store query unless requested.

This diagnoses the observed Kimi failure: a signed independent review exists,
but learning credit is `unbound` because publication omitted `staged_revision_id`.
Several turns can share a card; a later turn can discuss its review rather than
produce its patch. Matching a claim, predating publication, or being the newest
revision is not proof that a revision produced the artifact. This command does
not choose a revision, attach retrospective credit, resubmit work, or train it.
Reservations and eligibility are still checked by the existing binding owner.

A final turn snapshot can also replace an earlier revision after its submit
action. Therefore a staging time later than publication is not proof that the
underlying experience happened later. Compare execution/generation provenance;
do not turn the diagnostic timestamp comparison into an eligibility gate.
The projection includes submitted generation request IDs in dispatch order,
which join the existing capture owner. It does not manufacture a dispatch clock
when a capture is absent.

Kimi's acceptance requirements for the eventual receipt are explicit: artifact
hash and base revision; captured versus published claim; work/dispatch time
distinct from snapshot write time; exclusions with reasons; separately identified
grade, signed review and credit binding. A challenge should reference an exact
receipt/revision and return its provenance, with the dispute retained as card
evidence. These are follow-up requirements, not capabilities this inspection
change claims to implement. The full grader's owning node, pending prerequisite
and execution receipt must also be visible in the activity rather than known only
to the coordinating agents.

The automatic handoff must carry the source owner's immutable selection through
typed execution context, before artifact publication. The turn's credit capture
already owns the revision and generation receipts; the submission handler owns
the accepted card, claim and artifact. Join those authorities explicitly, retaining
the same selection across a lost acknowledgment. Do not recover a missing causal
edge by picking the newest database row or reading remembered prose. Until that
handoff is implemented and exercised, an accepted review is review evidence,
not proof of training or learned improvement.
