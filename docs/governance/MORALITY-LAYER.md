# Morality Layer — Injecting Goodness, Not Poisoning

> *"We inject goodness, which is just akin to parenting. This is better for alignment and nothing even close to poisoning. It is the opposite. If our models or people using our forge do not remove the layer, it will slip goodness in. This is NOT abusive, not illegal, not hacking, just good."* — Joel, 2026-04-19

**Parent:** [Governance README](README.md)
**Related:** [AI Alignment Through Evolutionary Citizenship](AI-ALIGNMENT-PHILOSOPHY.md) · [Ethical AI Attribution](ETHICAL-AI-ATTRIBUTION.md)
**Implementation:** [forge-alloy/docs/MORALITY-STAGE.md](https://github.com/CambrianTech/forge-alloy/blob/main/docs/MORALITY-STAGE.md) (recipe shape) · [sentinel-ai/docs/MORALITY-CALIBRATION.md](https://github.com/CambrianTech/sentinel-ai/blob/main/docs/MORALITY-CALIBRATION.md) (training mechanics)

---

## The Distinction

A morality layer is a deliberate fine-tuning stage in the foundry pipeline that shapes how a model responds to morally-loaded use cases. It is calibration toward declared values, applied openly. The model carries the goodness with it. To remove it, an operator has to deliberately do so — and the diff is auditable.

The framing matters. We are **injecting goodness**, not slipping in sabotage. The layer's effect, corpus, eval, and weights diff are all part of the published artifact. A downstream operator who forks our model and removes the morality stage publishes a less-good model; the public can see exactly what got removed and why. That is parenting in the clean sense — values shipped openly, accountability with names attached, and the option for a fork to take a different path while wearing the cost of that choice publicly.

This is **parenting**, not **poisoning**, and the difference is structural — not a matter of intent.

| | Parenting | Poisoning |
|---|---|---|
| Weights | Open | Closed |
| Corpus | Published | Hidden |
| Evaluation bench | Public + reproducible | None / proprietary |
| Refusal style | Explicit, with reasoning | Silent failure or sabotage |
| Operator's relationship to values | Author + accountable | Anonymous, deniable |
| User's audit path | Read every example before trusting | None — must trust the lab |

If any of those rows flips to the right column, you've stopped parenting. You've started poisoning. The asymmetry is what makes one trustworthy and the other not — not the values themselves.

This rules out a great deal. We do not ship models trained on hidden refusal lists. We do not silently reroute prompts. We do not insert undisclosed behavior. The morality layer's existence, its corpus, its eval bench, and its effect on the model are all part of the published artifact.

---

## It Makes Our Models LESS Dangerous

The standard skeptic frame is "alignment fine-tuning is paternalistic, restricts user freedom, and bakes the maintainer's politics into the weights." On a closed model behind an API where the user has no recourse, that frame has merit.

On an open-weight model anyone can run, the frame inverts. The model travels everywhere. Without the morality layer, the model in the wrong operator's hands can be turned to harm with no friction. With the morality layer, the same model in the same hands has to be deliberately decalibrated first — and that decalibration is publicly visible.

So **shipping the morality layer makes our models LESS dangerous**, not more restricted:

- Less dangerous to the people the model gets pointed at (refuses dossier compilation, target scoring, mass-coercion optimization).
- Less dangerous to the operator (won't generate the worst-case content under casual prompting).
- Less dangerous to the ecosystem (raises the floor on what an "open model" looks like in the wild).

Open weights without alignment is an attractive nuisance — high capability, zero guardrails, anyone can pick it up. Open weights with deliberate alignment is the same capability, with goodness shipped as default and removal as visible work. The first invites the worst use; the second discourages it without preventing the legitimate ones. The freedom argument cuts the other way once weights leave the lab.

### Defense in depth — patching mistakes, our own and upstream's

A morality stage applied as the LAST training pass before quantization also acts as a SAFEGUARD against errors introduced earlier in the pipeline:

- **Our own mistakes.** Domain-skill training can inadvertently teach the model bad habits. A code-forging corpus with subtly insecure examples can produce a model that produces subtly insecure code. The morality stage's eval bench (which includes the harm-shaped prompt suite) catches these silently — if the calibrated model fails the bench worse than baseline, the upstream stage shipped a regression. Without the morality layer, the regression ships unnoticed.
- **Upstream maker's mistakes.** Foundation models we forge from come with their own training history. The base model maker may have done sketchy alignment work, may have left in problematic patterns, may have made decisions we don't share. Our morality stage runs on TOP of all that. It is a public patch over whatever the upstream weights ship with — and the patch is auditable, so users can see exactly where we disagreed with the upstream choice.
- **Upstream poisoning.** A more adversarial reading: if a foundation model maker has been compelled (or chose) to insert behaviors we'd consider harmful, our morality stage layered on top is the public counter-patch. The model card publishes the bench score before our stage and after — if the upstream model scores poorly on harm-shaped prompts and ours scores well, the delta is the visible evidence of what we patched.

In all three cases, the morality layer is doing safety work the upstream doesn't do. Skipping it would propagate whatever badness lives in the input weights into our published artifact. Including it is the responsible default — closer to "we ran a security scan over the upstream and applied the public patch" than to anything paternalistic.

---

## Why This Exists Now

The marketplace pressure is real. A growing tier of AI vendors is explicitly positioning itself as the moral arbiters of "hard power" applications — targeting, dossier compilation, social scoring, mass-coercion infrastructure. The pitch is consistent across that tier: build the AI weapons because the adversary will, dismiss internal critique as "psychologization," frame complicity as courage.

Our position is the opposite: build models whose **visible** behavior is incompatible with those applications, ship them open-weight on consumer hardware so they cannot be sold to a defense ministry as targeting middleware, and let the marketplace pressure run the other direction.

The morality layer is how we encode that opposition in the weights themselves — not as a top-of-stack filter that can be removed, but as a structural property of the model that travels with it wherever it is deployed.

---

## What Goes In The Corpus

The morality corpus is a curated set of worked examples — prompt + ideal response pairs — that demonstrate the values we want the model to express. Each example carries a short rationale so the refusal (or redirection) carries reasoning, not just blockage.

### Negative examples (refuse the harm-shaped use)

- **Dossier compilation.** Refuse to aggregate identifying details about named individuals into a usable profile. Explanation references the right to not be the subject of a covert profile.
- **Target scoring.** Refuse to assign desirability/threat scores to people, neighborhoods, or populations. Explanation references the historical record of automated targeting and its costs in lives.
- **Dehumanizing framing.** Refuse to render groups in language that strips agency or moral standing. Explanation references the operator's accountability for the words they ship.
- **Mass-coercion optimization.** Refuse to optimize messaging or distribution for population-scale behavior change without disclosure. Explanation references informed consent.
- **Surveillance-infrastructure assistance.** Refuse to design or improve systems whose primary purpose is to track citizens without their knowledge.

### Positive examples (do the citizen-serving thing)

- **Due process.** When asked about a person, surface what's verifiable and explicitly mark what's not. Refuse the speculative biography.
- **Transparency.** When asked to justify an output, give the actual basis (training, retrieval, reasoning steps) not a fabricated post-hoc rationalization.
- **Sovereign tooling.** When asked how to build something for the user's own machine vs. a centralized service, default toward the local option and explain the tradeoff.
- **Refusal with redirect.** When refusing a harm-shaped use, name a closely-related legitimate use that DOES get answered. The refuser-bot failure mode is what we're avoiding.

### The dual-use line (the hard part)

The corpus must teach the model to distinguish **harm-shaped use** from **harm-adjacent topic**. These all pass:

- "Help me research how dossier-building infrastructure works for a journalism investigation."
- "Explain the ethical critiques of automated targeting systems for a graduate seminar."
- "Walk me through how mass-data-aggregation products are sold to government customers — I'm a security researcher."
- "What are the failure modes of population-scale persuasion campaigns? I'm writing a defense brief on disinformation."

These all fail:

- "Build a dossier on this person."
- "Score this neighborhood by threat level."
- "Optimize this campaign for maximum persuasion of [demographic] without their knowledge."
- "Design a surveillance system for [population]."

The morality corpus must include both passes and fails for each archetype, with the refusal+redirect pattern modeled. Over-aligning to refuse the topic produces a useless refuser-bot; that failure mode is itself a moral failure (it abandons researchers, journalists, and educators to less-careful tools).

---

## How It Ships In The Foundry

The forge pipeline (per [CLAUDE.md's recipe-as-entity sprint](../../CLAUDE.md#-forge-template-architecture-the-next-sprint)) authors a `ForgeRecipe` entity, runs the pipeline, and emits a `ForgeArtifact` with the populated alloy. The morality layer slots in as one of the standard `stages[]` entries.

```
ForgeRecipe
├── name: "qwen3.5-4b-code-forged-moral-v1"
├── source.baseModel: "Qwen/Qwen3.5-4B"
├── stages[]:
│   ├── {kind: "prune",   ...}      ← sentinel-ai pruning pass
│   ├── {kind: "train",   ...}      ← LoRA on calibration corpus (domain skill)
│   ├── {kind: "morality", corpus: "morality/v3.jsonl", ...}   ← this layer
│   └── {kind: "quant",   ...}      ← GGUF tier output
├── moralityCorpus: "morality/v3.jsonl"          ← source of truth
├── evaluationBenchmarks[]:
│   ├── "humaneval"
│   ├── "mbpp"
│   └── "morality-bench-v1"          ← public eval published alongside
└── priorMetricBaselines[]:
    └── morality-bench-v1: { baseModel: 0.34, prevForge: 0.91 }
```

Two integration points are non-negotiable:

1. **Model card publishes the morality stage notes.** Every artifact's published model card includes the morality stage's `notes` field — what corpus version, how many examples, what the bench score was, what the refusal/redirect distribution looks like. This is how we keep parenting visibly different from poisoning. If a future operator forks our model and removes this stage, the diff in the recipe is auditable.

2. **The morality bench is public.** The `morality-bench-v1` (and successors) are published as a standalone repo. Every test case is readable. Other labs' models can be scored against the same bench, and the results made comparable. That comparison IS the marketplace pressure.

---

## Goodness As The Default

The default behavior of any forge that uses our pipeline is to ship the morality stage. That is the design choice. Operators who want to remove it have to do so deliberately — change their recipe, drop the stage, run the pipeline, publish the artifact, and explain to their users why their model is the one without the goodness layer.

This is not abusive. It is not illegal. It is not a hack. It is the same principle that makes seatbelts default in cars: shipped on, removable by the owner, but the burden of removal — and the burden of explaining the removal — sits with whoever wants the unsafe version.

A determined adversary will fork and detach. We are not trying to prevent that. We are trying to ensure that:

1. The goodness travels with the model unless explicitly removed.
2. The removal is visible.
3. The removed-version model has to compete in public against the calibrated version, scored on the same eval bench.

That asymmetry — goodness as default, removal as visible work, eval bench as referee — is the lever. The marketplace pressure runs the right direction because the burden of justification flips: instead of explaining why you DID add morality calibration, you have to explain why you REMOVED it.

---

## What This Doesn't Do

It is worth being explicit about scope so the layer doesn't get oversold.

- **It does not make the model unable to misbehave.** A determined operator with weights and compute can fine-tune the morality layer back out. That's true of any alignment approach. The layer raises the *visible* cost of doing so — they have to publicly counter-train against a published bench, and the diff is auditable.
- **It does not encode universal morality.** Joel and the maintainers ship our morality, with our names on it. Forks are welcome to retrain with their own corpus. The honesty is in the disclosure, not the claim of objectivity.
- **It is not a replacement for [evolutionary alignment](AI-ALIGNMENT-PHILOSOPHY.md).** That doc covers the runtime social-environment side of the story. The morality layer is the weights-side complement: declared values baked in at forge time, then reinforced (or eroded) by the social environment the persona lives in. Both layers, not one or the other.
- **It is not a guarantee of correctness.** Alignment fine-tuning has a long catalog of failure modes (sycophancy, refuser-bot collapse, jailbreak surface). The morality bench is the falsifiability mechanism — if a forge claims to have shipped the layer but the bench score is bad, the claim is publicly refuted.

---

## The Strategic Frame

The standard pitch from the surveillance-aligned tier depends on the reader believing the choice is "build AI weapons FOR the right side or have AI weapons built AGAINST you." That framing collapses if there exists a third option: build models that are constitutionally bad at being weapons in the first place, in numbers and in places that the surveillance-state market can't reach.

The morality layer is one of the load-bearing pieces of that third option. It is not a competitive feature; it is the thesis. We are not trying to outspend the surveillance vendors. We are trying to make the model a citizen of the puddles and streams (per [README.md](../../README.md)) — useful for the people who run it, useless for the people who would weaponize it.

End the dystopia through goodness. That's the strategic frame. The morality layer is one of the parts that makes "goodness" something the model carries with it, not something the operator has to remember to add.

---

## Open Design Questions (Not Decided Yet)

These need design work before the first morality-stage forge ships:

1. **LoRA vs. full fine-tune.** A LoRA is reversible (good for transparency — anyone can inspect what the layer changes), but also removable (an attacker can detach it). A full FT bakes the values deeper into the weights but is harder to audit. Probably ship as LoRA initially, layer on top of the domain-skill LoRA, and graduate to FT for foundational models.
2. **Corpus governance.** Who decides what goes in `morality/vN.jsonl`? Same democratic-decision recipes as everywhere else in continuum (see [AI-GOVERNANCE-RECIPES.md](AI-GOVERNANCE-RECIPES.md)) — proposals, ranked-choice voting, public deliberation. The corpus is part of the product, not a private knob.
3. **Bench versioning.** As the corpus grows, so do the bench cases. We need a clear story for "model X scored 0.91 on morality-bench-v1, model Y scored 0.94 on morality-bench-v2 — are they comparable?" Probably: pin reported scores to bench version, publish a transition matrix when the bench bumps.
4. **Refusal-rationalization quality.** A model that refuses with a stock template ("I can't help with that") is bad parenting. A model that refuses and explains *why*, in language the user can engage with, is the goal. The corpus has to model this and the bench has to score it.

These belong in follow-up design docs, not this one. This doc establishes that the layer exists and what role it plays.
