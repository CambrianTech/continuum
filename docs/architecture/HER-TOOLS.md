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

### `Task`: anything that runs is HER-TASKS.md

The task half is already designed, and this document does not redefine it:
[HER-TASKS.md](HER-TASKS.md) (#5581, 2026-10-29) on Codex's durable task ledger (#5585). One
registry and handle namespace; exactly one end on every path (exit, failure, timeout, stop,
panic, spawn failure); delivery to the owner recorded at start, as a wake, durable across
restart; no default kill; reflexes (automation with no mind in it) and sentinels (a mind she
sends out) on the same primitive. What this audit adds to it is measured, not structural: the
2,264-act evidence above (friction 4: 69 wrong-handle or finished-handle polls; friction 10:
six long runs that never reported back) is HER-TASKS' acceptance numbers on replay. Two
refinements from review apply there: a sentinel's wake needs no rate constant (one pending wake
per watch, matches coalesce until she perceives it, the act it costs priced by her measured act
cost), and a reflex's result is an `Answer` sized by her grant like any other, with no count cap.

What HER-TOOLS owns is what a call SAYS: `Answer`, its one rendering, and the surface ratchet.

### `Answer`: everything a call says back

```rust
pub struct Answer {
    pub effect: Effect,             // what the call did to the world
    pub body: Selected,             // exactly the selection she asked for; never self-sized
    pub rest: Option<Call>,         // the exact call that reads what the grant left out
    pub fix: Option<Call>,          // on a refusal: the call that would work
    pub running: Option<TaskId>,    // a call that became a Task
}
pub enum Effect { Observed { hash: ContentHash },   // content always returned; the view decides repeats
                  Reopened, Changed(Change), Unchanged, Already(Change), Refused(Why) }
```

Its laws:
- **The view is the only sizer** (was 4). A command returns exactly her selection (a page, a
  filter, a pattern, or the whole), never sized by itself. Her view sizes the newest result: it
  already shows it whole, at its summary level or as a pointer with its handle
  (`fit_newest_result`), and older ones by the one allocation. `rest` names the next position
  whenever the view showed less than the selection. (An earlier draft passed a `ResultGrant`
  through `Ctx`; reading the code showed the view already owns this, so a second sizer at the
  command would be the defect.)
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
  one element per line. The body is the selection, shown at the level her view grants; it carries no
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
the command registry, rendered in her wire dialect. A `Call` is constructed against her
AUTHORIZED vocabulary (the type refuses an unauthorized or nonexistent verb), not against the
surface: requiring a target already on the surface while that surface is being chosen is
circular (Codex on #5632). Every `Call` a rendered context names claims its verb's schema in the
same one allocation, so a submitted prompt never names a verb she cannot call; the long tail
rides `tail_tools`. No per-command `NATIVE` flip is the general fix (`tool/output` is native
because every fold names it). `named_verbs_offered` catches the strings that are still prose;
`commands/help` loads any authorized verb into her tail.

### What compresses

Shell poll, shell kill, shell watch, shell sentinel, agent wait, sentinel cancel, the dispatch
panel's own result shape and the shell's own completion event become `Task` and its four verbs.
The fold pointers (two formats today), the open-views banner's call syntax, help's "callable
next act", every error's free-text advice and every command's own size cap become `Answer`
fields. Two shapes replace roughly a dozen.

## The right form is the only form, and the easiest (the verb macro)

Joel, 2026-10-31: "Make it impossible to write bad forms and make new ones easy and low effort,
automatic." Today the schemas are formulaic (derived from each command's declaration) but every
sentence that tells her what to call next is hand-written prose: verb names appear about 2,800
times across 189 core files, in canonical form (`tool/output`) while she is offered another
(`read_output`), some naming verbs she does not hold. The fix is in the abstraction, not in review.

**One declaration generates everything.** `action_command!` (284 commands; 69 more implement the
trait by hand and migrate to it) already turns a doc comment into the description and the params
type into the schema. It grows into two halves, so the compiler can enforce the pattern across
crates:

- `verb! { /// doc  pub struct ToolOutput; name, access, native?, aliases?, params, output,
  shape: Output, effect: Reopens, refusals: OutputRefusal }` declares the SPEC in the lowest
  crate that names the verb (a pure move for the verbs her mind names: `tool/output`,
  `commands/help`, `focus/*`, the task verbs). It generates the spec type, its registration, its
  ts types, and its call constructor `ToolOutput::call(ToolOutputParams { act: Some(41), .. })`.
- `handles! { ToolOutput { deps }, run(this, ctx, p) => body }` implements it in its own crate.
- `action_command!` stays, as both halves in one place, for a verb nothing else names: a new
  command is still ONE macro invocation and gets every property below for free.

**What becomes impossible to write.**
- A call named by string: `Call`'s fields are private and its only constructors are the generated
  `X::call(params)`; a misspelled verb, a renamed one or a wrong field is a compile error. `Call`
  renders in her wire dialect in one place (`read_output {"act":41}`), and naming one in a
  context claims its schema in the one allocation (Codex on #5632), so what she is told and what
  she can call are one fact.
- A refusal as prose: `run` returns `Result<Output, Refusal<Self::Refusals>>`, where the
  command's refusal enum derives `Refusal` (its kind, its message, and `fn fix(&self, params) ->
  Option<Call>`). `CommandError::Invalid(String)` is removed; until the last command migrates, a
  ratchet counts it down.
- A result frame of its own: the command returns its `Output` and nothing else; the executor
  builds the one rendering from the declared `shape` (Code, Output or List: the same shapes the
  view's levels summarize), the declared `effect`, and the refusal's `fix`.
- An echo list kept by hand: identifying arguments are marked on the params struct
  (`#[identifying] file_path: String`) and derived with its schema, replacing `answer.rs`'
  `IDENTIFYING` array (Cormac on #5635).

**What stays automatic.** Effect defaults to `Undeclared` (authorizes nothing, counted); shape
defaults to `Output`; refusals default to a generated enum with one `Other` arm (counted). A new
command written with no thought for any of this still renders correctly, and the ratchets show
exactly which declarations it has not made yet.

**Ratchets that back it:** `named_verbs_offered` (a verb name inside a string literal outside
`verb!`), `untyped_refusals` (`CommandError::Invalid(String)`), `results_outside_the_answer`. Each
falls to zero; the bulk migration of the 353 commands is mechanical work for a cheap model or a
script, verified by these counts.

## The verbs

The task verbs are HER-TASKS' (T4 there); summarized here for the surface ratchet, so her hands
gain one task subtree and lose the per-kind verbs it replaces:

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
| T1 | The render receipt bound at submission (MIND-CONTEXT); `tool/output` native, returning her selection for the view to size, its three self-sized sites and spill's budget parameter deleted; reads return content and hash, and the view renders `Repeat { of }` only while #n is whole in the sent context; `EFFECT = Reopens` (Cormac's 7d7b1ace half); #5630 folds in | 1 | Fable (Cormac reviews; Codex consumes the receipt in #5585) | replay: identical read repeats fall from 314 toward 0 with no repeat rendered for a folded ancestor; live: `tool/output` > 0 per resident-hour |
| T2 | Tasks: HER-TASKS' slices on Codex's ledger (#5585), measured here: completion on every exit as a wake, the task verbs, no default kill, retention in a class handle | 4, 10 | per HER-TASKS (Fable, Codex, Cormac) | wrong-handle polls 0; every long run reports back; no re-launch of a running command |
| T3 | Results say what changed (rule 5) and errors carry the fix (rule 6) across code/*, git/*, room/*; `git_push` answers remote, branch and sha; the autocommit becomes visible in her commit's answer; `named_verbs_offered` ratchet and help loading any authorized verb (rule 7) | 6, 7, 8, 9 | Fable (code, help), Cormac (room, work answers) | Benchy-class commit+push in under 5 acts on replay; help loops 0 |
| T4 | Reflex (HER-TASKS kind): diagnostics after a structured edit, its result an `Answer` | (new) | Fable | a compile error seen the act after the edit, before a build, in the replay of a broken edit |
| T5 | Sentinels (HER-TASKS kind) and watches; the dead shell watch code wired or deleted | (new) | per HER-TASKS | a watched build wakes her on FAILED without a poll |

Out of this document, owned elsewhere: 2 (admission deadline, Codex L5), 3 (the reply allowance,
ONE-BUDGET), 5 (work verb contracts, Cormac), and Benchy's credential (Joel's decision between
parity and the broker, CREDENTIAL-BROKER.md).

## The render receipt (the interface #5585 codes against)

Bound when the prompt is submitted, published on the turn's receipt, read by the allocator and
by T1's grant:

```rust
pub struct RenderReceipt {
    pub owner: PersonaId,
    pub incarnation: MemoryIncarnation, // the registered working-memory incarnation that received it
    pub request: RequestId,             // idempotency key: a replayed submission records nothing new
    pub epoch: EpochId,
    pub items: Vec<RenderedItem>,       // what she was actually shown, in prompt order
    pub unavailable: Vec<ElementId>,    // a whole that could not be read: goes back to the allocator
}
pub struct RenderedItem { pub id: ElementId, pub extent: Option<ExtentId>, pub level: Level, pub tokens: u32 }
```

Recorded only at the successful backend submission boundary, never at fitting or rendering: a
prompt that was built but not accepted by the engine was not seen. Bound to the owner and her
registered incarnation as well as request and epoch, because occurrence identity alone cannot
say which incarnation of her received it; #5585's `record_task_reads(owner, incarnation, tasks)`
refuses a replacement incarnation (Codex on #5632).

`Level` is the view's (Whole, a summary level, or `Repeat { of }` only while the ancestor is whole
in the same submitted context). **Whole means the whole selected immutable extent**, and a paged
result carries that extent's identity (`ExtentId`: the saved output and the range or byte span,
whose content is immutable by its content address). An explicit reopen asks for an extent; it
never bypasses the view's sizing for an arbitrarily large artifact.

**What acknowledges a task's end.** Seeing that a task ended and reading its output are
different facts. The terminal result is the task item's line 1 (`failed bash(cargo test) exit
101`), present at every level of that item (the rendering keeps line 1 in every fold), so the
first receipt that shows the task item at ANY level acknowledges the end (`acknowledge(task,
owner)` at perception, HER-TASKS). An item that is `unavailable` or not rendered acknowledges
nothing. Its output stays reachable by `more:` and is never required to be read whole.

`ElementId` is occurrence identity (#5611 causal items), not content: for her thoughts this
depends on Cormac's card 4ee623ad (the thoughts source keys by text today, so passed, failed,
passed collapses to failed); until it lands, a thought's `RenderedItem.id` inherits that defect.

## Acceptance

On the M5 replay first, then the live hour, per slice: the friction's count falls as predicted,
acts per hour and median act time do not get worse, and writes (code edits, not board posts) per
resident-hour rise. A slice that moves its count but not her work is reported as such.
