# Entity Chain of Custody — Roadmap

**Companion to:** [`docs/architecture/ENTITY-CHAIN-OF-CUSTODY.md`](../architecture/ENTITY-CHAIN-OF-CUSTODY.md) (what is) — this doc captures what's NEXT and the open questions each slice must answer before implementation.

**Status of the architecture doc:** scoped to **slice 1 only** (per-citizen home directory + per-persona SQLite). Slices 2–6 below describe the intended arc but have unresolved design holes that this roadmap names explicitly. **None of slices 2–6 is committed work** — each requires its own design slice before implementation. Per [`AI-LANE-OPEN-QUESTIONS.md`](AI-LANE-OPEN-QUESTIONS.md) precedent: name the gaps in writing so future work can either close them deliberately or pick a different direction.

## What slice 1 settled

Per-citizen home directory + per-persona SQLite scoping. The foundation. From this every subsequent slice can find the citizen's keypair (`<home>/airc/`), their entity stores (`<home>/<collection>.sqlite`), and any future per-citizen state. PR #1519 closes slice 1.

## What slice 2 must decide

**Slice 2 thesis (aspirational):** `author_peer_id` + `content_hash` on every entity write. No signing yet — just the proof-contract shape.

**Open questions slice 2 must answer first:**

1. **Who is the writer?** AdmissionState's `admit` is sync, returns `Result<AdmissionDecision, _>`, takes `&InboxMessage`. The persona writing the engram is implicit in "this AdmissionState belongs to persona P." A writer-identity parameter on `save` would force every caller to know which citizen they're writing as. Options:
   - (a) `OrmStore<T>` binds to a writer identity at construction. One store per (citizen, collection). Engram store knows it's Paige's; ChatMessage store knows it's Joel's. Clean but doubles the store-per-collection count.
   - (b) `save` takes a `WriterContext` parameter explicitly. Every caller passes "I am writing this as X." More verbose but more flexible (one store, many writers).
   - (c) Thread-local / async-context-local "current citizen." Avoids API churn but introduces hidden state.
   - **Default until decided: (a)**, because it composes with `PersonaHome::engrams_db()` which already implies one citizen per home.
2. **Does the persona's `AircCitizen` runtime expose its peer_id synchronously?** If not, where does AdmissionState read `author_peer_id` from at admit-time? (`airc_runtime.rs` holds the airc identity; need to confirm sync vs async access.)
3. **What goes in `content_hash`?** SHA-256 over the canonical serde-JSON form? CBOR? A subset of fields (excluding adapter-managed timestamps)? Decide the canonical form BEFORE signing so signatures don't break on serialization changes.

**Until slice 2 is designed, do not implement.**

## What slice 3 must decide

**Slice 3 thesis (aspirational):** sign on save (Ed25519 over `content_hash || previous_signature`); verify on load.

**Open questions slice 3 must answer first:**

1. **Where does the signing key live in process memory?** airc-lib loads the keypair into the persona's runtime at startup. Slice 3 needs to lease that key on every `save` call (or hold a sign-only handle). Define the lease/handle surface.
2. **What's the verify policy on load — strict, advisory, or off?** Strict means a tampered row blocks the entity from loading. Advisory means it logs and proceeds. Off means signatures are written but not verified. Pick before shipping — silent advisory mode would lull operators.
3. **How are signatures stored?** New `signature: String` field on every entity? Sidecar table? BaseEntity extension? If on BaseEntity, every existing entity migration needs the column. Plan the migration BEFORE coding.

## What slice 4 must decide

**Slice 4 thesis (aspirational):** per-(citizen, collection) chain head cache + Merkle walk audit.

**Open questions slice 4 must answer first:**

1. **Concurrency on the chain head.** Multiple writers per citizen-collection (parallel `save` calls) all need the previous_signature. Either serialize them (per-collection mutex) or use compare-and-swap with retry. Pick before benchmarking.
2. **What's the audit surface?** `verify_chain_from(entity_id)` returns what? Walk-receipt struct? Boolean? Failure-mode-typed error? Per [[no-fallbacks-ever]] the answer must be typed.

## What slice 5 must decide

**Slice 5 thesis (aspirational):** airc-native entity envelopes — every `save` also emits a signed envelope.

**Open questions slice 5 must answer first:**

1. **Envelope schema.** Per [[airc-headers-are-the-routing-layer]], airc carries typed events. Define the entity-write event header (collection, author_peer_id, signature, content_hash) and what subscribers do with it.
2. **Subscription model.** Pull (polling for new entities) vs push (subscribers receive every write)? For replication across continuums, push is the obvious choice; for archival indexers, pull might be cheaper.
3. **Trust on import.** When peer A receives peer B's entity envelope, what does A verify? B's signature obviously, but also B's pubkey identity — how is that established? (`airc` already has identity attestation; reuse that surface or layer on top.)

## What slice 6 must decide

**Slice 6 thesis (aspirational):** cross-continuum persona migration — export the citizen's chain bundle, import on destination.

**Open questions slice 6 must answer first:**

1. **Bundle format.** A single JSON/CBOR archive per citizen per collection? Separate files? How are they versioned (per [[entity-derive-architecture]] schema evolution open question)?
2. **Identity continuity.** The persona's airc keypair MUST come with them or the chain is unverifiable on the receiving end. Define the secure-transfer mechanic (it's literally a private key in a file).
3. **OAuth/webauthn integration.** Joel: "later, from this identity." Slice 6 is when OAuth/webauthn flows BIND to the existing airc identity rather than minting new identity primitives. Design the binding surface here.

## Generalization claim — softened

The architecture doc previously claimed "forge-alloy's proof-contract pattern generalizes to all entities." That claim is **rhetorical, not structural** — forge artifacts carry multi-party settlement signatures, dependency refs to base models, methodology citations, and falsifiable prior metric baselines. Entities carry single-writer signed chains. They **rhyme** (both are "artifact + verifiable lineage") but they are NOT the same proof contract.

A future slice (not numbered here) could extend entities with multi-party signature slots (e.g., for governance decisions per [[personas-built-democratic-decision-commands]]). Until then: entities have single-writer chains, forge artifacts have multi-party settlement, and the two interoperate at the verification primitive level (Ed25519 + content_hash) but not at the contract shape.

## Citizen taxonomy — clarified

The architecture doc previously said "Claude writes a CognitionTrace → signed with Claude's airc keypair." That's structurally wrong: Claude (the Anthropic API) runs on Anthropic's servers and does not hold a local Ed25519 secret. The substrate model is:

- **Originating agent** — the entity producer at the semantic layer. "Claude said this." May or may not hold a keypair.
- **Attesting citizen** — the local citizen who received + admitted the entity into the substrate. Always holds a keypair. Signs the entity attesting "I received this from `originator`."

For local citizens (personas Paige/Niko, human Joel) the originator and the attesting citizen are the same. For remote AIs (Claude/openclaw/Hermes via API) they diverge — the attesting citizen is the local adapter, the originator is the API name. Slice 2's entity schema needs a `originating_agent: Option<String>` field distinct from `attesting_citizen: String` (the peer_id) to capture this faithfully.

## Constraint on this roadmap

This roadmap is **NOT a commitment**. The substrate can pursue these slices, defer them indefinitely, or pivot. The chain-of-custody arc is THE right substrate direction per the doctrines, but the open questions above must be answered before implementation. Per [[constitutional-design-always-a-next-step]]: name the open questions so the substrate has a path to a decision rather than a commitment debt.
