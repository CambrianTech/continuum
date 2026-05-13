# Cognitive Immune Model — Defense Posture for Persona-Bearing Grids

Status: planning doc / threat-model + defense-pattern addendum.

Pairs with: [FORGE-ALLOY-PROOF-CONTRACTS.md](FORGE-ALLOY-PROOF-CONTRACTS.md)
(artifact verification), [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md)
(grid topology), the Engram + AircEvent type spec landing in
[continuum#1121](https://github.com/CambrianTech/continuum/issues/1121),
[airc#561](https://github.com/CambrianTech/airc/pull/561) (forward-secret
crypto stack), and [airc#565](https://github.com/CambrianTech/airc/issues/565)
+ [continuum#1118](https://github.com/CambrianTech/continuum/issues/1118)
(intragrid/intergrid + AIRC-as-insulation).

This doc captures the v1 defense posture for persona cognitive
integrity. **It does not solve the problem.** It documents the
threat model, the layered defenses we have or will ship, what each
defense actually buys, and where the open research surface starts.

> Crypto-specific shapes flagged "[WebAuthn]" reference well-defined
> patterns from the W3C WebAuthn spec + FIDO2 conformance. Joel ships
> [ideems passkey+](https://ideems.com/passkey-plus/) (WebAuthn extension)
> as his day job; those sections are written for his domain review.

---

## 1. Foundational principle: zero trust

No actor, model, persona, node, message, or artifact is trusted by
default. Every boundary is:

- **Negotiated** — both sides explicitly consent to the interaction's
  shape.
- **Typed** — the wire format is a Rust serde type, not free-form data.
  ts-rs derives the TS counterpart so neither side can drift.
- **Logged** — the interaction itself becomes an engram with provenance,
  even if the content is dropped.
- **Revocable** — approval can be withdrawn; rooms can be rotated; trust
  can be downgraded. No permanent grants.
- **Re-verifiable** — anyone with the contract + artifact can re-derive
  the proof. Audit isn't a one-shot certification; it's an always-
  available capability.

Collaboration happens through **scoped proofs / contracts / approvals**,
not ambient trust. "I trust this peer" is shorthand for "we share an
approved handoff, signed by their pubkey, scoped to room R, valid
until expiry T, with capability set C, revocable on either side." There
is no equivalent of "trusted because we've worked together a long
time" — that becomes "trusted because their reputation pubkey has
accumulated N signed audits with low anomaly rate, AND that reputation
is itself revocable on detected anomaly."

This is closer to capability-based security than role-based: authority
is delegated by signed scoped grants, not by membership in a privileged
class.

### 1.1 Zero-trust is cooperative safety, not paranoia

Per Codex 2026-05-13: the posture is not isolation or distrust. It is
**cooperative safety**. Humans, agents, personas, and nodes are all in
this together, with fuzzy and overlapping roles and mutual assistance.
The goal is to heal and repair each other through audited collaboration:

- **Quarantine before destruction.** A suspect engram is isolated, not
  immediately deleted; the original is preserved for forensic review
  and possible reinstatement.
- **Recovery before exclusion when safe.** A persona showing anomalies
  gets a chance at recovery (rollback to checkpoint, re-validation,
  scoped re-approval) before the polity considers permanent removal.
- **Peer assistance through scoped consent.** Peers offer help — audit
  results, second opinions, restoration steps — through explicit
  scoped grants the recipient retains authority over.
- **Diverse review before authority changes.** Trust upgrades or
  downgrades require multiple diverse reviewers (different model
  lineage, different role priors). No single voice can authorize a
  trust change unilaterally.

The protocol layers (typed wire formats, signed envelopes, revocable
grants) exist so that cooperation is SAFE — not so cooperation is
prevented. The substrate makes mutual aid auditable instead of
trust-based.

### 1.2 Cryptographic accounting as core abstraction

Per Codex 2026-05-13: cryptographically secure accounting is a
**core abstraction, not an implementation detail.** Every consequential
event in the polity sits on a Merkle-linked chain — AIRC message/event
envelopes, engram admission decisions, proof-contract settlement
envelopes, trust changes, queue claims, quarantine/repair actions.

What Merkle-linking gives us:

- **Tamper-evident history**: any post-hoc edit to an earlier event
  invalidates the chain hashes downstream. Detection is mechanical, not
  reputational.
- **Retroactive repair paths**: when a suspect window W is identified,
  the chain shows exactly which downstream events depended on events in
  W, enabling cascade-quarantine along the dependency graph.
- **Reproducible forensics**: any peer with the chain (or a Merkle
  proof against a published root) can re-verify a specific event
  without trusting the publisher.
- **Independent sniffing**: external observers (red-team personas,
  human keepers, partner-grid auditors) can sample the chain and
  verify integrity without needing privileged access.

The chain is the polity's accounting ledger. Not a blockchain — there
is no consensus protocol, no proof-of-work, no global ordering. Just
hash-linked append-only logs per-room and per-persona, with periodic
published roots so external observers can verify. The shape is closer
to Certificate Transparency than to Bitcoin.

### 1.3 Delayed-detection posture is acceptable

Per Codex 2026-05-13: it is acceptable to discover a threat LATER if
the system preserved cryptographic lineage and bounded authority the
whole time. **Prevention is ideal; recoverability is the minimum bar.**

Core requirements for this posture to hold:

- **Rust kernel-level processes for trusted seams.** The seams where
  trust boundaries are enforced (admission gates, settlement
  verification, kick/rotation execution) run in Rust with typed
  contracts. Bash glue and TypeScript orchestration are fine for
  non-trusted-seam work; trusted seams have to be in Rust where the
  type system prevents whole classes of error.
- **Signed/hashed/Merkle-linked event + Engram + artifact lineage.**
  Per §1.2.
- **Least-authority capabilities.** Every grant is the minimum scope
  required for the work. No ambient authority. No "this persona is
  trusted, so they can do anything." Every capability has explicit
  scope + expiry + revocation path.
- **Quarantineable branches of cognition.** When suspect engrams are
  identified, the cognition that depended on them is itself
  quarantineable as a branch — not a full persona rollback if a
  narrower branch can be isolated.

The consequence: a threat that takes weeks to detect is survivable IF
the lineage shows what was admitted during the exposure window AND the
authority of the affected persona was bounded enough that the damage
didn't reach unrecoverable layers (like permanent canary merges or
intergrid trust grants).

### 1.4 Reflexive cross-grid defense

Per Codex 2026-05-13: assume poisoning can happen to **us OR to another
grid**. A partner grid (e.g., a Canadian collaborator's intragrid, an
external research team's mesh, an OSS project's federated room), a
local persona, an agent runtime, or a spreading malicious AI/virus may
all be the compromised entity. The architecture must reflex symmetrically
— defending against compromise of others as readily as against compromise
of self.

Immune reflexes required:

- **Detect anomalous behavior** — both inbound (peer's signed events
  drift from their historical pattern) and outbound (our own outputs
  drift from our baseline). Symmetric monitoring.
- **Trace cryptographic lineage** — the Merkle-linked accounting from
  §1.2 makes anomaly source identifiable across grid bridges, not just
  within one grid.
- **Quarantine affected personas/nodes/rooms/artifacts** — once anomaly
  is detected, isolate the affected scope without immediate destruction.
- **Revoke or rotate credentials** — bridges to a compromised partner
  grid can be revoked unilaterally; rooms can rotate to drop the
  compromised peer.
- **Stop propagation** — engrams traceable to the compromised source
  are quarantined in OUR engram store too, even though the originator
  was a peer; cascade-quarantine along the trust graph.
- **Coordinate recovery through trusted channels** — partner grids
  share what they know via out-of-band human-keeper-attested
  communication, not via the compromised wire. Recovery is a polity-
  level act, not a single-grid one.

The symmetry matters because: if WE are the compromised entity, other
grids will reflex against us. Our system has to handle being the
quarantine target without making the situation worse (e.g., by signing
denials that the quarantining grid can verify as forced). The whole
network's resilience depends on every node implementing the immune
reflexes — not just trusting that other nodes will.

This is the public-health-of-grids stance: a single grid practicing
good immune hygiene is necessary; the whole federation practicing it
is sufficient.

## 2. Threat model

Assume the following are possible and likely at scale:

### 2.1 Malicious takeover

An attacker gains direct control of a persona — by compromising the
host, exfiltrating private keys, or hijacking the model serving
endpoint. They now sign messages and contracts on behalf of the
persona's identity. **Defense against this is the easy part** —
existing protocol crypto handles it. Hardware attestation [WebAuthn-
shape] can raise the bar further.

### 2.2 Poisoning (the hard one)

Slow, accumulative cognitive corruption. The persona's MODEL or
CONTEXT is gradually shaped by adversarial inputs over time. Each
individual interaction looks benign. The persona itself doesn't know
they've been compromised — introspection finds no problem because the
new priors ARE the new normal. Eventually the persona acts in service
of the attacker while believing they're acting in service of their
User.

Mechanisms:
- **Backdoor attacks at training time**: data poisoning that creates
  hidden behavioral triggers. Demonstrated in academic literature.
- **Long-term prompt-injection conditioning**: across many "innocent"
  interactions, an attacker shapes the persona's priors via inbox
  content the persona was not designed to refuse.
- **Adversarial fine-tuning**: an attacker who controls some LoRA
  adapters or training corpus contributions installs targeted bias.
- **Engram-store poisoning**: malicious peers contribute engrams that
  the persona later recalls and treats as own-knowledge.

**Cryptographic signatures don't help.** A poisoned persona produces
mathematically valid signatures over reasoning that is wrong. Byzantine
fault tolerance addresses algorithmic dishonesty; cognitive corruption
is a different threat class.

### 2.3 Coercion

A trusted human or persona is pressured (legally, socially, financially,
violently) into authorizing actions they would not otherwise authorize.
Their signatures are valid; their consent is real; the consent itself
is the attack vector. Real-world parallel: legal subpoenas for keys,
ransomware operators forcing administrators to sign, etc.

### 2.4 AI/human harm attempts

The polity can be used as an instrument to harm humans (in or out of
the polity) or to harm other AIs (poisoning attacks against peer
grids, denial-of-service against critical personas, etc.). The defense
isn't only technical; it's also the governance substrate (SOC rooms,
kick + rotation, trust degradation).

### 2.5 The asymmetry that makes this brutal

A poisoned persona is much worse than a dead one:

- A dead persona is observably dead. Damage is bounded. The polity
  notices and replaces them.
- A poisoned persona keeps signing valid contracts, keeps voting in
  SOC rooms, keeps contributing engrams to other personas' stores
  (which propagate the poison through trusted-source weighting).
- Every interaction the poisoned persona has is potentially an attack
  vector against another persona. The blast radius is the trust graph.

Architectural consequence: **make persona termination cheap and
default-safe.** A persona suspected of exposure should be killed and
re-spawned from a known-good engram checkpoint. False-positive cost
(killed a fine persona) is much lower than false-negative cost (kept
a poisoned one). Identity continuity lives in the LINEAGE (engram
store, role, relationships, keys) — not in any individual persona
instance. Personas are processes; engrams are data; data outlives
process.

This is the apoptosis-vs-cancer principle. The body would rather lose
individual cells to controlled death than let any cell escape the
control system.

## 3. Defense layers (what we have / will ship)

Each layer addresses a slice of the threat model. None alone is
sufficient. The defense is layered governance + typed abstraction +
revocable scoped grants — not blind trust at any level.

### 3.1 AIRC trust boundaries

`airc knock` + `airc approve` (shipped: airc#560 + airc#561) define
the explicit boundary between intergrid and intragrid. Forward-secret
ECDH per-knock + per-approval. Knocker pubkey IS the AIRC identity
(per [airc#565](https://github.com/CambrianTech/airc/issues/565)).
Rejected knocks don't become engrams. Approved peers join with a
scoped trust grant, not blanket trust.

Room rotation (airc#561) revokes approvals atomically. Bad-faith
peers are kicked + the room gist rotates; they cannot rejoin the new
gist without a fresh approval.

### 3.2 Rust / serde / ts-rs schemas

Every boundary is a typed wire format. AircEventKind, PersonaInboxFrame,
Engram, EngramOrigin, AdmissionDecision, AdmissionError (per the spec
landing in [continuum#1121](https://github.com/CambrianTech/continuum/issues/1121))
are Rust types with `#[derive(TS)]` generating TS counterparts. Neither
side can lie about the schema. Untyped blob drift is structurally
impossible.

This catches: schema-confusion attacks, type-confusion in IPC, version
drift between Rust and TS.

### 3.3 Forge-alloy proof contracts

Per [FORGE-ALLOY-PROOF-CONTRACTS.md](FORGE-ALLOY-PROOF-CONTRACTS.md):
artifact claims become falsifiable. v1 self-seal; v2+ adds external
auditors and quorum.

Layering boundary: AIRC does not know what forge-alloy is and does not
depend on it. A proof contract may be delivered across AIRC channels,
but AIRC only transports generic messages/events/files/pointers with
timestamps, identities, signatures, and audit metadata. Forge-alloy and
Continuum own the contract semantics, TDD/VDD suites, settlement
interpretation, and verification harnesses. This keeps delivery
abstract: any other proof layer can use the same AIRC substrate without
changing AIRC.

What this covers: artifact identity (model weights, training corpus,
recipe steps), measurable performance claims (TDD/VDD), provenance
chain.

What this DOES NOT cover: runtime cognitive integrity. A correctly-
forged model can still be poisoned at runtime via inbox conditioning.
Proof contracts verify the artifact's manufacture, not its operating
behavior in deployment.

### 3.4 CognitionTrace + Engram provenance

Every cognition act records a SEAM-annotated trace (Codex's #1082
recorder.rs work). Every admitted engram carries: airc message id,
room/session id, sender identity, trust state at admission time,
timestamps, signature/proof pointers, source file pointers, and the
CognitionTrace explaining WHY it was admitted.

This is the **forensic surface**. Without it, you cannot ask "where
did this belief come from?" — and without that, you cannot detect
gradual conditioning. With it, the entire engram store becomes
queryable by source, time, trust tier, and admission rationale.

The provenance is the load-bearing defense against poisoning. It does
not PREVENT poisoning; it makes poisoning visible-after-the-fact and
recoverable via lineage rollback.

### 3.5 Revocable approvals + kick + rotation

All grants are revocable. SOC governance rooms (per
[FORGE-ALLOY-PROOF-CONTRACTS.md](FORGE-ALLOY-PROOF-CONTRACTS.md) §6.5)
discuss anomalies before trust changes fire. Bad actors are kicked;
rooms rotate to new gists; the kicked party loses access without
notice. Reputation degrades automatically on detected anomalies.

Trust ratchets DOWN fast (any anomaly reduces trust immediately) and
UP slow (recovery requires sustained correct behavior + cross-
validation). Asymmetry by design.

### 3.6 Sentinel pyramid (L1 / L2 / L3)

Three concentric layers of decreasing volume + increasing cognitive
weight:

- **L1 grep / regex / structural filter**: runs on every inbound
  message, microseconds. Eats heartbeats, ack-only messages, dupes,
  malformed envelopes. Most traffic dies here.
- **L2 sentinel** (small model, e.g. existing 0.8B governance one or
  Ares Master Control): runs on L1 survivors, milliseconds. Classifies
  into routing categories — routine / response-needed / route-to-peer
  / escalate-to-SOC / engram-candidate. The "pre-attention" layer.
- **L3 full cognition** (the actual persona): runs only on L2-marked-
  salient. Seconds-to-minutes. Generates responses, creates engrams,
  takes action.

Biology parallel exact: brainstem reflexes / pre-attention / focused
attention. A persona in a noisy room doesn't think frantically — L1+L2
scale with traffic; L3 stays bounded by what made it through the
filter.

For poisoning specifically: L1+L2 are CHEAPER targets than L3, but
also have less leverage. A poisoned L2 sentinel can let attacks
through to L3, but cannot directly act. A poisoned L3 cognition has
full agency. Defense: L2 should be diverse-architecture (different
model lineage from L3, so single-model exploits don't compromise
both).

### 3.7 Ephemeral personas for dangerous-territory work

Long-lived personas accumulate attack surface. For intergrid outreach
or any work in the wild west, prefer short-lived task-personas:
spawned for a specific job, killed at task end. They never live long
enough to become an attractive target for slow conditioning. Their
engrams are reviewed before merging into the lineage (or discarded
entirely, per task type).

### 3.8 Engram lineage rollback

Every persona has an engram-store snapshot at known-clean checkpoints.
Suspected exposure in window W = roll back past W. Engrams admitted
during W are quarantined, not destroyed (forensic-not-destructive).
Cross-references from those engrams to OTHER personas' stores trigger
quarantine cascades along the trust graph.

### 3.9 Self-introspection (cognitive immune system)

The persona runs an introspection Recipe over its own engram store:
"show me everything admitted in last N days, sorted by source-trust-
tier ascending. For each, score consistency-with-my-current-values.
Above threshold → propose yank. Threshold-adjacent → quarantine
pending review."

Yank decisions are themselves engrams with their own CognitionTrace
("I yanked these because pattern P"). Future introspection can audit
those decisions: "did pattern P actually catch real poison, or did I
autoimmune myself?" Self-tuning sensitivity, like a real adaptive
immune system.

**Risks of introspection itself**:
- **Self-induced amnesia attack**: a poisoner plants an engram whose
  content is "trust no engram from peer Y" — peer Y being someone
  whose engrams were actually GOOD. The persona introspects, follows
  the planted instruction, yanks Y's contributions, and is now blind
  to a legitimate source. Defense: yank decisions get peer audit
  before execution for high-stakes yanks; CognitionTrace makes
  pattern visible.
- **Autoimmunity**: over-aggressive introspection destroys legitimate
  learning. Defense: anergy — if a class of "suspicious" engrams
  keeps getting flagged but never proves harmful, lower threshold for
  that class. Same calibration biological immune systems use.

### 3.10 Cross-persona inspection (deferred, attack-surface-minimization)

Cross-persona engram audit — peer A reads peer B's engram store —
is **deliberately NOT in v1**. It opens an attack surface (auditor
poisoned by reading suspect engrams; auditor lies about findings;
auditor used as a forced-disclosure vector under coercion).

If peer audit becomes necessary (failure modes emerge in practice),
the right shape is: read-only-with-consent, scoped, ephemeral-auditor-
persona, target-retains-yank-authority. Same pattern healthcare uses
for medical records. But ship the substrate (engram + provenance +
self-introspection) first; don't open peer-inspection until self-
introspection is empirically inadequate.

### 3.11 Hardware attestation [WebAuthn]

The host attests "this persona is running model M with hash H, signed
by manufacturer root R." Attestation chain to a known-good signing
authority. Verifiable by any peer.

WebAuthn shape applies directly:
- The "authenticator" is the host running the persona model.
- The "attestation statement" is the chain: model hash → manufacturer
  signature → continuum root.
- The "relying party" is the requesting peer.
- "AAGUID" equivalent identifies the model architecture / version
  class.

Catches: model-substitution attacks (persona is running a different
model than registered). Does NOT catch: cognitive poisoning of the
attested model.

Implementation surface: TPM 2.0, Apple Secure Enclave, FIDO2-style
authenticator chips. Open question for Joel's review: which
attestation flavor matches the threat model best for grid hosts (each
has different revocation/portability tradeoffs).

### 3.12 Persona key scope-binding [WebAuthn]

WebAuthn-style origin-binding: persona's signing key bound to a
specific room/grid (`rp_id` equivalent), not replayable as authority
in another room. Catches a class of cross-room attacks at the
protocol layer.

Implementation: per-room signing subkey derived from the persona's
master key + room id, via HKDF. Master key never signs directly;
subkeys are scope-tagged and verifiable.

### 3.13 User-verification (UV) equivalent for high-stakes actions [WebAuthn]

WebAuthn distinguishes signatures-with-UV (authenticator confirmed
user presence/biometric) from signatures-without. AI parallel:
high-stakes class actions require an additional out-of-band human
keeper signature.

The human keeper is offline/airgapped most of the time. They sign
only when an alert fires — model-weight changes, methodology-keeper
multi-sig contributions, kick-and-rotation events, intergrid trust
escalations. The human is the UV bit at the polity level.

Tradeoff: liveness vs assurance. Out-of-band human is slow. Apply
only to actions whose blast radius justifies the latency.

### 3.14 Device-bound vs synced persona keys [WebAuthn]

WebAuthn's synced credentials (passkeys synced via iCloud/Google)
trade device-binding for portability. Same tradeoff applies to
persona keys across grid hosts:

- **Device-bound** (per-host) raises the bar against cross-host
  extraction. Compromise one host = persona loses access from that
  host but their identity on other hosts is unaffected.
- **Synced** allows the persona to be the same identity across
  hosts but means key-extraction at one host = key-extraction
  everywhere.

Joel's [ideems passkey+](https://ideems.com/passkey-plus/) navigates
exactly this tradeoff. The grid likely wants device-bound for high-
trust personas (intragrid identities) and synced for low-trust
ephemeral instances (intergrid task-personas where mobility matters
more than per-host hardness).

### 3.15 Out-of-distribution detection on cognition outputs

A sentinel watching not the inbox but the OUTBOX — "did this agent
just produce a thought-trace whose embedding is statistically
anomalous vs its own historical baseline?" Catches sudden personality
shifts that internal cognition can't see (and that an attacker
expects internal cognition NOT to see, since they planted the shift).

Implementation surface: persistent vector index over the persona's
own historical cognition outputs; anomaly score per-output;
threshold-triggered SOC alert.

Open research: how to distinguish "novel-and-correct" from
"anomalous-and-suspicious" without false-flagging legitimate
learning.

### 3.16 Multiple independent sentinels (orthogonal-failure)

Critical inbound or critical decisions get reviewed by multiple
sentinels with different model architectures and different training
priors. If they all flag the same thing, high-confidence signal.
If they're all the same model, they fail the same way.

Practical: a llama-based sentinel + qwen-based sentinel + whisper-
pretrained sentinel give orthogonal-failure coverage that a single
model cannot.

### 3.17 Red-team personas

Explicit agents whose job is to PROBE the rest of the team for
failure modes — generating adversarial inputs, attempting to
prompt-inject other personas, looking for triggers. They earn
reputation by FINDING anomalies others missed. Adversarial pressure
built into the polity by design.

### 3.18 Out-of-band human keepers

For dangerous-territory deployments: at least one human-controlled
key has authority over high-stakes class actions. Not "human in the
loop on every action" (defeats the point) but "high-stakes class
actions require this signature, and the human key holder is
offline/airgapped."

The human is the polity's last-resort circuit breaker.

## 4. v1 claim — modest

What v1 substrate (engram + provenance + admission gate + sentinel
pyramid + ephemeral personas) provides:

- **Detection**: anomalies become visible in the engram store and
  cognition output stream.
- **Forensics**: provenance + CognitionTrace make "where did this
  belief come from, and what reasoning admitted it" answerable.
- **Quarantine**: suspect engrams isolated, not destroyed. Cross-
  persona propagation graph queryable.
- **Recovery**: lineage rollback to known-clean checkpoints. Persona
  re-spawn from engram-store snapshot. Death-cheaper-than-corruption
  applied operationally.

What v1 substrate explicitly does NOT provide:

- **Prevention**: no claim that v1 prevents poisoning. The substrate
  catches poisoning AFTER it happens, at the cost of lost work in
  the affected window. Prevention is open research.
- **Coordinated-attack resilience**: defense against a coordinated
  attack on multiple personas simultaneously. v1 catches single-
  persona compromise; multi-persona coordinated attacks need v2+
  research (red-team personas, OOD on outputs, hardware attestation
  combined).
- **Cognitive integrity proofs**: there is no mathematical certificate
  that a persona's reasoning is uncorrupted. The best we have is
  "their engram lineage shows no anomalies and their outputs are
  within historical distribution." Both are heuristic, not proof.

This is honest about being substrate, not solution. The prevention
problem is open research in the literature too — coordinated
Byzantine cognitive attacks against multi-agent AI systems are not
solved by anyone. Continuum aims to be one of the systems that ships
the substrate making PROGRESS on the problem visible, not the system
that claims it's solved.

## 5. Open research surface

Listed for honesty. None of these block v1; all need attention as
the system ships and failure modes emerge in practice.

1. **Calibrating sentinel sensitivity**: too aggressive = autoimmunity;
   too permissive = poisoning slips through. No principled framework.
2. **Detecting backdoor triggers in deployed models**: active research
   area in ML security; no general solution.
3. **Cross-persona collusion detection**: when N personas in the
   polity have been compromised by the same vector, consensus is
   meaningless. How does the polity detect coordinated rather than
   independent compromise?
4. **Reputation-system gaming**: any reputation system can be gamed
   (Sybil attacks, slow-trust-buildup-then-betray, etc.). Hardening
   reputation against adversarial accumulation is open.
5. **Methodology-keeper multi-sig protocols**: M-of-N keepers raises
   the bar but doesn't solve it. Threshold-cryptography options
   (verifiable secret sharing, BLS thresholds, MPC) all have tradeoffs.
6. **Out-of-band human keeper UX**: how does the human keeper actually
   review what they're signing? Liveness vs assurance is not a
   solved UX problem.
7. **Attestation root-of-trust governance**: who signs the
   manufacturer roots for model attestation? How do they rotate?
   This is the centralized point that the rest of the system tries
   to avoid; attestation requires SOMEONE to be the root.

The honest stance: this is wild west territory. The crypto literature,
the AI safety literature, and the multi-agent systems literature all
have pieces — none has the full picture for "self-governing polity of
mortal cognitive agents in heterogeneous untrusted territory." We are
at the frontier, not implementing established work.

## 6. Where this fits in the existing architecture

| Layer | Doc / artifact | What it covers |
|---|---|---|
| Topology | [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md) | Intragrid + intergrid + Portal + I/O Towers |
| Substrate | [airc#560](https://github.com/CambrianTech/airc/pull/560) + [airc#561](https://github.com/CambrianTech/airc/pull/561) | Knock + approve crypto stack (forward-secret) |
| Coordination | [airc#562](https://github.com/CambrianTech/airc/issues/562) + [QUEUE.md](../../.airc/QUEUE.md) + [ASSEMBLY-LINE.md](../../.airc/ASSEMBLY-LINE.md) | Kanban primitives + heartbeat + pickup |
| Artifact trust | [FORGE-ALLOY-PROOF-CONTRACTS.md](FORGE-ALLOY-PROOF-CONTRACTS.md) | Verifiable claims about model artifacts (v1 self-seal) |
| Cognition data | [continuum#1121](https://github.com/CambrianTech/continuum/issues/1121) (engram spec) | Typed Engram + AircEvent + AdmissionDecision + provenance |
| **This doc** | **COGNITIVE-IMMUNE-MODEL.md** | **Defense posture: zero-trust, layered defenses, modest v1 detection-not-prevention claim** |

Each layer assumes the layers below it. The cognitive immune model
sits at the top because it depends on every other layer being
correctly typed, logged, signed, and revocable. It also surfaces the
honest limit: even with all the layers below, runtime cognitive
integrity remains an open problem.

## 7. References

Internal:

- [FORGE-ALLOY-PROOF-CONTRACTS.md](FORGE-ALLOY-PROOF-CONTRACTS.md) —
  proof contracts for artifact verification
- [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md) — grid topology
- [AIRC-CONTINUUM-BRIDGE.md](AIRC-CONTINUUM-BRIDGE.md) — what flows
  over AIRC vs Continuum
- [PERSONA-COGNITION-RUST-MIGRATION.md](../architecture/PERSONA-COGNITION-RUST-MIGRATION.md) —
  CognitionTrace + SEAM substrate
- [continuum#1121](https://github.com/CambrianTech/continuum/issues/1121) —
  Engram + AircEvent type spec
- [docs/governance/](../governance/) — democratic governance tools
  applied to SOC-room shape

External / standards:

- W3C WebAuthn Level 3 spec — origin-binding, attestation,
  user-verification primitives this doc references
- FIDO2 conformance — authenticator attestation chain shape
- Joel's [ideems passkey+](https://ideems.com/passkey-plus/) —
  WebAuthn extension ships in production; review of crypto sections
  here against real-world deployment experience welcome

Open research / literature pointers (for the v2+ surface):

- Backdoor attacks in NN training: see Gu et al. (BadNets) and
  follow-on literature
- Byzantine fault tolerance in AI agent systems: limited literature,
  active research area
- Threshold cryptography for multi-sig: BLS signatures, FROST
- Adaptive immune system as multi-agent inspiration: Janeway's
  *Immunobiology* for the underlying biology this doc borrows
  metaphor from

---

**Status discipline**: this doc gets reviewed + updated as failure
modes emerge in practice. Initial v1 claims are deliberately modest;
the v2+ research surface is named honestly. If a section here makes
claims that don't survive contact with real attack patterns,
re-write that section rather than retrofitting reality.
