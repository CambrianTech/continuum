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
