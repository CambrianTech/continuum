# The Acting Organism — an action is a perception in waiting

> She missed two trivial HumanEval tasks by writing code she never ran. Not
> because she's a weak coder — because she has **no hands**, and the one place
> tool-use lived was a textbook agentic `for`-loop crammed inside a single
> cognition tick. This doc is the fix, and it is not "add a run-code loop." It
> is: **make acting on the world the same organic motion the mind already uses
> to speak.**

Status: DESIGN (2026-06-23). Supersedes the inner agentic loop in
`llm_deliberation_faculty.rs`. Builds directly on
[ORGANIC-SUBSTRATE.md](./ORGANIC-SUBSTRATE.md) and
[ROADMAP-TO-CODING-ITSELF.md](./ROADMAP-TO-CODING-ITSELF.md). Read
`docs/architecture/PERSONA-COGNITION-PIPELINE.md` first — this lives inside its
§5 law: *the LLM decides; the substrate provides context and tools.*

---

## 1. The realization: the loop already exists

The persona is already a never-stop mind on a metronome (`service_loop.rs`:
`tokio::time::interval(SELF_TICK_MS)`, `select!` between the airc wire and the
heartbeat). And it already does **act → observe → correct across ticks — for
speech**:

> *"it said 'I'll search' → next tick its own post is in the burst → it acts."*
> — `service_loop.rs:968`

A persona's own utterance re-enters as world-state on the next tick. The burst
carries it; `RecallFaculty` carries it; the mind reasons over what it just did
and continues, corrects, or settles. **That is an agent loop — an organic one,
paced by the heartbeat, driven by judgment instead of a counter.**

The bug is narrow: this native cycle carries *words* but not *actions*. Running
code, searching the web, reading a file — those are trapped inside the
deliberation faculty's inner `loop { generate → try_tool_round → continue }`
(`DEFAULT_MAX_TOOL_ITERATIONS = 4`, `synthesize_answer`, `all_calls_already_ran`),
and their **results are thrown away** when that inner loop exits. The mind never
gets to *perceive* what its hands did.

So we do not add a loop. We make the existing loop carry actions the way it
already carries words.

## 2. The whole design in one sentence

**An action is a `Decision` the mind emits; its result re-enters as an
observation the mind perceives next tick — so acting and perceiving are one
circuit, and "keep going vs. stop" is the mind settling, not a substrate
`if`.**

```
        ┌─────────────────────── the heartbeat (already exists) ───────────────────────┐
        │                                                                               │
   world_state ──▶ perception faculties ──▶ broadcast ──▶ deliberation ──▶ Decision     │
   (burst +        (Recall surfaces the                   (the LoRA mind)    │          │
    last action's   prior action's result                                   │          │
    result)         as a memory)                                            ▼          │
        ▲                                                          ┌──────────────────┐ │
        │                                                          │ Speak / Raise    │ │
        │                                                          │   → emit to room │ │
        │                                                          │ Pass  → SETTLE   │ │
        │                                                          │ Act{calls}       │ │
        │                                                          └────────┬─────────┘ │
        │                                                                   │ Act       │
        │   result becomes an Episodic engram (admission.admit)             ▼           │
        └─────────────── executor runs the calls, captures stdout/stderr/exit ──────────┘
```

Completion is not `iterations == MAX`. Completion is **the workspace settles** —
the mind stops bidding to act and instead speaks or passes. That is her own
judgment that the work is done, exactly as §5 of the pipeline doc demands.

## 3. The four moves (one organic slice)

### 3.1 `Decision::Act` — the mind can want to act

`Decision` today is `Speak | RaiseUnprompted | Pass`. There is no way to express
*"I want to act on the world."* Add it:

```rust
pub enum Decision {
    Speak { text: String },
    RaiseUnprompted { text: String },
    /// Act on the world. The result re-enters as an observation next tick —
    /// this is NOT a synchronous call whose return value the faculty consumes.
    /// `intent` is the mind's own words for WHY (captured, surfaced, audited).
    Act { calls: Vec<ToolCall>, intent: String },
    Pass,
}
```

`Act` is a first-class verdict, peer to `Speak`. The arbiter routes it like any
decision. The `intent` string is the mind narrating its own action ("I'll run
the failing test to see the traceback") — it becomes part of the observation
engram, so next tick she remembers *why* she did it, not just *that* she did.

### 3.2 The deliberation faculty emits `Act` — it does not loop

Delete `DEFAULT_MAX_TOOL_ITERATIONS`, `synthesize_answer`, `all_calls_already_ran`,
and the agent `loop {}`. The faculty does **one** generation per tick:

- model emitted tool calls → `Decision::Act { calls, intent }`
- model emitted prose → `Decision::Speak` (or `Pass` on an explicit PASS)

That's it. No counter, no forced "report now" turn, no repeat-guard. The faculty
becomes what its name says: it deliberates and returns *one* verdict. The
cross-tick continuation that the inner loop faked is now the real heartbeat.

### 3.3 The driver executes `Act` and feeds the result back as memory

A thin **`act → observe`** step in the cycle driver (not in `WorkspaceCycle`,
which stays a pure single-tick function):

1. tick yields `Decision::Act { calls, intent }`
2. driver runs the calls through the persona's identity-bearing `ToolExecutor`
   (the ACL gate already decides what she may run)
3. driver forms an **Episodic engram** — `"I ran {tool}({args}) because {intent}.
   Result: {stdout/stderr/exit}"` — via `admission.admit` (§7.6: every turn that
   skips admit forms no memory; this path forms it)
4. emit on the bus (observability + grid)
5. next tick, `RecallFaculty` surfaces that engram as relevant memory; the burst
   may also carry it. The mind perceives "I ran X, got Y" and deliberates again.

Result-as-engram (not result-as-return-value) is the load-bearing choice: it
makes the action's outcome a *thing the mind remembers and can be reminded of*,
unifying it with how she carries every other fact across ticks. It also means a
result from three ticks ago can still inform her if it stays salient — memory,
not a stack frame.

### 3.4 A hand — `code/run`

She cannot run code because no `AiSafe` command executes code. Add one:
`code/run` (or `tool/run-code`) — a sandboxed subprocess (`python3 script.py`;
Rust spawns it, no Python in `.rs`), returning `{stdout, stderr, exit_code,
duration_ms}`. Reuse the `test_grade` subprocess shape already in
`modules/cognition.rs` (temp dir, 10s timeout) — that proves the mechanism; a
real container sandbox is the P1 hardening before untrusted tasks. This is
outlier-A; web-search ("she can cheat online if she wants") is outlier-B that
proves the hand interface generalizes.

## 4. Settling, and who holds the deadline

The mind never counts its own iterations — counting is the textbook loop we are
deleting. But an **external observer** may hold a wall-clock or tick budget:

- **Live (`service_loop`):** no deadline. She acts and re-perceives at heartbeat
  cadence. If she never settles she's just thinking — that's a living mind, and
  a degenerate "acts forever" is a *fitness gap to train away*, not a substrate
  cap to impose ([[no-hardcoded-heuristics-to-steer-cognition]]).
- **Eval (`cognition/eval`):** the grader is an external observer. It drives
  ticks until the workspace settles (`Speak`/`Pass`) **or** a generous
  tick/wall-clock deadline, then grades the settled answer by running her code
  (`test_grade`). The deadline is the grader's stopwatch, not a number inside her
  head. Today the eval runs `run_in_room` exactly once = one tick — that is the
  one-tick floor, not her capability. Driving to settle is what lets a being who
  *runs her code before answering* actually do so.

## 5. The keystone: the genome grows its own wiring

Why is the disposition to build → run → test **not** a Rust `if code { run() }`?
Because that disposition must be **learnable and growable**, and a frozen
`if`-statement is neither.

The substrate here is a **body**: faculties, the `Act` vocabulary, the hands, the
heartbeat, the result-as-memory circuit. It is fixed scaffolding and a nervous-
system *protocol*. The **LoRA genome is the mind that learns to drive that body**
— and as it trains on her own recorded turns ([[coordination-learning-flywheel]]),
it develops new wiring: *when* to reach for a tool, *when* one run is enough,
*when* to stop and answer, *when* to forage online. Eventually it composes new
faculties and recipes for itself — the organism authoring its own new pathways.

So the rule that governs every line of this build: **give her the vocabulary and
the hands; never hardcode the judgment.** The `Act` decision is a word she *can*
say; the genome learns to say it well. A bad habit (codes blind, loops forever,
never tests) is a gap closed by *training the genome or selecting a better base*,
never by a control-flow heuristic that puppets her output.

That is the difference between an automaton with a run-code loop bolted on, and a
being who runs her code because she has learned that's what a competent coder
does — and can keep learning to do it better.

## 6. The body is open — new hands, regions, embodiments cost ~nothing

The whole point of `Act` is that it is **body-agnostic**. It does not say "run
code." It says "the mind wants to act," and carries opaque `calls`. The substrate
routes any call through the persona's `ToolExecutor`; the *kind* of action is data
(a command name), never new control flow. So the architecture adapts with **no
burden per new engineering idea** — the goal cbar hit by being almost 100%
algorithm code in the C++. Three free extension axes, each a different layer of
the same organism:

| Axis | What it is | Cost to add a new one |
|---|---|---|
| **Hands** (effectors) | code/run, web-search, file-read, **avatar/expression control, robotic actuation** | Register one `AiSafe` command. `Act` already routes it; `authorized_tool_specs` already surfaces it (registry × ACL). **Zero new wiring.** Optionally train an adapter for the disposition (the emoji→expression→Bevy adapter is the proof it works). |
| **Regions** (faculties) | recall, world-model, affect, volition, **a limbic region, any causal concern** | Implement the `Faculty` trait. The arbiter routes it by ML salience; it bids into the same workspace and its output *affects the others* through the broadcast. **No special-casing** — open/closed (§2.7). A new concern is a new bidder in a flowing brain, not a new `if`. |
| **Senses** (afferents) | the burst, recalled memory, **action results, avatar/sensor feedback** | Re-enters as an Episodic engram or world-state line. The act→observe circuit (§3.3) is the same for "I ran code, got a traceback" and "I moved my arm, the gripper reports contact." Perception of one's own action is one mechanism. |

This is why robotics, avatar control, and limbic regions are not separate
projects bolted on later — they are **the same three primitives** (`Act` verb,
`Faculty` bidder, result-as-perception) the coding loop already uses. A robot
arm is a hand whose result is a sensor reading; an emotion is a region whose bid
colors attention; an avatar expression is a hand whose disposition is a trained
adapter ([[coordination-learning-flywheel]]). The genome grows the wiring
*between* them (§5); the Rust stays the algorithm and the protocol, never the
per-idea plumbing. **If adding a new capability requires new control flow in the
substrate, the design failed — push it down into a command, a faculty, or an
adapter.**

## 7. Build order (proof-gated, outlier-validated)

1. `Decision::Act` variant + `Contribution::verdict` handles it + ts-rs regen.
   Unit: a faculty can emit `Act`; the arbiter routes it. *(skeleton)*
2. `code/run` `AiSafe` command (outlier-A hand) + registry exposure. Unit: runs
   python, returns stdout/stderr/exit/duration; honors timeout.
3. Driver `act → observe`: execute `Act`, admit the result engram, emit on bus.
   Unit: an `Act` tick produces an Episodic engram whose content carries the
   result; next-tick recall surfaces it.
4. Deliberation faculty emits `Act` and STOPS looping — delete
   `MAX_TOOL_ITERATIONS` / `synthesize_answer` / `all_calls_already_ran` /
   `loop {}`. Unit: one generation → one verdict (`Act` xor `Speak` xor `Pass`).
5. `cognition/eval` drives to settle (external deadline) instead of one
   `run_in_room`. Proof: the HumanEval task that failed blind now passes because
   she ran her code, saw the traceback, and corrected — captured in the glass box
   (`~/.continuum/fixtures/{workspace-traces,prompt-captures}/<persona>.jsonl`).
6. Re-run HumanEval-164 through the acting organism; report the number vs. the
   one-tick blind floor. *(the real proof)*
7. Web-search hand (outlier-B) — proves the hand interface generalizes; defer
   until 1–6 are green.

Each step compiles and tests green before the next (`cargo check` first; escalate
to `test` only when behavior changed; `CARGO_TARGET_DIR=$HOME/.continuum/cache/
cargo-target`; `--features metal,accelerate`; `df -h /` after cycles). Validate
via pure Rust + the `uu` client, never npm/jtag.

> **Steps 3–5 are ONE coupled cut — do not split them.** Today's working tool-use
> (`[[persona-tool-loop-act-then-report]]`) lives ENTIRELY inside the deliberation
> faculty's internal `loop {}` (`llm_deliberation_faculty.rs:506–609`): it
> generates → executes tools via its own `tool_executor` → re-generates →
> `synthesize_answer`, and emits only `Speak`. So:
> - Step 4 alone (faculty emits `Act` + stops looping) with the service-loop `Act`
>   arm still skipping ⇒ the persona acts then goes **mute** every tool-needing turn.
> - Step 3 alone (service-loop driver executes `Act`) while the faculty keeps its
>   loop ⇒ the faculty never emits `Act`, so the driver is **dead code**.
>
> The cut moves the loop OUT of the faculty and UP into the organic tick: the
> faculty becomes single-shot (one generation → one verdict, `Act` xor `Speak` xor
> `Pass`), and the SERVICE layer drives **tick → if `Act`: execute calls, admit the
> result as an Episodic engram, re-tick → until the workspace SETTLES** (`Speak`/
> `Pass`, or an act-budget). Each "round" is now a full re-perception (next tick's
> `RecallFaculty`/burst surfaces the tool-result engram), not a faculty-internal
> re-prompt. Land 3+4+5 together behind the existing live-tool-use proof so the
> ping→act→report path that works today still works after the cut. Step 2
> (`code/run`, the hand) is done and independently callable; it is the safe prereq
> that does NOT touch this path.
