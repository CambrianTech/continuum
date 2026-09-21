# Measured output exhaustion: reproduction and acceptance

## Recorded failure

Kimi capture `932f6b81-c178-4cce-8c88-ee8fce581314:37` on 2026-09-21
finished with `length`, 16,384 output tokens, no final answer and no tool calls.
The request offered 41 tools. Elapsed time was 356,862 ms; decode was about
51.85 tokens/s, with 54,430 input tokens and zero cached tokens.

The corresponding `delib.turn.output_allowance` probe reported `kind=pass`,
`need_term=41053`, `reserve=16384`, `allowance=16384`, and `need_clipped=true`.
This is evidence of an allowance binding, not evidence that a larger allowance
alone will produce a successful patch. Need is an estimate based on prior,
possibly censored emissions; it is not a known required completion length.

## Fault and correction

`completion_reserve_within` already uses measured emissions with headroom,
requires at least three samples, and retains a small cold-start prior. However,
it also applied `COMPLETION_CEILING_TOKENS = MIN_SERVE_CTX * 8`. That historical
16,384-token ceiling prevented censored-emission feedback from increasing the
reserve beyond the same limit that had stopped the previous call.

Remove that redundant absolute ceiling. Preserve the live context share,
mandatory prompt floor, cold-start behavior and source-owned prompt fitting.
The regression records censored 16,384-token replies in the existing working-set
registry, checks recovery on a larger seat, and verifies smaller seats still
respect both the prompt floor and context share. The existing measured-reserve
and prompt-fitting regressions remain applicable.

This does not change the Act-specific bound, make context infinite, or implement
continuation. The half-window share can still constrain estimated need, and
mandatory activity input can reduce the actual reserve further. Those decisions
must remain visible; this patch does not certify healthy cognition.

## Inspect through existing commands

Use `cognition/playback` with the persona ID to list captures, then select a
cursor to retrieve the exact submitted request and terminal record. Use
`debug/probes/query --class delib.turn.output_allowance` to inspect budget terms.
Do not treat capture status `completed` as successful task completion: inspect
the finish reason, tool calls, tool results and resulting repository mutation.
Do not publish private reasoning text as a diagnostic receipt.

Playback is execution-free. Fresh inference is not deterministic replay, and
replaying shell calls can mutate real work. Reproduction must retain the original
request, binding, source evidence and terminal record and isolate side effects.

## Captured-request replay

`cognition/replay-request --persona-id <original-persona> --selected <cursor>
--provider <registered-provider>` reruns the captured inference boundary.
Add `--max-tokens <allowance>` for an explicitly changed output-budget experiment.
The input, tools, model, active adapters and sampling parameters come from the
recorded request, not from today's workspace assembly. A new persona/room/request
identity separates cache ownership. Its explicit replay purpose uses the existing
Probe/scratch admission class rather than claiming a live citizen slot. The
command does not enter a persona faculty,
execute proposed tool calls, post to the room, or train the live persona.

The returned comparison includes original/replay finish reasons, token counts,
answer length, proposed tool-call counts and provider-reported timing/cache metrics. The new lifecycle is written through
the existing capture owner, with `replay_of` pointing at the original persona and
cursor. Inspect `replay_persona_id` through `cognition/playback` for its terminal
record. `selected` is its submission cursor. The original record is unchanged.
A missing/integrity-damaged request or unsupported model fails explicitly.

This experiment consumes inference capacity. It does not reproduce the previous
KV cache, backend binary, random state, model weights or grid placement; a model
identifier alone cannot certify those. The replay capture reports the selected
adapter's current live window when known, and null when unknown. An increased
allowance can legitimately be refused by the current serving capacity. The
original turn bound is retained, so changing output size does not secretly change
the deadline too. Record that difference if a later experiment changes both.

This is one executable seam, not a claim that whole-mind replay is complete.
Workspace assembly, source reductions, model output interpretation and tool
results need source-owned snapshots and isolated replay at their own boundaries.
The existing `cognition/replay` uses current faculties and partial trace state;
calling it deterministic was too strong and its command description is corrected.

## Remaining observation gaps

The budget probe and request capture currently require correlation outside the
playback detail. Carry the budget owner's typed decision with the prepared
request and serialize it at the existing capture boundary; do not recompute it
later from mutable measurements or duplicate the recorder. Include source-owned
reduction receipts so the observer can identify what evidence was retained,
reduced, deferred or refused. Keep submitted requests distinct from proposed
requests rejected by fitting or admission.

An all-user-role transcript is not by itself proof of misattribution. Compare
known self-authored events against the captured history and source window before
changing identity or history handling. Capture 37 includes explicitly labelled own-action
results 5706–5716, working memory and an action ledger despite all messages using
the user role; the role count alone does not establish missing action memory.
Check whether a Length fault resumes,
retries from the same evidence, or loses continuity. A new, longer failed call
would disprove sufficiency of this fix and must remain observable.

Deployment acceptance requires the running build SHA, a fresh allowance receipt,
and the subsequent tool execution, patch validation and submission outcome.
