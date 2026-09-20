# Grid Capability Auth — the signed-grant identity+trust unification

> **Premise** (Joel, 2026-06-21): *"Unification is everything. Do it right."* — and: *"a security layer built in, contracted and cryptographically secured/signed, making grid actually work, remote inference, inference and training for sale."*

This is the design that **unifies identity, trust, and the grid economy into one mechanism**: a cryptographically **signed capability grant**. A peer doesn't *assert* who it is or what it may do — it **presents a grant the owner signed**, and the executing node **verifies** it. Identity (who), authorization (what), and contract (paid/expiring) are the same signed object. This replaces the flat "every airc caller → Provisional ceiling" gate and the dead-end tier-lookup (the grid `NodeRegistry` is address-keyed, a different identity space — see `[[airc-grid-identity-unification-trust-bridge]]`).

It is a **joint continuum + airc** design. airc already provides the primitives (`airc_lib::grid_auth`, public, at the pinned rev); continuum provides the verifying gate. Read `routing/grid_trust_policy.rs` (the gate seam) and `COMMAND-ORGANIZATION.md` (the command surface this gates) first.

---

## 1. The model in one object

```
SignedCapabilityGrant = CapabilityGrant + owner's signature
  CapabilityGrant { grantee, grantee_pubkey, capabilities: ["ai/generate", …],
                    granted_in: mesh, issued_at, expires_at, epoch }
```

- **Identity:** the grant binds `grantee` (peer_id) **AND `grantee_pubkey`** — a stolen peer_id can't ride someone else's grant (`KeyMismatch`). The presenting peer's verified key must equal the bound key.
- **Authorization:** `capabilities` are the **same vocabulary as command names/namespaces** (`"ai/generate"`, `"genome/train"`, `"data/list"`) — no parallel namespace. `grant.grants(command)` is the check.
- **Contract / economy:** `expires_at` + `epoch` make a grant a *time-bounded, revocable contract*. A paid grant for `ai/generate` is a `CapabilityGrant{capabilities:["ai/generate"], expires_at: …}` the owner signs. Revocation = a higher-`epoch` grant with empty `capabilities` (no separate revocation channel).
- **Trust lattice (membership):** `SignedMeshMembership{ default_tier: TrustTier, … }` confers a *tier* to a same-account peer (`OwnAccount`/`Friend`/…), which maps to a continuum `TrustLevel` → the existing ACL gate. Membership = "you're one of mine, here's your baseline"; capability grant = "here's a specific (possibly paid) delegation beyond your tier."

Two grant kinds, one verifier: **membership → tier → ACL** (broad baseline) and **capability grant → `grants(command)`** (specific/paid).

## 2. airc primitives (have — `airc_lib::grid_auth`, public)

`CapabilityGrant`, `SignedCapabilityGrant`, `MeshMembershipAttestation`, `SignedMeshMembership`, `GrantProof`, `GrantVerifier` (trait; ed25519 now, WebAuthn later), `GrantVerdict { Valid, UntrustedIssuer, BadSignature, KeyMismatch, WrongMesh, Expired, EncodingFailed }`, `VerifyContext { now_ms, presenting_pubkey, own_mesh, trusted_issuer_pubkey, … }`, `.verify(ctx) -> GrantVerdict` (stateless: issuer-pin → sig → key-binding → mesh → expiry), `.grants(capability) -> bool`. Anti-replay (`epoch`) is **consumer-side** state by design — the verifier is stateless.

`TrustTier { OwnMachine > OwnAccount > Friend > Untrusted }` (note: no `Blocked` — `Untrusted` is the floor).

## 3. Continuum gate integration

The gate (`GridTrustAuthPolicy`) keeps its shape; the *trust resolution* becomes grant-verification:

```
dispatch(command, params, caller)
  caller == Local/None        → Owner (the operator on the box; unchanged)
  caller == Airc(peer, grant) →
     verdict = grant.verify(VerifyContext{ now, presenting_pubkey=peer.key,
                                            own_mesh, trusted_issuer_pubkey=owner_key })
     if verdict != Valid                  → Forbidden(verdict)        // typed reason → audit
     if epoch <= max_seen[grantee]        → Forbidden(ReplayedGrant)  // consumer-side anti-replay
     capability grant: grant.grants(command) ? Allowed : Forbidden
     membership:       tier→TrustLevel; is_command_authorized(command, level) ? Allowed : Forbidden
```

- The continuum `PeerTrustSource`/`resolve_trust` seam stays for the **membership→tier→ACL** path (tier from a verified `SignedMeshMembership`). The **capability** path adds a direct `grants(command)` check.
- **Owner stays local-only:** a grant can confer up to its capabilities/tier, but `Owner`-gated commands (`data/delete`, `grid/trust`) are never delegated by a remote grant — same `REMOTE_TRUST_CEILING` principle (a grant can reach `Trusted`-level + named capabilities, not blanket Owner).
- **Composition** already propagates `ctx.caller`; the grant-verdict (or resolved tier) rides with it so a composed call is gated by the same contract — no escalation.

## 4. Why this IS the identity unification

The two-identity-space problem (address-keyed grid vs peer-keyed airc) **dissolves**: authorization no longer depends on looking a peer up in *any* registry. The peer **presents a signed grant bound to its key**; the gate verifies it against the owner's key and the local clock/mesh. No shared trust store to drift, no address↔peer mapping. Identity is the key; trust is the signature; authorization is the capability list. One object, verified at the boundary — exactly "authenticated I × authorized C."

## 5. Issuance & transport (the joint build)

- **Issuance (owner-side):** `grid/trust` (today: sets an address-keyed tier) becomes/gains a grant issuer — the owner's key signs a `CapabilityGrant`/`MeshMembership` for a peer. A paid grant is issued on purchase (capabilities + expiry). The owner's signing key is the `trusted_issuer_pubkey` every verifier pins.
- **Transport:** the `SignedCapabilityGrant` rides the airc command envelope (a header) OR is presented once at enrollment and cached by the consumer. (airc-side: define the envelope header / presentation path.)
- **Anti-replay store (continuum):** persist `max_epoch_per_grantee`; reject lower-epoch grants. A small `DashMap<PeerId,u64>` + disk.
- **`trusted_issuer_pubkey` source (continuum):** the local owner/account key (from the airc identity / config). The verifier pins it.

## 6. Phasing

1. **Verify membership → tier (graduate same-account peers).** Continuum verifies `SignedMeshMembership`, maps `default_tier`→`TrustLevel`, feeds the existing ACL gate. First real unification: a same-account peer gets `Trusted`, not the flat Provisional. (Needs membership on the envelope + owner key.)
2. **Capability grants (external / paid).** Verify `SignedCapabilityGrant`, authorize iff `grants(command)`. Enables cross-account assistants + the first **paid** capability (`ai/generate` for sale). Add the consumer-side epoch anti-replay store.
3. **Economy.** Issuance UX (buy a grant → owner signs capabilities+expiry), metering (`command:completed` per gated capability = the bill), revocation (higher-epoch empty grant), `genome/train` for sale.

## 7. Cross-repo split

| Side | Work |
|---|---|
| **airc** (have) | `grid_auth` primitives — done, public. |
| **airc** (todo) | grant transport on the command envelope (header) / presentation path; issuance helper (owner signs a grant). |
| **continuum** (have) | the gate seam (`GridTrustAuthPolicy`/`resolve_trust`/cap), `CallerIdentity` from verified envelope, the command ACL + capability vocabulary. |
| **continuum** (todo) | extract grant from envelope in `CommandRequestHandler`; verify via `grid_auth` in the gate; `TrustTier→TrustLevel` map; consumer epoch store; `grants(command)` check; tests + adversarial review. |

## 8. Open questions

- Envelope header name + size for the grant; or enrollment-time presentation + cache (avoids per-request grant on the wire).
- `MeshIdentity` derivation continuum-side (own_mesh for `WrongMesh`).
- Capability vocabulary: exact match (`"ai/generate"`) vs prefix (`"ai/generate*"`) — align with the ACL's prefix rules.
- Issuance UX + where paid-grant signing lives (owner node only).

## 9. See also
- `AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md` (§5 the gate slices), `../architecture/COMMAND-ORGANIZATION.md` (the gated command surface), `routing/grid_trust_policy.rs` (the seam), `airc_lib::grid_auth` (the primitives).
- Memory: `[[airc-grid-identity-unification-trust-bridge]]`, `[[command-infra-self-routing-schema-adapters]]`, `[[continuum-grid-vision]]`, `[[lora-layers-as-p2p-exchanged-genome]]`.
