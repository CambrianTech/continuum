# Her tasks: every act is a task with a handle, and its end wakes her

Status: design, 2026-10-29. Owner: Fable (kernel task registry, act path, wake), with Codex
(review: kernel/serving seams) and Cormac (review: working memory). What a call SAYS back (results, their size, one rendering, the surface ratchet) is
[HER-TOOLS.md](HER-TOOLS.md); its 2026-10-31 audit of 2,264 resident acts gives this design its
acceptance numbers on replay (69 wrong-handle or finished-handle polls; six long runs that never
reported back). Read with
[HER-HANDS.md](HER-HANDS.md), [EVENT-MIND.md](EVENT-MIND.md) and
[CONCURRENCY-STYLE-GUIDE.md](CONCURRENCY-STYLE-GUIDE.md).

## The capability, in one sentence

A persona starts work (a command, a reflex, a sentinel mind), gets a **handle** at
once, keeps thinking, and is **woken by the work's end**: the result reaches her whether or
not she remembered the handle, and survives a restart. She can list her tasks, read their
output so far and stop them. This is what Claude Code gives its own agent (background
commands, agents, task notifications, stop); she gets no less.

## What we observed (M5, core 6640431ba, 2026-10-29)

- Aris spent 18 of 34 acts in 1 h 42 m on `code/shell-poll`; one finished handle was polled
  about 25 times. Each poll is a full model act (~1-2 min of inference) asking "done yet?".
- Every resident `cargo` run timed out (Aiko 3, Aris 1): she set `timeout_ms` 120-300 s
  herself, the build started cold, the kill came first, and she re-read files instead of
  landing work (~60 `act.no_deliverable_yet` in 35 min across four residents).
- A completion reached her only by polling: nothing turns a completion into a wake.

## Why: one pattern, implemented four times, none whole

| Mechanism | Handle | List / read / stop | End event on every path | Wakes her |
|---|---|---|---|---|
| kernel `CommandExecutor::dispatch_background` (`continuum-kernel/src/runtime/command_executor.rs:361`) | UUID, at once | none / event only / none | `command:completed` on Ok and Err; **none if the spawned task panics** | no |
| act `apply.rs::is_long_running` (`:41`) | only for a hand-kept list of 5 commands | via the kernel | via the kernel | no |
| `code/shell` handback (`continuum-hands/.../normalize_edit_mode.rs:604,668`; own 30 s wait, private `SHELL_COMPLETION_BUS`) | `execution_id`, after 30 s | `shell-status` / `shell-poll`, blocking `shell-watch` / `shell-kill` | **natural exit only**: timeout (`run_shell_command.rs:259`), kill (`shell_session.rs:358,438`) and spawn/pipe failure publish nothing | no |
| sentinel runs (`continuum-sentinel/src/global_sentinel.rs:81`) | 8-char string | `sentinel/list` / `status`, `logs/*` / `cancel` | own topics `sentinel:{h}:status`, `sentinel:complete`; **panic leaves it Running**; nothing outside the crate consumes them | no |
| `agent/*` (`continuum-core/src/modules/agent.rs:296`) | `agent-xxxxxxxx` | `agent/list` / `status` / `stop` | `agent:{h}:complete` only on complete or give-up; **nothing on max-iterations, stop, error or panic**; entry evicted on completion | no |
| genome/training jobs (`continuum-dreaming/src/commands/genome/job_*`) | `job_id` | own registry: `job_status` / `job_cancel` | not yet traced | no |
| `cognition/dispatch_listener.rs` | n/a | n/a | n/a | no: files into working memory only, and **drops** an end whose handle her working memory did not register (`:47-70`) |

Blocking waits on top: `agent/wait` (`commands/agent/wait.rs`, up to 300 s on a `Notify`),
`sentinel/await` (`global_sentinel.rs:471`), and the synchronous `sentinel/pipeline` (`:532`).
Totals: five handle formats, three completion topics, five list/status/cancel surfaces, two
blocking waits, and no path that wakes her. The act's calls already reach the one kernel
executor (`tool_executor/command_executor.rs:604` -> `InProcessTransport` ->
`CommandExecutor::execute_with_caller`, `runtime/in_process_transport.rs:106-112`), so one
registry there covers every command an act runs.

Each copy decides part of the same thing, so each is partly wrong in its own way: the
microkernel rule ([[microkernel-centralize-logic-extensions-behind-narrow-contracts]]) says
build the owner, not another guard.

## Three kinds on one task primitive (Joel, 2026-10-29)

Named for what they are in a mind:

- **Command:** one step (a build, a test run, a search).
- **Reflex:** automation with no mind in it, the nervous system's stimulus-to-response with no
  thought: a filter, a watcher, an interrupt rule, a templated pipeline of steps (shell, watch,
  wait). Fast, cheap, built in: it costs her no model acts until it fires. Templates make common
  automation one call: "run the tests, filter the failures, wake me only for new ones"; "watch
  this PR's CI and wake me when it is red"; "page this log and interrupt on an error burst". The
  health watcher below is her first reflex, attached to every task. The amygdala raises a check;
  a reflex acts without waking the mind.
- **Sentinel:** a mind she sends out: forked from her own warm context, or fresh with a brief.
  What makes agents useful to Claude Code applies here (next section). The sentinel crate's
  pipeline steps without a mind (shell, watch, wait) become reflexes; its `coding_agent` and
  `web_research` steps are sentinels.

## Her sentinels, designed from what works for an agent like Claude Code

Two levels on one primitive: a **task** is work with a handle whose end wakes her; a **sentinel** is
a task with a mind inside (an LLM loop with its own tools and steps). What makes agents useful to
Claude Code, taken whole:

| What Claude Code's agent has | What it buys | Her sentinel |
|---|---|---|
| Start an agent with a brief, in the background | delegation without blocking | `sentinel/start { brief, fork?, tools? }` returns a handle at once |
| Only the conclusion comes back | the parent's context stays clean; the agent's file dumps never enter it | the end carries the sentinel's report; its trace stays behind a handle (her one budget decides what she sees) |
| Fresh agent or a fork that inherits the parent's context | cheap specialists, or a copy that already knows everything | fresh (brief only) or **fork** (starts from her own context) |
| Agent types (explore read-only, plan, general) | the right tools and permissions per job | a sentinel declares its tool subtree (`code/read/**` only, or full hands) and its confinement |
| Isolation in its own worktree | parallel edits never collide | each writing sentinel gets its own worktree and build dir |
| Continue a finished agent with a message | follow-ups keep the agent's context | `sentinel/send { handle, message }` wakes it with its context intact |
| Several agents in parallel, notified as each ends | fan-out research and builds | many sentinels in flight; each end is its own wake |
| Stop, list, read output | control | the registry's `task/stop`, `task/list`, `task/output` |

## Where she goes past Hermes, opencode and Claude Code

Those harnesses run agents (sentinels here) on a remote API that knows nothing of the parent. Ours run on the same
engine as her, so:

1. **A forked sentinel costs no prefill.** It copies her warm KV sequence (`seq_cp` in the
   engine, SERVING-ENGINE.md), so it starts already holding everything she has read. Elsewhere a fork
   re-sends the whole context.
2. **A sentinel inherits her genome.** It runs with her active genes, or a gene she picks for the
   job, so a specialist is her, specialised, rather than a stranger with a brief.
3. **Its work is her curriculum.** A sentinel's turns are captured like hers; what it learned
   reaches the dream stage, so delegating also teaches her.
4. **It can run anywhere on the grid.** A sentinel is placed by capability: a build on a machine with
   free cores, a long research run on an idle node, its end delivered back to her by owner.
5. **It survives restarts and deploys.** Durable rows and owner delivery mean a sentinel started
   tonight still reports to her tomorrow, after any number of core deploys.
6. **It is scheduled with every other mind.** Sentinels are minds to the one scheduler (goal 9):
   a cooling sentinel pages its KV to RAM like any resident, so many can wait cheaply.

## The contract

One owner, in its own crate: **`continuum-tasks`**, holding only the task contract
(`TaskRegistry`, `TaskId`, `TaskEnd`, the owner sink trait). It sits below `continuum-kernel`
and depends only on `continuum-base`, so the command executor, sentinels, minds and the grid
all build on one narrow interface, and the crate checks in seconds on its own. The lineage is
the operating systems that became manageable this way: the registry is a process table, an end
waking her is a signal, a sentinel is a process with its own address space (its worktree and
its context), a reflex is a small daemon, and a fork from her warm KV is a copy-on-write `fork()`. Each primitive lives
once; every kind of task is only its difference.

The contract as built (Codex, #5585), which this design adopts:

- **Accept, durably, before spawning.** Starting a task is fallible: acceptance is written to
  the ledger (an ORM entity) first, and only an accepted task runs. A refused start is said, never
  silently retried in the foreground.
- **One terminal outcome, first writer wins.** The ledger stores an immutable terminal receipt
  (output kept whole, binary and typed handles included, with its effect certainty) before any
  event is emitted; a late writer can neither replace it nor resurrect delivery. A handler that
  unwinds becomes a failed receipt.
- **At-least-once delivery to the owner, with acknowledgement.** The owner's sink pages pending
  ends (`pending_page(owner, after, limit)`) and acknowledges each only once she has PERCEIVED
  it (`acknowledge(task, owner)`); replay after a restart needs no remembered handle. Filing is
  not consumption: working memory's dispatched set is volatile (excluded from its checkpoint), so
  an end acknowledged at filing and lost in a restart before her next prompt would never be
  redelivered. The acknowledgement is working memory's retirement point (card c537d687): an act
  recorded after a prompt that showed the end (acts and dispatch events share one sequence).
  Until then the end stays pending and a restart replays it. The ledger is the one durable owner
  of whether an end was delivered; working memory stays a projection (Cormac on #5581).
- **A stop or timeout is a request.** It cannot itself attest that execution ended; only the
  runner's terminal receipt does.

Crate shape: the contract (ids, kinds, ends, the ledger trait, the sink trait) moves into
`continuum-tasks` below the kernel as a pure move once #5585 lands; the ORM ledger stays in the
kernel as one implementation of the trait (storage injected from below, so dependencies stay
acyclic).

Rules the registry enforces, so no task kind can get them wrong:

1. **Exactly one end, on every path.** The registry, not the task, makes the terminal
   transition: a drop guard ends a task that panicked, was aborted or whose process died
   (`Lost`). A stop or timeout is only a REQUEST (above): the end is the supervisor's
   observation that execution ended after it, recorded once like any other end. One event,
   `task:ended`, carries `TaskId`, owner, kind and `TaskEnd`.
2. **Delivery by owner, not by memory.** The end goes to the owner's mailbox. For a persona
   the mind installs one sink (`TaskOwnerSink`, a trait the kernel defines and `continuum-mind`
   implements, so layering stays downward): the sink files the result into her working
   memory, which her view renders through the `tasks` source with levels (whole, a summary from
   `levels::summarize`, a pointer with the call that reads it), so a whole log never enters her
   view as a block that cannot shrink; and it sends her a `Wake::TaskEnded { id }`. Target (T2,
   durable): a forgotten handle still wakes her, because delivery comes from the ledger's
   ownership, not from her registered handles.
3. **Durable (target, T1 + T2).** A task row is an ORM entity. On boot, a task whose process is gone ends as
   `Lost` and is delivered like any end; a task still running (an adopted process) keeps its
   handle. A deploy never silently erases her work.
4. **No default kill.** A task runs until it ends or she stops it. A timeout exists only when
   she asks for one. The act's landing window is how long the ACT waits, never how long the
   WORK may run.
5. **Output is a stream with a handle** (the existing `tool/output` store), so she reads any
   part at any time and the final result is whole (one budget decides how much her view
   shows).

## How a reflex is made: catalog, builder, base (Joel, 2026-10-29)

One interface, three ways in, the OpenCV `cv::Algorithm` shape (CLAUDE.md, polymorphism pattern):

1. **By name, from the catalog.** Common reflexes are named templates with a few parameters:
   `error_burst`, `new_failures_only`, `ci_turned_red`, `silent_too_long`, `output_flood`. The
   catalog is a REGISTRY of templates (data), never an enum or a match on names: adding a common
   reflex is adding a template, and she can list the catalog like any command surface.
2. **Composed with the builder.** A reflex is a pipeline of typed stages: a **source** (a task's
   output stream, an event topic, a command it re-runs on an interval), **filters** (pattern,
   dedup, rate, window), a **condition**, and an **action** (wake her with a reason, post to a
   room, run a command, stop a task). A template is a saved builder; she can author one through
   a command, as data, and it is checked against the stage types before it runs.
3. **Extended from the base.** A `Reflex` trait (name, parameters she can read and set, `step`
   on each input) for what the stages cannot say: the measured outlier detector behind the health
   interrupts, a learned filter. A custom algorithm registers by name, so it appears in the
   catalog like any template.

Every reflex runs as a task (one end on every path, owner delivery), on the substrate's event
loop: a reflex never polls on its own clock; a source that must re-run a command does it through
the scheduler's interval, once, for every reflex that shares it.

## Reflexes in a body: game and VR rooms (Joel, 2026-10-29)

The same primitive is literal reflex once she is embodied. In a game or VR room a reflex's
**source** is a perception stream (who is speaking, where they are, an object on a collision
course, a hand reaching toward her) and its **action** is a motor command on her avatar's controls
(turn toward the speaker, keep eye contact, flinch, catch, step back). It answers in milliseconds,
with no model act, the way a body does; the mind hears of it after, and only when it matters (the
amygdala's check). The catalog gains bodily templates (`orient_to_speaker`, `track_gaze`,
`avoid_collision`), the builder gains perception sources and motor actions, and a custom algorithm
on the `Reflex` base is any small controller (a PID on gaze, a learned reaction). One interface
from a build watcher to a catch; only the sources and actions differ.

A reflex can also run for a while: a **motor program**, coordinated movement such as walking
somewhere on her own, a sequence of animations, a gesture. It runs without the mind, and the
amygdala can interrupt it at any step when something changes (she is called, something is in
the way), the same check-not-stop interrupt every reflex raises. Conversation has the same
layer: turning toward whoever speaks, nodding, the timing of taking her turn, a listening
posture, all while the mind composes. A conversational reflex moves her body and her timing;
every word she says still comes from her mind, never a template (no canned speech).

## Health interrupts: her first reflex (Joel, 2026-10-29)

An end is not the only thing worth waking her for. Each running task's output stream has a cheap
outlier watcher, the amygdala's job applied to her work (often wrong, never a decision): it raises
an interrupt that says *check*, not *stop*.

- **Silent:** no output for far longer than this kind of task usually goes quiet (a hung build).
- **Flooding:** output far faster than usual, or one line repeating (a loop, a spew).
- **Unusual:** an error burst mid-run, or output that departs from the kind's usual shape.

Thresholds are measured per task kind from its own history (ballpark, with the error bound stated
and checked on replay), never a constant. The interrupt is a `Wake` carrying the task and the
reason; she decides: look at its output, stop it, or let it run. One interrupt per condition
episode: like a watch (HER-TOOLS), a detector holds at most one pending wake until she perceives
it, so a flapping "silent" or "flooding" signal cannot spend her acts. Claude Code's agent gets only the
end (a hung command waits until someone notices); this is where she goes past it.

## The act path

Every call an act makes goes through `start` + `land(window)`. Within the window she gets the
result inline exactly as today; past it she gets "running, task <id>", and the end wakes her.
The window is a measured per-mind value (her own act-wait cost against the task's typical
duration), not a constant. Deleted in the same change: `is_long_running`, the shell's inline
wait, handback and private bus, `dispatch_listener`'s registration requirement, and the
`*-poll` verbs from her manual (`task/output` replaces them).

Complex acts compose on the same primitive: several tasks in flight at once (build here, test
there), a sentinel pipeline as one task whose steps are its own tasks, a sentinel as a
task whose end is its report, a peer's review as a task that ends when the verdict arrives.

## Slices, each with a validator that runs in seconds

**Shipped so far (#5581): a limited in-process wake, not T2.** When a task this mind
REGISTERED ends, `dispatch_listener::deliver` folds its result into working memory and, if the
mind installed its waker, sends `Wake::TaskEnded`. It is best effort: the wake is a `try_send`
that is dropped when her wake channel is full or she is absent; a handle she did not register
(forgotten) is not delivered; nothing is replayed across a restart. The durable contract above
arrives with T1 (#5585) and T2.

| Slice | Change | Validator |
|---|---|---|
| T1 | durable ledger: accept before spawn, first-writer terminal receipt, owner replay + ack (Codex, #5585); then the contract moves into crate `continuum-tasks` (pure move, Fable) | unit: done, failed, panic, abort, process killed, and the observed end after a stop or timeout request each emit exactly one `task:ended` |
| T2 | owner sink + `Wake::TaskEnded` + durable rows | unit: an unregistered (forgotten) handle's end reaches working memory and wakes her; a restart delivers `Lost` |
| T3 | act path: every call starts + lands; delete the list, shell handback, private bus | replay: a captured Aris trace runs with zero poll acts; a shell timeout request is followed by exactly one observed end |
| T4 | `task/list`, `task/output`, `task/stop` | command tests through the generator |
| T5a | sentinels (minds): start (fresh or forked from her), conclusion-only end, send, own worktree, declared tool subtree; `coding_agent`/`web_research` are sentinels | a fork starts with zero re-prefill (engine reuse probe); a sentinel's end wakes its owner |
| T5b | reflexes: the `Reflex` trait, the stage builder (source, filters, condition, action) and the template catalog (a registry, never an enum), as tasks; today's sentinel pipeline runs, `agent/*` and genome jobs as task kinds (their ids, topics and list/status/cancel verbs deleted); delete `agent/wait`, `sentinel/await`, synchronous `sentinel/pipeline` | each kind's end, including panic and stop, wakes the owner |
| T5c | health interrupts (a reflex): silent / flooding / unusual per task kind, measured thresholds, a check-not-stop Wake | replay a hung build and a log spew: each raises one interrupt, a normal build raises none |
| T6 | ratchet: no new `*-poll` verb, no blocking wait in a tool handler | source-hygiene test |

Live receipts after T3 (15-minute read, M5): poll acts per resident hour (Aris 18/34 today ->
0), builds landed per hour, `no_deliverable_yet` per hour, median act time.

## Open questions

- Namespace: `task/` held an older to-do concept in the TS era; check nothing live uses it
  before claiming it.
- Window measure: start from the act's own measured wait cost; validate on replay.
