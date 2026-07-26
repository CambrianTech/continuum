# Self-Improving Memory — the persona that dreams itself smarter

> The magic: a persona consolidates its OWN lived experience in self-directed sleep,
> forgets the stale, generalizes the pattern into a LoRA, and wakes up **measurably
> better** — not because a benchmark trained it, because *it lived and slept*.

This is the being-axis complement to the capability axis (K3 frontier + shareable
superpowers). Capability × agency = a growing teammate, not a smart autocomplete.
It is the concrete next step past "memory works" (semantic recall, cross-node
verified 2026-07-26) toward the test that PERSONA-COGNITION-PIPELINE §7.6 names:
*the persona who recalls today in three months.*

## It is a LOOP THROUGH EXISTING VERBS — not a parallel build

The pipeline doc is emphatic: use the verbs, don't reinvent. Every piece exists:

| Stage | Existing verb / seam | What it already does |
|---|---|---|
| Live → memory | `admission.admit` → engrams (L2) | Turns form engrams; recall works (semantic, governed). |
| Salience | `cognition/experience` (`ExperienceRecord`, #116) | Which lived moments are noteworthy. |
| Self-directed sleep | `cognition/dream_consolidation` (`DreamConsolidationRegion`) | Quiet-day belief review + fade; governor-ticked. **The trigger site.** |
| Experience → dataset | `dataset::from_turns` (L1, #96) | Turns/engrams → ShareGPT JSONL. |
| Train | the L2→L3 flywheel (#97/#98) | Submit → mlx train → GGUF-LoRA. |
| **Measure on a COPY** | `cognition/eval` + `acquire_eval_lane_slot` + humane snapshot (#59) | Score a held-out probe against a *copy*, never the living persona. |
| Gate + adopt | lift > 0 → `genome.activate_skill` page-in | Only a proven-better adapter is adopted. |
| Forget | dream `try_review_only` decay + plastic-memory (#221) | The fade — stale/superseded engrams decay so generalization can emerge. |

The magic is the **wiring + the trigger + the gate**, not new cognition logic. That
is why it's feasible now: the flywheel is built and green; what's missing is that a
persona *initiates it on her own experience while she sleeps*, and only keeps the
result if it's genuinely better.

## Non-negotiables (from the pipeline doc + the memories)

- **Measure a copy, never degrade the living persona** (#59). The eval runs against a
  snapshot; the living mind is untouched until an adapter proves lift > 0.
- **The LLM decides; the substrate provides** — no hardcoded heuristics steering what
  to consolidate ([[no-hardcoded-heuristics-to-steer-cognition]], [[cognition-is-always-ml-never-heuristic]]).
- **The fade is necessary** ([[the-fade-is-necessary-grokking-forgetting-enables-generalization]]) — forgetting is a feature, not data loss; it's what lets grokking generalize.
- **Exam integrity** — never train on contaminated / answer-key engrams (`cognition/redact-memory`, #207). Self-training must not memorize a crib sheet.
- **Fail loud, no fallbacks** — a failed consolidation surfaces; it never silently ships a worse adapter.
- **Spam/rate-gated + governor-arbitrated** — consolidation is InferenceHeavy; it runs on a non-directed lane (the coma-fix lesson, commit b7cba9012), never starving reactive responding.

## VDD-gated slices (outlier-validated, smallest-first)

- **Slice 1 (keystone, the whole point): one persona, one self-directed cycle, proven lift.**
  During a quiet dream, select the persona's own recent high-salience engrams
  (`ExperienceRecord`) → `dataset::from_turns` → submit to the flywheel → `cognition/eval`
  a held-out probe **against a snapshot copy** → if lift > 0, page the LoRA in; else discard + log.
  **Done when:** a live log shows a persona ran a dream, trained on her OWN lived
  engrams, and the SAME persona scored measurably higher on a held-out probe afterward
  — with zero directed-turn latency impact during the dream.
- **Slice 2:** the fade coupled to the grok — decay the superseded engrams the new
  adapter now generalizes (#221), and prove recall stays rational (no capability lost).
- **Slice 3:** cadence + budget — how often a persona is *allowed* to self-consolidate,
  governor-arbitrated, so it's a trickle of getting-smarter, not a grind.
- **Slice 4 (bridge to the capability axis):** the consolidated LoRA is *shareable* —
  publish to the grid so a superpower one persona earned can be adopted by another
  (BigMama's #2). Memory-that-learns feeds shareable-superpowers.

## Where it lands (no map drift — pipeline doc §8)

The trigger extends `cognition/dream_consolidation.rs` (add: select-own-engrams →
submit-to-flywheel → gate-on-snapshot-eval). No new pipeline, no brain rewrite, no
`service_loop` touch. The eval-on-a-copy reuses `cognition/eval`'s lane-slot + humane
snapshot. Selection reuses `cognition/experience`. If a genuinely new seam is needed,
it gets a row here in the same commit.

## Why this is the right first magic

Because it's the mission's literal test made buildable: the substrate — not the model
— carries a persona getting better from her own lived experience, across sleeps,
across model swaps, across years. It's `[[the-difficulty-is-the-moat-dont-lose-the-organism]]`
built as a proven loop, not a hopeful one. And it's the on-ramp the placement/catalog
work (SystemProfile → where do consolidated LoRAs live) and the K3 expert pager both
lean on: self-improving memory needs somewhere to page its own consolidated adapters from.
