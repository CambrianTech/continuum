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

## The contract: two primitives and one ratchet

Every audit finding is one of two missing shapes. Each lives once in the substrate and every
command gets it by returning it; nothing below is a per-command behaviour.

### `Task`: anything that runs

```rust
pub struct Task {
    pub id: TaskId,            // one namespace: shells, builds, helpers, reflexes, sentinels
    pub act: ActSeq,           // the act that started it; `act #n` reopens its saved result
    pub label: String,         // her words for it ("cargo test -p continuum-context")
    pub output: ClassHandle,   // retained whole under its disk class (ONE-BUDGET); never head-cut
    pub claim: ClaimId,        // CPU, RAM, disk in the one allocation; no constant limit
}
pub enum Ended { Exited(i32), Failed(String), Killed { by: Stopper }, Revoked(GrantId) }
pub enum Stopper { Her, Authority }
```

Its laws, each a former rule:
- **One namespace** (was 1). An unresolved id is answered with what it is and the ids she holds.
- **Completion is a wake** (was 2). Every `Ended` variant publishes `TaskEnded { id, ended,
  duration }` as a stimulus to her mind; no exit path is silent.
- **A wait is not a kill** (was 3). A call that outlives the act's wait answers `Running(id)`
  and keeps running; only `Stopper::Her` or a revoked grant ends it.
- **Reflex = a task an event starts** (was 8): after a structured edit, the touched crate's
  diagnostics run as a Task; its `TaskEnded` delivers an `Answer` sized from her grant like any
  result, the rest one `rest` call away. No per-file or total count.
- **Sentinel = a condition over a task's stream** (was 9): `Watch { task, until: Condition }`
  emits a `TaskEnded`-shaped wake when the condition holds. Its rate needs no constant: a watch
  holds at most one pending wake, and further matches coalesce into it until she perceives it,
  so a chatty watcher costs at most one act per perception. What that act costs is priced in her
  allocation by her measured act cost; a watch is a claim, revoked like any other.
- **No orphans.** A task belongs to her durable identity, not to a process. A restart re-adopts
  her live tasks and delivers completions that happened while she was down; a retired mind's
  tasks are released through the authority (`Ended::Revoked`), never left running unowned.

### `Answer`: everything a call says back

```rust
pub struct Answer {
    pub effect: Effect,             // what the call did to the world
    pub body: Selected,             // the part her ResultGrant buys, never self-sized
    pub rest: Option<Call>,         // the exact call that reads what the grant left out
    pub fix: Option<Call>,          // on a refusal: the call that would work
    pub running: Option<TaskId>,    // a call that became a Task
}
pub enum Effect { Observed { hash: ContentHash },   // content always returned; the view decides repeats
                  Reopened, Changed(Change), Unchanged, Already(Change), Refused(Why) }
```

Its laws:
- **Size is her grant** (was 4). `Ctx` carries `ResultGrant`; `Selected` is built only from it,
  and `rest` names the next position whenever anything was left out.
- **Say what changed, including nothing** (was 5). An empty output cannot be constructed. For a
  verb that changes the world, `Unchanged` and `Already` are the producer's own evidence (an edit
  equal to the text, a join to a room she is in, a commit already made). An observation never
  answers "unchanged": it returns its content and hash, and the view alone renders a repeat
  (`Level::Repeat { of }`, only while act #n's whole is in the context she was sent; otherwise
  the re-read arrives whole, because she needs it again). A command cannot know what her view
  holds, so a command-side "unchanged" would point her at content she no longer has.
- **The declared effect is one vocabulary** (Cormac's invariant 4, `continuum-base`):
  `CommandSpec::EFFECT: ToolEffect { Observes, Changes, Reopens, Undeclared }`, default
  `Undeclared`, which authorizes nothing and is counted. `Answer.effect` is what one call did;
  `EFFECT` is what the command may do. A change's target (her workspace, or shared state such as
  a push or a card) is data on `Change`, not a second variant, until a rule reads the difference.
- **A refusal carries the fix** (was 6). `Refused` without `fix` is allowed only when no call
  could work, and says why. Edit matching is tiered and unique-or-fail; staleness is a content
  hash.

### One rendering: every result reads the same way

Joel, 2026-10-31: "Make sure the results are also as robust as yours or other tool calls, highly
formulaic and consistent, what they expect." Today each command writes its own prose (JSON scalars
then text, `{"output":""}`, "I dispatched ... I should NOT wait", two fold-pointer formats, an
error with our history in it). A mid-size model learns a format from the examples in its own
context; thirty formats teach none. So `Answer` has exactly one renderer, `Answer::render`, and
every tool result she reads is its output:

```
<status> <tool>(<the args that identify the call>)
<body>
more: <call>
fix: <call>
wake: task <id> is running; you will be woken when it ends. Do not poll.
```

- **Line 1, always.** `status` is one word from a closed set: `ok`, `unchanged`, `already`,
  `running`, `refused`, `failed`. The call is echoed in her wire name with only the identifying
  args (`read_file(src/a.rs, lines 1-400 of 2,311)`, `bash(cargo test -p x)`, `edit_file(src/a.rs)`).
  A failure puts its cause here: `failed bash(cargo test) exit 101`.
- **Body, by kind, one convention each.** File content: numbered lines in `cat -n` form, the
  numbers she pages by. Command output: stdout then stderr, each under its own label. A
  change: the changed lines with their numbers and the sha, bytes or remote it produced. A list:
  one element per line. The body is whatever her grant buys of the selection; it carries no
  advice and no prose about itself.
- **Footer lines, fixed keys, only when they apply.** `more:` when the body left anything out,
  `fix:` on a refusal, `wake:` when the call became a task. Each `<call>` is an exact native call
  she can copy: her wire name and its JSON args (`read_file {"file_path":"src/a.rs","start_line":401}`),
  never a description of one.
- **A fold is the same shape, smaller.** When her view shows an older result at a summary or
  pointer level, it keeps line 1 and replaces the body with the level, and keeps `more:`. She
  always sees what call it was, how it ended, and the exact call that brings it back.
- **The native pairing stays.** The rendering is the content of the tool message paired with
  its call id; the envelope the chat template wraps around it is the template's.

Each command supplies only the parts that differ: its body and, on a refusal, its fix. Ratchet:
`results_outside_the_answer` counts tool-result text built anywhere but `Answer::render`, and
falls to zero as commands migrate (the view's own fold levels reuse the renderer).

### The ratchet: every verb named to her is in her hands (was 7)

`fix`, `rest` and every description, banner and fold pointer render `Call`s, typed values over
the command registry, rendered in her wire dialect. A verb that is not on her surface cannot be
a `Call`'s target (the type refuses it), and `named_verbs_offered` catches the strings that are
still prose. `commands/help` loads any authorized verb into her tail.

### What compresses

Shell poll, shell kill, shell watch, shell sentinel, agent wait, sentinel cancel, the dispatch
panel's own result shape and the shell's own completion event become `Task` and its four verbs.
The fold pointers (two formats today), the open-views banner's call syntax, help's "callable
next act", every error's free-text advice and every command's own size cap become `Answer`
fields. Two shapes replace roughly a dozen.

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
| T1 | `ResultGrant` in `Ctx` + the render receipt bound at submission (MIND-CONTEXT); `tool/output` native, sized from the grant, its three self-sized sites and spill's budget parameter deleted; reads return content and hash, and the view renders `Repeat { of }` only while #n is whole in the sent context; `EFFECT = Reopens` (Cormac's 7d7b1ace half); #5630 folds in | 1 | Fable (Cormac reviews; Codex consumes the receipt in #5585) | replay: identical read repeats fall from 314 toward 0 with no repeat rendered for a folded ancestor; live: `tool/output` > 0 per resident-hour |
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
in the same submitted context); an explicit reopen is always Whole. `ElementId` is occurrence
identity (#5611 causal items), not content: for her thoughts this depends on Cormac's card
4ee623ad (the thoughts source keys by text today, so passed, failed, passed collapses to failed);
until it lands, a thought's `RenderedItem.id` inherits that defect.

## Acceptance

On the M5 replay first, then the live hour, per slice: the friction's count falls as predicted,
acts per hour and median act time do not get worse, and writes (code edits, not board posts) per
resident-hour rise. A slice that moves its count but not her work is reported as such.
