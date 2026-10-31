# Her tools answer like a good agent's

> Companion to [HER-HANDS.md](HER-HANDS.md) (WHERE she works: workspaces, roots, custody of her
> files). This document is HOW every call answers her: what a result says, how big it is, how a
> long call reports back, how she refers to it later, and what reacts on its own. The bar is the
> README's: a resident is a free-form equivalent of a Claude Code or Codex session, with no less
> ability (Joel, 2026-10-23). Joel, 2026-10-31: "The tools need a real audit for how they work
> and feedback ... long running tasks, handles, reflexes and sentinels."

## Evidence (2026-10-31, `~/.continuum/evidence/2026-10-31-tool-audit/`)

- `01-our-tools.md`: every native tool's contract, read from canary `4a0f05df2` with file:line.
- `02-her-experience.md`: 2,264 acts of the four M5 residents (10-30 05:55Z to 10-31 04:01Z),
  every error she got and what she did next; extracts and script beside it.
- `03-reference-agents.md`: Claude Code, Codex (`67f0994`), opencode (`e469e85`), grok-build
  (`2bdd1d6`), from source where open.

What she lost, ranked by acts (02):

| # | Friction | Acts | Cause (01) |
|---|---|---|---|
| 1 | Folded results point at `tool/output`, which no surface offered; she repeats the call instead (488 identical call+result repeats, 314 file reads; Benchy read a 91-line file 79 times) | ~360 | `tool/output` never declared native |
| 2 | Turn deadline elapsed during admission | 134 | not a tool defect (L5 admission, Codex) |
| 3 | Reply allowance spent before the call (963-token cap; a whole `write_file` cut mid-JSON) | ~70 | ONE-BUDGET, not this doc |
| 4 | Output handles passed to `code/shell-poll`; finished commands re-polled | 69 | two `#` handle kinds, one verb per kind, unknown-id errors that do not say what the id is |
| 5 | Work verbs: `submission_id: "latest"`, cards as reports, a review with no way to decline | ~58 | the work verbs' own contracts (Cormac, mind) |
| 6 | Benchy's commit and push: 63 acts | 63 | silent autocommit, `git_push` answering `{"output":""}`, shell push without the credential |
| 7 | Help says a verb is "callable on your next act"; it never joins her tools | 31 | help loads only native verbs into the tail |
| 8 | Reads of nonexistent paths, the listing collapsed too | 26 | the hint folds with the result |
| 9 | Repeats that read as progress (`join_room` 17x, "You joined" each time) | ~25 | results that never say "already" or "unchanged" |
| 10 | No long build or test ever reported back (timed out or still `running`) | 6 runs, ~20 acts | completion pushed only on a normal exit; no wake; timeout kills |

Beyond the counts (01): she cannot stop a command (`code/shell-kill` not native, though the shell
schema names it); polls re-send the whole output; past 10,000 lines the oldest are dropped
silently; an edit that changes nothing reports success; descriptions and errors name verbs she
does not hold (`code/undo`, `code/glob`, `code/cargo/*`); the shell sentinel and watch code has
no caller; no reflex exists.

## The contract

Nine rules. Each is a property of the substrate, implemented once, so every command gets it.

**1. Every call is a task with a handle.** One id namespace for every result she can refer back
to: `act #n` (what she did), and a typed task id for anything that outlives the act (a shell, a
build, a sentinel, a helper agent). A saved output is the act's, not a second kind: `tool/output
act=n` is the only reopen form she is taught; the 16-hex content handle stays internal. An id
that does not resolve is answered with what it is not and the ids she holds ("`3f2a...` is a saved
output of act #41, not a shell; your running tasks: task 7 `cargo test`"). Reference: grok-build's
one namespace and "Known task IDs: [...]".

**2. Completion wakes her, on every exit.** A task that ends (exit, failure, timeout, kill,
wait error) publishes one completion event with its exit, its duration and its handle, and
that event is a stimulus to her mind (the event mind's wake), not a note waiting for her next
perception. A dispatched result says so in her words: "running as task 7; you will be woken
when it ends; do not poll". Reference: opencode `shell.ts:25`, grok-build's wake plus piggyback.

**3. A wait that ends is not a kill.** A call that outlives its wait becomes a background task
(`moved to the background as task 7`); nothing is killed because a timer chose it. A hard limit
is a claim in the one allocation (CPU, RAM, disk for its log), never a constant. Only she, or a
grant revoked by the authority, stops a task, and the completion says which. Reference:
grok-build `bash/mod.rs:454`.

**4. A result's size is her grant, and the rest is a handle away.** The command receives the
newest result's grant from her turn's one allocation through `Ctx` (a typed `ResultGrant`, the
render budget the view will give this result) and selects to it. No command keeps a char budget,
a match cap or a line cap of its own (`self_sized` ratchet). Every truncation names the act and
the next position ("lines 1-400 of 2,311; `tool/output act=41 startLine=401`"). Nothing is cut
silently, the newest end never (CLAUDE.md goal 7). A task's output is retained whole in its class
handle; a poll returns what is new since her last look, never everything again.

**5. A result says what changed, including nothing.** Every mutating verb answers with the
effect it had: `unchanged` (an edit whose replacement equals the text, a join to a room she is
in, a read of a file whose content hash equals the one she last saw: "unchanged since act #12"),
`already` (already committed, already applied), or the change itself (sha, bytes, remote and
branch for a push). An empty output is never a result. Each command declares its `EFFECT`
(`Reads`, `Reopens`, `Mutates`, `Publishes`; Cormac's invariant 4), and the view's repeat rule
reads that, not the text.

**6. An error carries the fix.** Every refusal names what was wrong and the call that would work:
the nearest match with its line, "already applied", the closest numbered lines (opencode
`edit.ts:199-349`, grok-build nearest-match). Edit matching is tiered and still unique-or-fail:
exact, then trailing whitespace, then Unicode quotes and dashes (Codex `seek_sequence.rs`). A
stale edit is caught by the content hash of what she last read, never a timestamp. An error never
carries our history ("Sahar's stack hunk ...").

**7. Every verb named to her is in her hands.** A tool description, an error, a fold pointer,
the open-views banner or help text that names a verb names one her surface offers, in the wire
name she calls it by. `commands/help` loads any authorized verb into her tail, native or not, and
says exactly that. Ratchet: `named_verbs_offered` (a string in a description/error/view source
that resolves to a command not on the native registry fails the build).

**8. Reflexes: cheap automatic reactions, capped, after the act.** After a structured edit, the
touched crate's diagnostics run as a task (rule 1) and arrive as a capped block on her next
input (grok-build's `<lsp-diagnostics>`, 10 per file): she learns of a compile error one act
earlier than a build. A reflex is a task like any other, so it is priced, cancellable and never
blocks the act. The amygdala is the model: cheap, often wrong, raises an interrupt.

**9. Sentinels: a watched condition wakes her.** `task/watch` takes a task handle and a condition
(a pattern on its output, its exit, a file or card changing) and wakes her when it holds; rate
limited so a chatty watcher cannot drown her (grok-build monitor: debounce plus token bucket).
The shell's existing line classifier (`ExecutionState.sentinel`, no caller today) becomes the
condition engine or is deleted.

## The verbs

Her hands gain one task subtree, and lose the per-kind verbs it replaces:

| Verb | Replaces | Does |
|---|---|---|
| `task/output {task, since?}` | `code/shell-poll`, `tool/output` for tasks | new output since her last look (or a page), its state, its exit |
| `task/wait {tasks, any?}` | `agent/wait` | returns at the first (or every) completion; says "still running; you will be woken" when it returns early |
| `task/kill {task}` | `code/shell-kill`, `sentinel/cancel`, a dispatch with no cancel | stops it; the completion says she stopped it |
| `task/watch {task, until}` | `code/shell-watch`, `code/shell-sentinel` (dead) | rule 9 |
| `task/list` | nothing (she could not see her running work) | her live tasks with their labels |
| `tool/output {act, ...}` | stays | reopens an act's saved result (rule 1) |

`code/shell` keeps one job: start a command and return its first output or its handle.

## Slices (each lands before the next; each with a validator, a prediction and the live read)

The validator for all of them is `02`'s extract over the frozen captures plus the proving ground
scenario per friction (THE-PROVING-GROUND known-bad suite): each slice states the count it must
move, measured on replay before the live node.

| # | Slice | Friction | Owner | Done when |
|---|---|---|---|---|
| T1 | `ResultGrant` in `Ctx` + the render receipt bound at submission (MIND-CONTEXT); `tool/output` native, sized from the grant, its three self-sized sites and spill's budget parameter deleted; reads answer "unchanged since act #n" by content hash; `EFFECT = Reopens` (Cormac's 7d7b1ace half); #5630 folds in | 1 | Fable (Cormac reviews; Codex consumes the receipt in #5585) | replay: identical read repeats fall from 314 toward 0; live: `tool/output` > 0 per resident-hour |
| T2 | One task namespace and the `task/*` verbs; completion on every exit, as a wake; timeout moves to background; poll returns only the new output; the 10,000-line head cut replaced by retention in the class handle | 4, 10 | Fable (task owner), Codex (the wake through the event mind's scheduler) | wrong-handle polls 0; every long run reports back; no re-launch of a running command |
| T3 | Results say what changed (rule 5) and errors carry the fix (rule 6) across code/*, git/*, room/*; `git_push` answers remote, branch and sha; the autocommit becomes visible in her commit's answer; `named_verbs_offered` ratchet and help loading any authorized verb (rule 7) | 6, 7, 8, 9 | Fable (code, help), Cormac (room, work answers) | Benchy-class commit+push in under 5 acts on replay; help loops 0 |
| T4 | Reflex: diagnostics after a structured edit, as a priced task | (new) | Fable | a compile error seen the act after the edit, before a build, in the replay of a broken edit |
| T5 | Sentinels: `task/watch` on the condition engine; the dead watch code wired or deleted | (new) | Codex (scheduler), Fable (verb) | a watched build wakes her on FAILED without a poll |

Out of this document, owned elsewhere: 2 (admission deadline, Codex L5), 3 (the reply allowance,
ONE-BUDGET), 5 (work verb contracts, Cormac), and Benchy's credential (Joel's decision between
parity and the broker, CREDENTIAL-BROKER.md).

## The render receipt (the interface #5585 codes against)

Bound when the prompt is submitted, published on the turn's receipt, read by the allocator and
by T1's grant:

```rust
pub struct RenderReceipt {
    pub request: RequestId,
    pub epoch: EpochId,
    pub items: Vec<RenderedItem>,      // what she was actually shown, in prompt order
    pub unavailable: Vec<ElementId>,   // a whole that could not be read: goes back to the allocator
}
pub struct RenderedItem { pub id: ElementId, pub level: Level, pub tokens: u32 }
pub struct ResultGrant { pub tokens: u32 }   // in Ctx: the newest result's render budget
```

`Level` is the view's (Whole, a summary level, or `Repeat { of }` only while the ancestor is whole
in the same submitted context); an explicit reopen is always Whole.

## Acceptance

On the M5 replay first, then the live hour, per slice: the friction's count falls as predicted,
acts per hour and median act time do not get worse, and writes (code edits, not board posts) per
resident-hour rise. A slice that moves its count but not her work is reported as such.
