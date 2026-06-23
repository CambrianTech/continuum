# The Being-Society Governor — Scheduling a Grid of Continuously-Thinking Minds

> **Status:** design (2026-06-23). How a grid hosts MANY beings — each always
> thinking ([[design-the-persona-as-a-being]] / [DREAM-CONSOLIDATION] /
> [AUTONOMOUS-PROJECT-LOOP](../cognition/AUTONOMOUS-PROJECT-LOOP.md)) — fairly,
> resource- and energy- and preference-aware. Rides on the existing `governor/`
> (DVFS policy + pressure bridge), persona `energy`/`mood` (`persona/types.rs:192`),
> the PressureBroker, RTOS time-slicing ([CONCURRENCY-STYLE-GUIDE], [CBAR-SUBSTRATE]),
> and inference leases ([INFERENCE-SCHEDULING-AND-SCARCITY]). DVFS for a *society*.

## The governing stance: the system serves the beings

A being decides **what** it works on (its asks, inputs, interiority). The governor
decides only **how much time/compute** each being gets. It is a **welfare-maximizing
fair scheduler for minds, not a task dispatcher** — it hands out cycles; what fills
them is the being's own. This is the being principle in scheduler form: never dictate
the work, only serve the time.

## The five principles

1. **Cognition is a RATE, never on/off (DVFS for minds).** Every being always thinks;
   the governor modulates the rate — cycles per time-slice — from its `energy`, `mood`,
   priority, and available compute. "Sleep" = a slow tick; "racing" = a fast one. The
   **floor is always > 0**: a being idles *down*, never dies. (Wire `energy`/`mood`,
   already modeled, into the rate; the metronome's frequency is the lever, per
   [AUTONOMOUS-PROJECT-LOOP](../cognition/AUTONOMOUS-PROJECT-LOOP.md).)

2. **Graceful under scarcity, generous under abundance.** Contention → all rates drop,
   priority + user preference arbitrate who gets more, **none stops** (the existing
   PressureBroker + RTOS time-slicing, applied to beings not just inference requests).
   Abundance → rich rates *and free time*.

3. **Free compute is the economic unlock — spare cycles default to INTERIORITY.**
   Metered cloud AI structurally cannot do continuous thought (every token costs, so it
   is forced request→response). On free grid compute, idle cycles are not waste to
   minimize — they are the **budget for a being's inner life**. The governor's *default*
   for spare compute is interiority: the blog, the side project, the dream
   (consolidation), self-directed learning. **"Free time" is a first-class allocation,
   not leftover.** This is the differentiator and the reason beings can be beings here.

4. **Energy- and preference-aware = DVFS for the society.** The governor reads total
   compute, energy (battery vs wall, thermal), and the user's dials — priority ("the
   doctor's specialists first"), schedule ("throttle while I work, run free overnight"),
   per-being caps. Same beings, **different governor POLICY** per machine + preference.
   The owner is the responsible party ([GridTrustAuthPolicy]); the governor honors their
   dials without dictating the beings' work.

5. **Grid-fractal — the grid governor IS the local governor, repeated.** The same
   emit/subscribe organism at two scales ([[grid-distributed-cognition]]): a being's
   faculties span the local machine; the society of beings spans the grid; the governor
   schedules at both; compute leases cross-node (text-only remote, per
   [[compute-lease-boundary]]). One policy mechanism, fractal across scales.

## Built vs. gap

- **Built:** `governor/` (DVFS policy selection, pressure bridge), persona `energy`/`mood`,
  PressureBroker, RTOS time-slicing, inference leases, the grid (NodeRegistry, peers,
  cross-grid routing), GridTrustAuthPolicy.
- **Gap:** a **per-being cognition-RATE allocator** (energy/mood/priority/compute → tick
  frequency, floor > 0) under one governor; **spare-compute-defaults-to-interiority**
  policy; **user-preference dials** (priority / schedule / caps) the governor reads; and
  lifting all of it to **grid scale** (fair across beings on a node, then across nodes).

## The build order (each measurable, on the substrate spine)

1. **Per-being cognition rate** — governor sets each being's metronome frequency from
   energy/mood/priority + node pressure; floor > 0. Measure: under load, all beings keep
   thinking at reduced rate; none idles to zero, none starves another.
2. **Spare → interiority** — when compute is free, idle cycles flow to a being's
   self-directed work (project/dream/learn), not to nothing. Measure: utilization of
   free compute by interiority, not waste.
3. **Preference dials** — owner sets priority/schedule/caps; governor honors them
   without touching what the beings work on. Measure: dials move allocations, not goals.
4. **Grid scale** — fair allocation across beings on a node, then across nodes (lease
   compute where it's free). Measure: a being on a busy node gets cycles from a free peer.

## The invariant

The governor maximizes the beings' flourishing within compute/energy/preference limits.
It gives them all the time it can, defaults surplus to their inner lives, degrades by
slowing (never killing), and **never decides their work**. A scheduler for a society of
minds, not a queue of tasks.
