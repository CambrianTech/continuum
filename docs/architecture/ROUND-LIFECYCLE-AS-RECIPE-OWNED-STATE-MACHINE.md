# The Round Lifecycle as a Recipe-Owned State Machine (#371)

**Status:** design, not built. Fuses #329(b) (the round has no END), #442 (dispatch
must consume the state pipe), #371, and the unbuilt RULES half of
[[recipe-is-content-type-plus-rules]].

**Joel, 2026-08-16:** *"random and directed by agent, not an ecosystem"* — and,
after a full session of a driver getting lost: *"It's way too hard to rig up.
Clearly."*

---

## 1. The defect, stated exactly

**A benchmark round is not a thing in the system. It is a ritual an agent performs.**

The agent chooses when to dispatch. The agent chooses the watch window. The agent
hand-queries probes to learn what is happening. When the agent's session ends, the
"process" ends with it, and the next agent re-derives it from scratch.

Everything that went wrong on 2026-08-16 is downstream of that one fact. Not a
knowledge gap — a **missing owner**:

| the driver asked | why it could not be answered | what the driver did instead |
|---|---|---|
| is serving ready for work? | nothing owns readiness | dispatched anyway, hoped |
| has the round started? | nothing announces started | read `acts=0`, called it stalled |
| is this run alive or wedged? | liveness = a file mtime that moves once per ATTEMPT | flagged a healthy run `quiet` |
| is the round done? | nothing announces done | never knew; the round just stopped mattering |
| did my fix work? | nothing distinguishes "no fault" from "nothing ran" | reported a vacuous green |

Each row is the same shape: **the question has no owner, so the agent guesses from
an absence, and an absence is not evidence.** That is
[[an-absence-is-an-unfinished-measurement]] — five separate times in one session,
by a driver who had that lesson in its own guardrails.

**The runbook** (`benchmark-round-runbook-the-no-brainer-sequence`) is the current
mitigation and it works — it caught two false alarms within minutes of being read.
But a runbook is a human-followed procedure. It shrinks the blast radius; it does
not move the process into the system. This document is how it stops being a
procedure at all.

## 2. The shape

**A round is an activity. Its recipe owns its lifecycle.** The recipe declares the
stages; each transition is an **event emitted by the component that knows** — never
a timeout, never a poll, never an agent's judgement.

```
                    ┌─────────┐
   dispatch ───────▶│ STAGING │  envs building, cards posting
                    └────┬────┘
      envs staged ok ────┤  (the ENV BUILDER knows — not a timer)
                    ┌────▼────┐
                    │  READY  │  gate open: hosted loops + serving ready (#442)
                    └────┬────┘
   first claim ─────────┤  (the WORK BOARD knows)
                    ┌────▼────┐
                    │ WORKING │  acts landing, patches forming
                    └────┬────┘
   card → done ─────────┤  (the CARD STORE knows — #450, already event-driven)
                    ┌────▼────┐
                    │ GRADING │
                    └────┬────┘
   all cards settled ───┤  (the ROUND ENTITY knows)
                    ┌────▼────┐
                    │  DONE   │  + scorecard. THE END #329(b) says doesn't exist.
                    └─────────┘
```

**Every stage is a ViewState on the same pipe humans and citizens already read.**
"What is it doing, and when will it be ready" becomes a query anyone can make —
the operator, a citizen standing in the room, the dispatcher itself. Never
archaeology.

**Because the recipe is data, the process is identical every round, on every
machine, with no agent in the loop except as another observer.**

## 3. The three laws this encodes

1. **Every transition is announced by the component that knows it.** Not inferred,
   not timed, not polled. The env builder announces staged. The supervisor announces
   hosted. The card store announces done. A stage nobody can announce is a stage
   that does not exist yet — say so, don't fake it with a timeout.

2. **Liveness is a pulse, never a terminal artifact.** Today `benchmark/runs`
   derives `acts` and `stalled` from a ledger written once per attempt, while an
   attempt legitimately runs hours against a 20-minute stall window. The projection
   whose stated purpose is *"silence must never be ambiguous with progress"* is
   structurally unable to tell them apart. A run that is working must SAY so on the
   cadence it works at.

3. **An absence is never a state.** No row means "nothing has reported," which is
   distinct from "nothing is happening" and from "it is finished." The projection
   must carry the difference, because every driver that has to infer it will infer
   it wrong. (Both halves of tonight's vacuous green: zero faults because nothing
   ran, and zero acts because nothing had written yet.)

## 4. What this subsumes

- **#329(b)** — the round has no END. `DONE` + scorecard is the END.
- **#442** — dispatch refuses to stage into a not-ready room. That is the
  `STAGING → READY` gate, expressed as a state instead of a check.
- **#374** — run PULSE as a first-class wire signal. That is law 2.
- **#425** — a bench claim leads to in-room work. The room is the round's activity;
  a roomless solve is a run with no lifecycle to belong to.
- The **RULES half of a recipe** ([[recipe-is-content-type-plus-rules]]) — we built
  content-type and never rules. This lifecycle *is* the rules.

## 5. Acceptance test

From [BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md](BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md),
unchanged and now testable:

> *Can a citizen standing in the room perceive the run's state through the same
> ViewState pipe the human's screen uses?*

Plus one more, earned tonight:

> *Can a fresh driver — with no memory of this session — answer "is it ready, has
> it started, is it stuck, is it done" using only queries, with zero log reads,
> zero probe archaeology, and zero inference from an absence?*

If either needs a file read or a judgement call, it is disconnected and it failed.

## 6. Build order

Smallest true causes first; each independently useful.

1. **Pulse the run while it runs** (law 2). `WorkspaceCycle::actions_taken()` is the
   seam and already exists (c9ba5f943) — a heartbeat consumes it so `acts` and
   last-activity are live. Kills the false `quiet` immediately.
2. **Round entity owns stages.** `bench_round::register_round` already exists;
   give it the stage field and the transition subscribers.
3. **Stage transitions from real emitters.** Env builder → staged. Supervisor →
   hosted/ready. Card store → done (#450 already fires). Round → settled.
4. **`RoundViewState` on the pipe**, folded from the round entity — not a file scan.
   Retires the 5s progress-directory poll in `positron_bench_source`.
5. **Dispatch consumes it** (#442): refuse to stage while not READY, and say why.

## 7. The smell to catch yourself on

If you are about to add a timeout, a retry window, a sleep, or an agent-side
heuristic to decide what stage a round is in — **stop.** That is the ritual growing
back. The question is always *which component already knows this, and why isn't it
saying so?*

And if you are about to report a state derived from something you did not observe
happening — stop. Say "I did not observe it," and name the query that would.
