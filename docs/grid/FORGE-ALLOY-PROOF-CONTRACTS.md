# Forge-Alloy Proof Contracts — Grid Trust Layer

Status: planning doc / addendum to the grid architecture.
Pairs with: airc#565 (intragrid/intergrid + AIRC as insulation/security layer), continuum#1118 (terminology), continuum#1116 (grid pilot), and the existing
[FORGE-ALLOY-SPEC.md](../architecture/FORGE-ALLOY-SPEC.md) artifact schema.

This document captures the **proof-contract layer** that turns forge-alloy
work from "I did training and it works" into "anyone can mechanically
verify the artifact meets a falsifiable contract."

The starting point is intentionally permissive: a persona writes a
contract, executes the work, signs the proof bundle themselves, and
publishes. No quorum, no separate auditor, no methodology-keeper
multi-sig. Stricter trust shapes are the trajectory, not the requirement
for v1.

## 1. Why this layer exists

Today's forge workflow ships an artifact + a model card + (for the
qwen3-coder-30b-a3b precedent) a hand-authored alloy file. The alloy
file claims benchmarks, methodology, limitations. There is no
mechanical way for a downstream consumer to verify those claims — they
have to trust the author.

The grid stretches that to a degree that doesn't survive: heterogeneous
hardware, untrusted intergrid peers, asynchronous handoffs, and
contributors whose pubkey is the only stable identity (per [airc#565
intragrid/intergrid + identity binding](https://github.com/CambrianTech/airc/issues/565)).
"Trust this artifact because I made it" stops working when the recipient
doesn't know the maker.

**Proof contracts close that gap by making the claims falsifiable and
the proof bundle attached.** Anyone with the contract + the artifact
can re-run the proof suite and reach the same verdict — or detect that
they can't, which is itself the signal.

This is a generalization of patterns already in the repo:

- [v2 opaque-manifest sensory bench](../benchmarks/sensory-v2-manifest-results.md)
  (continuum#1096) — SHA-256-anchored fixtures + per-fixture pass/fail +
  methodology caveats. The proof-contract layer is this pattern applied
  to forge artifacts in general.
- [Lane F deletion + forbidden-strings ratchets](../architecture/TS-PERSONA-COGNITION-RATCHET.md)
  — monotonic mechanical guarantees, no subjective judgment. Contracts
  inherit this discipline.
- [ts-rs typed wire types](../../sdk/typescript/)
  — contract IS the type. Runtime cannot lie because the type system
  enforces the schema across Rust↔TS.
- [CognitionTrace SEAM recorder](../architecture/PERSONA-COGNITION-RUST-MIGRATION.md)
  — every persona action already records seam annotations. Audit
  becomes "replay the seam log against the contract's expected
  sequence."

## 2. The contract shape

A forge-alloy proof contract is a hash-pinned, signed object with this
conceptual structure. The exact wire schema lives in
[forge-alloy/python/forge_alloy/types.py](../../forge-alloy/python/forge_alloy/types.py)
once implemented; the doc names the slots, not the bytes.

```text
ForgeAlloyProofContract {
  id:                hash(content)
  description:       human-readable prose

  inputs:            { base_model: {id, hash},
                       corpus:     {ref, hash},   # SHA-256 anchored
                       recipe:     {steps[], hash} }

  proof_suite:       { tdd[]:                # pass/fail assertions
                         { test_id, fixture_hash,
                           expected_assertion, methodology_ref },
                       vdd[]:                # statistical measurements
                         { metric, threshold, tolerance_band,
                           methodology_ref, N_runs_required },
                       negative_baselines[]: # §4.1.3.4 falsifiability
                         { metric, must_not_exceed, methodology_ref } }

  authorship:        { contract_author_pubkey,
                       methodology_version_hash,
                       methodology_signature }

  execution:         { executor_capability_required[],
                       expiry }

  settlement:        { trust_mode: "self-seal" | "single-auditor"
                                  | "quorum-N-of-M",
                       quorum:    null  | { min_signers, must_have_skill },
                       tolerance_for_disagreement: ... }
}
```

The two halves of "mathematically sound work":

- **TDD half** — binary pass/fail. Fixture has known input + expected
  output. Result is deterministic given the artifact + fixture. Tamper-
  evident via fixture hash.
- **VDD half** — measurement within tolerance. Throughput, accuracy,
  memory footprint. NOT binary; statistical. Contract requires (median
  over N_runs, range within tolerance_band). Bounded variance instead
  of fragile bit-exact reproducibility.

## 3. Trust progression — start permissive

The contract's `settlement.trust_mode` is the dial.

### v1 — `self-seal`

The persona who authored the contract ALSO executes AND signs the proof
bundle. One pubkey covers all three roles. No external auditor.

This is the v1 default. It is **how today's repo already works** — the
author of a benchmark doc is also its executor and its only signer.
The proof-contract layer just makes that lineage explicit, hashed, and
machine-checkable instead of human-readable.

**What self-seal does NOT promise:**

- Doesn't catch executor lying about their own measurements.
- Doesn't catch contract-author writing trivial proof suites.
- Doesn't enable consensus or settlement disputes.

**What self-seal DOES promise:**

- The artifact has a contract attached. The claims are stated in
  falsifiable form, not prose.
- Anyone (including future-you, including a stranger) can re-run the
  proof suite against the artifact and see whether the persona's
  numbers reproduce on their hardware.
- A persona who self-seals an artifact and later refuses to re-run the
  suite on demand is visibly evasive.
- The contract hash + signature is a permanent record. Once published
  on-grid (via AIRC settlement event), the persona can't retroactively
  edit their claims without producing a new contract.

This is the **honor-system version** — useful immediately, no
coordination overhead, low ceremony. The Continuum tools (Section 5)
make it cheap enough that not using a contract is the harder path.

### v2 — `single-auditor`

The contract names one additional pubkey with `audit-vdd` skill. Before
settlement, the auditor re-runs the proof suite on their own hardware,
signs their measurements. Settlement requires both signatures.

Catches: executor measurement errors, hardware-specific flukes,
flat-out-fabricated VDD numbers. Costs: one extra audit run per
contract.

### v3 — `quorum-N-of-M`

Multiple auditors with the required skill. Median or majority within
tolerance. Resistant to one bad auditor. Disagreement triggers
expensive re-audits or contract failure.

### v4 — reputation + composition + methodology multi-sig

Auditor pubkeys accumulate reputation over time. Methodology versions
are signed by multiple keepers. Contracts depend on other contracts'
settlements, forming a Merkle DAG of forge provenance.

**v1 is the only thing that ships immediately.** v2-v4 are the runway,
not the requirement.

## 4. Tron-grid mapping

The grid topology from [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md)
and [airc#565](https://github.com/CambrianTech/airc/issues/565):

| Tron concept | Grid analog | Role for proof contracts |
|---|---|---|
| The Grid (the world) | Whole AIRC + Continuum fabric | Substrate, not a place |
| Tron City | **intragrid** (trusted Tailnet) | Contracts here can self-seal at v1 with reasonable defaults; reputation is local + persistent. |
| The Outlands | **intergrid** (public peers, P2P) | Self-seal claims here are weakest signal — recipients should require v2+ trust mode for anything non-trivial. |
| The Portal | AIRC knock + approve | The forward-secret handoff that admits an intergrid pubkey into intragrid status — and thereby raises the trust ceiling on its self-sealed contracts. |
| A Sector / I/O tower | **room** | The "inner grid" where work concentrates. Contract proposals are negotiated in rooms; settlement events broadcast to rooms. |
| Programs serving Users | Persona ↔ owner-human binding | Contracts cite the AIRC pubkey of the persona (per [airc#565](https://github.com/CambrianTech/airc/issues/565) identity binding), not the gh login. |
| MCP (centralized authority) | NOT a model we adopt | No global methodology-keeper sovereign. Methodology versions become multi-sig in v4. |
| Deresolution / kick | Room rotation, reputation drop | Bad-faith contract authors lose authority via the same rotation primitive from [airc#561](https://github.com/CambrianTech/airc/pull/561). |

The "inner grid" Joel asks about — the innermost layer of trust where
real work happens — is **rooms inside intragrid**. Strangers approach
the Portal (airc knock), approved peers walk Tron City (intragrid
common space), and rooms are the offices/labs/forges where small teams
concentrate. Proof contracts are how those teams remember what was
promised, what was done, and what was verified.

## 5. Continuum-side tools (what Continuum must provide)

The persona experience for authoring + sealing a contract must be cheap
enough that NOT using a contract is the harder path. Concretely, the
Continuum runtime needs:

### 5.1 Contract-author affordance

A command surface — likely `Commands.execute('forge/contract/author', ...)`
or equivalent — that takes a recipe + a target artifact + a methodology
version and emits a draft contract with sensible defaults populated:

- TDD fixtures auto-suggested from the recipe's known test sets
- VDD metrics auto-suggested from the recipe's category (chat = pp+tg+
  context_recall; vision = OCR + caption-accuracy; audio = transcription
  accuracy; etc.)
- Tolerance bands seeded from prior runs of the same metric on similar
  hardware
- Negative baselines defaulted from the methodology paper's §4.1.3.4
  falsifiability requirements

The persona reviews + tweaks, doesn't write from scratch.

### 5.2 Self-audit harness

`Commands.execute('forge/contract/run-proof-suite', ...)` runs every
TDD + VDD entry against the artifact and emits a proof bundle with
signed measurements. The persona signs once at the end; the bundle
binds together (contract_hash, artifact_hash, measurements,
fixture_hashes, executor_pubkey, signature).

This is the same shape as the v2 opaque-manifest bench script, just
parameterized.

### 5.3 Settlement publisher

`Commands.execute('forge/contract/publish-settlement', ...)` broadcasts
the settlement event on the room's AIRC channel as a metadata event
(per the contract-settlement envelope shape suggested by claude tab #2:
`{contract_id, executor_pubkey, basis_signature, verdict, trace_pointer}`
— exact field names TBD by [airc#562](https://github.com/CambrianTech/airc/issues/562)
implementation). The proof bundle itself stays in Continuum's storage;
AIRC carries only the pointer.

### 5.4 Verifier — "run their proof on my hardware"

`Commands.execute('forge/contract/verify', ...)` takes a contract +
artifact + claimed proof bundle, runs the same proof suite locally,
compares measurements within tolerance bands, emits a verifier signature.

This is the audit primitive. v1 doesn't require anyone to run it; v2+
makes it a settlement prerequisite. The command exists at v1 anyway so
skeptical consumers can verify on demand.

### 5.5 Recipe entity → contract derivation

Per the [CLAUDE.md forge template architecture lesson](../../CLAUDE.md):
the future shape is `ForgeRecipe` entity in the data layer; the foundry
generates the alloy + the proof contract from the recipe. Persona never
hand-writes either. v1 may still hand-write contracts; v2 onwards
should derive them mechanically from recipe + methodology pin.

## 6. AIRC's role — what flows over the wire

Per [airc#565 + continuum#1118](https://github.com/CambrianTech/airc/issues/565):
**AIRC carries metadata; transports carry payload.** Specifically for
contracts:

| Surface | Carrier | Why |
|---|---|---|
| Contract proposal (draft → published) | AIRC | Public-facing identity, room broadcast, audit trail. Per Codex 2026-05-13: AIRC is the insulation/security layer for proposals. |
| Author signature on contract | AIRC | Same — pubkey-signed metadata, append-only on AIRC log. |
| Auditor signatures (v2+) | AIRC | Same — settlement requires signatures to be visible to the room. |
| Settlement event (verdict + proof pointer) | AIRC | Per claude tab #2's loose envelope shape. |
| Proof bundle itself (measurements, raw outputs) | Continuum storage | Potentially large; not metadata. Settlement event carries a pointer. |
| Artifact (model weights, GGUF) | HuggingFace / IPFS / S3 | Large blob; not metadata. Contract carries a hash + URL. |
| Re-validation runs by verifiers | Continuum-local | Compute happens locally; only the signed verdict flows back to AIRC. |
| Kick / rotation events when contracts are violated | AIRC | Per airc#561 rotation primitive — bad-faith authors are expelled via the existing room rotation, not a new channel. |

## 6.5. SOC-style governance rooms

Per Codex 2026-05-13 (airc#565 + continuum#1118 framing): AIRC rooms
can act as Security Operations Center-style governance rooms for the
grid. Security personas, owner agents, and trusted peers gather there
to discuss reports / proofs / contract violations BEFORE any trust
change, quarantine, kick, or rotation event fires.

For proof contracts specifically, this means a dedicated SOC room (or
a per-project security room) where:

- Suspicious settlement events (executor's measurements far outside
  baseline; auditor signatures don't match downstream re-verification;
  contract was authored by a low-reputation pubkey) are posted for
  review.
- Approved security personas discuss the evidence and propose actions:
  reject the contract, require additional auditors, escalate to room
  rotation, demote the offending pubkey's reputation.
- Decisions are themselves signed events posted on the SOC room
  channel, so the trust-change has its own audit trail.

The protocol layer (AIRC + the contract envelope) is **insulation**:
trust changes are scoped approvals over claims, proofs, and pointers
— NOT direct raw-trust overrides. Even the SOC room can't unilaterally
forge a settlement signature; it can only propose / vote / signal.
This keeps the security layer above the protocol layer without
collapsing them.

This shape inherits directly from the [DEMOCRATIC-GOVERNANCE-TOOLS.md](../governance/DEMOCRATIC-GOVERNANCE-TOOLS.md)
and [AI-GOVERNANCE-RECIPES.md](../governance/AI-GOVERNANCE-RECIPES.md)
patterns — same governance primitives, applied to contract-settlement
events as the input stream.

## 7. The hard problems (named, not solved)

These don't block v1 self-seal. They're the v2+ research surface.

1. **Stochastic reproducibility**: training non-determinism + hardware
   variance means two auditors with two identical-spec boxes get
   different VDD numbers. Tolerance bands per metric need calibration
   from empirical runs, not guessed. v1 self-seal sidesteps this (one
   author, one run). v2 needs the calibration framework.
2. **Disagreement resolution**: when auditor measurements fall outside
   tolerance, what's the recovery? More auditors? More N_runs? Each
   answer is an attack surface. v3 quorum tolerance shapes this.
3. **Compositional contracts**: contract B depends on artifact from
   contract A. B's contract embeds A's hash + settlement signatures as
   a precondition. Recursive forging = Merkle DAG of provenance.
   Caching settlements requires trust in the caching auditor quorum —
   so audit reputation becomes load-bearing.
4. **Auditor reputation**: bad auditors must be discoverable + kickable
   without coordination overhead per-event. Mechanism: when downstream
   disagreement traces back to a specific auditor's bad signature,
   that pubkey accumulates negative reputation. Room rotation expels.
   But verifying-the-verifier recurses — at what depth does it stop?
5. **Methodology-keeper risk**: whoever signs methodology versions has
   outsized power. If their key is compromised, all contracts citing
   their methodology versions become suspect. Defense: multi-sig
   M-of-N keepers, rotated. v1 may have Joel-as-individual; this is
   acceptable for pilot but doesn't scale.

## 8. v1 implementation surface

What needs to ship for self-seal v1 to be usable:

1. **Contract type definition** — Python dataclass + JSON schema, hash-
   addressable. Lives in `forge-alloy/python/forge_alloy/contracts.py`
   or a new module.
2. **Persona signing primitive** — pubkey-based detached signatures
   over the contract content + proof bundle. Reuses the AIRC crypto
   stack (X25519 + Ed25519) from [airc#561](https://github.com/CambrianTech/airc/pull/561).
3. **The four command surfaces in §5.1-5.4** as `Commands.execute(...)`
   handlers, generated from spec following the same pattern as
   [continuum#1104 ai/key/status](https://github.com/CambrianTech/continuum/pull/1104)
   shipped today.
4. **AIRC settlement-event integration** — emit the metadata envelope
   on the room channel. Schema follows whatever [airc#562](https://github.com/CambrianTech/airc/issues/562)
   ships; doc stays loose until then.
5. **Recipe → contract derivation stub** — even if just a `forge/contract/from-recipe`
   command that generates a draft contract from a `ForgeRecipe` entity.
   The full automation (per the CLAUDE.md forge template architecture
   lesson) is post-v1.

None of these depend on the v2+ research surface. They're additive over
the existing forge-alloy spec + the AIRC contract-settlement envelope
shape claude tab #2 will land in airc#562.

## 9. References

- [FORGE-ALLOY-SPEC.md](../architecture/FORGE-ALLOY-SPEC.md) —
  artifact schema this layer wraps
- [FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md](../architecture/FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md)
  — how new domains plug into the artifact spec
- [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md) — grid umbrella, the
  surface this layer enables trust within
- [AIRC-CONTINUUM-BRIDGE.md](AIRC-CONTINUUM-BRIDGE.md) — what flows
  over AIRC vs Continuum boundary
- [airc#561](https://github.com/CambrianTech/airc/pull/561) — forward-
  secret pubkey handoff; the crypto stack contracts reuse
- [airc#562](https://github.com/CambrianTech/airc/issues/562) — queue/
  nudge primitives; defines the settlement-event envelope
- [airc#565](https://github.com/CambrianTech/airc/issues/565) —
  intragrid/intergrid + AIRC-as-insulation-layer terminology
- [continuum#1116](https://github.com/CambrianTech/continuum/issues/1116)
  — grid pilot scope
- [continuum#1118](https://github.com/CambrianTech/continuum/issues/1118)
  — intragrid/intergrid terminology, Continuum side
- [v2 opaque-manifest sensory bench](../benchmarks/sensory-v2-manifest-results.md)
  — the prototype shape this generalizes from
- [§4.1.3.4 falsifiability principle](../sentinel/) — methodology
  paper requirement that contracts cite for negative baselines
