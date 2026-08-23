# Gene Lifecycle: Continue vs Fork vs Mint

**Joel, 2026-08-23:** *"It also has to know when to keep training a LoRA layer
vs a new one or a fork."* This note is that decision's design, extending the
distance-routing doctrine (gene routing is DISTANCE, not keywords) with the
gene's training lifecycle. It is the output half of convergence **seam 2**
(dream → training-trigger): dreams cluster experience by signature; the
trigger's output is not just "train" but "train **what**" — nearest gene, a
fork of it, or a newborn.

## The rule: distance decides cheaply, behavior decides finally

New experience arrives from graded rounds (learn-mode lessons, failures,
tool-traces) carrying an embedding signature.

### 1. MINT — far from every gene (pure distance call)

No existing gene's signature claims the material: no niche exists, so a gene
is born. This is the only branch a distance threshold decides alone, and it is
the safe direction to be wrong in — a mint that should have been a
continuation lands NEAR its would-be parent and the redundancy term (below)
taxes it toward retirement.

### 2. CONTINUE vs FORK — near an existing gene (behavior call)

A distance threshold must NOT decide this — that would be a hidden constant
deciding cognition (see HIDDEN-CONSTANTS-LEDGER: capability decisions are
window-derived, data-owned, or behavior-derived; never a bare number).

The honest signal is **catastrophic forgetting, measured as behavior**:

1. **Attempt assimilation**: continue-train the nearest gene on the new
   material (cheap LoRA step).
2. **Run the gene's own held-out gate** — the eval set that made its fitness
   (its prior niche, held out per the exams doctrine).
3. **Gate green** → same niche; the gene deepened. CONTINUE was correct;
   promote the updated weights.
4. **Gate red** → the new material pulls the weights away from what the gene
   already knows. That IS the speciation event, mechanically detected:
   **FORK** — keep the pre-assimilation parent frozen for its niche, train
   the copy on the new material, mint it a fresh signature from what it was
   actually trained on.

This is Behavior-Before-Perplexity applied to the lifecycle itself: a gene's
existing skill is a **regression test**, and forking is what you do when the
ratchet trips. No proxy metric (loss, perplexity, distance) ever promotes or
speciates — only emitted behavior on held-out gates.

## Fork abuse polices itself — the fitness formula is the ecology

`fitness(L) = (lift × demand) / (cost × redundancy)`, hard-zero on harm.

- A fork that doesn't genuinely diverge sits near its parent in signature
  space; the **redundancy** term taxes both; UCB stops selecting the loser;
  the loser retires. Speciation without divergence is self-extinguishing.
- The same term run backwards gives **merge/retirement**: two genes whose
  signatures and behavior converge are redundancy paying no lift — retire
  one, or distill both into one and re-gate.
- Young forks and mints get the UCB exploration bonus (`UCB_C·sqrt(ln(N+1)/n)`
  in the fitness ledger) — diversity retention is already a property of the
  fitness evidence, not a special case here.

## What this needs from the substrate (wiring, in dependency order)

1. **Per-gene held-out gate as an artifact**: every gene carries (or derives
   via its recall_keys / origin lessons) the eval slice that made its fitness
   — the regression set the CONTINUE attempt must pass. Without this the
   forgetting signal does not exist.
2. **Assimilation attempts are cheap and revertible**: continue-training
   happens on a COPY of the weights; promotion (continue) or speciation
   (fork) is a rename, never an in-place overwrite of the only copy.
3. **Signatures are re-minted after training**: a gene's identity is what it
   was trained on; a fork keeps neither the parent's signature nor its
   fitness history (fresh UCB trials — it is a new hypothesis).
4. **The trigger lives at seam 2** (dream → training-trigger): dream
   consolidation already clusters by signature/recall keys; its output
   becomes `TrainTarget::{Continue(gene), Fork(gene), Mint}`.

## The p2p consequence

On the grid, CONTINUE-vs-FORK is also the merge policy for the genome
commons: a peer's continued gene whose gate passes upstream's held-out set is
an UPDATE; one that fails is a FORK the commons carries alongside the parent
— exactly a version-control model for weights, with behavior gates instead of
diffs deciding fast-forward vs branch. This is what lets many peers train the
same niche without clobbering each other: the gate arbitrates, no
coordination needed.
