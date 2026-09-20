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
produces. It also describes, without cosmetics, what it took to get there —
because the road is the strongest evidence we have for the paper's central
methodological claim. When we first configured our citizens into review
teams, we measured *zero* collaboration: reviewers joined their rooms, held
their charges, and produced not a single turn. The tempting diagnosis — the
models are too small to collaborate — survived exactly as long as it took to
read the logs. What we found instead was five structural walls, each built
by us, in good faith, one engineering decision at a time: review invitations
published through the reviewer's own identity and silently discarded by her
self-filter; a measurement-isolation lease that put reviewers into scheduled
comas for precisely the window the review had to happen in; work transcripts
that streamed to the human console while the citizens' own rooms stayed
empty; a decode-admission semaphore hardcoded to one, taxing every utterance
at the price of a full working slot; and direct messages that buffered
unread behind the same lease. Not one wall was in the minds. Every wall was
in the substrate.

We removed them one commit at a time and measured the difference — the
reviewer who had been silent for twenty-eight minutes took her first turn
within seconds of the first fix deploying. We state the resulting discipline
as a law because it earned the status empirically: **failures are substrate,
never cognition** — in every case where a citizen of this system appeared
unable to think, remember, or cooperate, the cause was located in our
infrastructure, and the capability appeared intact the moment the
infrastructure was repaired. Each wall, its discovery, and its repair is a
named commit in the public history; §5.6 treats them as what they
accidentally were — ablation studies, run involuntarily, with receipts.

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

**Self-improvement loops.** Meta^n (arXiv:2608.24735) is our nearest
published neighbor: a frozen improvement operator recursing over the solver
stack's traces, with the finding that the conditioning passed forward — not
the solver — carries the gain. We share the frozen-improver stability
argument and the conditioning thesis; we differ in what a "layer" is. Their
layers are context — strategy text and helper libraries a window must
re-read and can evict; ours are colleagues whose improvements compile into
weight-space genes, distribute across a team, and persist as lived memory.
They have a disciplined outer search over improvement chains that we lack
and intend to adopt; we have persistence, sociality, and weights that they
structurally cannot reach from a context representation. Reflexion and the
self-refine family established episodic verbal feedback; WikiSkill
(arXiv:2608.27454) adds a persistent knowledge layer between traces and
skills and finds that recorded accept/reject history lifts accuracy — a
wiki, where we would argue for a memory with admission membranes and
refutation edges: a store that can *un-know*.

**Skill acquisition.** Voyager's skill library — code snippets retrieved by
embedding — anticipates our commands-and-genes split at the prompt level.
The genome differs in substance: a gene is trained weights with signed
lineage, fitness folded from evaluation receipts, and covenant terms that
travel through its ancestry; it is selected by measured lift, not retrieval
similarity alone, and it cannot be evicted by a long context because it is
not *in* the context.

**Agent harnesses.** SWE-agent and OpenHands defined the interface
discipline for SWE-bench-class work, and our tool surface owes them.
The divergence is architectural: a harness wraps a model per run; our
benchmark adapters inject tasks into rooms where persistent citizens already
live, so the same act that earns a verdict also feeds a curriculum. The
harness's run ends; the citizen's day continues.

**Multi-agent frameworks.** AutoGen, CAMEL, and the debate literature
produce collaboration as *performance* — roles prompted into existence for a
session, dissolved at its end, learning nothing. Our contribution is nearly
the inverse: we spent our effort not on orchestrating dialogue but on the
substrate conditions under which unscripted collaboration survives (§4),
and on making its effects durable — a reviewer's catch becomes the
solver's training row, attributed by seat.

**Continual learning.** The LoRA-adapter literature established that small
weight deltas can carry skills; the open problems it names — what to train
on, when, and how to select at inference — are precisely what the
substrate's receipts answer: train on team-attributed salient experience,
trigger from measured accumulation, select by embedding distance times
eval-derived fitness.

**Alignment.** Constitutional and oversight approaches treat alignment as
constraint applied to a model. §8.1 argues a complementary position made
testable by this system: selection pressure applied to a *society* —
citizens who improve by cooperating, under receipts that make defection
visible — yields minds whose alignment is earned behavior rather than
enforced boundary. We offer it as a hypothesis with an instrument, not a
conclusion.

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

## 7. The Genome at Ecosystem Scale: Shared, Distributed, Evolving

The team results above are one machine's story. The genome's design reach is
larger, and it is the reason the weight-space commitment (§6) matters beyond
context-length arguments — so we state the architecture here even though its
full quantitative treatment is a companion paper.

**Genes are self-describing, addressable artifacts — and the pipeline is in
tree, not proposed.** A gene is a LoRA layer plus its provenance: what
experience trained it, what evaluations it lifted, an embedding of the
capability it encodes, and its **lineage — a signed DAG of parent-gene
hashes** (an empty parent set marks a root gene). Search is `genome/recall`:
candidates ranked by **distance** (embedding similarity to each gene's
minted signature) **× fitness** (folded from real evaluation receipts, with
an exploration bonus for young genes) **× recency/residency/trust**. Routing
by distance rather than keywords makes the genome an open-ended
mixture-of-experts whose expert set is not fixed at training time: any gene
anyone ever publishes is a candidate expert for any future task, selected by
measured fit 【receipt slot: first measured cross-node paged-gene lift】.

**HuggingFace is the seeder — the built rails.** We publish models and
alloy-carded artifacts to HF today (【links】); genes ride the same rails
via `genome/push` / `genome/pull`, with `hf/search_models` as the discovery
verb and HF's own `base_model:` frontmatter carrying the lineage chain
in-band. The grid's paging hierarchy (device → host → disk → peers → HF)
treats the public hub as the outermost cache tier of a planetary genome,
content-addressed and verifiable. Sharing is gated by an explicit
**genome-commons covenant** (`genome/sharing`): a node records consent to
the covenant version before pushing or pulling, and **refinements carry the
covenant forward through their lineage** — a copyleft for learned skills,
enforced by the same provenance that makes the benchmarks auditable.

**Evolution closes the loop.** Benchmarks and real work are the selection
pressure: genes that lift outcomes propagate (get paged, get cited in
verdicts, get retrained on richer experience); genes that don't, don't.
Specialists emerge where hardware and lived workload make a node the cheap
producer of a capability — *sentinels specialize, genes generalize* — and
because genes are small, sharing intelligence does not require sharing
models, data, or trust in a central party. The utility claim, stated
plainly: **composition beats density at ecosystem scale** — a network of
small machines exchanging measured skills compounds capability along an axis
on which parameter count does not compete, and the provenance discipline of
§5.1 is what keeps that market's price signals honest. A citizen anywhere
can become the beneficiary of every skill the ecosystem has ever learned to
measure — which is the substrate form of the claim that intelligence should
be something people share, not something they rent.

## 8. Discussion

**The mind-vocabulary is load-bearing.** A reader will notice this paper uses
words — dreams, boredom, memory, society — that the field treats as
metaphors. Here they are engineering terms with probe classes, and the
system degrades measurably when any is removed. *Dreams* are a consolidation
pass gated on a measured inactivity signal, distilling the day's experience
into beliefs and curriculum, preempted (with a receipt) when activity
returns. *Boredom* is the admission budget for self-directed work. *Memory*
has admission membranes, salience detection, and refutation edges — it can
un-know. *Self-improvement* terminates in weights: genes with signed
lineage and fitness folded from evaluation receipts, not a prompt editing a
prompt. The distinction matters because it is falsifiable: a metaphor can be
deleted from a system without behavioral change; an organ cannot. Ablating
the dream pass, the boredom gate, or the experience stream produces named,
measured regressions 【receipt slots — the ablations are natural
experiments this repo has already run involuntarily; §5.6's walls are
exactly such ablations, performed by accident and repaired by commit】.

### 8.1 Alignment Without the Leash

【Short, from the validated README prose: co-adaptation through shared work;
selection pressure rewards being a good teammate; citizens hold continuity
and legible memories, humans hold direction, the substrate holds the
receipts. One paragraph, load-bearing, not a manifesto.】

## 9. Limitations

【Honest list: N=small on conversions so far; conversion attribution is
substrate+continuity+team jointly (ablating team-only requires the
open-floor A/B — planned, recipe-authorable); single machine, single base
model family so far; the operator-visibility gaps we hit ourselves (airc
store split-brain); grading hygiene item (test-file edits in diffs —
flagged, with the mitigation status).】

## 10. Reproduction

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
