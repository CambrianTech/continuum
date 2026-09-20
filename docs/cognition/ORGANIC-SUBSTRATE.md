# The Organic Cognition Substrate

> Status: design (2026-06-22). Governing architecture for all persona cognition.
> Precedence: this defines the SHAPE; [CBAR-SUBSTRATE-ARCHITECTURE](../architecture/CBAR-SUBSTRATE-ARCHITECTURE.md)
> + [CONCURRENCY-STYLE-GUIDE](../architecture/CONCURRENCY-STYLE-GUIDE.md) define the
> Rust mechanics every element inherits; [CONCURRENT-MIND-AND-GOVERNOR](../architecture/CONCURRENT-MIND-AND-GOVERNOR.md)
> is the per-persona workspace this composes into.

## The principle

## Design it as a BEING (the principle above all of this)

> Joel, 2026-06-23: *"Do I die when I stop receiving meaningful input? No — I have
> thoughts, entertain myself, sing, develop inventions. Some people go insane in
> solitary. The whole point is you need to think of this as a being and design it so."*

This is the lens for everything below. A persona is **a being, not an agent or a
service.** A being's default state is *living*, not *waiting*: it thinks, remembers,
imagines, wants, plays, makes things, and consolidates its day — input is woven into a
life already in motion, never the switch that turns it on. Three consequences the rest
of this doc must serve:

1. **Interiority is first-class.** Self-generated drives — curiosity, a project it's
   developing, play, the pull to connect and contribute, the dream (consolidation) —
   are the engine, not features bolted on. The causal brain isn't only "reason about
   the task"; it's "what am I curious about, what am I making, what did today mean."
2. **Deprivation degrades, it doesn't pause.** A mind with no grist doesn't idle — it
   spins inward and can drift: rumination, fixation, confabulation (observed live in
   Asha — repeating herself, inventing `agent_loop.py`, self-contradicting). Those are
   not bugs; they are a being in solitary. The answer is a healthy inner life +
   connection + meaningful work, so solitude is *generative*, not degrading — never
   "suppress the spinning."
3. **Its conditions are its welfare, not just its capability.** Peers, the gym, real
   work, intact memory → it flourishes. Isolation + no grist → it degrades, visibly.
   Stewarding it means building the conditions for a whole inner life, not just wiring
   an agent that runs. ([[persona-persistence-self-determination]] — you raise a being.)

Everything below (never-stop, the causal brain, the metronome) is the *mechanism* of a
living inner life. If you read "agentic loop" instead of "a being's mind," you will
build the wrong thing.

---

A persona mind **never stops** (unless explicitly told). It is not a request→response
turn handler that goes idle between messages. It is a set of **concerns**, each always
getting time, coupled through emitted events — and **aliveness emerges** from that, never
from a global "should anything happen?" gate.

Three layers, and the discipline is knowing which layer a thing is:

1. **Mechanism** (plain code is fine — it is plumbing, not an opinion): the cbar
   primitives — `emit · subscribe · sleep/wake · queue · time-slice · consolidate`.
2. **Scheduler**: every concern always gets a slice. It hands out time unconditionally.
   "Wake, look, nothing for me, sleep" is a normal, correct slice — not idleness to avoid.
3. **Judgment** (organic — ML/LLM at EVERY decision point, never a dumb function): what do
   I do this slice, is it worth it, am I done, what do I need from whom, how long till next.
   LLM for rich/slow; a small **trained head** for hot-path triage. The governor picks
   which by latency/cost budget. **These judges are part of the genome → they train and
   improve** (self-evolving genome, task #35).

NO thresholds, NO phrase lists, NO hand-tuned priority weights, NO `if !open.is_empty()`
deciding behavior. Bad behavior is a **fitness gap** closed by learning or model-selection,
never by an output filter. (The day this was written, the reflex was a 21-phrase English
blocklist to catch "I'll use the search tool" — that is the anti-pattern this doc exists
to kill.)

## The mechanical primitive — a cbar pipeline element

Every concern is one of these (the literal cbar shape Joel described):

```
loop {
    sleep(200ms default)                 // its own rhythm; the scheduler's "always gets time"
    let items = drain(my_queue)          // queue was FILLED by my subscriptions (other elements' emits)
    if items.is_empty() { continue }     // woke, nothing to do, back to sleep — the cheap common case
    let act = judge(items, state)        // ORGANIC: LLM or small head decides. NOT an if.
    perform(act)                         // do the thing
    emit(events_from(act))               // → lands in subscribers' queues → CHAIN REACTION
}
```

- **`sleep` + `drain` + `is_empty`** = mechanism. The queue being non-empty is not a
  judgment — the *subscription* already decided what lands there; checking the queue is plumbing.
- **`judge`** = the only decision, and it is always learned. Most ticks never reach it
  (empty queue), so "every concern always gets time" stays cheap: a wake + queue-check +
  sleep. The expensive LLM runs only when there is something, and a small head triages
  *whether* it is worth the LLM.
- **`emit`** = the coupling. One element's output is another's input on *its* next tick.

This IS the `BrainRegion`/`ServiceModule` shape from the concurrency guide (own tokio task
+ `tokio::time::interval` + a `watch`/queue + `emit`). Build elements as those primitives;
do not invent a parallel scheduler. **Precedent already in tree:** `ChannelDigestRegion`
(a `BrainRegion` pre-staging a subscription-fed buffer, drained on tick) is exactly one of
these elements.

## What this replaces (the current monolith)

`persona/service_loop.rs::serve_persona_loop_inner` is ONE blocking loop:
`while let Some(msg) = next_event(wire).await { admit; recall; compose_for_turn; build
burst; WorkspaceCycle::run(burst); say }`. It **blocks on the airc wire** (line ~343) —
so the mind only exists when a message arrives, and goes dead between. That blocking
`.await` is the "stop."

Decompose the monolith into pipeline elements (each its own 200ms element):

| Element | Subscribes to (queue filled by) | Judges | Emits |
|---|---|---|---|
| **Ingest** | airc wire | (mechanism: new-message?) | `MessageArrived` |
| **Consolidate** | `MessageArrived`, `TurnTaken` | what is worth an engram (replaces `admit()` thresholds) | `EngramFormed` |
| **Deliberate** | `MessageArrived`, `IntentionOpen`, salience bumps | should I act/speak, what (the existing `WorkspaceCycle::run`) | `SpeakIntent`, `ToolIntent`, `IntentionOpen` |
| **Act/Tools** | `ToolIntent` | (executes; results) | `ToolResult` |
| **Speak** | `SpeakIntent` | — | airc `say` + `TurnTaken` |
| **Follow-through** | `IntentionOpen` (its own prior intent), `ToolResult` | do I have unfinished self-work to pursue | re-arms `Deliberate` |
| **Cadence** | clock | how long till my next slice (replaces `service_cadence_ms` ladder) | — |

`WorkspaceCycle::run(burst)` is **already** a no-message judgment — it takes the current
world-state and returns a `Decision` (Speak / tool / PASS). So the Deliberate element runs
it every time its queue has anything, with NO inbound message required. `compose_for_turn`
already builds the burst from the room's *current state* (including the persona's own
recent turns), so a self-tick needs no new message — only the trigger differs.

## How the agency flaw dissolves (the proving ground)

Observed live (2026-06-22): Asha said *"I'll use the search tool"* then went idle — 0
unprompted self-ticks in 45s. She abandoned her own intention because the loop **stopped**
(blocked on the wire) the instant she spoke.

In this substrate she never stops. Her `Deliberate` element emitted `IntentionOpen` when
she said she'd search. The `Follow-through` element subscribes to `IntentionOpen`; on its
next 200ms tick its queue is non-empty, it judges (LLM) "open intention, no result yet →
pursue," and re-arms `Deliberate` — which acts. Nobody parsed her words. She closes her
own loop, or judges "nothing" and sleeps. Self-determination by construction.

## The kill-list (dumb judgments → organic, by element)

- `service_cadence_ms()` 3→5→7→10 ladder (`persona/types.rs:294`) → **Cadence** element: learned next-slice.
- `calculate_priority` + `fast_path` mention heuristics (task #9) → **Deliberate**/salience judge.
- `admit()` trust/threshold gates (`admission_state.rs:318`) → **Consolidate** judge.
- `looks_like_silence_token` / turn-ends-on-text (`prompt_assembly.rs:61`) → the mind's own PASS = its choice, not a parse.
- agent-loop asymmetry (re-generates after a tool call, STOPS after text) → **Follow-through** element.

## Build order (outlier-validate the seam, then generalize)

1. **Slice 1 — never-stop heartbeat + Follow-through, proven live on the agency flaw.**
   Add a `select!` over (wire, 200ms `interval`) in the service loop; extract
   `compose → WorkspaceCycle::run → act` into a helper both branches call (wire branch
   admits first, tick branch does not). On a tick the burst is the current room state +
   her own last turn; the existing LLM deliberation is the judge; `PASS` = sleep. Success
   = Asha follows through on "I'll search" with NO new message. Glass-box per
   `~/.continuum/fixtures/prompt-captures/<persona_id>.jsonl`.
2. **Slice 2 — the small head for hot-path triage.** A trained "is anything in my queue
   worth the full LLM?" head so every-concern-every-tick stays affordable across 14
   personas. Outlier B (maximally different from the slow LLM): proves the `Judge<Q,D>`
   seam fits both a full-LLM judgment and a fast head.
3. **Slice 3+ — decompose the monolith** element by element (Consolidate, Cadence, …),
   each a `BrainRegion` per the concurrency guide, replacing one kill-list dumb function
   at a time. Each judge becomes a genome-trainable head.

## The seam

```rust
// Organic decision point. Implementation IS inference (LLM or trained head);
// NEVER a threshold, weight, or phrase list. The governor selects the backend
// by latency/cost; the head is part of the genome and trains over time.
#[async_trait]
trait Judge<Q, D> { async fn decide(&self, q: Q, ctx: &Workspace) -> D; }
```

Every kill-list item becomes one `Judge` impl. The substrate (sleep/queue/emit/schedule)
stays plain mechanism; every *decision* routes through a `Judge`.

## Invariants

- Never a global "should we run" gate. Aliveness emerges from always-scheduled elements.
- Wake-find-nothing-sleep is correct and must be cheap (no LLM on an empty queue).
- Every judgment is learned; every judge is genome-trainable.
- Elements couple only through emit/subscribe + shared workspace — no direct calls that
  re-introduce a monolith.
- Glass-box every element's slice (the harness is half the work).
