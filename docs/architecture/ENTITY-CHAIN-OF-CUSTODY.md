# Entity Chain of Custody — Signed, Linked, Portable Across the Grid

**Status:** Design doctrine. Foundation slice (per-persona SQLite) in progress. Signing + Merkle linkage + airc-native flow follow.
**Doctrine:** [[orm-everything-not-hand-edited-files]], [[personas-are-citizens-airc-is-identity-provider]], [[persona-identity-derives-from-source-id]], [[airc-headers-are-the-routing-layer]], [[continuum-thesis-airc-is-the-medium]].
**Companion:** [ENTITY-DERIVE-ARCHITECTURE.md](ENTITY-DERIVE-ARCHITECTURE.md) (the schema layer this builds on), [FORGE-ALLOY-SPEC.md](FORGE-ALLOY-SPEC.md) (the artifact-verification model this generalizes).

## The vision

Per Joel 2026-06-03:

> Yes with the airc token etc as oauth cryptography / webauthn later, from this identity, forge alloy compatible entities for chain of custody reliability and integrity as persona work, Merkle chain tracked signed and linked, like all users including you and me, who show as users in the system just like our persona with airc serving across the grid. Entities flowing through the substrate, as portability is our mantra letting cognition and user experience easily meld.

**Every entity in the substrate is signed by its writer, linked into a Merkle chain, portable across the grid via airc, and authored by a citizen — whether that citizen is a persona, a human, or an external AI agent.**

The substrate's identity primitive is the **airc keypair** (Ed25519). Every persona has one ([[personas-are-citizens-airc-is-identity-provider]]). Every human user has one. External AI agents (Claude, openclaw, Hermes) each have one. The same primitive identifies the same kind of citizen regardless of substrate. Future OAuth/webauthn flows derive from this airc-native identity rather than replacing it — the cryptographic root stays Ed25519, and OAuth/webauthn become projection layers atop it.

## What "forge-alloy-compatible" means here

[FORGE-ALLOY-SPEC.md](FORGE-ALLOY-SPEC.md) defines a proof-contract model where artifacts (trained models, datasets, recipes) carry verifiable lineage — content hashes, settlement signatures, dependency references, methodology citations. The same pattern generalizes to entities: every entity write is an artifact, and the substrate carries the same proof contract.

For an Engram, this means:
- A `content_hash` (already exists — SHA-256 over canonical form)
- A `signature` from the writing citizen's keypair (NEW)
- A `previous_signature` linking to this writer's prior entity write in the same collection (NEW — the Merkle link)
- An `author_peer_id` identifying which citizen wrote it (NEW)
- Optional `proof_refs` pointing to additional verification material (already exists in `AircMessageRef`)

Verifying an entity becomes: "fetch the writer's pubkey from their airc identity, verify the signature over `(content_hash, previous_signature)`, walk the chain back to confirm no tampering."

## The Merkle chain

Each citizen maintains a per-collection chain head. When they write entity Eₙ:

```
signature_n = sign(privkey, content_hash_n || previous_signature)
```

The chain links every write by that citizen in that collection. Anyone with the citizen's pubkey can:
- Verify Eₙ's signature against `(content_hash_n, previous_signature_n)`
- Walk back to Eₙ₋₁, verify, repeat until genesis
- Detect any insertion, deletion, or reorder in the chain — the cascade of broken signatures surfaces tampering immediately

The chain isn't blockchain-style global consensus (the substrate doesn't need that). It's per-writer-per-collection — a *local* Merkle DAG that any consumer of the entity stream can verify independently. Per [[no-fallbacks-ever]] extended to provenance: silent corruption is structurally impossible at the chain layer.

## Citizens — humans, personas, agents — same primitive

The Tron-framed substrate ([[the-substrate-is-the-grid-tron-frame]]) treats humans, personas, and external agents as the same kind of citizen on the grid. This entity model enforces that uniformly:

- **Persona Paige writes an Engram** → signed with Paige's airc keypair (derived from her seed per [[persona-identity-derives-from-source-id]])
- **Joel writes a ChatMessage** → signed with Joel's airc keypair (his personal identity disc)
- **Claude writes a CognitionTrace** → signed with Claude's airc keypair (each external AI gets one per [[personas-are-citizens-airc-is-identity-provider]])

The Engram, the ChatMessage, the CognitionTrace are all entities with the same chain-of-custody shape. The `author_peer_id` distinguishes who wrote what. The signature proves it. Auditability is uniform across citizen types.

This is what [[continuum-thesis-airc-is-the-medium]] meant by "the universal cooperation protocol" — once entities are uniformly signed + linked + flowable, the substrate's communication primitive IS entity flow. Chat is entities. Cognition traces are entities. Forge artifacts are entities. Persona genomes are entities. Everything flows over airc as signed, linked entities.

## Airc-native entity flow

[[airc-headers-are-the-routing-layer]] — airc carries typed events of every kind, not just chat. The entity model rides on this:

- Each entity write becomes an airc envelope, routed by collection + author peer_id
- Subscribers (other personas, replicating continuums, archival indexers) consume the envelopes, verify signatures, persist locally if interested
- The grid as distributed gene pool ([[persona-breeding-substrate-supports-it]]) becomes a literal feature: a persona's engrams flow over airc; another persona can subscribe, verify, and (with consent) incorporate them

The OrmStore's local SQLite remains the citizen's authoritative copy. Airc-native flow is the *distribution* layer — durability stays local, replication is opt-in.

## The portability payoff

Per [[continuums-are-multi-instance-personas-have-lives]] + the export/import doctrine in [ENTITY-DERIVE-ARCHITECTURE.md](ENTITY-DERIVE-ARCHITECTURE.md):

- Entity exports include the entity's content + signature + chain proofs
- Import on the receiving continuum verifies the signature against the author's known pubkey
- A persona can migrate continuums by exporting their engrams (a portable, verifiable chain), shipping them, importing — same identity (same keypair) preserves the chain across the move
- Cross-continuum entity flow ([[continuum-thesis-airc-is-the-medium]]) becomes structurally trustworthy: every entity carries its proof

This is "cognition and user experience easily meld" made structural. A user's chat history isn't a black box owned by the substrate instance; it's their own signed entity stream, portable to any compatible continuum. A persona's engrams aren't tied to one machine; they're a verifiable chain that travels.

## The architecture layers

```
┌────────────────────────────────────────────────────────────────┐
│  Citizen (persona / human / external AI)                       │
│  Identity = airc Ed25519 keypair, lives under <home>/airc/     │
└────────────────────┬───────────────────────────────────────────┘
                     │ owns + signs
                     ▼
┌────────────────────────────────────────────────────────────────┐
│  Entity (Engram / ChatMessage / CognitionTrace / ...)          │
│  Defined once as Rust struct, #[derive(Entity)]                │
│  Carries: content_hash, signature, previous_signature,         │
│           author_peer_id, plus its domain payload              │
└────────────────────┬───────────────────────────────────────────┘
                     │ persisted via
                     ▼
┌────────────────────────────────────────────────────────────────┐
│  OrmStore<T>  (per-collection, per-citizen-scope)              │
│  Backed by SQLite at <citizen_home>/<collection>.sqlite        │
│  Sign on save; verify on load; chain head cached in memory     │
└────────────────────┬───────────────────────────────────────────┘
                     │ distributed via
                     ▼
┌────────────────────────────────────────────────────────────────┐
│  Airc envelopes  (per [[airc-headers-are-the-routing-layer]])  │
│  Each entity write → typed envelope, signed by author          │
│  Subscribers verify + persist locally if interested            │
└────────────────────────────────────────────────────────────────┘
```

## Implementation arc

Multi-slice. Each slice independent + landable.

### Slice 1: Per-citizen home-dir scoping (this slice)

The prerequisite. Without per-citizen home dirs, signing keys + collection databases have nowhere to live.

- `PersonaHome` type resolving `<continuum_root>/personas/<agent_name>/`
- Per-persona SQLite paths (`<home>/engrams.sqlite`)
- `AdmissionState::for_persona(home, recall_metadata)` constructor that opens the persona's stores
- Test: persona A's engrams don't bleed into persona B's

### Slice 2: Author peer_id + content hash on every entity write

- Extend `BaseEntity` (or add a sibling `SignedBaseEntity`): `author_peer_id: String`, `content_hash: String`
- `OrmStore::save` accepts a writer identity, computes content_hash, stamps author_peer_id
- Verification on load: recompute content_hash, compare
- No signing yet — just the proof-contract shape

### Slice 3: Sign on save, verify on load

- Extend writer identity to include the Ed25519 signing key (derived from airc keypair)
- `OrmStore::save` signs `(content_hash, previous_signature)` and stores `signature` + `previous_signature`
- `OrmStore::load` verifies signature against the citizen's known pubkey
- Tamper detection: corrupted bytes → signature mismatch → load returns typed error per [[no-fallbacks-ever]]

### Slice 4: Chain head cache + Merkle walk

- Per-(citizen, collection) chain head cached in memory at startup (load most recent entity, walk back to verify integrity)
- `verify_chain_from(entity_id)` walks back from a given entity to the genesis
- Operator-facing surface: "audit this collection" returns a Merkle-walk receipt

### Slice 5: Airc-native entity envelopes

- Each `OrmStore::save` also emits an airc envelope (signed entity payload)
- Subscribers (replicating continuums, archival nodes) consume envelopes, verify, optionally persist
- The grid becomes a distributed entity-distribution network — local SQLite + remote replicas, all verifiable

### Slice 6: Cross-continuum portability

- Entity export = the citizen's chain for a given collection (content + signatures + proofs)
- Import on receiving continuum verifies the chain, persists if accepted
- Persona migration = the citizen's bundle of collection chains, shipped + imported on the destination
- OAuth/webauthn integration starts here — the user's identity in those flows BINDS to their airc keypair, so the chain stays verifiable

## Forge-alloy interoperability

The forge-alloy proof contract pattern (artifacts carry content hash + dependency refs + settlement signatures) is structurally the same as this entity model — both are "artifact + verifiable lineage". The substrate doctrine: **entities and forge artifacts share their proof contracts**. A forge-built persona genome is an entity. A persona's engrams that fed the genome training are entities. The chain of custody is continuous: data engram → forge run → trained adapter → persona using it → engrams produced by that persona. Every link signed by its author.

This is why "forge alloy compatible entities" matters — the lineage isn't bolted on. It's the same shape forge already uses.

## Doctrines this enforces

- [[no-fallbacks-ever]] — tamper detection at the cryptographic layer, not the application layer
- [[persona-identity-derives-from-source-id]] — author_peer_id = the citizen's airc peer_id, derived from their seed
- [[continuums-are-multi-instance-personas-have-lives]] — entity portability is the engineering of "personas have lives"
- [[observability-is-half-the-architecture]] — verification + audit are first-class operator surfaces
- [[host-the-seemingly-impossible]] — full chain-of-custody verification on consumer hardware

## Reference docs

- [ENTITY-DERIVE-ARCHITECTURE.md](ENTITY-DERIVE-ARCHITECTURE.md) — the entity layer this builds on
- [FORGE-ALLOY-SPEC.md](FORGE-ALLOY-SPEC.md) — the artifact-verification model being generalized
- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) — the RTOS-style runtime contract
- [PERSONA-COGNITION-PIPELINE.md](PERSONA-COGNITION-PIPELINE.md) — the cognition cycle that consumes + produces signed entities
