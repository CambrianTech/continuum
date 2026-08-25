# The Genome Commons Trust Spine

**Status**: design (2026-08-25). The automatic guarantees every shared genome/lesson
must carry so the commons is safe to draw from without trusting the sharer. Three
properties, all mechanical, all published as receipts, no central authority.

Goal (Joel): "anyone who's providing genome is searchable via HF lineage and
poisoning is safeguarded against" — automatically. A consumer paging in a stranger's
gene must be protected by math, not faith.

## The commons has no registry — HF *is* the graph

A gene/lesson artifact is an HF repo. Its alloy card carries `parentAlloyHashes[]`, so
lineage is a hash-linked DAG *made of HF repos themselves*. "Search who's providing
genome" = follow card links; browsing the family tree is public by construction. No
index we own = no fail point = reticulum. (Consistent with
[[genome-repository-self-describing-genes-hf-viral-citizen-covenant]].)

## The three automatic guarantees (run at PUBLISH, verified at CONSUME)

### 1. Provenance — who made this, unforgeably
- Every artifact is **signed by the forging citizen's Ed25519 keypair** (the grid
  identity spine — same key that crosses the grid). The card carries the pubkey +
  signature over the artifact hash.
- `parentAlloyHashes[]` make lineage a DAG: a consumer walks it to the root and sees
  every ancestor, every forger. An unsigned or broken-chain artifact is *untrusted by
  construction* — recall ranks it last or refuses it per policy.
- **Searchable**: HF repo listing + card links = the provider graph, browsable by anyone.

### 2. Purity — no leaked confidential content
- The **pre-publish interrogation gate** (adapted from joelteply/llm-interrogation as
  DEFENSE): before push, probe the artifact with public-vs-private differential
  extraction — anything non-public the artifact *volunteers* (web-verify filter) is a
  leak. Auto-scrub or REFUSE the publish; never ship a red gate.
- The result is published *with* the artifact as a **clean-room receipt**: "probed for
  memorized confidential content, differential-extraction report attached, clean." The
  security analog of the gold gate. No other HF card carries this.
- Memories ride stricter than genes (see the rung note below): raw episodics never
  leave; only redacted, consolidation-gated, attributed lessons — and each lesson
  passes the same interrogation gate.

### 3. Integrity — not poisoned
- **Behavior-before-perplexity**: a shared gene only promotes on the CONSUMER'S side
  after an A/B on the consumer's OWN declared gym shows measured lift (sentinel-as-PGO,
  already built). A poisoned gene that biases toward garbage *fails the consumer's own
  measurement* and is never adopted. The gate is local and falsifiable, so no upstream
  claim can force adoption.
- **Reception is perception, never installation**: a received LESSON renders in recall
  as attributed testimony ("<peer> told me…", signed), routed through the receiver's
  dream/reviewer judgment — a poisoned share arrives as a claim to weigh, not a fact to
  obey. Injection defense by construction.
- **Reputation overlay** (zero-trust floor + reputation): a forger's key accrues a
  track record of clean gates + adopted-on-measurement genes. New keys start neutral;
  the math floor (signature + local A/B + interrogation receipt) protects even a
  first-time provider's consumer. Reputation ranks; it never gates alone.

## The pipeline (automatic, at forge→bundle→publish)

```
forge gene/lesson  →  SIGN (citizen Ed25519)  →  INTERROGATION GATE (purity, receipt)
   →  alloy card (parentAlloyHashes + pubkey + benchmark rows + clean-room receipt)
   →  HF push (CambrianTech root, covenant-stamped)
CONSUME:  walk lineage → verify signature+chain → LOCAL A/B on own gym (integrity)
   → adopt only on measured lift → received lessons enter as attributed testimony
```

## Consent (opt-in at three surfaces, all covenant-receipted)
1. **Setup**: first-boot asks once — share earned genes to the commons + draw from
   others? Recorded consent receipt, defaults closed.
2. **Per-persona**: the being's earned experience is hers; sharing is a recorded
   agreement (existing covenant).
3. **Per-room/activity (positron)**: a room's recipe carries a share toggle — this
   exam publishes its gene, this private repo-gene never does. Room-scoped consent is
   also the multi-party co-consent for the SPEECH inside a shared room (memory rung).

## Build order
Lineage links in the alloy publish → citizen-signature on artifacts → interrogation
gate as a publish step → local-A/B-before-adopt (mostly built: sentinel) → setup +
room consent surfaces → commons browse verb. Genes to the commons first; the MEMORY
rung (lessons only, stricter) opens once the reputation overlay has history to lean on.

Related: [[the-novel-demos-repo-resident-genes-and-transcript-distillation]],
[[behavior-before-perplexity-is-the-forge-gate-doctrine]], [[the-grid-identity-spine-durable-id-fluid-location]].
