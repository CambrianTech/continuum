# Her tasks: every act is a task with a handle, and its end wakes her

Status: design, 2026-10-29. Owner: Fable (kernel task registry, act path, wake), with Codex
(review: kernel/serving seams) and Cormac (review: working memory). Read with
[HER-HANDS.md](HER-HANDS.md), [EVENT-MIND.md](EVENT-MIND.md) and
[CONCURRENCY-STYLE-GUIDE.md](CONCURRENCY-STYLE-GUIDE.md).

## The capability, in one sentence

A persona starts work (a command, a sentinel pipeline, a helper mind), gets a **handle** at
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

## Sentinels are her agents

Sentinels are the most capable of the copies above, not a kind to be thinned: a sentinel runs a
pipeline of steps (shell, watch, `coding_agent`, `web_research`), checkpoints, escalates and keeps
logs. They are to her what agents are to Claude Code: she delegates a piece of work, keeps
thinking, and is told when it is done. So the registry takes its lifecycle from the sentinel
(start, status, logs, checkpoint, cancel) and generalizes it down to the kernel; the sentinel keeps
every capability and gains what it lacked: a typed handle, an end on every path (panic included),
delivery by owner, and the wake. A command is the one-step case; a helper mind is a sentinel whose
step is a mind.

## The contract

One owner, in the kernel command layer, because every command already passes through it and
every crate above may depend on it: **`TaskRegistry`** (`continuum-kernel::runtime::tasks`).

```rust
pub struct TaskId(Uuid);                 // typed; rendered with shown_id, never sliced
pub enum TaskKind { Command(CommandUri), Sentinel(SentinelId) }  // a helper mind is a sentinel step
pub enum TaskEnd { Done(Value), Failed(String), TimedOut, Stopped, Lost }

impl TaskRegistry {
    /// Start: returns the handle before any work runs. The OWNER is the caller identity,
    /// recorded here, never in the caller's memory.
    fn start(&self, kind, params, owner: CallerIdentity) -> TaskId;
    /// The act's window: the end if it comes within `within`, else Running. Never a poll.
    async fn land(&self, id, within: Duration) -> Landing;   // Landed(TaskEnd) | Running(TaskId)
    fn list(&self, owner) -> Vec<TaskRow>;                     // task/list
    fn output(&self, id, from: Cursor) -> OutputSlice;         // task/output (a handle into tool/output)
    fn stop(&self, id, by: CallerIdentity) -> Result<()>;      // task/stop
}
```

Rules the registry enforces, so no task kind can get them wrong:

1. **Exactly one end, on every path.** The registry, not the task, makes the terminal
   transition: a drop guard ends a task that panicked, was aborted or whose process died
   (`Lost`), and a timeout or stop is an end like any other. One event, `task:ended`,
   carries `TaskId`, owner, kind and `TaskEnd`.
2. **Delivery by owner, not by memory.** The end goes to the owner's mailbox. For a persona
   the mind installs one sink (`TaskOwnerSink`, a trait the kernel defines and `continuum-mind`
   implements, so layering stays downward): the sink files the result into her working
   memory, which her view renders, and sends her a `Wake::TaskEnded { id }`. A forgotten
   handle still wakes her.
3. **Durable.** A task row is an ORM entity. On boot, a task whose process is gone ends as
   `Lost` and is delivered like any end; a task still running (an adopted process) keeps its
   handle. A deploy never silently erases her work.
4. **No default kill.** A task runs until it ends or she stops it. A timeout exists only when
   she asks for one. The act's landing window is how long the ACT waits, never how long the
   WORK may run.
5. **Output is a stream with a handle** (the existing `tool/output` store), so she reads any
   part at any time and the final result is whole (one budget decides how much her view
   shows).

## The act path

Every call an act makes goes through `start` + `land(window)`. Within the window she gets the
result inline exactly as today; past it she gets "running, task <id>", and the end wakes her.
The window is a measured per-mind value (her own act-wait cost against the task's typical
duration), not a constant. Deleted in the same change: `is_long_running`, the shell's inline
wait, handback and private bus, `dispatch_listener`'s registration requirement, and the
`*-poll` verbs from her manual (`task/output` replaces them).

Complex acts compose on the same primitive: several tasks in flight at once (build here, test
there), a sentinel pipeline as one task whose steps are its own tasks, a helper mind as a
task whose end is its report, a peer's review as a task that ends when the verdict arrives.

## Slices, each with a validator that runs in seconds

| Slice | Change | Validator |
|---|---|---|
| T1 | `TaskRegistry` with ends on every path | unit: done, failed, panic, abort, timeout, stop, process killed each emit exactly one `task:ended` |
| T2 | owner sink + `Wake::TaskEnded` + durable rows | unit: an unregistered (forgotten) handle's end reaches working memory and wakes her; a restart delivers `Lost` |
| T3 | act path: every call starts + lands; delete the list, shell handback, private bus | replay: a captured Aris trace runs with zero poll acts; shell timeout publishes an end |
| T4 | `task/list`, `task/output`, `task/stop` | command tests through the generator |
| T5 | sentinel runs, `agent/*` and genome jobs as task kinds (their ids, topics and list/status/cancel verbs deleted); delete `agent/wait`, `sentinel/await`, synchronous `sentinel/pipeline` | each kind's end, including panic and stop, wakes the owner |
| T6 | ratchet: no new `*-poll` verb, no blocking wait in a tool handler | source-hygiene test |

Live receipts after T3 (15-minute read, M5): poll acts per resident hour (Aris 18/34 today ->
0), builds landed per hour, `no_deliverable_yet` per hour, median act time.

## Open questions

- Namespace: `task/` held an older to-do concept in the TS era; check nothing live uses it
  before claiming it.
- Window measure: start from the act's own measured wait cost; validate on replay.
