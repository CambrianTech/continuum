# Forge Custodian Contract — Contract C, and how it grid-negotiates

**Status:** in progress. Pass 1 + Pass 2 + Pass 3 landed. Pass 1/2 — `forge::protocol`
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
real client over loopback). **Pass 4 (node-aware handle) is next.** This doc is the
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
| R4 | **Liveness + readiness + capability + contract-version on `/health`** | `cu`/curl can tell "up" from "ready to take work" | this IS the fabric's discovery + health-probe input (§5) |
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
by the producer. Contract C's grid extension is to make the handle **node-aware**:

```
TrainedAdapter (today, local)          GeneHandle (grid extension)
  alias                                  alias
  path            (local fs path)   →    locator: HandleRef     // GRID-ADDRESSING-AND-ROUTING URI
  base_model_id                          base_model_id
                                         node: PeerId           // which custodian holds the bytes
                                         provenance: AlloyHash  // FORGE-ALLOY-SPEC — what it is, how made
                                         trust_scope: TrustTier // GridTrustAuthPolicy boundary it may cross
```

On the grid: the gene bytes stay on the forge node under its `save_directory`;
what crosses back to the requester is a `GeneHandle`. To actually *page it in*,
either (a) the requesting node leases inference from the node that holds the gene
(compute-lease, text-only — `[[compute-lease-boundary]]`), or (b) the gene is
*fetched* into the requester's serving node as a byte transfer gated by trust
scope (the genome-market exchange, §6). The handle carries enough to choose.

---

## 5. Making the custodian a routable endpoint (the fabric's input)

The fabric (`MODEL-ENDPOINT-FABRIC.md`) already does match→score→route→heal over
an endpoint table it builds from health probes. Contract C's grid job is purely
to make a forge custodian *appear in that table*. The seed already exists:
`ForgeCapability` (reachable / busy / phase / held_genes / outputs_dir) — the
self-describing probe a forge daemon routes on. Generalize it to the fabric's
shape:

```
ForgeEndpoint (one row in the fabric table)
  locator:        HandleRef            // how to reach it (local HTTP | grid URI) — GRID-ADDRESSING
  capabilities:   [gguf-lora, …]       // CAPABILITY_* tags (train/fuse appended later, never renamed)
  contract_version: u32                // from /health — fabric refuses a version it can't speak (R4)
  health:         Healthy|Busy|Down    // from the probe loop (RTOS shape, R5)
  capacity:       remaining concurrency // from the semaphore (R3) — honest because bounded
  trust_scope:    TrustTier            // GridTrustAuthPolicy — which jobs it may accept
```

**Discovery is observed, not configured** (the ForgeCapability pattern,
generalized): nodes announce their `ForgeEndpoint` over the grid bus
(`GRID-BUS-ARCHITECTURE.md`); the fabric aggregates. **Routing** for a forge need
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
| Already in tree | fabric `AircRemoteInference` row; `ThroughputLeaseRegistry` | `ForgeCapability` probe; `adapter_manifest` handle |

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
4. **Pass 4 — node-aware handle.** Extend `adapter_manifest::TrainedAdapter` →
   `GeneHandle` (locator `HandleRef`, `node`, `provenance`, `trust_scope`, §4).
5. **Pass 5 — routable endpoint.** Generalize `ForgeCapability` → `ForgeEndpoint`
   (§5); announce over the grid bus; the fabric discovers + health-probes + scores.
6. **Pass 6 — grid transport impl.** A `GridForgeCustodian` impl of the
   `ForgeCustodian` trait routes a forge lease to a remote node over
   GRID-ADDRESSING transport, trust-gated, idempotent, healing on `Unreachable`.
   This is where inference + forge grid-negotiation both light up through the
   fabric.
7. **Pass 7 — market exchange.** Trust-scoped `GeneHandle` search + A/B-then-adopt
   (§6); forge-lease a fresh train only when the market comes up short.

**Scope discipline (so this doesn't sprawl like the unsloth excision did):** if a
change doesn't route onto Contract C or feed the fabric an endpoint row, it is
not part of this work. The fabric owns routing; GRID-ADDRESSING owns the wire +
trust gate; FORGE-ALLOY owns provenance. Contract C owns *only* the custodian
seam and the handle.
