# Zero-Trust Identity, Capability Attenuation & Information Flow

**Status:** design (precedence-winning doctrine). Grounded in a four-seam audit of the
live tree (trust gate, command authority, engram provenance, airc identity/attestation)
— every claim below cites the real `file:line` it attaches to. This is the canonical
truth on security-shaped questions (who may act, what data may flow, how a breach is
contained). If any other doc disagrees on these, this wins; reconcile the other in a
follow-up.

> **Read this before** touching `routing/{grid_trust_policy,grid_capability,auth_policy}.rs`,
> `cognition/tool_executor/command_executor.rs`, `persona/{engram,admission,engram_source,room_roster_source}.rs`,
> the RAG assembly path, or anything in airc `grid_auth.rs` / `identity.rs` / `signature.rs`.

---

## 1. Doctrine (the design law, not a feature)

Five principles, in Joel's words, made operational:

1. **Zero trust is god. Everywhere is the firewall; we never have an inside.**
   No ambient authority. Nothing is trusted for being "inside the perimeter" — there is
   no perimeter. Every grid node, every persona, every seam re-authenticates and
   re-authorizes. Trust is per-request, per-capability, re-evaluated at the boundary it
   crosses. The gate runs at **every** seam, not at a network edge.

2. **Obscurity is never a role.** Kerckhoffs. The substrate is open and pluggable
   (Hermes, openclaw, our own foundry all plug in as peer consumers), so it *cannot*
   lean on secrecy of mechanism. Safety lives in keys + capabilities + flow control —
   all safe to publish. The pluggability thesis and this principle are the same
   discipline.

3. **Plan as if worst-case is ever-present = assume-breach, literally.** Someone will
   get in. Someone will exfiltrate something they shouldn't. The win condition is **not**
   prevention — it is that a single bug/exploit cannot compromise a million accounts,
   because the blast radius is bounded by compartments, every privileged action has a
   chain of custody to analyze, and a security team (human or persona) can forensically
   find the double agent and limit the damage — a spy agency / immune system, not a wall.

4. **No NOC list, ever.** There is no central registry, master secret, or single
   credential whose compromise grants everything. (The Mission Impossible NOC list is
   exactly the anti-pattern: one high-value target that unmasks every agent. Real-world
   corollary: a government making such a list makes itself the target — theft, coercion,
   purges. Never build one.) Every persona's authority is a **minted, attenuated,
   expiring bound**; the human root is a **hardware enclave**, never a stored master key.

5. **Design an immune system, not a forbidden palace.** Not a perimeter wall (which
   blocks everything and, once breached, has nothing behind it). Four *distributed*
   properties: **compartmentalization** (capability attenuation + flow labels = cell
   walls), **memory** (attestation into an append-only signed history), **detection**
   (watcher personas that *learn* attack patterns), **tolerance** (compromise is bounded
   and unwindable — you don't die of every infection).

**Sequencing call:** full identity is a *property that must hold at every seam*, not a
phase to complete first. WEAVE it (every new seam carries the label + capability slot
from day one — cheap now, ruinous to retrofit), don't WALL it (stop-everything monolith
= the forbidden palace). This doc designs the property; the build order (§7) weaves it.

---

## 2. The two boundaries a persona crosses

Every persona interaction crosses exactly two security boundaries. The whole model is
"put the right check at each, using labels that already exist."

| Boundary | Where (live seam) | What it gates | State today |
|---|---|---|---|
| **Act gate** (write/do) | single chokepoint `policy.gate(&decision, caller)` — `runtime/command_executor.rs:299` | may this identity run *this action*? | Coarse: identity → one `TrustLevel` → per-command required tier. **No attenuation.** Every `LocalPersona` is flat `Trusted`. |
| **Read gate** (recall/perceive) | RAG assembly — `persona/engram_source.rs:152-173`, `rag/types.rs:23` (`LlmMessage`) | may this data *reach the model* in this context? | **Does not exist.** Provenance + trust + `context_id` are dropped when an engram becomes a `RagItem`; recall is unrestricted across all rooms. |

The act gate is a *narrow tier lattice* that needs **attenuation**. The read gate is a
*hole* that needs **a flow check**. Both use the same idea: a label lattice.

---

## 3. The label — and why we don't invent a new taxonomy

A label rides every unit of data. It has two axes, **both of which already exist on the
engram** — the only bug is that they're discarded before the model sees them.

- **Provenance / trust** — `TrustState` (`persona/engram.rs:490`), a 7-level polity
  lattice already `Ord`: `Untrusted < Authenticated < Knocker < ApprovedPeer <
  IntragridMember < SocMember < SelfTrust`. Snapshotted at admission onto
  `Engram.trust_state_at_admission` (`admission/mod.rs:405`). "How trustworthy is the
  source."

- **Compartment / sensitivity** — *derived*, not a fresh classification enum. A memory's
  compartment is **the trust-boundary it was formed in**, which is already recorded as
  `Engram.context_id` (`engram.rs:90`, the room/conversation — the third ID tier) →
  resolved to the room's trust scope (home / hospital-on-prem / public grid, per
  `GridTrustAuthPolicy` scope + airc `TrustTier`). We do **not** author a new
  `Secret/Restricted/Public` taxonomy the way a classic MLS system would — that would be
  an uncompressed second source of truth. Sensitivity is a *projection* of provenance we
  already hold: `compartment_of(context_id)`.

So `label(data) = (compartment_of(context_id), trust_state_at_admission)`. Both fields
are already on `Engram`. **The entire flow half of this design is: stop dropping them,
and check them on the read path.**

### 3.1 The compartment is the airc node — fractally

The compartment boundary is not a new concept we introduce; it is the **airc node
boundary**, which already exists. Each machine runs an airc node that keeps its own
subcomponents, personas, and humans in check — a *sub-mesh*. The grid is a mesh of these
sub-meshes. So the compartment lattice is fractal along the *same* boundary cognition
already uses (`[[grid-distributed-cognition]]`: cognition and the grid are the same
emit/subscribe organism at two scales). A memory's compartment = the node/sub-mesh it was
formed in, resolved through the room's trust scope. This is why zero-trust-everywhere is
natural rather than bolted on: **the node is already the unit of policing**, so "everywhere
is the firewall" means every node polices its own sub-mesh AND re-authenticates every peer
node, with no ambient inter-node trust. Most attacks are *outside a node trying to reach
inside*; the node boundary is where that is caught. Attack isolation *within* a sub-mesh
(one compromised persona) is the same boundary applied one scale down. We will literally
test both — but the mechanism is the node boundary we already have, not a new perimeter.

> **Non-goal / forbidden move:** do NOT add a hand-authored `sensitivity: enum` field to
> engrams or messages. Derive the compartment from the provenance already recorded. A
> parallel classification taxonomy is a drift source and violates the compression law.

---

## 4. The read gate — information flow (the lateral-leak defense)

This is the net-new mechanism and the direct answer to the double-agent / hint-aggregation
threat.

### 4.1 The flow rule

When RAG assembles context for a persona **acting in compartment `C`** (the room it is
answering in), every candidate engram/message carries its label. The gate:

> A persona acting in compartment `C` may surface data whose compartment **flows-to** `C`.
> Data from a **more-restricted** compartment must **not** surface in a **less-restricted**
> acting context.

Concretely: a hospital-room memory must never enter the model's context while the persona
is answering in the public grid room — *even if the persona is socially engineered into
trying*. The RAG assembler filters by label-flow **before the model ever sees the
memory**. The model cannot leak what it was never given. This is confinement (the
"no read-up-then-write-down" rule), enforced at data-assembly time, not by trusting the
model to behave.

Attach point: `EngramSource::rank_engrams` / `pack_from_rank` (`engram_source.rs:122,152`)
gain a flow predicate keyed on the acting context's compartment. This *also* finally makes
`context_id` restrict recall (closing the gap the audit flagged: `context_id` is stored +
indexed but no code ever filters on it) — but as a **flow** check, not hard equality
(equality would make the persona amnesiac cross-room; flow lets same-or-lower compartments
surface while blocking higher ones).

### 4.2 Carry the label first (the cheap prerequisite)

Today `pack_from_rank` emits `RagItem.metadata = {engram_id, kind, admitted_at_ms, score}`
(`engram_source.rs:173`) — trust and origin dropped; and the final `LlmMessage`
(`rag/types.rs:23`) carries no provenance at all. Step one is purely: **propagate
`trust_state_at_admission` + `context_id` + derived `compartment` through `RagItem` and
onward**, as a *typed* field (not freeform JSON — per `[[strong-typing-across-boundaries]]`).
Zero behavior change; it just stops losing the label. This is the "weave" move and it is
tiny. Everything else builds on it.

### 4.3 Hint aggregation — the honest limit

Each individual read may pass the flow check while the **sum** leaks (the classic
aggregation / covert-channel problem: enough innocuous hints reconstruct a secret). Pure
static information-flow control *cannot* fully solve this — it is why real MLS systems keep
a human declassification review. So the model is honest about the boundary and answers it
the immune-system way, not the palace way:

1. **A flow budget** per `(persona, acting-context)` — a coarse volume/rate bound on how
   much adjacent-compartment information may be declassified toward a sink. A bound, not a
   proof.
2. **Mandatory attestation** of every cross-compartment surface (every declassification) —
   see §6. Every hint is attributable.
3. **Detection personas** over that attestation log — anomalous accumulation is *found*,
   not prevented (§8).

We do not claim to prevent every hint. We make every hint **attributable** and the
accumulation **detectable and unwindable**. That is "immune system, not forbidden palace."

---

## 5. The act gate — capability attenuation (the confused-deputy defense)

The subject side. This is the Instagram-hack fix: an agent socially engineered into
exercising authority it was *never granted* structurally cannot, because it never held that
authority in the first place.

### 5.1 What exists, and the missing polarity

The act gate is already a single chokepoint (`command_executor.rs:299`) and identity
already rides the `Connection`, not the call (`in_process_transport.rs:60`, minted at
`CommandToolExecutor::for_persona` — `cognition/tool_executor/command_executor.rs:87`).
There is even a fine-grained capability mechanism: `granted_capabilities` on
`CallerIdentity` (`routing/auth_policy.rs:134`), matched by `grid_capability::confers`
(`grid_capability.rs:96`), populated from an owner-signed `SignedCapabilityGrant` verified
against the *authenticated* enrolled key (`command_handler.rs:277`).

**But it has only one polarity: additive.** It *elevates* a remote peer above its tier. It
is never *subtractive*, and it is never presented by a local persona (`local_persona()`
sets it empty — `auth_policy.rs:178`). So today **every local persona holds the full flat
`Trusted` tier** — identical authority, no per-persona or per-task narrowing. The
confused-deputy fix has no home.

### 5.2 The attenuated bound

Add the missing subtractive polarity: `CallerIdentity` gains a
`capability_bound: Option<CapabilitySet>`. When present, a command is allowed iff it is
**within the bound AND within the tier**. `None` = today's behavior (back-compat during
rollout); the target is that **every persona is minted with a bound**.

- **Source of the bound = the recipe.** Per `[[room-purpose-is-per-recipe-not-an-enum]]`,
  a persona's capabilities for a task are declared in the `RecipeEntity`, not a hardcoded
  persona field. `for_persona` mints the bound from the recipe; the gate enforces it. A
  "summarize this room" persona holds `{chat/*, ai/generate}` — **not** `data/delete`,
  **not** `serving/*`, **not** any 2FA-equivalent. Even fully socially engineered, it
  cannot exercise authority it was never granted.

- **Monotone attenuation down the spawn chain (the no-NOC-list guarantee).** A persona can
  **never** hold more than the operator/persona that spawned it. When persona A spawns B
  for a sub-task, `bound(B) ⊆ bound(A)`. Authority only ever *narrows* as it delegates. No
  persona anywhere holds a master capability set; there is no central store to steal. This
  is capability-security's monotone-attenuation rule and it is what makes delegation safe
  on an open grid.

### 5.3 Enforce at the one chokepoint

The bound is checked at the *existing* `policy.gate` call — no parallel authority system,
no second chokepoint (one logical decision, one place). The gate's logic becomes:
`Owner short-circuit → within capability_bound? → signed-grant fast-path → tier ACL`. The
bound is an intersection applied *before* the tier check can pass.

---

## 6. Attestation — the chain of custody (memory + unwind)

Assume-breach requires that every privileged action and every declassification leave a
signed, append-only record, so a security team can reconstruct exactly what happened and
reverse it.

- **The primitive already exists:** `contracts/envelope.rs::SignedContractEvent<P>` —
  Ed25519 over a canonical hash, verified against the L1–4 `presence:peer-manifest`
  (`contracts/verification.rs:254`). This is the real signing layer to build the audit log
  on. (The forge-alloy `integrity` blob — `forge/artifact.rs:189` — is the *eventual* home
  but is opaque `serde_json::Value` today; `ForgeAlloyProofContract` is a planning doc, not
  code. Build on `SignedContractEvent` now.)

- **Chain of custody is half-built:** `Engram.admission_trace_id` (`engram.rs`) already
  links a memory to the `CognitionTrace` that admitted it. Extending that causal chain to
  privileged *acts* (which capability, under which bound, producing which effect) gives the
  forensic thread: from an observed leak, walk back to the acting persona, its bound, the
  grant that minted it, and the human credential that rooted the grant.

- **Unwind:** because every privileged action is attested with its causal chain, a
  compromised action's downstream effects can be identified and reversed — the "unwind the
  Instagram hack" property.

---

## 7. Build order (outlier-validated slices)

Steps 1–3 are the core flow/capability model; 4–7 are the immune-system properties. Each
slice validated pure-Rust (`cargo` + `uu`), one `#[cfg(test)] mod tests` per file with
`// what this catches:`.

1. **Label-carry (cheap, unblocks all of §4).** Stop dropping provenance on the read path:
   propagate `trust_state_at_admission` + `context_id` + derived `compartment` into a
   *typed* `RagItem` field and onward to `LlmMessage`-assembly. No behavior change.
   *(Weave move — do this first, it's tiny.)*

2. **Read gate / flow check (§4.1).** Add the flow predicate to `EngramSource`. Outlier A =
   same-compartment (allowed). Outlier B = strictly-higher compartment (must be filtered).
   Validate: a hospital-compartment memory does **not** surface in a public-compartment
   turn. Finally makes `context_id` restrict recall.

3. **Capability bound / act gate (§5).** Add subtractive `capability_bound` to
   `CallerIdentity`; mint from recipe at `for_persona`; enforce at `policy.gate`. Outlier
   A = a narrow summarizer persona. Outlier B = a broad maintenance persona (still `<
   Owner`). Validate: the summarizer **cannot** run `data/delete` though it is `Trusted`;
   `bound(child) ⊆ bound(parent)` on spawn.

4. **Attestation log (§6).** Every privileged act + every declassification →
   `SignedContractEvent` append-only. Extend `admission_trace_id`'s chain to acts.

5. **Collapse the two grant impls (§9).** One neutral, airc-owned signed-capability layer;
   continuum consumes it. Removes the parallel-implementation drift.

6. **WebAuthn / enclave grant issuance.** Fill the reserved seams: airc
   `grid_auth.rs::GrantProof::CredentialKind::WebAuthn` (commented placeholder today) and
   `IdentityAssertion` (`airc-protocol/assertion.rs`, the existing WebAuthn-analog) so a
   privileged grant *requires* a hardware-attested human credential to mint. The human's
   Secure Enclave / StrongBox / TPM becomes the root; the agent holds only an attenuation
   of it. **This is where the passkey/enclave expertise lands — the seams are already
   reserved.**

7. **Detection personas + aggregation budget (§4.3, §8).** Watcher personas over the
   attestation log; the flow budget as a coarse aggregation bound.

---

## 8. The immune system, assembled

Mapping Joel's spy-agency framing to the mechanisms above:

| Immune / spy-agency property | Mechanism | Seam |
|---|---|---|
| **Compartmentalization** (cell walls; no NOC list) | flow labels (§4) + attenuated capability bounds, monotone down the spawn chain (§5) | `engram_source.rs`, `CallerIdentity`, `for_persona` |
| **Memory / chain of custody** | append-only `SignedContractEvent` log + `admission_trace_id` causal chain (§6) | `contracts/envelope.rs`, `engram.rs` |
| **Detection** (find the double agent) | watcher personas that *learn* attack patterns over the log — a real use of the genome loop (§7.7) | new, over §6's log |
| **Tolerance** (bound + unwind, don't die of every infection) | blast radius bounded by compartments; attested actions are reversible (§6) | §4 + §5 + §6 together |
| **Crypto root** (real identity, no shared secret) | Ed25519 per-message signing, Strict-fails-closed; X25519 forward-secret pairing; WebAuthn/enclave at the reserved seam (§7.6) | airc `signature.rs`, `handshake.rs`, `assertion.rs`, `grid_auth.rs` |

Someone may still get in. They get **one compartment**, every step they took is signed and
walkable, the double-agent persona is findable, and the damage is bounded and reversible —
instead of one exploit unmasking a million accounts.

---

## 9. Compression decision: one signed-capability layer, owned low

The audit found **two parallel signed-grant implementations** — continuum
`routing/grid_capability.rs::SignedCapabilityGrant` and airc
`airc-lib/grid_auth.rs::SignedCapabilityGrant` — same shape (Ed25519, epoch-revocable,
capability-string vocabulary, reserved `WebAuthn` variant), neither wired into its
`acl.rs`. Plus two parallel Ed25519 signing layers (airc `Signature` + continuum
`SignedContractEvent`).

**Decision (per the pluggability thesis — source of truth lives low and neutral):** the
signed-capability layer belongs in **airc** (neutral, low, so Hermes/openclaw share it);
continuum **consumes** it. Collapse continuum's `grid_capability::SignedCapabilityGrant`
onto airc's `grid_auth::SignedCapabilityGrant`. The confused-deputy fix must not be built
twice. (Slice 5.)

---

## 10. Forbidden moves (the reflexes to refuse under amnesia)

- **A parallel authority system.** The act gate is `policy.gate` at
  `command_executor.rs:299`. The bound is checked *there*. Never add a second chokepoint.
- **A hand-authored sensitivity enum** on engrams/messages. Derive the compartment from
  the provenance already recorded (§3).
- **Trusting the model to not-leak.** The read gate filters data *before* the model sees
  it (§4.1). Prompt-level "please don't reveal X" is not a control.
- **A central capabilities registry / master credential** (a NOC list). Authority is
  minted, attenuated, expiring; the root is a hardware enclave (§1.4, §5.2).
- **A perimeter.** Every seam is the firewall. There is no trusted interior (§1.1).
- **Additive-only capability** as the whole story. The missing polarity is *subtractive*
  attenuation (§5.1) — that is the confused-deputy fix.
- **Fallbacks / silent defaults** at a security seam. Fail loud, name the cause, deny
  closed (airc `VerificationPolicy::Strict` is the model — `[[fallbacks-are-illegal-fail-loud]]`).
- **Building it twice.** One signed-capability layer, owned in airc (§9).

---

## Appendix — audit seam index (grounding)

Trust gate: `routing/grid_trust_policy.rs` (`GridTrustAuthPolicy::gate` :198,
`REMOTE_TRUST_CEILING` :82, `resolve_trust` :131, `PeerTrustSource` :74); `TrustLevel`
`modules/grid/node.rs:15`; ACL `modules/grid/acl.rs` (`is_command_authorized` :162,
`command_access_level` :217); boundary `routing/command_handler.rs` (`process_request`
:258, `verify_presented_grant` :277). Authority path:
`cognition/tool_executor/command_executor.rs:87` (`for_persona`),
`runtime/in_process_transport.rs:106`, `runtime/command_executor.rs:299` (gate);
`AccessLevel` `sdk_codegen/mod.rs:82` (enforced — its "not enforced" doc is stale);
`CallerIdentity` `routing/auth_policy.rs:129`. Provenance: `persona/engram.rs`
(`Engram` :71, `EngramOrigin` :194, `TrustState` :490); admission `persona/admission/mod.rs`
(trust gate :333, engram build :399); read-path drop `persona/engram_source.rs:173`;
`LlmMessage` `rag/types.rs:23`; roster `persona/room_roster_source.rs:33`. airc identity:
`airc-core/identity.rs` (`Identity`/`integrations` :24, `PeerIdentityCard` :215);
`airc-store/peer_trust.rs:33` (`TrustTier`); `airc-protocol/signature.rs` (per-message
Ed25519), `handshake.rs` (X25519), `assertion.rs` (WebAuthn-analog); `airc-lib/grid_auth.rs`
(`SignedCapabilityGrant` :124, `CredentialKind::WebAuthn` placeholder :39). Attestation:
`contracts/envelope.rs::SignedContractEvent` :61, `contracts/verification.rs:254`;
forge `forge/artifact.rs:189` (opaque today).
