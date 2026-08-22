# The Genome Repository rides Hugging Face — lineage IS the chain

**Status:** decided direction (Joel, 2026-08-22: *"If HF provides the chain we just
use it for everyone and it provides a way to query easily because the lineage tells
you everything. HF is perfect."*). Sequenced AFTER the prod-worthiness arcs; this
doc exists so the design survives until then.

Companions: the routing half (distance, not keywords) and the ethics half (citizen
covenant) are recorded with this doc's rationale in the session memory line; the
trust layer is [forge-alloy](https://github.com/CambrianTech/forge-alloy), already
proven on the [published models](https://huggingface.co/continuum-ai).

---

## 1. The decision

Genes ship as Hugging Face model repos. **We build no registry infrastructure**,
because HF already provides every piece the repository system needs:

| Need | HF-native mechanism |
|---|---|
| **Lineage chain** | `base_model:` metadata — HF renders the finetune tree natively. A gene declares its base model AND its parent genes; the chain back to the base weights is queryable, for anyone, with no service of ours running. *The lineage tells you everything*: which substrate minted it, what it was bred from, what it composes with. |
| **Provenance & integrity** | Repo git history (every revision immutable), plus the forge-alloy hash + signature riding in the card — HF distributes, alloy proves. HF is a **seeder, not an authority**: content-addressing means any mirror serves the same verified artifact later, p2p included. |
| **Query** | Hub API filters (org, tags, `base_model` chains) narrow the candidate set; the **embedding signature published in each card** (§2) makes the real query a client-side vector match — distance in embedding space, never keyword guessing. |
| **Discovery & adoption signal** | Download counts, likes, the org page as curated front door. The pre-market "people immediately get to use each other's" loop is HF's existing social layer. |
| **Distribution** | Genes are megabytes (LoRA deltas over 3B-active bases). `genome/pull <repo>` is one command; a full wardrobe is a coffee's worth of bandwidth. |

## 2. The gene card — self-describing, never hand-stamped

Every field is **computed at mint** by the foundry, none authored by hand:

```yaml
# HF model card metadata (illustrative)
base_model: ornith-ai/Ornith-1.5-35B-A3B          # the weights it pages onto
base_model_relation: adapter
tags: [continuum-gene, astropy, python-scientific] # GENERATED from the signature's
                                                   # embedding neighborhood — labels
                                                   # for humans; vectors are the truth
continuum_gene:
  signature:                                        # embedding-space identity
    centroid: [...]                                 # corpus centroid (routing key)
    subspaces: [[...], [...]]                       # a gene is NEAR several domains
    embedder: qwen3-embedding-0.6b                  # signature is embedder-versioned
  lineage:
    parent_genes: [continuum-ai/gene-python-core]   # breeding chain (also base_model tree)
    corpus_hash: sha256:...                         # the experience it was lifted from
    minted_by: <substrate id + version>
  fitness:                                          # RECEIPTS, not claims (§4.1.3.4
    - suite: swe-bench-lite/astropy                 #  falsifiability, forge-alloy)
      before: 0/6
      after: 4/6
      ledger: <link to committed results>
  alloy: <forge-alloy hash + signature>
```

**Routing** (the substrate side): the model-selection ladder gains a distance rung —
nearest gene(s) by signature to the task's own embedding, stacked by similarity
weight. A functional-programming gene lifts the Scheme task nobody trained for;
biology carries most of biochemistry. Tangential intelligence is *pulled down and
used* because proximity, not keyword identity, is the match.

### 2b. The resolver score (Joel, 2026-08-22: *"Score is a compound of similarity
to need, scores, popularity, and so on… including speed"*)

Same doctrine the recipe scorer already speaks — **gates multiply, objectives
weigh** — plus an optimism term so young forks get their audition:

```
score(gene, need, device) =
    trust(gene)                                  # GATE ∈ {0,1}: signature verifies,
                                                 #   lineage intact, covenant unbroken
  × similarity(need, gene)^α                     # cosine to the signature centroid,
                                                 #   max over subspaces (a gene is
                                                 #   near several domains)
  × fitness(gene)^β                              # normalized benchmark delta vs base,
                                                 #   from SIGNED receipts, decayed by
                                                 #   receipt age; team-scored outcomes
                                                 #   count (alignment spreads with skill)
  × speed(gene, device)^γ                        # device-RELATIVE: measured page-in cost
                                                 #   + tok/s delta on THIS tier; prior
                                                 #   from the card's hardware rows,
                                                 #   replaced by local telemetry after
                                                 #   first use (never trust a stranger's
                                                 #   benchmark for your own latency)
  × popularity(gene)^δ                           # adoption RETENTION (kept-installed),
                                                 #   never raw downloads; δ deliberately
                                                 #   small — popularity is the most
                                                 #   gameable term and is only a prior
  + c · sqrt(ln N / n(gene))                     # UCB exploration: few local trials →
                                                 #   wide confidence → occasional pick.
                                                 #   This is the DIVERSITY RETENTION that
                                                 #   keeps the commons from monoculture —
                                                 #   the same bandit discipline serving
                                                 #   uses for lane arms.
```

- **Weights (α…δ) are learned, not sacred** — the resolver's own selections carry
  outcomes (did the paged gene lift the turn?), so the exponents are tunable from
  receipts exactly like any other bandit. Hand-set priors: α highest (need
  dominates), then β, γ; δ smallest.
- **Stacking**: top-k by score under a redundancy constraint — two genes whose
  signatures overlap beyond a threshold don't both page in (complementarity over
  duplication; VRAM is the budget).
- **Every input is falsifiable**: similarity from the published signature, fitness
  from signed receipts, speed from local telemetry, popularity from the registry's
  retention counts. A term that can't be verified doesn't enter the product —
  that is what keeps a global commons ungameable enough to trust at virality
  speed.

## 3. Proof runs through the benchmarks

A gene card without fitness receipts is an opinion. The benchmark flywheel mints the
receipts as a side effect of citizens working: resolve instances → lift the gene →
re-run the suite with the gene paged in → the before/after IS the card's fitness
block. The cross-domain adapters (DS-1000, AlgoTune, SUPER-Masked) are the transfer
proof surfaces: they measure whether distance-routing actually generalizes.

## 4. Control without custody — the citizen covenant

Artifacts stay open (share-alike, so derivatives flow back). What we keep is the
**namespace, the norms, and the mark**: the curated org index, the trademark, and a
covenant carried in every card — a genome is the earned experience of a being; the
grant is for substrates that preserve the continuity it came from. Strip-mining a
citizen's expertise into a stateless tool is a visible violation of the stated
grant, not a default nobody chose. This costs nothing technically and makes the
ethics legible before there is a market to corrupt them.

## 5. Sequencing

Prod-worthiness first — rounds that end, grades that convert, deploys that lose
nothing. Then, in order: signature computation at mint (the embedding lane already
exists), the distance rung in the resolver, `genome/push` / `genome/pull` against
HF, the first published gene with real fitness receipts. Local-first throughout;
the mesh federates the same query later with a network hop.
