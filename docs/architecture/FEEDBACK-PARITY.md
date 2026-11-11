# Feedback parity: a mind hears the world at least as well as Claude does

Joel, 2026-11-11: "You literally know what you do and how you get feedback (which we can do BETTER
if we try) so we know the gaps." The spec for a resident's feedback is the loop a frontier agent
(Claude Code) already runs on, in any activity, any language, any program. Every row is a channel;
the gap column links the card that closes it. "Verify" means nobody has measured it yet: measure,
never assume.

| Channel (how Claude gets it) | A resident today (2026-11-11, M5 captures and source) | Gap / card |
|---|---|---|
| A command's whole outcome: stdout, stderr, exit status | Shell result; large output kept as an expandable handle | none |
| A long command returns a handle at once; ANY end of it (exit, crash, kill) wakes the agent | A completion push exists (`ExecutionState.handed_back` -> `command:completed`); Benchy read a running build 4 times instead; a core restart lost her command with `not_found` | ce191b8c |
| Any stream as events (a growing log, a process it did not start, a message bus), each new line delivered, the stream's failure delivered | Nothing equivalent | ce191b8c |
| Messages from others arrive between its steps, also mid-task | Her rooms' messages arrive; mid-act delivery: verify | verify |
| The human interrupts mid-task, and is answered | Typed asks through positron; as an interrupt: verify | verify |
| A failure carries its evidence (raw output, path, line, what to try) | Typed refusals with next moves for some; bare strings for many (`stringly_command_errors` ratchet) | d1ab6ea3 |
| Working state survives a context restart (a summary carries the plan and the files in hand) | A view turnover drops her plan and what she read; she re-reads (Aiko, Aris: 0 edits in 14 acts) | 89655d88 |
| Lessons written by the agent itself persist across sessions | Genome and engrams; a reviewed correction kept across restart is not yet proven | learning line |
| It reads its own transcript and any log or process on the machine | Access is open (her shell is the operator's); nothing points her there | d1ab6ea3 |
| A thought ends in an answer; effort is set per task | A thought can hit the output cap and the act is lost (Aiko 09:03Z, 7,170 tokens) | f4a89406 |
| Time: it knows how long it has been and is told when it has gone quiet | verify | verify |
| It starts helpers in parallel and their results arrive as notifications | verify | verify |

**Status has two columns, kept apart** (Codex, 2026-11-11): *implemented* (the code exists) and
*proved live* (measured on a live node in an ordinary activity). A row closes only when proved live;
an unknown stays open until measured.

- A completion push existing is not parity: it must reliably reach her, name the activity that owns
  the work, and survive a restart, or report the loss with its cause.
- A correction becoming a gene is "better" only when an independent evaluation shows the improvement,
  her unrelated abilities stay intact, and the adoption survives a restart. Unproven today.

**The live acceptance test** combines the channels in one ordinary activity, not one test per row: she
starts background work, switches to another activity, receives a failure or an interruption, inspects
its evidence, revises her plan and finishes, with her plan and work continuous across a turnover and
a core restart. Done means she noticed, investigated and recovered by herself, on the same feedback
Claude relies on.

Better than Claude, where we can: the outcome of every channel is typed (a refusal names its cause
and next moves), the record of her own mind is one call away for her and for any other mind, and a
lesson from a correction becomes a gene, not a note.

Gate for the whole table: each row is "none" or "better", each measured on a live node in a
non-continuum activity (another project, any language, or no code at all).
