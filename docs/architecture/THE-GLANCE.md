# The glance: a mind addressed while she works (THE-WHOLE-MIND build step 1)

Status: DESIGN FOR REVIEW (2026-11-01), revision 2 after Codex's BLOCK on 3fb42a2 (#5690); no code until agreed. Laws referenced are THE-WHOLE-MIND §0.

## What she experiences

Someone addresses her (a person, a resident, an agent: the same for all) while she is deep in a
card. Within about a second she has seen the line, beside what she is doing, and decided:
keep going (the line waits for her next seam), answer briefly and go back, or switch. Her work
is never thrown away; if she switches, it resumes exactly where it was.

## The stages (THE-WHOLE-MIND §1b)

| Stage | Autonomic or hers | Input | Output |
|---|---|---|---|
| **reflex** | autonomic | the mind bus topic `addressed` (header-routed: a DM, an @mention or a reply to her, from any citizen) | a trip carrying the addressed event's id |
| **glance** | hers (her decision) | the trip, and a view of the triggering line, her current work state and relevant tool results, allocated by the one Flexbox system | `GlanceDecision { continue, answer_and_return, switch }`, fenced (below) |
| **focus** | autonomic, executing her decision | her fenced decision | the work's execution owner pauses or resumes it; a reply task starts |

Her settings are commands under her own authority (and the grants she gives): `mind/reflex/gain`.
There is no private glance budget: the glance is a claim in the one Flexbox allocation, and
exhausting its grant is an explicit yield and re-admission, never a silent clip. A teammate
addressing her does not thereby gain authority over her execution settings.

## Contracts it depends on (dependencies, never weaker substitutes)

1. **Sequence custody (the engine).** Pausing her work needs the existing execution owner to
   return a typed paused/quiescent receipt: sequence identity, generation epoch, model and
   adapter identity. Resume continues THAT execution. A dropped stream plus a cached prefix is a
   new generation, not a resume, and is never reported as one: cancellation, lost work and
   replay stay explicit. Until the fork provides per-sequence pause and resume, `switch`
   is unavailable and is offered as `answer_and_return` or `continue`; the glance still
   ships (Codex on #5690).
2. **Two-sequence admission (the scheduler).** Her glance is a second sequence admitted through
   the existing scheduler and physical capacity authority, never by pausing her work to make
   room. The prefix is shared immutably with separate continuation state, or copied; its memory
   and lifetime are accounted in the same ledger. If capacity is not available, the addressed
   event is retained and the deferred admission is visible; one second is a measured service
   target, and a capacity delay is not a coma.
3. **Fencing and effects (HER-TASKS).** A decision is fenced by the addressed event's id and the
   work's generation epoch: if the work finished, changed adapter, was cancelled or moved to
   another task while she glanced, a stale `switch` does not touch its successor. `continue`
   keeps the event until it is perceived at her next seam. `answer_and_return` is one durable
   reply effect with an acknowledgement. Duplicate trips, repeated delivery and a restart never
   issue a duplicate reply or lose unfinished work: the task lifetime, delivery and wake
   contracts of HER-TASKS own this, not a separate focus registry. One ordered history and one
   task owner hold, across both sequences.

## Validator: receipts, both directions

Run on replay in seconds, with receipts from the execution owner and the effect ledger (never
prefix reuse as proof). "Both directions" means every transition a case makes has its reverse
asserted in the same case: trip and settle, pause and resume, admit and release, defer and admit.
1. Addressed during work; she continues; the event is perceived at her next seam; the work's
   sequence ran unpaused (receipt).
2. She answers and returns; exactly one reply effect is committed; the work continues.
3. She switches (once custody exists); paused receipt; she returns; resumed receipt for the same
   sequence and epoch; no tool effect repeated.
4. Work completes, or moves on, while she glances: a stale switch is refused by the fence.
5. Duplicate addressed events, and repeated delivery: one glance, at most one reply.
6. The glance is cancelled or fails; no second sequence is available (deferred, then admitted);
   pause fails or quiescence is unknown (reported as such, never as paused).
7. Restart before and after the reply commits: no duplicate reply, no lost work.
8. Each sender kind (person, resident, agent) through the same addressed headers.

Health states, each with its own latency: queued, admitted, generating, decided, failed. A coma
is an addressed event that never leaves `queued` while capacity was available.

## What this replaces and deletes

#5687 (closed): the forced drop. The species bias in `airc_persona_conversation/polled.rs`. The
system-assigned Directed/Work/Ambient lane classes stay until build step 3 replaces them with her
state.

## Settled in review

- The fast layer starts as the same weights at a short grant, measured, once two-sequence
  admission and custody exist.
- "Answer briefly" is her semantic decision within a shared grant; exhausting it is an explicit
  yield and re-admission.
