# Degrade, never refuse: four patterns that remove collapses and rejections for every activity

> Joel, 2026-11-07: "Refusal is insane." / "It's the complete collapses and rejections we have got to fix with elegance and pattern improvements. Think long and hard about how to fix for many cases at once."

Status: DESIGN. Owners per pattern below. Evidence: `~/.continuum/evidence/live-read-20261107/` on the M5 (handholding-audit.md, relaunch-1613-plan-grid.log, engine-abort-1613Z.txt).

## What happened on 2026-11-07

On the M5, 40.7% of captured acts ended in a refusal or a redirect (583 of 1,432), and 30% were outright refusals. The node was effectively down for about 2.5 hours. The model and engine were fast: Windows residents act every 8-9 s with 89-95% cache reuse. Our layer around them wasted the acts.

| Failure | Pattern that was missing |
|---|---|
| The in-engine trainer's backward assert aborted llama-server mid-serving | 2 fault domains |
| The relaunch planned from the dead engine's "0 bytes"; 35,328-token lanes against 50-70k needs; nothing grew back; 2 h parked on capacity | 1, 3 |
| Needs recorded from uncollapsed overflow (902k on a 262k model); every mind "unservable"; the planner's demand_window at 23.6M | 1, 3, 4 |
| A relock deadlock in one module's reconcile wedged the whole core's boot | 2 |
| The pointer tool level offered `{}` per argument; 67 argument refusals in one Windows window | 1, 4 |
| Refusals suggested the exact call that had just refused | 1, 4 |
| Her newest result was shrunk to a handle 159 times (~105 follow-on acts) | 1 |
| Work-verb guards refused ("claim it first", "no submission yet", "missing repo") | 1 |
| Prompt captures stopped silently; the pool never re-grew after a relaunch | 2, 3 |

## Pattern 1: degrade, never refuse

Every request is served at the best level that fits. The request may be an act, a tool call, an engine launch or a learn step. The flexbox already does this for the prompt (#5814). This generalizes it into one primitive:
- **A ladder per request:** each request declares levels from best down to an irreducible minimum.
- **Serve the highest rung that fits:** the system picks the highest level that fits and serves it.
- **The minimum is servable by construction.** For an act, that is her newest result whole plus her hands. For a call, it is its unambiguous reading (#5836). For a launch, it is the seated held needs (#5835). For a learn step, it is skipping the step.
- **Below the minimum, wait.** When even the minimum doesn't fit, the request waits in admission with a typed position. It is never refused and never parked silently.
- **Refuse only when nothing serves her intent.** That means a security boundary, a card held by another, or an irreversible action she didn't name. Even then, the refusal carries the call that works (#5830 + the self-suggestion guard f151abc3).

## Pattern 2: fault domains

A failure costs its own component, never the node. Every component declares a boundary and a degraded mode.
- **Boot binds the core's socket first.** Each module initializes under a deadline and, past it, comes up degraded with a typed reason. No module can wedge boot.
- **Learning validates before it mutates and refuses with a type** (fork #99, card 4365916b). A fault suite drives every learning entry with bad inputs and requires `/health` afterwards.
- **Writers never latch dead.** A failed write is retried and probed (the saved-state contract, #5832).

## Pattern 3: unknown is a type; every loop recovers both ways

The dead engine's 0 bytes was unknown, not zero. The 902k need was censored overflow, not a need.
- **Every measurement a decision reads carries its state:** Measured, Gap or Censored.
- **Decisions on an unknown use the last known good with its variance** (the controller, card 8a983e31; gap budget, card in #5835's review).
- **Every resizing loop has a grow-back path.** The pool, lanes, grants and windows all recover upward (#5835's `recovering`), with the same damping and deadband as their shrink path.

## Pattern 4: one source generates both sides of every contract

These pairs each come from one source, with a round-trip test across all instances:
- the offered schema and its decoder, from the Params type, at every collapse level (#5800);
- the recorded need and the planner's demand (#5833; wish never a floor);
- a refusal and its next call (#5830).

The ratchet: for every registered tool at every level, a value generated from the offered schema decodes through that tool's own Params.

## The ledger that keeps it

- **Every collapse is a typed event on the live read:** every refusal, park, wedge, abort and silent stop. The read counts them per resident against acts, with a target near zero (#5836's `refused_hour`).
- **Every class above zero names the pattern that failed,** and its fix goes into that pattern, never into the leaf.
- **The patterns are activity-agnostic.** A game, voice or chat inherits all four, so no new activity rebuilds these failures.

## Order

Pattern 2's boot deadline goes first, because a wedge takes the whole node. Next comes pattern 1's admission wait in place of parking. Then pattern 3's typed budget and need, and pattern 4's all-tools round-trip ratchet. Each lands with its ledger class going to zero on the next live read.

## Review answers (Codex, 2026-11-07)

- **The admission wait has states, not immortality.**
  - **Tell apart a transient shortage from an incompatible demand.** A transient shortage waits for capacity to free. An incompatible demand is one whose irreducible rung exceeds what the model can physically hold.
  - **A wait wakes on a named transition:** eligibility, a replan, or a placement or owner change. It can be cancelled by her or by her activity.
  - **An incompatible demand never waits forever.** It is a typed result that asks for a different placement (grid) or model.
- **A schema-generated sample is not an all-instance proof.** Pattern 4 keeps two things:
  - **structural schema equivalence** at every callable level, for every tool;
  - **decoder tests** covering unions, refs, required fields and constraints.
  Custom `Deserialize` validations that the JSON schema can't express need explicit fixtures. The ratchet lives on the existing `CommandSpec` conformance boundary, not in a parallel registry (card 9ad69f13, Codex).
- **A writer's retry first reconciles an uncertain append** under the existing owner (receipt or tail check) before it replays, so a retry never duplicates.
- **"Last known good" is bounded.** A last-good measurement counts only when it matches the current engine incarnation and config, and it expires. Stale capacity never stands in for a measurement.

## Review answers (Cormac, 2026-11-07, patterns 1 and 3)

- **The act's minimum includes room for her reply:** her newest result whole, her hands, and her reply's measured need. An act without room to answer isn't servable.
- **A rung changes only at an act's start, never mid-act.** That keeps the prefix stable for the engine's cache (the context codec's rule: collapse only at a start).
- **A skipped learn step is typed** (`LearnSkipped{reason}`), never silent. The learning ledger counts it like any other collapse.
- **A censored measurement carries its bound:** `Censored{at_least}`, never a bare "unknown". Decisions use the bound with its variance.
- **Grow-back is an upward trial,** with the same damping and deadband as a shrink, and kept only if the trial's measurement holds (every downward move has a scheduled upward test). The proven pool it trials toward is keyed by (model, config, engine incarnation), on card 8a983e31.
