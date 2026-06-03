# Entity Chain of Custody — Scope: Slice 1

**Status:** Slice 1 (per-citizen home + per-persona SQLite) **landed in PR #1519**. Slices 2–6 are **not yet committed work** — see [`docs/planning/ENTITY-CHAIN-OF-CUSTODY-ROADMAP.md`](../planning/ENTITY-CHAIN-OF-CUSTODY-ROADMAP.md) for the design intent + open questions each subsequent slice must answer.
**Doctrine:** [[orm-everything-not-hand-edited-files]], [[personas-are-citizens-airc-is-identity-provider]], [[persona-identity-derives-from-source-id]], [[airc-headers-are-the-routing-layer]], [[continuum-thesis-airc-is-the-medium]].
**Companion:** [ENTITY-DERIVE-ARCHITECTURE.md](ENTITY-DERIVE-ARCHITECTURE.md) (the schema layer this builds on).

## Why this doc exists (and what it scopes to)

Joel 2026-06-03 described a vision in which every substrate entity is signed by its writer, linked into a Merkle chain, portable across the grid via airc, and authored by a uniform "citizen" abstraction (persona / human / external AI). That vision is *correct direction* but multi-slice; this PR delivers **only the foundation slice**. Everything beyond that is intentionally moved to the planning doc so the substrate isn't enshrining commitments it hasn't designed.

The architecture doc describes what IS built; the planning doc describes what's NEXT with open questions named.

## What slice 1 settled

Per-citizen home directory + per-persona SQLite scoping. Every citizen (today: personas; tomorrow: humans + external AIs) has a directory on disk:

```
<continuum_root>/personas/<agent_name>/
    airc/              ← airc keypair (managed by airc-lib)
    seed.json          ← PersonaIdentityProvider's seed
    engrams.sqlite     ← OrmStore<Engram> + OrmStore<EngramRecallMetadata>
```

From this single rooted path the substrate finds:
- The citizen's identity (airc Ed25519 keypair in `airc/`).
- The citizen's entity stores (per-collection SQLite files in the home root).
- Any future per-citizen state (signing-key derivation, Merkle chain head, per-collection metadata).

`PersonaHome::for_persona(continuum_root, agent_name)` is the typed surface; `AdmissionState::for_persona(home, recall_metadata)` is the one-call entry point that opens the engram store, wires the production `OrmPersistenceSink`, rehydrates state from disk, and returns the configured AdmissionState.

The per-citizen scoping is what makes everything beyond it possible — without separate homes, signing keys + chain heads + persona-specific entity stores have nowhere to live. **This is the only commitment in this PR.**

## Citizen taxonomy

The substrate distinguishes two roles when an entity gets written:

- **Originating agent** — the semantic producer. "Claude said this." May or may not hold a local keypair.
- **Attesting citizen** — the local citizen who received + admitted the entity into the substrate. Always holds a keypair. Signs the entity attesting "I received this from `originator`."

For local citizens (personas Paige/Niko, human Joel running the substrate on his own machine) the originator and the attesting citizen are the same. For remote AIs accessed via API (Claude, openclaw, Hermes) they diverge — the attesting citizen is the local adapter that received the response; the originator is the API agent that produced it. The local adapter has a real keypair and signs the attestation; the originator name is recorded but not cryptographically attested.

**This distinction is design intent for slice 2** (when entities gain `author_peer_id` + `content_hash`). It is not implemented today. The planning doc names it explicitly so slice 2's entity-schema work captures it.

## Relationship to forge-alloy

[FORGE-ALLOY-SPEC.md](FORGE-ALLOY-SPEC.md) defines a *proof pattern*: an artifact carries a content hash, signature(s), dependency references, methodology citations. The chain-of-custody work generalizes the **pattern** (artifact + verifiable lineage), not the specific contract — forge artifacts carry multi-party settlement signatures, dependency refs to base models, methodology citations, and falsifiable prior metric baselines; entities under slice 2+ would carry single-writer signed chains. They **rhyme** but are not the same proof contract.

A future trait `ProofContract` (not yet written) might capture the shared shape — `content_hash() / signature() / previous()` — so verifying code generalizes across both. That's a slice-2+ design decision.

## Reference

- [`ENTITY-CHAIN-OF-CUSTODY-ROADMAP.md`](../planning/ENTITY-CHAIN-OF-CUSTODY-ROADMAP.md) — what's NEXT (slices 2–6) + open questions
- [`ENTITY-DERIVE-ARCHITECTURE.md`](ENTITY-DERIVE-ARCHITECTURE.md) — the entity layer this builds on
- [`FORGE-ALLOY-SPEC.md`](FORGE-ALLOY-SPEC.md) — the proof-pattern this generalizes
- [`PERSONA-COGNITION-PIPELINE.md`](PERSONA-COGNITION-PIPELINE.md) — the cognition cycle that produces / consumes entities
