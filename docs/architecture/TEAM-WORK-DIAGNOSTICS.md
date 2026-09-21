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
