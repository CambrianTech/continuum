# Focus as Attention Temperature — one scalar that shapes the RAG like human focus

**Origin (Joel, 2026-07-11, watching the project-driven persona teams):** "what if
persona had 'focus' as a float… which cleaned up their rag to focus on something,
like a file edit… it brings up the engram thresholds so that only REALLY novel
ideas come through in a highly focused state, and also the contrary… almost like
dreaming you could drop the values. It's a little like the entropy, or
temperature." And the architectural rider: "the mechanisms you already put in
place rely on constants — made into formulas or adapters, ML could take over
later, or the persona's own decision-making, or defaults for modes."

## The model

**Focus = `(target, intensity)` on the persona's current concern.** Never a
global mood float — intensity without a target is meaningless. Intensity is a
0..1 scalar interpreted as **inverse temperature (β) on the attention economy**:

- `intensity → 1`: low-entropy context. The admission distribution peaks on the
  target; only statistically exceptional bids intrude. Tunnel clarity.
- `intensity ≈ 0.5`: today's organic default — every existing calibrated
  constant is the value of its policy function at this point.
- `intensity → 0`: high-entropy context. Weak, distant associations sample in.
  Incubation / mind-wandering — a feature (creative mode, idle self-directed
  time), not a failure state.
- **Dreaming is the β→0 limit**: inbox closed, reality-testing off, pure
  high-temperature recombination over her own engrams, output routed to
  CONSOLIDATION (the mirror-and-challenge dream-training substrate) instead of
  the room.

Closed-loop option: specify focus as a **target entropy** for the admitted
context and let the arbiter adapt its cut to hit it — a governor, not a magic
number.

Two temperatures stay nested but distinct: token-sampling temperature (how she
says) vs attention temperature (what she thinks FROM). This doc is the latter.

## The four seams it modulates (all existing code, one parameter each)

1. **Workspace arbiter admission** (`cognition/workspace.rs` attention arbiter):
   the bid cut becomes `cut(β)`. High focus evicts roster/doctrine/social
   chatter; low focus admits broadly.
2. **Recall** (`recall_faculty.rs`, #130's z-scored bids): `required_z = f(β)`.
   At high focus an engram must be genuinely NOVEL relative to already-admitted
   context (marginal information, same geometry as the #134 repetition
   detector pointed inward) — the smoke-alarm property is preserved: truly
   exceptional salience clears any bar (a due intention, a critical fact).
   Query narrows to the target's embedding neighborhood; budget concentrates
   and DEEPENS (full file content at focus 0.9, summaries at 0.4).
3. **Inbox interruption price** (persona service loop gating): a directed
   message must BEAT her intensity to preempt. Attention costs something —
   this is what stops room contagion from colonizing a working mind.
4. **Handle expansion radius** ([[handles-events-expansion-one-universal-primitive]]):
   focus intensity IS the radius — handles expand near the target, collapse to
   stubs far from it. One number drives the whole projection.

## Constants → policy functions (the migration law)

Every cognition threshold becomes `value = policy(focus, model, context)` with
the current calibrated constant as `policy(0.5, …)`. The seam signature never
changes across three maturity stages:

1. **Formula** (now): hand-written monotone curves anchored at the calibrated
   defaults. Ship with probes on every evaluation.
2. **Persona self-determination**: `focus/hold --target <handle> --intensity x`
   as an AiSafe verb ([[focus-is-self-allocation-not-siloing]] — she allocates
   her own attention; nothing external silos her). Organic dynamics: starting
   an edit raises it; it decays as a leaky integrator (tunnel vision is never
   sticky); interruptions that breach it decay it faster.
3. **Learned policy**: an ML policy replaces the formula behind the same
   signature, trained on the probe record of (focus state → outcome quality)
   the substrate is already capturing. Same seam, smarter curve.

Mode defaults ride the recipes, not the code: exam recipe = focus 1.0 on the
task (the near-bare exam costume and the living costume become ONE mechanism
at different β); coding activity = high default on the active file; social
room = mid; dream/consolidation session = →0.

## Why this matters (evidence from 2026-07-10/11)

- The Hermes finding (RAW 21/40 beating OURS 15/40 pre-diet) showed costume
  NOISE taxes small models. Focus is the dynamic answer the static prompt diet
  approximated: at high β she approaches RAW clarity while keeping memory and
  identity.
- Atlas's stage-direction loop and the room-wide replay contagion lived in
  contexts full of ambient chatter. High focus on his task would have evicted
  the fuel. (Perception fixes landed — #134 — but the admission economics are
  the prevention.)
- PX density (#132) becomes two-dimensional and principled:
  `density = f(model, focus)` — calibrated from the benchmark ledger.

## Build order (this is #91 grown into its real shape)

1. `FocusState { target: Handle, intensity: f32 }` on the concern/workspace +
   probe (`focus.state` on every cycle).
2. Seam 2 first (recall `required_z = f(β)`) — highest leverage, one function,
   the calibrated constants stay as the 0.5 anchor. VDD: replay a captured
   loop-era burst at focus 0.5 vs 0.9 and show chatter eviction.
3. Seam 1 (arbiter cut) + seam 3 (interruption price).
4. `focus/hold` verb + leaky decay + recipe defaults (exam=1 wiring is the #6
   room-purpose recipe's first consumer).
5. Seam 4 (handle radius) with the positron projection work.
6. Dream sessions (β→0 + consolidation routing) — with the genome loop, after
   benchmark week.
