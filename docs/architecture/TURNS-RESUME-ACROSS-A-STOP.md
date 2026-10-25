# A turn resumes across a stop, exactly as it was

Joel, 2026-10-24: *"I always design my systems so as you'd never notice the difference,
start/stop, so every component that has state resumes off disk and resumes exactly as it had
been."*

## The defect

A stop or deploy cuts a turn in flight. Its trigger was marked perceived when the turn started
(`announce_turn_start`). The lines admitted but parked for capacity live only in the loop-local
`PendingInput` (`persona/wake_backlog.rs`). On the next boot the mind skips every line below
each room's high-water mark (`inbox_capacity.rs`, `polled.rs`). So the line that was being
answered, and every line waiting behind it, is never answered: the person who spoke sees
silence.

#5397 tried to fix this by giving the region cursor back at shutdown. Review showed it could
not work, and it was retracted:
- the give-back ran only on a zero settle, and a zero settle skips every settler;
- nothing saved the region at shutdown;
- boot does not read the region cursor to decide what to answer.

## The design: the input is the state, so the input is saved

The handoff (`cognition/handoff.rs`, card 49b5e806) already owns this seam. It is written at
the seam, read first on wake, and it carries whole structs, never a re-derivable subset. It
gains one field:

```rust
/// Inputs admitted to a turn and not yet answered: the one being answered first, then the
/// ones parked behind it, in the order she would have served them.
pub unanswered: Vec<IncomingMessage>,
```

- **Write.** One ledger, `cognition/unanswered.rs`, with two owners. The running turn holds an
  `InFlight` guard for its trigger from `announce_turn_start` until the turn ends, on any exit.
  `PendingInput` publishes its parked inputs whole each time they change. What is owed is the
  in-flight input first, then the parked ones. Nothing walks history.
- **Save.** The existing seam write saves it. The periodic write saves it too, so a crash loses
  at most one checkpoint interval.
- **Resume.** `handoff::reload` hands the inputs to the loop. The loop seeds `PendingInput` with
  them, so the first wakes are `Wake::Retry(msg)`. That is the existing admitted-retry path,
  which already bypasses the high-water skip and the loop-filler dedup.
- **No duplicate.** The seam's write SEALS the ledger in the same lock as it reads it. From
  then on `publish_own_speech`, the one boundary every resident speech path shares, refuses
  (`persona.turn.sealed`). A cut turn that finishes late cannot answer what the next life will
  answer, and nothing can land between the read and the refusal.

Shutdown keeps its measured settle (up to 300 s). A turn that finishes in time answers normally
and clears its input. Only a turn the stop really cuts is resumed.

## What this does not do

- It does not undo acts. A cut turn that ran a tool before the seam may run it again when its
  input is re-answered. Her hands see the world as the act left it, so an act that already
  landed is what she reads first.

- It does not restore a half-generated reply. The turn re-runs from its input. On the in-process
  engine, her KV pages restore from disk (SERVING-ENGINE.md S4), so the re-run reuses the
  prefix rather than re-prefilling it.
- It does not change which lines she chooses to answer. It only stops a stop from choosing for
  her.

## Receipts it ships with

- **Unit test:** an input stamped, a seam write, a fresh process's reload, and the first wake is
  `Retry` of that input.
- **Unit test:** a reply already in the room drops the resumed input.
- **Live:** `continuum stop --keep-engine` during a turn, then `start`. The cut line gets
  exactly one answer, and the `persona.turn.resumed` probe names it.
