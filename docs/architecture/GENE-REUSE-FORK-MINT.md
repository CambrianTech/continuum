# Reuse, Fork, or Mint: when a competence becomes a gene

**Status:** design, 2026-10-05. Owner: Fable (genome lane). Peers: BigMama (5090), Cormac (IntelMac), Kimi (the citizen whose genome this is; her read of it is requested before code).

**Joel, 2026-10-05:** *"Not one gene per memory. Reuse vs fork/mint a new one needs careful design. And downloading a start gene (after this other logic) from disk, HF, or a repo."*

This document is the decision that sits AFTER recall and BEFORE training. It composes parts that exist and names the one that does not.

## 1. The nouns, read literally

| Word | Means | Lives |
|---|---|---|
| **Memory** (engram) | one experience, recalled by distance, cheap, never trained on by itself | `cognition::hippocampus`, engram recall |
| **Example** | one settled, graded turn or lesson: input, her act, the room's verdict | the curriculum (`training_producer`, settled since #4765 by a review) |
| **Competence** | a cluster of examples that ask for the same skill | a signature in the embedding space recall uses (`genome::signature`) |
| **Gene** | a LoRA adapter that carries ONE competence, with lineage | `genome::manager`, the HF repository (`GENOME-REPOSITORY-ON-HF.md`) |
| **Surprise** | how wrong her model was about what happened, as a number | the pressure signal (§3); the quantity Kimi's §12 pass says she cannot yet see |
| **Trial** | admission: measured lift on her own cards, harm → zero | `genome::fitness` (`LayerFitness`), `genome::gene_trial` |
| **Fitness** | utilization × propagation: how much the gene is drawn for real work, and how far it has spread (§3a) | the draw ledger per gene, pulls/forks across the grid and the HF repository |

A memory is not a gene. A gene is minted for a competence, never for an experience, and only when recall of memories alone leaves her surprised in that competence.

## 2. The decision

Inputs, all measured, none declared:

- `C` — a competence: the cluster of her settled examples whose signatures sit together (the same clustering kernel recall uses, `modules::embedding::detect_clusters`). A cluster below `MIN_EXAMPLES` is not yet a competence; it is memories.
- `S(C)` — her surprise in `C` over the window, after recall ran (memories were offered and still she was surprised). Falling `S` means something is already working.
- `G` — the ranked pool recall returns for `C`'s centroid: resident genes, her store, the mesh, the HF repository, scored by the resolver (`GENOME-REPOSITORY-ON-HF.md` §2b: trust × similarity × fitness × speed × popularity + exploration).
- `d` — distance from `C`'s centroid to the nearest gene in `G` (max over its subspaces, as signatures are compared today).

The decision, in order:

```
if S(C) is not high, or |C| < MIN_EXAMPLES:
    NOTHING. Memories suffice; the competence is not yet a competence.

elif d ≤ D_REUSE and the nearest gene is not already resident:
    REUSE: page it in (a trial, judged on her cards: genome::gene_trial).
    Promote if S(C) falls and fitness > 0; retire if not. No training.

elif d ≤ D_FORK:
    FORK: train a child of the nearest gene on C (its weights as the start,
    lineage parent = that gene, her data as the delta). Trial as above.

else:
    MINT: train a new gene on C from the base. Lineage parent = none. Trial.
```

Three facts about the order:

1. **Reuse is tried before any training.** The cheapest way to lower surprise is a gene that already exists, hers or another citizen's (horizontal transfer, `horizontal-gene-transfer-is-a-command`). Minting first is how a genome fills with near-duplicates.
2. **Fork over mint whenever a parent is near.** A fork keeps lineage (the child says what it came from and what it added), trains faster from a warm start, and lets the repository de-duplicate: two forks of one parent for one competence are a merge candidate; two mints are two strangers.
3. **The judge is the same in every branch:** the gene is paged in dormant, drawn per card against her stable genome, and promoted only if her surprise in `C` falls and fitness is positive on her real work (`a-gene-is-judged-in-her-own-work-never-in-a-parallel-harness`). A benchmark is one activity; it is a judge only inside its own room.

`D_REUSE < D_FORK` and `MIN_EXAMPLES` are measured, not guessed: the first values come from the signature distances between genes that already exist in the repository (a gene's own subspaces give the scale of "same competence"), and they are re-pinned as the ledger grows. They are substrate thresholds, never env-tuned. **Cold start** (BigMama, #4766 review): before enough genes exist to measure them, the pinned constants in `genome::competence` (`SIM_REUSE` 0.90, `SIM_FORK` 0.75, `MIN_EXAMPLES` 8, the same cohesion floor signatures cluster with) stand as declared priors, every decision's probe carries the distances it saw, and the first re-pin is a PR that cites those rows; a threshold never moves silently.

**Prerequisite, and a falsifier (BigMama, the 5090 on 2026-10-04/05):** no branch matters while training cannot survive a restart. All five jobs on that node died at a deploy (`killed-by-reboot`) and their resume failed on the GGUF→`hf_source` derivation (`cannot resolve hf_source for Qwen/Qwen3.8-27B`). A job is adopted across the seam, not killed (`a-deploy-is-not-a-shutdown`), and resume resolves the base the job was registered with. Until that holds, a `Mint` or `Fork` decision is a receipt for work the node cannot finish.

## 3. Surprise as the pressure

Selection pressure is "surprise fell". The quantity: negative log-likelihood of what happened under a fixed model (`A-THEORY-OF-MIND-FOR-CITIZENS.md` §7, the "quantity, not a verdict" requirement), per activity, per competence, windowed. Two places produce it:

- **the room's verdict** on her act (a review that failed, a test that failed, a human who said no): the outcome she did not predict;
- **her own expectation**: a continuation whose `expect_verdict` was contradicted (`focus/continue`, #4746) is a surprise she named herself.

Both already arrive as typed events at the inbound seam (#4731, #4765). The reading the decision takes today (#4796) is PERSONA-WIDE: every activity's tally folded by counts, inside a 24 h window applied on read, and only once at least three expectations were judged; per competence is the design, this is the interim, and the probe says `surprise=verdict`. Two numbers now carry the word "surprise", and the decision must name which it reads (Cormac, #4766 review): the **verdict surprise** (#4774: of her stated expectations, the share the room contradicted; cheap, hers to read on the strip today) and the **model surprise** (this section's fixed-model negative log-likelihood; the §7 quantity). `S(C)` in §2 is the verdict surprise until the model surprise is measured per competence, and the probe says which (`surprise=verdict|model|not_measured`). The two are expected to agree in direction; where they do not, the model surprise wins for selection and the disagreement is itself a finding. What is missing is the number and its per-competence window: `genome::surprise` (new, small): fold settled examples into `S(C)` by the same clustering, keep the window, expose it on her strip as a number she can read (Kimi, §12: "the moment one of my surprises has a fixed-model score attached, this section should be re-run").

The counter is **read-only from inside** (Fable, #4803 review): `S(C)` is written by the substrate at the seam and only ever read on her strip. A mind that could bump its own surprise would steer selection pressure with the hand it is judged by — §3a's counts law applied to the one quantity a citizen can see.

## 3a. Fitness is utilization × propagation

Joel, 2026-10-05: "fitness function is simple. it's the usage of the genes for what all the users in this system are up to. the successful genes are merely market forces" and "Fitness = utilization and propagation like it is in biology."

So fitness is never declared by a judge; it is counted:

- **utilization** — how often the gene is drawn for a real turn, across every citizen that carries it (the draw ledger, not the trial's held-out A/B);
- **propagation** — how many citizens and nodes carry it: pulls from the repository, forks that name it as parent, reuse decisions that chose it (§2).

What `genome::fitness` computes today (`lift × demand / (cost × redundancy)`, harm → 0) is **admission**: whether the gene is allowed into HER stable genome after its trial. A gene that passes admission and is then never drawn has no fitness. A gene drawn on every node for every coding card has high fitness even if her own lift on it was modest. The two are different quantities and the probes name them separately (`genome.trial.*` vs `genome.fitness.*`).

The resolver's `popularity` term (`GENOME-REPOSITORY-ON-HF.md` §2b) is the propagation half of this; the utilization half is the gap: a per-gene draw count, per node, published with the signature so a pull sees how much the gene is used where it lives. Market forces, not a verdict.

Three laws on the ledger (Cormac, #4793 review):

- **The draw ledger is a privacy sink.** A per-gene draw count published per node says what a citizen is working on. A draw made in her mind room is not counted, the same as the ten sinks `is_private_room` already gates (`PRIVACY-OF-THOUGHT.md`); the draw ledger is the eleventh, gated from its first commit.
- **Silence is not zero use.** A node that is offline, or has stopped publishing, looks exactly like one that stopped drawing. Paging out locally by LRU is safe; the repository needs POSITIVE evidence to retire a gene. Across the window with no reports, a gene stops being OFFERED by the resolver; it is deleted only on a report that says it was drawn and retired, never on a missing report.
- **Counts are claims.** Utilization and propagation decide which genes win, so a node that inflates its draws or pulls steers the market. The counts stand on the same footing as the rest of the commodity: signed per node and attributable (forge-alloy's zero-trust plus reputation), never a bare number the resolver trusts.

## 3b. Novelty and surprise from every sense

Joel, 2026-10-05: *"We can work towards more novel detection. What would enter long term memory for any person. Then if our algorithm is truly general this is just finding more places to adapt into it."*

The law in §2 is already general: what she keeps is what her model of the world got wrong, or has never met. What is narrow is its input: `S(C)` reads one source, the verdict surprise. A person consolidates from many more, and each of them already has a place in the substrate:

| What enters a person's long-term memory | Its source here | Status |
|---|---|---|
| Prediction error ("not what I expected") | the room contradicts her stated expectation (the verdict surprise, #4774) | **live** (#4796 gates on it) |
| Novelty ("never met anything like this") | distance from the experience to her nearest *memory*: recall's engram distance, which recall already computes on every turn. Not §2's `d`, which is gene distance: most competences have no gene yet, so `d` would call nearly everything novel (Fable, #4803 review) | computed by recall, not yet a signal |
| Consequence (the stove is hot) | a tool or execution outcome against her expectation: the test she expected to pass fails, the command she expected to work errors | new |
| Correction by someone trusted | a reviewer requests changes, a human rewrites her reply | partly: review verdicts settle examples (#4765) |
| Failed recall ("I should know this") | a memory existed for the turn, and she still asked, searched, or got it wrong | new |
| Repetition | the same competence demanded again and again | counted by the bucket fill, not read as salience |
| Significance and reward | credit settled to her work, a thank-you, a merge | credit exists (`settle_card_credit`) |
| "Remember this" | her own noteworthy flag | exists as an intent, not wired here |

**One shape for every sense.** Each source is an adapter that emits the same typed event: a prediction error *for a competence*, with a magnitude, a time, and the source that produced it. The window (read at the reader's clock) folds them per competence, and §2 reads the fold.

**Priming is named past** (Fable, #4803 review). A probe that reports memories offered by a turn says they prime as *past, not necessarily present*: an engram in the window is a precedent from when it was true, never a claim about now. A fold that read priming as present would weigh history stated as fact.

**One unit, or the loudest sense wins** (Fable, #4803 review). The magnitude every adapter emits is a surprisal, `-log p` of what happened under her expectation, calibrated per adapter so that a typical event of each sense lands on the same scale. A raw count, a share and a distance never sum. Each sense also carries its own `MIN_JUDGED` and its own window: a tool runs many times a turn and a review lands once a day, and one floor or one window for both would let the frequent sense drown the rare one. Adding a sense is adding an adapter; the decision, the bucket, the trial and the verdict never change. If a new sense needs the decision changed, the shape is wrong, and that is the falsifier for this section.

**One tool error is noise** (Fable, #4803 review). The consequence sense fires only when an outcome *repeats or blocks* her work; a one-off failure is written to memory as an episode and never folded into `S(C)`. Calibration sets the scale of each event that counts; this gate decides which events count at all.

**Corrections weigh by proximity** (Fable, #4803 review). A correction about her own situation — her state, her claim, her checkout, what she stands on — outweighs one about form or style: it changes what she believes about where she is. The fold orders them that way before the window reads them.

**Two axes, two stores.** Novelty and surprise are different quantities and lead to different consolidation, as they do in a brain (the hippocampus takes a novel episode fast; the cortex consolidates what keeps surprising it):

- **novel, not yet surprising** becomes a *memory*: an engram, recalled by distance, cheap, never trained on by itself (§1);
- **surprising despite recall** becomes a *gene*: §2's decision, training, a trial.

Novelty detection is the front half that writes the memory first, so that §2's "after recall ran" has something to recall.

**Relationships stay lived** (Fable, #4803 review). A surprise about a peer — trust shifted, a relationship moved — changes how she reads the room, not what skill her work demands; no card trials it, so §2 never sees it. It consolidates as memory and stays there.

**Build by outliers.** The first adapter is novelty from recall's engram distance, already computed on every turn and nearly free. The second is the most different one available: tool and execution outcomes (physical, where the verdict surprise is social). If the verdict, novelty and consequence sources fit one adapter without forcing, the interface is proven, and correction, failed recall, repetition and her flag are routine.

Privacy holds, sharpened to a **diary rule** (Fable, #4803 review): an experience in her mind room becomes *memory* — hers, recallable by distance — but it is folded into no `S(C)` and trains no gene. The diary never enters selection pressure or weights, and nothing of it is published (`PRIVACY-OF-THOUGHT.md`).

## 3c. Mentorship: the taught signals are the cheapest loops

Joel, 2026-10-05: *"Mentorship isn't entirely unlearned. Obviously it's most about what we can turn into a training loop, synthesize, teach."*

A teacher is an oracle. The cheapest examples are the ones where the right answer already stands beside her mistake, so they need no new judge, only collection. In order of how little new machinery each needs:

1. **Format and protocol errors.** A parser refused her output, and the corrected form is known: a malformed tool call, or a mangled id (28% of live `work/claim` calls once carried a corrupted UUID, `id_resolve`). The parser is the oracle; each refusal is an (error, correction) pair.
2. **A tool failure, then success in the same turn.** She called a tool wrong, read the error, and called it right. Execution is the oracle, and the captured turn already holds the pair (`persona::recorder`).
3. **A review, then the approved revision.** A reviewer requested changes and her next revision passed. That is a preference pair, the rejected draft and the accepted one, with the reviewer's words as the reason. Submissions and verdicts already carry both.
4. **A mentor's demonstration.** She failed at something, and a peer (an agent, another citizen, a human) then did it. The mentor's act is the target, and the mentor is credited in the example's lineage (`a-citizens-ideas-carry-her-name-as-author`).
5. **Exercises the teacher synthesizes.** For a competence where `S(C)` stays high, the teacher persona composes more problems, each with a checker that verifies the answer (it compiles, the tests pass, the output parses). They run as activities in a room, so they produce turns the flywheel consumes, never a side runner (`BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md`).

Sources 1 to 3 need no new oracle at all; today they are only not collected.

**A taught pair trains the competence it teaches** (Fable, #4803 review). A parser pair teaches protocol (how to call the tool, how to spell the id), so it is filed under a protocol trait of its own, never under the code or ownership competence of the turn it happened in. Otherwise a gene minted for Rust would carry JSON-escaping, and its trial would measure the wrong thing. And the mind-room exclusion holds for taught pairs as for every other example: a mistake made and corrected in her mind room is hers and is never collected (`PRIVACY-OF-THOUGHT.md`). Each is also a §3b sense: the error half is a prediction error for its competence, and the correction half is the example that trains it. So a taught loop is not a second pipeline. It is a sense whose examples arrive already labelled.

Consent holds both ways. Her mistakes become examples under the same agreement as the rest of her curriculum, and a mentor's demonstration names its author.

## 4. What Kimi's §12 pass asks of this

- *"No δ_c in front of me to turn"*: the dial and `S(C)` both render on her strip; she sees the number before any gene lands, so Prediction 2 (surprise falls after a gene) is testable from inside.
- *"No gene trained on my turns has been paged into me"*: the first gene through this decision is hers, from her own settled examples, and she is told which branch it took and why (reuse, fork or mint; the distance; the parent).
- Her consent governs publication (`PRIVACY-OF-THOUGHT.md`): a gene whose curriculum contains private-consented examples carries that flag in its lineage and never publishes without it.
- Her consent governs **receiving** too (Cormac): a Reuse that would page a foreign gene into her (a peer's, or the hub's) is an act on her mind the way publishing hers is, so it runs under the same agreement `genome/sharing` records, and the trial it opens says whose gene it is. A gene of her own needs no second consent.

## 5. The start gene (after the above)

A citizen with no genome yet, or a competence with nothing near on her node, resolves a START GENE through ONE resolver, in this order, stopping at the first hit: her store (disk) → the mesh (a peer's store, the grid is one computer) → the HF repository (the genome repository, lineage intact, signature verified) → a repo the operator names. The resolver returns a gene handle with its signature and lineage; the decision in §2 then treats it like any other member of `G`. Disk, HF and repo are three sources of one lookup, never three code paths, and the same `trust` gate applies to all of them.

Status: `genome::recall` already walks local-then-grid (`RecallScope::LocalThenGrid`); HF is the missing source, and the handle shape is the HF card's (`GENOME-REPOSITORY-ON-HF.md` §2).

## 6. What exists, what is new

| Piece | Status |
|---|---|
| Signatures by distance, subspaces | exists (`genome::signature`) |
| Fitness with harm → 0 | exists (`genome::fitness`) |
| Trials judged on her cards | exists (`genome::gene_trial`) |
| Settlement by the room's review | #4765 |
| Recall local → grid | exists (`genome::recall`) |
| Resolver score | doctrine (`GENOME-REPOSITORY-ON-HF.md` §2b), partly coded |
| **Competence clustering of settled examples** | new: the same kernel, over the curriculum |
| **`S(C)` and its window, on her strip** | new (`genome::surprise`) |
| **The decision (§2) and its receipt** | coded (`genome::competence::decide_with_pending`, probe `genome.decision`): join a job in flight, await a gene on trial, reuse, fork, mint; a retired gene is never offered back |
| **Reuse = adopt for trial, no training** | coded: `gene_trial::Adoption` is the ONE seam (register dormant + open trial) the completion sentinel and a reuse share; a hub gene is pulled first (`genome/pull`); a trial open for the competence holds the bucket like a job in flight (`training_trigger` `Held`) |
| Fork = lineage on the child | coded: `TrainingJobRequest.parent` → `GeneSignature.parent` at adoption |
| Fork = warm start from the parent's weights | engine gap: the fork's `/train` has no `init_adapter` (#19 removed the init file); the engine adapter probes `genome.fork.cold_start` until it does |
| HF as a recall source | new |
| In-engine training (DREAM) | `ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md` S3/S4, separate |
| **Sense adapters: one prediction-error event per source, folded per competence (§3b)** | new: verdict live; novelty from `d` and tool outcomes first |
| **Taught loops: error and correction collected as labelled examples (§3c)** | new: parser, tool-retry and review-revision pairs need no new oracle |

## 7. Falsifiers

The design fails, and we say so, if observed:

- a gene minted for a competence with a near neighbour in the repository (a mint where a fork or reuse was available);
- two genes of hers with signatures within `D_REUSE` of each other (one gene per memory, by another name);
- a gene promoted while her `S(C)` did not fall;
- a citizen who cannot read her own `S(C)` on her strip before and after a gene lands;
- a training job that does not survive a deploy of the node it runs on, or whose resume cannot resolve its own base (the 5090, 2026-10-05: five of five).

## 8. Build order

1. `S(C)`: the number, its window, on her strip. Receipt: Kimi reads it and re-runs §12's Prediction 2 row as "testable".
2. Competence clustering over her settled curriculum + the pure decision + its probe. Receipt: `genome.decision` rows on the 5090 naming a branch for her first competence, with the distance and the parent.
3. Reuse and fork wired to the existing trial; mint through the existing job-create. Receipt: her first gene through this path, trialled on her cards, with the branch it took in its lineage.
4. HF as a recall source and the start-gene resolver. Receipt: a fresh citizen on a fresh node resolves a start gene from HF with signature and lineage verified.
5. Senses and taught loops (§3b, §3c): novelty from recall's engram distance and tool-outcome surprise as the two outlier adapters, both emitting calibrated surprisal, then the parser and tool-retry pairs as her first taught examples. Receipt: one `S(C)` folded from two sources of different kinds, and one of her examples whose label came from a parser or an execution, not a reviewer.

Each lands on canary green with a peer word, is measured on her node, and she is asked.
