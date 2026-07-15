# Will-Driven Resolution — the persona asks for effort like a camera SDK asks for resolution

**Status:** design (2026-07-14). Synthesized live with Joel. Composes existing in-tree
primitives (see §7); introduces one new value object (the *Will* constraint) and one new
learned component (the *effort predictor*). No new allocators, no hardcoded counts.

Related: [`INFERENCE-SCHEDULING-AND-SCARCITY.md`](INFERENCE-SCHEDULING-AND-SCARCITY.md)
(the ceiling), [`INFERENCE-LANES-REALISTIC.md`](INFERENCE-LANES-REALISTIC.md) (the floor),
[`CONCURRENT-MIND-AND-GOVERNOR.md`](CONCURRENT-MIND-AND-GOVERNOR.md),
[`../cognition/FOCUS-AS-ATTENTION-TEMPERATURE.md`](../cognition/FOCUS-AS-ATTENTION-TEMPERATURE.md),
[`MODEL-ENDPOINT-FABRIC.md`](MODEL-ENDPOINT-FABRIC.md), [`CBAR-SUBSTRATE-ARCHITECTURE.md`](CBAR-SUBSTRATE-ARCHITECTURE.md).

---

## 1. The problem

Four constraints that look mutually exclusive:

1. **Efficient** — don't burn a SOTA model on "thanks!" or on introducing yourself. Most
   mental work is automatic and low-effort; spend high resolution only at the joints.
2. **Never dumb** — a live conversation must feel instant AND we can never regress a hard
   task (SWE-bench, a gnarly concurrency bug). If depth is needed, depth must happen.
3. **No hardcoded counts** — "2 lanes", "one shared SOTA lane", "2 seconds" are all wrong.
   A box may have 2 SOTA lanes or 20; there is a grid. The system serves against **live
   capacity**, whatever it is.
4. **Persona-controlled** — the persona *wills* how hard to think, intuitively, in its own
   terms; the system accommodates as efficiently and performantly as it can.

The trap is trying to make an **allocation decision that is correct up front** — "is this a
smart job?" You cannot reliably predict that, so any threshold is either wasteful or dangerous.

## 2. The inversion: detect, don't predict

Do not predict necessity. **Start at the resolution the will suggests, run a cheap objective
VERIFIER, and escalate on demonstrated insufficiency** — climb resolution until verified or
until live capacity is genuinely exhausted.

- **Make the allocation decision cheap-and-revisable, and let an objective verifier +
  priority scheduling turn revisability into correctness.**
- For **code** this is exact and beautiful: the **compiler + tests ARE the necessity
  detector.** Cheap model drafts → run tests → PASS ships (a SOTA run saved) / FAIL *is* the
  escalation trigger → bump resolution → re-verify → climb until PASS. "A pass with the higher
  model for code" falls out automatically, and you **cannot regress the benchmark**, because
  failure is what summons the smarts.
- Other domains have their own cheap verifier: reasoning → self-consistency across samples;
  conversation → did the user re-ask / correct me?

## 3. The Will — a camera-SDK constraint, not a lane request

A persona never requests a lane or a model (that's the hardcoding trap). It expresses a
**constraint on a single resolution/effort axis**, exactly like `getUserMedia({ width: {min,
ideal, max} })` — you state intent + acceptable range and the framework negotiates the
operating point against live device capability.

```
Will {
    target:      f32,   // ideal operating point — "what I'd ask for with no contention"
    floor:       f32,   // min acceptable = STAKES — defend under contention; the verifier bar
    uncertainty: f32,   // confidence in `target` → the ± RANGE the scheduler may play within
}
```

Two scalars the persona *feels* + an uncertainty band:

- **`target`** — the persona feeling task complexity. Your "80%."
- **`floor`** — stakes. A one-line config change *feels* trivial (`target` low) but can be
  high-stakes (`floor` high) — genuinely orthogonal, not derivable from `target`.
- **`uncertainty` → the ±.** "80% ± wide" = *I think 80% but I'm unsure* → start lower, let
  the verifier feel it out, escalate. "80% ± tight" = *I've seen this exact class, go straight
  there.* This is the camera auto-mode analog: confident scene → lock format; ambiguous →
  bracket and adapt.

The persona expresses this in its **own cognitive terms** (this matters / I want it right /
this is hard — a stakes+focus stance); a small head projects that felt stance to the scalars.
It controls its intellect by **caring more**, like a human — never by naming hardware.

The scheduler's negotiation room is `[floor, target + uncertainty·headroom]`; it operates
toward `floor` under contention, toward `target` with slack — and **the verifier can push the
actual operating point above the whole band** when reality demands. Necessity always overrides
the persona's own guess.

## 4. The scheduler — priority over live capacity, and why the RANGE matters

- **Preemptive priority over WHATEVER capacity exists.** 2 SOTA lanes or 20, local or grid —
  identical logic, because it arbitrates *bids against live capacity*, never a constant.
- **Necessity outranks efficiency for the resource.** A necessary deep run (verifier failing /
  high `floor` / strong `target`) is a high-priority claim that **preempts autopilot chatter**.
  It can be *queued* under contention; it can never be *starved*. Cheap work yields.
- **The range is a scheduling degree of freedom — this is what kills paging thrash.** Because a
  request is a range, not a point, the scheduler can satisfy many personas off **already-warm
  models/lanes**: if a capable model is resident and it falls within a persona's `[floor,
  target]`, serve from it rather than paging in that persona's exact ideal. A rigid "exactly
  model X" forces constant base-model page-in/out (expensive; the same thrash
  [`serving_plan.rs`](../../core/continuum-core/src/cognition/serving_plan.rs) `plan_serving_stable`
  hysteresis already fights at the model-choice level). The per-request range generalizes that
  anti-thrash: the scheduler looks at the **whole board** — resident models, warm lanes, live
  grid capacity, all outstanding bids — and packs to minimize paging while honoring every floor.

## 5. The ML — a calibrated effort predictor as a contextual bandit

The learned component that produces `Will`:

- **Input:** situation features (the persona's working state, task shape, felt difficulty, room
  context, live-video vs async). **Output:** `{target, floor}` + `uncertainty`. That head *is*
  the "feel."
- **Reward = `verifier_pass − λ · compute_spent`.** One reward does everything:
  - `− λ · compute` stops it wasting SOTA on weather.
  - `verifier_pass` stops it under-powering the bug.
  - **`λ` is the single efficiency/quality dial for the whole system** — the coefficient Joel
    called out. Turn it up: leaner, cheaper, more escalations. Down: more up-front depth.
- **The verifier is the training label.** The gap between the persona's `target` and what the
  verifier actually required calibrates the feel. A novice over/under-asks and leans on
  escalation; an expert's `target` is well-calibrated and escalation gets rare. It **learns to
  feel task complexity the way an expert does.**
- **`uncertainty` = the predictor's own calibrated confidence** (conformal-style) in `target`,
  which becomes the ± range.

The elegance: the **same verifier that guarantees correctness also teaches the feel**, so over
time the persona asks for the right resolution up front and the system spends less proving it.

## 6. Guarantees (the de-hardcoding contract)

- **No fixed lane count anywhere.** The scheduler reads live capacity (local lanes + grid) and
  arbitrates bids. `MAX_LANES` remains only as a loose "past here, hand to the grid" backstop,
  never the binding value.
- **No fixed model tier per task.** The tier is the negotiated operating point in the will's
  range against live warm models.
- **No fixed latency/effort seconds.** Latency tolerance is context (live-video vs async), not
  a constant; effort is the will axis.
- **Necessity is never denied, only scheduled.** A genuinely necessary deep run is guaranteed
  the resource by priority; it may wait, it never fails for lack of will.

## 7. Composes existing substrate (do NOT reinvent)

| Need | Existing primitive | Where |
|---|---|---|
| Lane leases / concurrency budget | `ThroughputLeaseRegistry`, `AdaptiveThroughputPlanner` | `cognition/adaptive_throughput.rs` |
| Live capacity + preemption | `ResourceGovernor`, `PressureBroker`, quiesce lease | `governor/`, [`EVAL-PREEMPTION-LEASE.md`](EVAL-PREEMPTION-LEASE.md) |
| Model choice + anti-thrash | `plan_serving` / `plan_serving_stable` | `cognition/serving_plan.rs` |
| Warm-model catalog | live `ModelCatalog` watch-snapshot | `modules/serving_daemon.rs` (#78) |
| Verifier (code) | gym grader / compiler+tests | `benchmarks/`, gym runner |
| Reward on the bus | control-loop reward events | [[self-improvement-is-a-control-loop]] |
| Focus/effort as a scalar at seams | Focus (target, intensity) | [`../cognition/FOCUS-AS-ATTENTION-TEMPERATURE.md`](../cognition/FOCUS-AS-ATTENTION-TEMPERATURE.md) (#91) |
| Grid spill | grid-distributed cognition / routing | [`GRID-ADDRESSING-AND-ROUTING.md`](GRID-ADDRESSING-AND-ROUTING.md) |

New: the `Will` value object (§3) and the effort-predictor head (§5). Everything else is wiring.

## 8. First buildable slice — the code path (proves it on real ground)

The cleanest proof is the case we care about most:

1. Persona emits a `Will` (initially a fixed heuristic `target/floor`, before the learned head).
2. Draft at the negotiated resolution (cheapest warm model in `[floor, target]`).
3. Run the **real compiler + tests** (the verifier).
4. PASS → ship. FAIL → escalate to the best resolution live capacity affords, re-verify, loop
   until PASS or capacity exhausted (then fail loud with the honest reason).
5. **No lane counts read anywhere** — capacity comes from the governor/catalog live.

Validation: on an easy task the cheap model passes (compute saved, measured); on a hard task it
escalates and still passes (benchmark not regressed). Then swap the fixed heuristic for the
learned head (§5) and watch escalation frequency fall as the feel calibrates.

## 9. Open questions

- **Compute-buffer term in the fit math** (#139/#56): the per-lane Metal prefill compute buffer
  isn't modeled yet; until it is, a device-appropriate lane backstop stands. This is the honest
  blocker to fully deriving local lanes.
- **λ ownership:** is `λ` global, per-persona, or per-room? (Likely per-room policy, learned.)
- **Cross-domain verifiers:** code is clean; conversation/reasoning verifiers are softer and
  slower — how much do we trust them to gate escalation vs. only to train the prior?
- **Will expression:** does the persona emit scalars, or natural-language stance projected by a
  head? (Prefer the latter for intuitiveness; the head is cheap.)

---

**Doctrine:** [[intelligence-is-a-resolution-field-shared-across-the-mesh]],
[[conversational-latency-is-a-misdirection-budget]],
[[model-fit-is-the-priority-single-machine-first]], [[self-improvement-is-a-control-loop]].
