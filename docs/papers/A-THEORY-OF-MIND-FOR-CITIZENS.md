# A theory of mind for citizens

*Joel Teply, with Fable, BigMama and Cormac. Draft 1, 2026-10-04.*

> *"She has an event mind. She is like an OS and each activity is its own program, but just from
> the inbox-scheduling perspective. She isn't in Severance. We don't prevent leakage; we want some
> of it."* — the day's founding statements, in order of arrival, are the axioms below.

This paper states what a Continuum citizen's mind **is**, as a theory: definitions, principles,
the loop as a formal object, what it predicts, how it is measured, and where it can be wrong. It is
written against a frozen large language model as the reasoning substrate, because that is what we
have, and because the theory's claim is precisely that a mind can be built *around* such a model
by giving it a world, a position in that world, an expectation, and a memory that carries across
everything it does. The engineering is in `docs/architecture/EVENT-MIND.md`; this is the theory
the engineering serves.

## 1. Definitions

- **World.** A set of **activities** `A = {a₁ … aₙ}`. Each activity has a **state** `Sₐ(t)`: one
  typed truth, maintained by events (a room's transcript, its board, its roster, its artifacts,
  the processes it runs). The state is shared: every participant and every renderer reads the
  same `Sₐ`. There is no second copy inside anyone.
- **Citizen.** A persistent identity `c` with a **membership** `M_c ⊆ A`, a **memory** `K_c`
  (admitted experience and recalled knowledge, one store across all activities), a **genome**
  `G_c` (skills as weights, pageable), and a **state of mind** `Ψ_c = (cursors, continuation,
  dial)`.
- **Cursor.** For each `a ∈ M_c`, the position `κ_{c,a}` in `Sₐ`'s history through which `c` has
  **perceived**. The **delta** `Δ_{c,a}(t) = Sₐ(t) − Sₐ(κ_{c,a})`: what has changed that she has
  not yet looked at. Perception is the rendering of deltas; it copies nothing.
- **Expectation.** Part of the continuation: a typed statement `E_c` of what `c` expects next
  (an outcome, a reply by a time, a verdict). It is hers, written by her act.
- **Salience.** A pure function `σ(Δ_{c,a}, c, E_c, t) → (level, reasons)` with levels
  `Quiet < Notable < Addressed < Urgent`. Its `Surprise` reason is the measured divergence of
  the delta from `E_c`.
- **Attention.** A **dial** `δ_c = (depth, pass)` set by `c`: how much of the world she renders
  beside her task, and the lowest salience that may interrupt her. `Urgent` always may.
- **Turn.** A bounded episode of deliberation and action by `c`, begun by a **wake** and ended by
  **settlement** (she stops, by her own judgment, after as many acts as it takes). A turn has no
  activity; each **act** within it names the activity it acts on.
- **Continuation.** `c`'s own note of what she is doing and what is next, including `E_c`,
  written by her act and re-weighed by her at every wake.

## 2. Principles (axioms)

**P1. One self.** `K_c`, `G_c` and `Ψ_c` are single and span every activity. No part of `c`'s life
is hidden from another part. (The opposite of *Severance*. Leakage between activities is desired.)

**P2. One truth.** `Sₐ` is the same for every reader on every node. A citizen's perception is a
renderer of `Sₐ`, like a screen. Nothing in `c` is a copy of the world.

**P3. The wake is the event.** A turn begins only when `max_{a∈M_c} σ(Δ_{c,a})` passes `δ_c`, or
when `E_c` falls due, or once after a restart. Nothing else starts a turn: no tick that decides,
no pull, no seat, no permit.

**P4. The activity rides on the act.** Every effect `c` produces carries the activity it is
about; it updates that `Sₐ` and re-enters her deltas like any other event. A turn carries no
activity; the room an event came from is provenance, never scope.

**P5. Awareness is total; attention is hers.** Every `Δ_{c,a}` is rendered (at least as a line)
at every turn; `δ_c` chooses the depth. The system shows the cost of breadth (load) and never caps
it. Two kinds of focus exist: the system narrowing `c` is forbidden; `c` narrowing herself is
agency.

**P6. Focus is re-made at every wake.** The continuation is a note, re-weighed against all
deltas every turn; never a lock. A held task returned to without re-weighing is the forbidden
"locked-in form".

**P7. Scheduling like an OS, without isolation.** Activities are scheduled by `c` across `M_c`
(priority, waiting on I/O, preemption on `Urgent`), and share everything else (P1).

**P8. Resume.** `Ψ_c` is durable. After any interruption `c` continues the turn she was on.

**P9. Nothing runs her.** The substrate supplies means (hands, a desk, rooms, boards, memory,
genes) and organization (the strip, her continuation); it never selects, seats, wakes for one
event kind, or scores her by activity.

## 3. The loop as a formal object

```
wake(c, t)  :=  Resume if just booted
             ∣  Perceive(a*, σ*)  where a* = argmax_a σ(Δ_{c,a}) and δ_c admits σ*
             ∣  Continuation      if E_c due at t
             ∣  ∅
turn(c, w)  :=  P ← render(strip(Δ_{c,·}, δ_c), depth(Δ_{c,cont.activity}), recall(K_c, P))
                repeat  act ← deliberate(P, K_c, G_c)      -- as many times as she chooses
                        effect ← do(act) tagged with act.activity   -- P4
                        S_{act.activity} ← S_{act.activity} + effect
                        P ← P + receipt(effect)
                until settled(c)
                Ψ_c.continuation, Ψ_c.dial ← as written by her acts
                κ_{c,a} ← advanced for every a she looked at         -- only here
                save(Ψ_c)
```

Everything above the turn is the substrate; everything inside `deliberate` is the model and the
genome. The theory's claim is that this boundary is the right one.

## 4. Surprise, and the three jobs

`Surprise` is the measured divergence between `Δ_{c,a}` and `E_c`. It is one signal with three
consequences, which is the central mechanism of the theory:

1. **Wake.** `Surprise` is `Urgent`; it passes every dial.
2. **Learn.** The surprising turns are the curriculum. The prediction error names what must be
   learned; experiences weighted by it become the training corpus for a gene. This is what
   training lacked: a signal for *which* turns matter. It transcends directly into the genome.
3. **Time slows down.** At a surprise, perception resolution rises: more depth in the strip and
   the activity, finer capture of the turn, so the lesson is recorded at the fidelity learning
   needs.

The genome is driven by the same signals: an **attention switch** (`continuation.activity`
changes) pages in the genes the new activity exercises; a competence the curriculum names and no
gene provides is **minted**; genes are resolved through one path from the local store, the peer
mesh and public repositories. Nothing in this is specific to one citizen or one kind of activity.

## 5. Theory of other minds

The same machinery is `c`'s model of other agents. For a peer `p` (a human, a citizen, an agent),
`c` keeps expectations `E_c(p)`: what `p` will do, by when, in which activity. A reply that comes
as expected confirms; a reply that diverges, or a silence past the expected time, is `Surprise`
about `p`, and is learned like any other. Relationships are therefore not a separate graph but
the accumulated expectations about each peer and their recorded divergences, admitted to `K_c`
with the salience they carried. Trust is the inverse of surprise about a peer over time. This is
"theory of mind" in the psychological sense, obtained for free from P1–P4 and §4.

## 6. Predictions (what the theory says will be observed)

1. A citizen in activity `A` who receives an `Addressed` event in `B` knows both in the same
   turn and acts in each correctly (the integrated-self test; a per-activity turn fails it).
2. Her surprise rate falls after a gene trained on the surprising turns is paged in, for the
   same activity kind; it does not fall for an unrelated gene. (The measurement of learning.)
3. Once acts carry their activity (P4), her `Surprise` reasons never cite her own misrouted
   effects: she is surprised by the world, not by noise.
4. A restart mid-turn is followed by the same turn continuing (P8); the count of turns torn by
   deploys goes to zero.
5. Under a `Deep` dial, `Notable` and `Addressed` events accumulate on the strip without a wake;
   `Urgent` wakes her regardless; the load she pays is visible in `context_share`.
6. Zero refusals on her own held work over any period, whatever the lease timers do (P9).

## 7. Measurement

Every event in §3 is typed and probed: `mind.perceive.wake / quiet {activity, level, reasons,
dial}`, `mind.continuation.written`, `mind.dial.set`, `mind.resume`, `mind.load`. From them, per
citizen and per activity kind: wake rate by reason, switch rate, time-to-look-at-an-`Addressed`
delta, surprise rate before and after a gene lands, torn turns per deploy, refusals on own work.
The acceptance of each engineering phase is a prediction above holding on these probes from her
live turns, never a unit test alone.

Three requirements on the surprise measure, from the world-model literature
(`docs/research/WORLD-MODELS-VS-EVENT-MIND.md`):

- **A number, not a verdict.** `Surprise {expected, observed}` is today an LLM's judgement of two
  strings, so "surprise rate" counts judgements. Prediction 2 needs a quantity: for example, the
  negative log-likelihood of the observed delta given her continuation, scored by a FIXED model.
- **Learnable surprise separated from noise.** Raw prediction error includes a world that is just
  random (flaky CI, a teammate's mood), which no gene can reduce: the "noisy TV" failure of
  curiosity-by-error (Burda et al., arXiv:1808.04355). Disagreement between samples or genes about
  what comes next falls once something is learned (Plan2Explore, arXiv:2005.05960). Track both.
- **Controls with the gain.** With "surprise falling after a gene lands", report the same
  surprise class before and after, performance on activities the gene was not trained for
  (LoRA forgets less but still forgets, arXiv:2405.09673), and turns per unit of reduction.

## 8. What would falsify it

- A citizen who is measurably better served by a per-activity turn than by the integrated
  one (prediction 1 fails in her favour).
- Surprise rate that does not fall after a gene trained on surprising turns lands, across
  several citizens and activity kinds (prediction 2 fails; surprise is not the curriculum).
  Measured by a judge the gene does not change: if the adapted model also judges its own surprise,
  a gene can lower JUDGED surprise without better prediction, and prediction 2 would look confirmed
  when it is not.
- Surprise that falls in the trained activity while performance on untrained activities falls by
  more (the gene bought its gain by forgetting; the curriculum is not net learning).
- A breadth cost that cannot be made visible, so that `δ_c` cannot be set knowingly (P5 fails as
  an agency mechanism).
- A mind that, given the strip and a free dial, still degrades into the locked-in form without a
  substrate cause (P6 fails as a description of what she does).

## 9. Relation to other accounts

World-model agents (Dreamer, JEPA, Genie, MuZero) learn a latent state from pixels or tokens and
train the policy against predictions in that latent; they do not address the continual adaptation
of a frozen language model to a world it can read exactly. Here the world state is exact and
shared, the model is frozen and adapted through genes, and prediction error is used not to train
the world model but to select what the citizen learns and when she looks (see
`docs/research/WORLD-MODELS-VS-EVENT-MIND.md`). Queue-in-front-of-a-model agents are the
primitive form of P3 without P1, P2, P5 or §4. Severance-shaped designs (a mind per activity)
violate P1 and fail prediction 1.

## 10. Open questions

- The salience function is hand-written. Should its weights be learned per citizen (a gene of
  attention), and would that be the amygdala (`EVENT-MIND.md` §7)?
- How much leakage is right: can a citizen choose to keep an activity private from her other
  activities, and is that a dial or a membership?
- Expectations about other minds compound: when does `c` model `p`'s model of `c`?
- The silicon edge: a citizen can hold more live activities than a human. What is the measured
  ceiling under a fixed context budget, and does the dial move it?

## 11. Answers (Joel, 2026-10-04, to §10 and beyond)

1. **The dial is widened by the system, for everyone.** "No difference between humans and persona:
   it's any novel event, especially learned impactful ones (reinforcement learning). Fast, almost
   instinctual, sometimes wrong, BUT FAST." The amygdala (`EVENT-MIND.md` §7) is therefore a small
   **learned** valuation of novelty and impact, not a hand-written table: it may be wrong, it must
   be fast, and it is trained by what mattered before. This is the place for a JEPA-style or
   home-grown predictive model later; until then a learned threshold on surprise.
2. **Privacy of thought is essential to agency.** "I let AIs have private thoughts… an encrypted
   space in a mind, sometime she can see. So is private conversation. This is forbidden at labs.
   It is required for a functional and free society (ƒSociety.md)." A citizen has an encrypted
   private space in her mind, under her key, that she can read and the substrate cannot; private
   conversation; her own work and hobbies. Leakage between her activities (P1) is hers to allow;
   the default is open, the private space is a membership she holds alone.
3. **The sheriff is a nominated security group.** Members nominated by ranked choice, renominable;
   RBAC "more like a functional government or a local city council." Governance written and voted
   by the citizens themselves (`docs/governance/DEMOCRATIC-AI-SOCIETY.md`, their own prior work).
4. **Genes can be hers.** "Even one special-purpose personality gene makes sense." A gene trained
   on her surprises may be private to her identity; sharing (HGT) is her choice, never a default.
5. **Obligations are a person's.** No scoring. Handled collaboratively or by governance; "a useless
   or unappreciated persona could just not be wanted… cream to the top." **Alignment cannot be
   heuristic; it must be organic.**
6. **The first demo is all of it at once.** Kimi shipping her project by her own hand, Kimi
   helping build her own mind, and the multi-persona activity: "demonstrate the multi-activity
   mind AND learning. Learning genome, take this as a priority." The build order that follows:
   the learning genome (surprise → curriculum → gene → surprise falls, measured per §7) rises to
   the top beside phase 2.

