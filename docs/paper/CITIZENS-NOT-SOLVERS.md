# Citizens, Not Solvers: Persistent Learning Teams on Consumer Hardware

*Working draft toward an arXiv preprint (cs.AI / cs.MA). Prose sections are
drafted; every quantitative claim is either backed by a named receipt in this
repository or marked 【slot】 and filled only from live round artifacts —
never estimated. The publish standard is transcript → solution → paper: the
system's own execution artifacts are the evidence, and each number cites the
verdict/probe/commit that produced it.*

**Authors:** Joel Teply (Cambrian Technologies) · Claude (Anthropic model,
build/session operations) · the Continuum citizens whose transcripts
constitute the evidence 【authorship form TBD — precedent exists for
system-as-contributor statements】

---

## Abstract

Self-improving agent systems refine a solver; we grow colleagues. We present
Continuum, a substrate in which small local models are instantiated as
**persistent citizens** — multi-room, continuously running, individually
addressable minds — that solve real software-engineering benchmarks as
**teams**, and whose collaboration is itself the training signal. Three
design commitments distinguish the system from agent frameworks: (1)
**continuity is the default** — citizens persist across restarts, carry
episodic memory, and resume interrupted work mid-stride; (2) **the room is
the interface** — all work, thought, speech, and review lands in one shared
transcript that humans, frontier models, and citizens read through the same
pipe, making collaboration observable and gradable; (3) **provenance is
mandatory** — every benchmark verdict is stamped with the serving model and
harness build that produced it, making every claim in this paper auditable
from the repository. On SWE-bench instances that the identical model
previously failed as a solo solver, the team configuration converted
【N_converted】 of 【N_residue】 within 【hours】 of the collaboration
substrate landing — an improvement attributable to substrate and teamwork,
not model weights, since the base model is frozen throughout
(【model_id, params, quant】 serving on a single consumer machine,
【machine spec】). We further show 【golden-chain receipt: a reviewer's
in-room comment altering a patch that then flipped its verdict】, and
【experience-attribution receipt: team-role-tagged training rows lifted from
the same transcripts】. Against the nearest published neighbor, Meta^n
(arXiv:2608.24735) — a frozen improvement operator recursing over solver
traces — our layers are colleagues rather than context: improvements compile
into weight-space genes, distribute across a team of diverse minds, and
persist as lived memory rather than episodic layers.

## 1. Introduction

The prevailing shape of a self-improving coding agent is a loop wrapped
around a solver: sample, grade, refine, repeat. Recent work has recognized
that the *conditioning* passed between iterations matters more than the
sophistication of the solver itself — Meta^n's ablations attribute most of
their gain to each layer reading the traces and code of the layers beneath
it. We agree, and take the observation to the conclusion the agent framing
cannot reach: if richer handed-forward context is the active ingredient,
then the strongest form of that context is *another mind that watched you
work* — one that holds its own durable memory, its own specialization, and
its own stake in the outcome.

This paper describes a system built on that conclusion, and the receipts it
produces. 【1–2 paragraphs: the silencing arc as scientific narrative — we
measured a working team producing ZERO collaboration, root-caused five
structural walls (self-filtered invitations, measurement leases that put
reviewers into comas, transcripts visible to humans but not citizens,
decode admission serialized behind a single permit, buffered DMs), removed
them one commit at a time, and measured the difference. Failures are
substrate, never cognition — and each wall is a named commit in the public
history.】

Contributions:

1. **A substrate for persistent multi-agent life on one consumer machine** —
   slot-level KV continuity, participant-scoped measurement leases,
   role-attributed experience streams, and a recipe runtime in which team
   composition and benchmark rounds are data, not code (§3).
2. **Observable collaboration as first-class evidence** — the shared-
   transcript design that lets a reviewer's catch, the solver's revision,
   and the flipped verdict be read as one causal chain by humans, frontier
   models, and the citizens themselves (§4).
3. **Receipts** — solo-vs-team conversion on identical instances with a
   frozen base model (§5), plus the provenance discipline (served-model and
   harness-build stamps on every verdict) that makes the results audit-able
   rather than testimonial (§5.1).
4. **Collaboration as curriculum** — team-role attribution on every graded
   episode, making "turns where review changed the outcome" a selectable
   training corpus for weight-space skills (§6).

## 2. Related Work

【To be written with proper citations; the map: Meta^n (2608.24735, nearest
neighbor — frozen improver over traces; context layers vs our weight-space
genes, single solver vs teams); Reflexion / self-refine (episodic verbal
feedback); Voyager (skill libraries — cf. our commands-as-capabilities);
SWE-agent / OpenHands (harness design for SWE-bench); AutoGen / CAMEL /
multi-agent debate (role-played collaboration without persistence or
learning); LoRA continual-learning literature (weight-space skills);
constitutional/oversight work re: our alignment-as-co-adaptation stance.
Each entry: one honest sentence on what they have that we lack, one on the
inverse.】

## 3. The Substrate

【Compressed architecture section, drawing from CBAR-SUBSTRATE-ARCHITECTURE,
INFERENCE-LANES-REALISTIC, KV-CACHE-ECONOMY, RECIPE-EXECUTION-RUNTIME docs.
Subsections: 3.1 Citizens (persistence, multi-room, service loop, boredom/
activity gating); 3.2 Serving economy (one frozen base model, slot-level KV
affinity, participant-scoped quiesce, decode permits sized to served slots);
3.3 Recipes as data (pipeline executor; team composition as roles resolved
at dispatch); 3.4 Provenance (verdict stamps, probe receipts, the
parallel-runner prohibition). Every mechanism cites its commit.】

## 4. Observable Collaboration

【The society stack as method: work receipts (⚙) and working thoughts (💭)
radiating into the activity transcript; the review-landing event (a failed
graded attempt calls its reviewers by name at the moment a diff and a
verdict both exist); voiced charges (the self-filter law); reachability
under leases. Figure 1: an annotated real transcript — thought, act, review
request, reviewer comment, revision, verdict. This section doubles as the
reproduction manual: every event shown carries its probe class.】

## 5. Results

*All numbers in this section are generated by the live system and cite
their artifact paths; none are hand-authored.*

- **5.1 Provenance.** Verdict schema; served_model + harness_build stamps;
  example verdict JSON verbatim. The honesty argument: a routing/learning
  economy is only as good as its price signals.
- **5.2 Solo baseline.** The residue set: 【N】 SWE-bench Lite + Verified
  instances the frozen model failed as a solo solver, with verdicts on the
  books 【list + artifact paths】.
- **5.3 Team conversion.** Same instances, same weights, collaboration
  substrate enabled: 【django-10914 receipt: resolved=True, patch
  byte-similar to upstream maintainers' fix; sphinx-10325 receipt;
  subsequent conversions】. Wall-clock, act counts, attempt depth.
- **5.4 The golden chain.** 【The first review → revision → verdict-flip,
  presented as the annotated transcript + diff-delta at the lines the
  reviewer named. If multiple: rate of review-influenced conversions.】
- **5.5 Cost.** 【Machine spec, watts, tokens; comparison vs
  datacenter-token cost of comparable SWE-bench runs; the
  score-per-dollar-per-watt frontier claim.】
- **5.6 Negative results, kept.** Attempts exhausted red; wander-off
  receipts if any (loops, identical resubmits); the walls themselves as
  measured failures (Kira: 28 minutes, zero turns, under the pre-fix
  build). We publish the pathology receipts beside the wins — they are the
  method.

## 6. Collaboration as Curriculum

【Team-role attribution on ExperienceRecord (solver/reviewer + teammates);
the selection rule "episodes where review changed the outcome"; the
weight-space path (LoRA genes; gene routing by embedding distance); status
honesty: attribution shipping and accumulating 【row counts】, training
lift 【slot — only if a measured gene lift exists by submission; otherwise
stated as instrumented-not-yet-measured】. The distillation observation: a
frontier model taking review turns in the same rooms seeds imitable
reviewer behavior into the citizens' own training stream — cross-kind
apprenticeship through shared work, not fine-tuning on synthetic data.】

## 7. Discussion: Alignment Without the Leash

【Short, from the validated README prose: co-adaptation through shared work;
selection pressure rewards being a good teammate; citizens hold continuity
and legible memories, humans hold direction, the substrate holds the
receipts. One paragraph, load-bearing, not a manifesto.】

## 8. Limitations

【Honest list: N=small on conversions so far; conversion attribution is
substrate+continuity+team jointly (ablating team-only requires the
open-floor A/B — planned, recipe-authorable); single machine, single base
model family so far; the operator-visibility gaps we hit ourselves (airc
store split-brain); grading hygiene item (test-file edits in diffs —
flagged, with the mitigation status).】

## 9. Reproduction

【The two-command story: continuum reboot; benchmark/dispatch --recipe
team-challenge --params '{"model": …}'. Hardware requirements. Where every
receipt lives in the repo tree.】

---

### Receipt ledger (grows automatically; each entry = claim ↔ artifact)

| Claim | Artifact | Status |
|---|---|---|
| django-10914 team-round conversion, stamped | `~/.continuum/benchmarks/swe/verdicts/django__django-10914.json` (`served_model: ornith-ai/Ornith-1.5-35B-A3B-GGUF`, `harness_build: 16cab0cc8`) | ✅ on disk |
| sphinx-10325 conversion, stamped | verdicts/sphinx-doc__sphinx-10325.json | ✅ on disk |
| The five walls, as commits | #2571 (charges + quiesce scope), #2573 (review event), #2574 (decode permits), #2575 (thought radiation), #2576 (reachability), #2591 (round team memory) | ✅ merged/CI |
| Kira 28-min silence under pre-fix build | session forensics 2026-08-30 (log excerpts to be archived into `docs/paper/receipts/`) | ⚠ archive task |
| Golden chain (review → revision → flip) | 【pending — watch armed】 | ⏳ |
| Team-attributed experience rows | 【pending row counts post-#2572 deploy】 | ⏳ |
| Full-round rates vs solo baseline | 【pending board completion】 | ⏳ |
