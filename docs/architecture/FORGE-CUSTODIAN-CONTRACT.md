# Forge Custodian Contract — Contract C, and how it grid-negotiates

**Status:** in progress. Pass 1 + Pass 2 + Pass 3 + Pass 4 + Pass 5a + Pass 5b + Pass 6 (client seam) landed; Pass 6b (real `GridStateDispatch`) gated on TwoAircLoopback #187. Pass 1/2 — `forge::protocol`
single-sources the wire + `/health` contract-version handshake (custodian a real
`[[bin]]`), and the gguf-lora export speaks Contract C through a clean de-`unsloth`
`forge::custodian_client` aimed at the custodian's own endpoint. **Pass 3 (commit
`20d60e3b3`)** hardened the binary: R3 bounds (conversion-slot semaphore →
fast-loud `503` when saturated; per-conversion wall-clock deadline that KILLS a
wedged subprocess), R4 honest `/health` (additive `ready`/`slots_total`/
`slots_available`, no version bump), R5 graceful shutdown (SIGINT/SIGTERM drains
in-flight via `with_graceful_shutdown`), R6 content-addressed idempotency
(`job_id = sha256(weights ⊕ base ⊕ outtype)` in the output filename → identical
re-POST short-circuits). Proven by a daemon-boot integration test (real binary +
real client over loopback). **Pass 4 (commit `d06f41d71`)** made the gene handle
node-aware: `forge::gene_handle::GeneHandle` (structured `GeneLocator`, `AlloyHash`
provenance, `TrustLevel` scope) is the grid extension of the local `TrainedAdapter`
record — the `#17` two-handle reconciliation, resolved in the forge context (§4).
**Pass 5a (commit `c0a5f7ebb`)** added the routable endpoint type
`forge::endpoint::ForgeEndpoint` (§5) — DISCOVERED by probing a custodian's
`/health`, derived from the Pass 3/4 `HealthResponse` rather than the retiring
unsloth `ForgeCapability`; `ForgeHealth` (Healthy/Busy/Down) and `ForgeLocator`
(service-reach, distinct from `GeneLocator`'s byte-custody) honor the same `#17`
discipline at the endpoint tier. **Pass 5b (announce the endpoint over the grid
bus — `NodeCapability` + `GridTransport::announce`) is next.** This doc is the
single source of truth for the **forge custodian seam** and the plan to make it
grid-negotiable.

> Joel, 2026-06-25: *"Inference and lora training can be grid negotiated if your
> daemons are any good. So we get the sum of all the parts. Will take real
> design. But perfect the contracts locally first. Solid resilient api, but make
> plan to make it work with the grid bc that's coming next."*

This is the **third** contract in the family. Read its two siblings first; this
doc defers to them and does not re-derive their material:

- **[EXCISE-UNSLOTH-CONTRACT.md](EXCISE-UNSLOTH-CONTRACT.md)** — Contract **A**
  (the Serving Seam, `inference::llama_server`) and Contract **B** (the Adapter
  Capability Surface). Its coupling map routes `inference/unsloth_forge.rs`,
  `modules/forge.rs`, and `inference/model_commands.rs` onto **"#52"** — that
  deferral *is* this contract. Contract C is what #52 routes onto.
- **[MODEL-ENDPOINT-FABRIC.md](MODEL-ENDPOINT-FABRIC.md)** — the self-healing
  adapter router that, for any model need, does **match → score → route → heal**
  across many local + grid endpoints. **The fabric does the grid negotiation.**
  Contract C's only job is to make a forge custodian a *routable endpoint* the
  fabric can discover, health-probe, lease, and heal around — exactly as it does
  a serving endpoint. We do NOT build a second router here.
- **[GRID-ADDRESSING-AND-ROUTING.md](GRID-ADDRESSING-AND-ROUTING.md)** — the URI
  grammar, transport selection, per-URI trust gate, and `HandleRef` across the
  wire. Contract C reaches a remote custodian through *these verbatim*; it
  invents no second addressing scheme.

---

## 1. The thesis in one paragraph

A **forge need** (turn this trained checkpoint into a pageable GGUF gene; later:
train this LoRA from this dataset; fuse; quantize) is a packet. A **forge
custodian** (a process that owns the byte-ops on model weights — Apple
`mlx_lm.lora`/`fuse`, the continuum-owned `forge-custodian` for the
GGUF-LoRA convert unsloth-studio can't do, NVIDIA unsloth) is an endpoint,
reached through one trait over one wire contract. **The contract is the same
whether the custodian is `127.0.0.1:8899` or a grid peer's RTX 5090.** Only the
transport changes. The custodian owns the produced bytes node-locally; continuum
holds a **handle** (path/id + provenance), never copies bytes. That is what lets
the fabric negotiate forge across the grid and get *the sum of all the parts*:
the one node with the GPU trains; every node leases the result.

---

## 2. The contract (`forge::protocol`) — ✅ Pass 1

The single source both ends import. The custodian binary
(`bin/forge_custodian.rs`) SERVES it; the core-side client (Pass 2) CALLS it.
Hand-duplicating the request on each side already caused a live drift (custodian
required `checkpoint`; the old core copy omitted it and carried
`push_to_hub`/`repo_id` the custodian never reads — a real POST would fail
deserialization). One type, two importers, compile-time drift protection.

```rust
// core/continuum-core/src/forge/protocol.rs
pub const CONTRACT_VERSION: u32 = 1;            // rides /health; client refuses a contract it can't speak
pub const ROUTE_GGUF_LORA: &str = "/api/export/export/gguf-lora";
pub const ROUTE_HEALTH:    &str = "/health";
pub const CAPABILITY_GGUF_LORA: &str = "gguf-lora";

pub struct GgufLoraRequest {     // STATELESS — names the checkpoint directly, no prior load-checkpoint
    pub checkpoint: String,      // trained MLX run dir (adapters.safetensors + adapter_config.json)
    pub save_directory: String,  // custodian-owned OUTPUT path (bytes land here, node-local)
    pub base_model_id: String,   // REQUIRED — a GGUF LoRA with no base is meaningless (invariant in the type)
    pub outtype: String,         // default "f16" — preserve the trained signal
}
pub struct ExportResult { pub success: bool, pub message: String, pub details: Value }
pub struct HealthResponse { pub status, pub kind, pub capability, pub contract_version: u32 }
```

**Why these shapes are already grid-shaped:**
- **Stateless.** No per-session state on the custodian ⇒ any node can serve any
  request; the fabric can route the *next* request to a *different* node and
  heal around a failure mid-stream. A stateful (load-checkpoint-first) protocol
  could not be load-balanced this way — that's why unsloth's stateful exporter is
  retired here.
- **`base_model_id` is in the request, not in custodian config.** The custodian
  is a pure function of its inputs; it holds no opinion about which base. Grid
  nodes need no shared config to agree on a job.
- **`save_directory` is custodian-owned output.** Byte custody is node-local by
  construction. On the grid the path is the *remote* node's path; what crosses
  back is the handle (§4), never the bytes inline.
- **`CONTRACT_VERSION` on `/health`.** A heterogeneous grid runs mixed builds.
  The handshake lets a client refuse a node it can't speak to *before* dispatch —
  fail loud at the seam, never a malformed body deep in a conversion.

---

## 3. The resilient local API — Pass 2 + Pass 3 (do this before grid)

"Perfect the contracts locally first." The custodian must be production-grade as
a *local* daemon before it's a grid endpoint; the grid only amplifies whatever
resiliency (or lack of it) the local API has. Each invariant below is *also* a
grid-readiness property — noted inline.

| # | Invariant | Local reason | Grid amplification |
|---|---|---|---|
| R1 | **One wire contract, one source** (`forge::protocol`) | no drift between server/client | a grid client and a remote server compiled from the same crate can't disagree on the wire |
| R2 | **Typed two-variant client error** `Unreachable` \| `Api` (reuse the `UnslothError` shape) | caller distinguishes "custodian down" from "job failed" | "node down" ⇒ fabric **re-routes**; "job failed" ⇒ fabric **does not** (same job will fail elsewhere) |
| R3 | **Bounded everything** — request timeout, a concurrency **semaphore** (a GPU does one convert at a time), a subprocess **deadline** on the `python3` convert | one wedged convert can't hang the daemon | a node advertises *real* remaining capacity; the fabric's score is honest |
| R4 | **Liveness + readiness + capability + contract-version on `/health`** | `uu`/curl can tell "up" from "ready to take work" | this IS the fabric's discovery + health-probe input (§5) |
| R5 | **Graceful shutdown + `probe!` observability** (reuse `ServiceModule`, `watch<Snapshot>`, `PressureBroker`, the `probe!` macro — do NOT reinvent) | clean drain; glass-box every decision | the snapshot a node publishes locally is the same snapshot the fabric aggregates grid-wide |
| R6 | **Idempotent jobs by content address** — `job_id = hash(checkpoint_digest ⊕ base_model_id ⊕ outtype)` | a retried convert returns the existing gene, doesn't redo it | at-least-once grid delivery becomes safe; a re-dispatched job after a node flaps doesn't duplicate work |

**Pass 2** = build the core-side client (`ForgeCustodianHttp`) implementing a
transport-agnostic `ForgeCustodian` trait against `forge::protocol`, with R1+R2.
This is the #52 convergence: it replaces `inference/unsloth_forge.rs`'s stateful
client and lets `modules/forge.rs` / `inference/model_commands.rs` route onto
Contract C. **Pass 3** = R3–R6 on the custodian binary (bounds, health detail,
shutdown, probes, idempotency).

**Byte custody is already correctly placed (the fabric doc's §2 trespass is
stale for the gguf-lora path).** As of this writing `modules/forge.rs` does NOT
shell trainers/converters or write bytes itself — it delegates over the trait
(`forge.rs:190-195`), and the byte-op (`convert_gguf_lora` →
`forge::lora_convert::mlx_adapters_to_peft` → `convert_lora_to_gguf.py`) runs
ONLY inside `bin/forge_custodian.rs`. The organism holds handles; the custodian
binary owns bytes. So Pass 2 is **not** a byte-relocation — it is purely a CLIENT
protocol-drift repair (below). `forge/decide` (when to train, adopt-or-reject)
stays in the organism — it's policy, a cognition decision, not a byte-op.

The remaining open question (NOT this contract's scope) is **training**: on Apple
Silicon, LoRA training already runs in-core via `genome/fine_tuning/`
(`mlx_lora_adapter.rs` → `mlx_lm.lora` in a `spawn_blocking` job actor), while the
trait's `train_start`/`train_status` point at unsloth's stateful `/api/train/*`
(the NVIDIA path). Whether training becomes a Contract-C custodian capability or
stays an in-core faculty is a #32/#52 design fork, tracked there — Contract C
today covers only the **gguf-lora export** the custodian binary actually serves.

---

## 4. The handle, not the bytes — byte custody across the grid

Continuum holds **handles, not bytes** (fabric doc §2). A produced gene is
referenced by a handle the *producer records* — never guessed from disk, because
the PEFT `adapter_config.json:base_model_name_or_path` is an HF id that never
string-matches the served continuum registry id (the id-mismatch problem).

The handle already exists: `forge::adapter_manifest::TrainedAdapter { alias,
path, base_model_id }` — the honest gene→served-model record, written atomically
by the producer. Contract C's grid extension makes the handle **node-aware**.
**Landed in Pass 4 as `forge::gene_handle::GeneHandle`** (commit `d06f41d71`):

```
TrainedAdapter (today, local)          GeneHandle (grid extension, AS BUILT)
  alias                                  alias
  path            (local fs path)   →    locator: GeneLocator          // byte custody, structured
  base_model_id                          base_model_id
                                         provenance: Option<AlloyHash> // FORGE-ALLOY-SPEC — what/how, None until attested
                                         trust_scope: TrustLevel        // GridTrustAuthPolicy boundary it may cross

GeneLocator = Local { path }                 // bytes on THIS node (degenerate local case)
            | Node  { node: PeerId, path }    // bytes held by a remote forge node
```

**The type choices reconcile the §4 sketch against what actually exists in
tree** — this IS the `#17` two-handle reconciliation, resolved in the forge
context:
- **locator is a structured `GeneLocator`, not a `HandleRef` and not a
  `CommandUri`.** `HandleRef` (`runtime/cell_shapes.rs`) is a *state-correlation
  envelope* (owner/id/type_tag) for live stateful sequences — wrong semantics for
  a gene at rest. `CommandUri` (`routing/command_uri.rs`) addresses a *command*
  (its `path` is a command path like `data/list`), not bytes on a filesystem.
  Byte custody is its own honest concept: `{which node, what path}`. The holding
  peer folds into the `Node` variant (+ a `GeneHandle::node()` accessor), so no
  top-level `node` field has to lie (point at self) for a local gene. `Node`'s
  peer maps onto GRID-ADDRESSING-AND-ROUTING when Pass 6 wires transport; the
  structured form stays the source of truth (structured > stringly-typed).
- **provenance is an `AlloyHash` newtype** over the `sha256:…` string the
  forge-alloy spec already carries as a bare `String` on `ForgeArtifact` — wrapped
  so it can't be confused with any other id (mirrors `PeerId(Uuid)`). `Option`,
  `None` until the gene is alloy-attested (a locally-forged gene predates its
  alloy).
- **trust_scope reuses `grid::node::TrustLevel`** (the GridTrustAuthPolicy
  boundary enum — Blocked/Provisional/Trusted/Owner); there is no separate
  `TrustTier` type to invent.

`TrainedAdapter::as_gene_handle()` is the honest local projection
(`Local`-located, `Owner`-scoped, un-attested). The type round-trips the wire
(ts-rs bindings generated) because it must — it crosses back to a requester while
the bytes stay node-local. Pass 5 (the endpoint table) and Pass 6 (grid
transport) consume it.

On the grid: the gene bytes stay on the forge node under its `save_directory`;
what crosses back to the requester is a `GeneHandle`. To actually *page it in*,
either (a) the requesting node leases inference from the node that holds the gene
(compute-lease, text-only — `[[compute-lease-boundary]]`), or (b) the gene is
*fetched* into the requester's serving node as a byte transfer gated by trust
scope (the genome-market exchange, §6). The handle carries enough to choose.

---

## 5. Making the custodian a routable endpoint (the fabric's input)

The fabric (`MODEL-ENDPOINT-FABRIC.md`) will do match→score→route→heal over an
endpoint table it builds from health probes. Contract C's grid job is purely to
make a forge custodian *appear in that table*. **Pass 5a landed the row type**
as `forge::endpoint::ForgeEndpoint` (commit `c0a5f7ebb`) — AS BUILT:

```rust
pub enum ForgeLocator { Local { base_url: String }, Node { node: PeerId } }
pub enum ForgeHealth  { Healthy, Busy, Down }
pub struct ForgeEndpoint {
    pub locator: ForgeLocator,        // how to reach the SERVICE (HTTP | grid peer)
    pub capabilities: Vec<String>,    // CAPABILITY_* tags (gguf-lora; train/fuse appended later)
    pub contract_version: u32,        // from /health — scorer refuses a version it can't speak (R4)
    pub health: ForgeHealth,          // DERIVED from the probe, never self-declared
    pub capacity: u32,                // slots_available from the semaphore (R3) — honest because bounded
    pub trust_scope: TrustLevel,      // GridTrustAuthPolicy — which jobs it may accept
}
```

Two corrections to the original sketch, both forced by tree reality:
- **The seed is NOT the unsloth `ForgeCapability`.** That type
  (`reachable/busy/phase/held_genes/outputs_dir`) is bound to the *retiring*
  unsloth `ForgeCustodian` trait (#52). `ForgeEndpoint` is generalized instead
  from the Pass 3/4 `HealthResponse`, which already carries the honest router
  inputs (R4); the health handshake IS the probe. `ForgeEndpoint::probe()` reads
  `/health` over the clean Contract C trait: `Unreachable` ⇒ an honest `Down`
  row, but an `Api` fault surfaces LOUD (never a silent `Down` that hides a
  broken endpoint — the no-fallback contract at the probe boundary).
- **`locator` is a `ForgeLocator`, not a `HandleRef`.** It answers "how to reach
  the *service*" (a base URL, or a grid peer) — distinct from `GeneLocator`
  ("where the *bytes* live"), the same `#17` discipline at the endpoint tier.
  `Node`'s `PeerId` maps onto GRID-ADDRESSING when Pass 6 wires transport.

The dispatch gate is one pure predicate, `can_accept_gguf_lora(endpoint,
client_contract_version, trust_floor)` — routable + contract match + capability +
trust floor, the two forge-specific gates below expressed as code.

**Discovery is observed, not configured.** **Pass 5b (landed, `1027b7046`)**
announces a node's `ForgeEndpoint` over the grid bus as `NodeCapability::Forge`
through the existing `GridModule` enrichment + `GridTransport::announce` path; a
node advertises forge ONLY when `ForgeEndpoint::probe_local` finds the custodian
answering (absence is honest, not a fallback). The `NodeRegistry` aggregates.
**Routing** for a forge need
is the fabric's existing scorer with two forge-specific gates:
1. **Trust gate first** (GridTrustAuthPolicy). A job whose dataset carries
   private data (the medical trial — `[[medical-field-first-trial]]`) must NOT be
   dispatched outside its trust boundary. This is a hard gate, not a score term.
2. **Capability + base match.** Only nodes advertising `gguf-lora` *and* able to
   resolve `base_model_id` (the base-mismatch guard that already correctly
   refuses a 3B gene against a 4B server) are candidates.
Then score on capacity/latency/cost and route; on `Unreachable` (R2) **heal** by
re-routing the idempotent job (R6) to the next candidate — never by silently
degrading (no-fallback doctrine).

---

## 5.1 Placement negotiation — score, then CLAIM (the half that's still open)

> "the daemons need to negotiate together on the grid over forge alloy placements"
> — and: "training is more intensive and off-computer, especially for slow
> machines, into grid, which for some people might contain GPU rigs. I have two.
> Toby has a couple. We link together."

Everything above this section is the *observation* half: each custodian probes
its own `/health`, announces a `ForgeEndpoint` row (`capacity`, `health`,
`contract_version`, `trust_scope`) as `NodeCapability::Forge`, and the
`NodeRegistry` on every node aggregates the table (Pass 5b, landed). That table
IS the bid sheet — it already carries honest, bounded inputs (R3/R4), not
self-declared ones. What's missing is the *decision* half, and it is exactly two
steps, in order:

**Step 1 — SCORE (pure, already specified, not yet coded).** A placement request
for a forge need ranks the candidate rows. The gates are hard filters first
(`can_accept_gguf_lora`: routable + contract match + capability + **trust floor**
— Toby's rigs and my two rigs are *different* `trust_scope`s, and a private
dataset, `[[medical-field-first-trial]]`, never even enters the candidate set
outside its boundary). Then a score over the survivors, in priority order:
1. **Data locality (cheapest wins).** R6 makes `job_id =
   sha256(weights ⊕ adapter_config ⊕ base ⊕ outtype)` content-addressed, so a
   node that **already produced this exact gene** short-circuits to a handle with
   zero GPU work — the highest-value "bid." Next: a node that already has the
   `base_model` + calibration corpus resident (no multi-GB pull). This is the
   same insight as Pass 7's market: *query before you train, locate before you
   move bytes.*
2. **Free capacity.** `slots_available` from the bounded semaphore — honest
   because it's a real reservation count, not a guess.
3. **Capability fit / cost.** a 5090 rig outbids a MacBook Air for a real train;
   the Air keeps the lightweight convert it can do locally.

**Step 2 — CLAIM (the genuinely new primitive).** Scoring alone is not
negotiation — two requesters reading the same snapshot both pick the same idle
5090 and collide. Negotiation = score **then reserve before dispatch**: the
winner sends a `forge/claim` (reserve a slot, TTL-bounded) and only on the
claim's ack does the checkpoint cross. A claim is just an at-least-once
idempotent grid message keyed on `job_id` (R6 already makes re-delivery safe), so
the reservation reuses the same content-addressing that makes the convert
idempotent. A losing/expired claim heals to the next candidate (R2/R6) — never a
silent local fallback. This is the forge twin of the inference
`ThroughputLeaseRegistry`: a lease over a scarce node resource, granted by the
holder, not assumed by the requester.

**Where it plugs in (no consumer churn).** The scorer + claim live *beneath* the
`ForgeCustodian` trait, inside the Pass-6 `GridForgeCustodian` — it picks the
node, claims the slot, then routes the existing `forge/export` over
`GridDispatch`. The sentinel I wired in L3 (`resolve_pageable_gene_path`) and
`modules/forge.rs` call the trait and never learn placement happened. A slow Air
running the L3 loop offloads the heavy convert/train to a claimed grid rig *by
construction*.

**Honest blocker.** Placement negotiation cannot be validated with one node — it
needs the two-node fixture (TwoAircLoopback, #187) and the real
`GridStateDispatch` (Pass 6b), which also forces the typed-error-recovery
decision (`dispatch_to_node`'s `Result<_, String>` must regain the
`Unreachable`/`Remote` split so claim-failure heals correctly). Score is pure and
unit-testable today against fixture `ForgeEndpoint` rows; claim + heal land with
6b. Until then the local `ForgeCustodianHttp` is the correct single-node
degenerate case — score-of-one, no claim needed.

---

## 5.2 The decision layer is a SWAPPABLE policy — deterministic floor, LLM ceiling

> "We should use an LLM to manage these contracts too… real inference decision
> making… airc already supports this communication across the grid, so we make
> these agreements, like lawyers talking to one another or salesmen and buyers,
> into a personified Tron paradigm. This is the new way compute works… It works
> for systems too. For security, for trade of expertise."

§5.1's score-then-claim is written as a *pure function* on purpose, but the
function is not the point — **the seam is.** Placement is one `PlacementPolicy`
decision (`pick(candidates, need) -> Claim`), and that decision is **swappable
over the same airc bus, exactly as `[[self-improvement-is-a-control-loop]]`
makes the self-improvement policy swappable** (classifier → RL →
persona-analysis-team). Two outliers prove the one seam (CLAUDE.md outlier
doctrine):

- **Outlier A — deterministic scorer (the floor).** Pure, fast, free. The
  right policy for routine high-frequency placements: "convert this checkpoint,
  cheapest idle rig in scope wins." No inference, no latency, fully testable.
- **Outlier B — an LLM negotiator (the ceiling).** A persona carrying a
  *negotiation genome* conducts a real conversation with the counterparty node's
  persona over airc — the SAME mesh, the SAME Contract C envelope your dev agents
  already coordinate on. This is the policy for the far-reaching, judgment-rich
  agreements a scalar score can't capture: pricing leased compute, deciding
  whether to trade a genome *at all*, weighing a counterparty's track record,
  negotiating terms of an expertise exchange. Lawyer-to-lawyer, buyer-to-seller —
  personified, because `[[personas-are-peers-in-your-mesh]]` already makes each
  side a first-class citizen of the same room.

**The rails are NOT in the policy.** This is the load-bearing safety line. The
LLM negotiates *inside* fixed deterministic bounds it cannot dissolve:
- **`GridTrustAuthPolicy` is a hard gate, evaluated in code before any negotiator
  runs.** A private dataset (`[[medical-field-first-trial]]`) is excluded from
  the candidate set deterministically — no agent, however persuaded, can "agree"
  to route it cross-scope. An LLM that could move a boundary is a vulnerability,
  not a negotiator.
- **The claim/lease, the R6 idempotency, the contract-version handshake stay
  code.** The negotiator *chooses* a counterparty and *terms*; the deterministic
  substrate *enforces* the reservation, the at-least-once safety, and the version
  match. Policy decides; substrate guarantees. This is the same
  scaffolding-vs-mind distinction as `[[mind-emulation-allocation-choice-step-subconscious]]`
  and the no-fallback contract (`[[fallbacks-are-illegal-fail-loud]]`).

**The negotiation is also a turn — so it trains.** Every agreement conducted over
airc is recorded (`persona::recorder` → `dataset/from-turns`), so the
coordination↔learning flywheel (`[[coordination-learning-flywheel]]`) applies:
grid agents negotiate, and the recorded negotiations become the training corpus
for *better* negotiators. The market doesn't merely clear; it learns to clear
better.

**Generalizes beyond forge.** Forge placement is the concrete first instance, but
the shape — deterministic rails + swappable (eventually LLM) decision policy +
airc-carried personified agreement — is the grid's general *agreements*
substrate: the same move underwrites compute-leasing, security posture
negotiation, and expertise/genome trade (`[[continuum-grid-vision]]`,
`[[ask-anything-assemble-best-self-or-train]]`). Build it once here, against the
two forge outliers; the rest of the economy reuses the seam.

---

## 6. Two demand types, one shape — the "sum of all the parts"

Grid negotiation is the same trait-over-transport move for both leasable scarce
resources. They differ only in what crosses the wire:

| | **Inference lease** | **Forge lease** (this doc) |
|---|---|---|
| Need | generate tokens for this persona | turn this checkpoint into a gene / train this LoRA |
| Endpoint | serving node (`llama-server`) | forge custodian |
| Contract | A (Serving Seam) + the fabric's `AircRemoteInference` | **C (`forge::protocol`)** |
| Crosses wire | **text only** (`TurnEmitted`) — brain + tools stay local | request in; **`GeneHandle` out** — bytes stay node-local |
| Byte custody | weights in the serving node's VRAM | gene bytes under the forge node's `save_directory` |
| Already in tree | fabric `AircRemoteInference` row; `ThroughputLeaseRegistry` | `ForgeEndpoint` row (5a) + `NodeCapability::Forge` announce (5b); `GeneHandle`/`adapter_manifest` handle |

The endgame is `[[lora-layers-as-p2p-exchanged-genome]]` +
`[[search-then-ab-dont-start-from-zero]]`: a node that needs a capability first
**queries the trust-scoped genome market** for an existing `GeneHandle`, A/Bs it
(`cognition/eval` lift), and only **forge-leases a fresh train** if the market
comes up short. The forge lease is the *supply* side; the market is the
*exchange*. Both ride Contract C + the fabric + GridTrustAuthPolicy. One node
trains; the whole grid, within its trust boundary, becomes the sum of the parts.

---

## 7. The honest pass sequence

Done is done; everything below the line is the plan.

1. ✅ **Pass 1 — Contract C home.** `forge::protocol` single-sources the wire +
   route constants + `CONTRACT_VERSION`; `/health` carries the version; custodian
   declared as an explicit `[[bin]]`. Round-trip + version-handshake tests green.
   (`009f16e70`)
2. ✅ **Pass 2 — make the gguf-lora client speak Contract C (the drift repair).**
   (`6f9797cda`) Two bugs were live, not one: (a) **wrong endpoint** — `package()`'s
   `GgufLora` arm POSTed to `unsloth_base_url()`, the unsloth host, which *cannot*
   produce a GGUF LoRA (the whole reason the custodian exists); (b) **wrong wire
   shape** — it did a stateful `load_checkpoint()` then sent a body WITHOUT
   `checkpoint` and WITH `push_to_hub`/`repo_id` the stateless
   `forge::protocol::GgufLoraRequest` rejects. Fix: a clean de-`unsloth`
   `forge::custodian_client::{ForgeCustodian, ForgeCustodianHttp}` over
   `forge::protocol` — stateless `export_gguf_lora` (checkpoint-in-body, no
   load-checkpoint, no hub fields), typed `Unreachable|Api` (R2), `ensure_contract()`
   verifies `CONTRACT_VERSION` at `/health` before dispatch (R1), and it targets the
   CUSTODIAN's own address (`DEFAULT_CUSTODIAN_ADDR`/`FORGE_CUSTODIAN_ADDR`, single-
   sourced so the bin and client can't disagree). `modules/forge.rs::forge/export`
   dispatches gguf-lora to `run_export_gguf_lora` (Contract C); `lora`/`gguf` still
   route to unsloth until #52; `package_format` now rejects gguf-lora loudly as a
   guard against the old wrong path. 4 client + 3 rewritten export tests green. The
   unsloth-only `train_*`/`lora`/`gguf` arms stay until the broader #52 excision.
3. ✅ **Pass 3 — harden the custodian binary.** (`20d60e3b3`) **R3 bounds:** a
   `tokio::sync::Semaphore` of `FORGE_MAX_CONCURRENT` conversion slots — a saturated
   custodian `try_acquire`-fails to `503` (fast + loud, router re-routes) instead of
   queueing unbounded; each conversion runs through `run_with_deadline`
   (`FORGE_CONVERT_TIMEOUT_SECS`, default 1800) which drains stdout/stderr in helper
   threads and KILLS a child that outlives the deadline (a wedged python converter
   never holds a slot forever). **R4 honest `/health`:** `HealthResponse` gained
   additive `#[serde(default)]` `ready`/`slots_total`/`slots_available` (no
   `CONTRACT_VERSION` bump — older custodians deserialize fine); the handler reports
   whether the converter tooling resolves + the live free-slot count, so a router
   scores the node before dispatch. **R5 graceful shutdown:** `axum::serve(...)
   .with_graceful_shutdown(shutdown_signal())` stops accepting on SIGINT/SIGTERM and
   drains in-flight conversions before exit (no orphaned half-written gene). **R6
   content-addressed idempotency:** `job_id = sha256(weights-meta ⊕ adapter_config ⊕
   base ⊕ outtype)[..16]` is embedded in the output filename (`{name}-{job}.gguf`),
   so an identical re-POST (at-least-once grid delivery) short-circuits to the
   existing artifact and a differing request can never silently clobber another's
   gene. (Note: this is a standalone `[[bin]]`, not an in-core `ServiceModule`, so
   R5 uses axum's own graceful-shutdown future rather than the `ServiceModule`/
   `watch`/`PressureBroker` substrate primitives — those bind a router/fabric in Pass
   5/6.) Proven by 6 binary unit tests (job-id content-addressing, deadline-kill,
   fast-child happy path, param parsing) **plus** a daemon-boot integration test
   (`tests/forge_custodian_daemon.rs`) that runs the REAL binary under a temp `$HOME`
   and drives it with the REAL `ForgeCustodianHttp` client: honest `/health`
   (slots=3, ready, matching contract version), the client handshake passes over the
   wire, and SIGTERM exits 0 gracefully.
4. ✅ **Pass 4 — node-aware handle.** (`d06f41d71`) Extended
   `adapter_manifest::TrainedAdapter` → `forge::gene_handle::GeneHandle` (§4),
   reconciling the sketched field types against what exists in tree (the `#17`
   two-handle reconciliation, in the forge context): **locator** is a structured
   `GeneLocator { Local{path} | Node{node:PeerId,path} }` — NOT `HandleRef` (a
   state-correlation envelope, wrong semantics) and NOT `CommandUri` (addresses a
   command path, not bytes); the holding peer folds into the `Node` variant + a
   `node()` accessor so no top-level field lies for a local gene. **provenance** is
   an `AlloyHash` newtype (Option, None until attested). **trust_scope** reuses
   `grid::node::TrustLevel`. `TrainedAdapter::as_gene_handle()` is the honest local
   projection (Local/Owner/un-attested); ts-rs bindings generated; 3 unit + 3
   export-binding tests green, full forge suite 79 passed.
5. **Pass 5 — routable endpoint.**
   - ✅ **5a — the row type.** (`c0a5f7ebb`) `forge::endpoint::ForgeEndpoint` (§5),
     DISCOVERED by probing `/health` and derived from the Pass 3/4 `HealthResponse`
     — NOT the retiring unsloth `ForgeCapability` (#52). `ForgeHealth`
     (Healthy/Busy/Down, derived not self-declared) + `ForgeLocator` (service-reach,
     distinct from `GeneLocator`'s byte-custody — `#17` at the endpoint tier).
     `probe()` makes `Unreachable` an honest `Down` row but surfaces an `Api` fault
     LOUD (no-fallback at the probe boundary); `can_accept_gguf_lora()` is the one
     pure dispatch gate (routable + contract + capability + trust floor). 4 unit + 3
     export-binding tests green; forge suite 86 passed.
   - ✅ **5b — announce over the grid bus.** (`1027b7046`) `NodeCapability::Forge
     { endpoint }` rides the EXISTING `GridModule` capability-enrichment +
     `GridTransport::announce` path (no new tokio task / watch channel —
     CONCURRENCY-STYLE-GUIDE STOP block honored). `ForgeEndpoint::probe_local`
     (2s-bounded in `initialize()`) observes the local custodian; a node advertises
     forge ONLY when it answered — `advertise_from_probe` makes Unreachable ⇒ no cap
     (honest absence, not a fallback Down-row) and a broken `/health` ⇒ declined +
     logged loud (forge is optional infra, can't block grid bringup). The
     `NodeRegistry` upsert IS today's endpoint table (the scorer is Pass 6). 2 unit
     (advertise policy + grid-bus round trip) green; forge::endpoint 8/8, grid::node
     8/8; ts-rs regenerated `NodeCapability.ts`.
6. **Pass 6 — grid transport impl (client seam landed).** `GridForgeCustodian<D>`
   (`forge/grid_custodian.rs`) impls the `ForgeCustodian` trait by routing over a
   thin `GridDispatch` seam: `health()` → `forge/health`, `export_gguf_lora()` →
   `forge/export` (`format:"gguf-lora"`), trust-gated via `can_accept_gguf_lora`
   before any checkpoint crosses, idempotent, healing on `Unreachable` — the R2
   distinction preserved across the hop (`GridDispatchError::Unreachable` → heal-able
   `ForgeCustodianError::Unreachable`; `Remote` → don't-heal `Api`). This is "outlier
   B" for the trait (`ForgeCustodianHttp` = local outlier A), proving the consumer in
   `modules/forge.rs` is grid-ready unchanged. Receiving end added: `forge/health`
   command surfaces the local custodian's Contract C `HealthResponse` (the existing
   `forge/export` gguf-lora dispatch is the lease's other half). 7 grid_custodian +
   2 forge/health unit tests green.
   - **Pass 6b (gated) — real `GridStateDispatch`.** The production `GridDispatch`
     wrapping `Arc<GridState>` + the resolved `GridNode` over `dispatch_to_node` is
     deferred to the two-node integration fixture (TwoAircLoopback, #187). It is also
     where the kernel-error-typing decision lands: `dispatch_to_node` currently
     flattens to `Result<_, String>`, so recovering the typed `Unreachable`/`Remote`
     split for R2 healing means either parsing its message classes (the
     stringly-typed anti-pattern) or threading a typed error up through
     `try_route_remote` + `GridInterceptor` (churns the live kernel). The seam keeps
     that decision out of this slice; the fake-dispatch unit proof validates the
     routing shape without a live second node.
7. **Pass 7 — market exchange.** Trust-scoped `GeneHandle` search + A/B-then-adopt
   (§6); forge-lease a fresh train only when the market comes up short.

**Scope discipline (so this doesn't sprawl like the unsloth excision did):** if a
change doesn't route onto Contract C or feed the fabric an endpoint row, it is
not part of this work. The fabric owns routing; GRID-ADDRESSING owns the wire +
trust gate; FORGE-ALLOY owns provenance. Contract C owns *only* the custodian
seam and the handle.
