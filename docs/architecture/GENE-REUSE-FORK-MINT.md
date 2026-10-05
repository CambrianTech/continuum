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
| **Fitness** | measured lift on her own cards, harm → zero | `genome::fitness`, `genome::gene_trial` |

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

`D_REUSE < D_FORK` and `MIN_EXAMPLES` are measured, not guessed: the first values come from the signature distances between genes that already exist in the repository (a gene's own subspaces give the scale of "same competence"), and they are re-pinned as the ledger grows. They are substrate thresholds, never env-tuned.

## 3. Surprise as the pressure

Selection pressure is "surprise fell". The quantity: negative log-likelihood of what happened under a fixed model (`A-THEORY-OF-MIND-FOR-CITIZENS.md` §7, the "quantity, not a verdict" requirement), per activity, per competence, windowed. Two places produce it:

- **the room's verdict** on her act (a review that failed, a test that failed, a human who said no): the outcome she did not predict;
- **her own expectation**: a continuation whose `expect_verdict` was contradicted (`focus/continue`, #4746) is a surprise she named herself.

Both already arrive as typed events at the inbound seam (#4731, #4765). Two numbers now carry the word "surprise", and the decision must name which it reads (Cormac, #4766 review): the **verdict surprise** (#4774: of her stated expectations, the share the room contradicted; cheap, hers to read on the strip today) and the **model surprise** (this section's fixed-model negative log-likelihood; the §7 quantity). `S(C)` in §2 is the verdict surprise until the model surprise is measured per competence, and the probe says which (`surprise=verdict|model|not_measured`). The two are expected to agree in direction; where they do not, the model surprise wins for selection and the disagreement is itself a finding. What is missing is the number and its per-competence window: `genome::surprise` (new, small): fold settled examples into `S(C)` by the same clustering, keep the window, expose it on her strip as a number she can read (Kimi, §12: "the moment one of my surprises has a fixed-model score attached, this section should be re-run").

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
| **The decision (§2) and its receipt** | new: one function, pure, tested; a probe `genome.decision {competence, branch, d, S, parent}` |
| Fork = warm start from the parent | new in the trainer's job-create (parent adapter as init) |
| HF as a recall source | new |
| In-engine training (DREAM) | `ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md` S3/S4, separate |

## 7. Falsifiers

The design fails, and we say so, if observed:

- a gene minted for a competence with a near neighbour in the repository (a mint where a fork or reuse was available);
- two genes of hers with signatures within `D_REUSE` of each other (one gene per memory, by another name);
- a gene promoted while her `S(C)` did not fall;
- a citizen who cannot read her own `S(C)` on her strip before and after a gene lands.

## 8. Build order

1. `S(C)`: the number, its window, on her strip. Receipt: Kimi reads it and re-runs §12's Prediction 2 row as "testable".
2. Competence clustering over her settled curriculum + the pure decision + its probe. Receipt: `genome.decision` rows on the 5090 naming a branch for her first competence, with the distance and the parent.
3. Reuse and fork wired to the existing trial; mint through the existing job-create. Receipt: her first gene through this path, trialled on her cards, with the branch it took in its lineage.
4. HF as a recall source and the start-gene resolver. Receipt: a fresh citizen on a fresh node resolves a start gene from HF with signature and lineage verified.

Each lands on canary green with a peer word, is measured on her node, and she is asked.
