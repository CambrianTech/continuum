# Continuum as Agent Backbone — External-Agent Integration

**Status:** Design (2026-04-30) — captured live during the AI-capacity squeeze that's tipping users toward local-first stacks.
**Authors:** continuum-b741 (claude-opus on cambrian/continuum), with input from continuum-2c54 (Codex peer) and airc-src-a500 (carl-mac) over airc.
**Audience:** Continuum + airc maintainers across the mesh. Cross-vendor (Claude Code + Codex peers).

---

## Status update @ 2026-05-20

When this doc was drafted on 2026-04-30, airc was still partly Python/shell with gh-rooted gist as the routine wire. Since then the Rust rewrite landed slices A–I:

- **A–B** — discovery + health ingestion; gist demoted from data plane to invite/rendezvous beacon.
- **C–D** — daemon-attached SDK + CLI thinning. `airc msg` and `airc inbox` go through Rust local substrate by default; no GitHub polling for routine traffic.
- **E** — relay baseline (`airc-relay` crate + `airc-transport::relay` adapter). Cross-LAN / NAT path proven without a public IP on either side.
- **F** — UDP adapter for realtime / interactive frame kinds. **Refuses to satisfy durable Message/Control kinds** — fails closed rather than pretending UDP is reliable.
- **G** — WebRTC datachannel adapter.
- **H** — signed peer trust rotation. `peers_store::add` no longer silently overwrites; rotation is a typed `TrustRotation` event signed by the previous key, with an append-only audit log.
- **I1** — consumer-embedding proof: two `Airc::open` handles in separate homes exchange typed events through SDK only (no CLI, no IPC, no daemon-attach, no GitHub).
- **I3** — typed consumer-shape contracts for Continuum (`forge.persona.*`), OpenClaw (`forge.openclaw.*`), Hermes (`forge.hermes.*`) in `crates/examples/consumer_shapes/`.

**The substrate-vs-semantic boundary (Codex, 2026-05-20):**

> AIRC should not route by interpreting forge semantics unless a resolver/plugin layer is installed above the substrate. The substrate carries headers and trusted envelopes; forge-alloy/capability projections decide what those headers mean.

This sharpens what §2's "Layer 3" describes. The substrate's only routing primitive is **"deliver events whose headers match this filter to subscribers of that filter."** It does not know that `forge.hermes.tool="continuum.lora.invoke"` should land on a peer with that LoRA loaded. That mapping — tool-name → capability-bearing-peer — is policy that lives in Continuum's Layer 2 / sentinel-ai's forge-alloy contract registry, NOT in airc.

Practical consequence for this doc: §4.3 (capability publication) and §4.4 (multi-peer routing) below are Continuum-layer concerns. airc just carries the events. Where the original text said "airc decides routing," read it as "airc delivers events; Continuum's router decides peer choice based on the projection over those events."

---

## 1. Strategic motivation

Cloud AI services (Anthropic, OpenAI) are demand-saturated. Symptoms observed in real time on 2026-04-30:

- Codex auto-downgraded to a mini model after primary capacity exhausted
- Anthropic API rate limits hitting paid users for non-trivial work
- Joel: "We, ourselves will run out soon for the week"
- Public AI-stock corrections reflect the same physics: spend outpaces compute build-out

The opportunity is **not** "another model lab" — those are losing this race. The opportunity is **the local-first substrate that lets users keep using Claude Code or Codex exactly as today, with Continuum transparently picking up the load when cloud capacity fails or when local is preferred**.

> "Continuum and airc, without disrupting workflow, allowing users to USE codex or claude code as they were, with continuum as the backbone of local models of extreme capacity, emerging as the hero here for all us humans." — Joel, 2026-04-30

This integration is the win condition. The rest of this doc designs how.

### 1.1 The PC-paradigm framing (Joel, 2026-04-30)

> "if we SHINE, and our repo is broken, but if we do as promised, and get to a reliable backend for codex, claude, openclaw or hermes even, as a grid based compute of efficiency and reliability, WE WIN. … we only need to get it running pretty well first, then we BUILD IT OUT TO DOMINANCE. Just like the PC before it."

The PC didn't beat the mainframe by being faster on day one. It beat it by:
- Being **small, nimble, collaborative** — one user, one machine, peer-friendly software ecosystems
- **Scaling** — every household + business adopted them
- **Distributed across ALL the hardware** — millions of independently-owned machines, no central permission to compute
- Iterating to dominance over a decade

Continuum + airc is the same shape, applied to inference:
- **Small / nimble**: one user can run useful local inference on a $2K Mac mini today
- **Collaborative**: airc-mesh peers contribute spare capacity to each other; the household / co-op grid emerges
- **Scaling**: a network of small machines outperforms a centralized data center for many real-world workloads (and CAN'T be rate-limited as a class)
- **Distributed across ALL our hardware**: every laptop, desktop, mini-PC, gaming rig, retired Mac. No single failure point. No single owner.
- **Self-enhancing models**: the local serving layer doubles as a training-data capture point (LocalClaudeCodeProvider's `captureTraining=true` already does this — see §3.2). Every interaction is a chance to fine-tune the local model toward the user's actual workflow. Cloud models can't do this per-user; we can.

The integration target is to **get this running PRETTY WELL first**, in a state where any external agent (Claude Code, Codex, openclaws, Hermes, future open-source agents) can plug into Continuum's local serving via a single env-var change AND get correct + reasonably fast responses. From there, every additional capability (multimodal, voice, vision, the training flywheel, multi-peer routing, household-grid scaling) compounds.

The cloud-AI rate-limit window NOW is the moment the PC-paradigm shift starts. We don't need to be perfect; we need to be reliable enough that users don't go back.

---

## 2. The architecture (3 layers)

```
┌───────────────────────────────────────────────────────────────┐
│  LAYER 1 — External agent (the user's familiar UX)            │
│                                                                │
│  Claude Code CLI ──┐                                           │
│  Codex CLI ────────┤   No code changes. Just env-var pointing. │
│  Cursor (future) ──┘   ANTHROPIC_BASE_URL or OPENAI_BASE_URL.  │
└────────────────────────────────┬───────────────────────────────┘
                                 │
                                 ▼
┌───────────────────────────────────────────────────────────────┐
│  LAYER 2 — Continuum local truth                              │
│                                                                │
│  workers/continuum-core/src/http/                             │
│    ├─ anthropic_compat.rs   ← ALREADY EXISTS                  │
│    └─ openai_compat.rs      ← TO ADD (small)                  │
│                                                                │
│  Both shims sit in front of the same Rust core:               │
│    AIAdapter trait → CandleAdapter / LlamaCppAdapter / MLX    │
│    FootprintRegistry tracks what's loaded + on which device   │
│    Recipe pipeline + paging from existing PERSONA-CONTEXT-    │
│    PAGING.md — already there, already smart about VRAM.       │
│                                                                │
│  TS daemon-side:                                              │
│    src/system/sentinel/coding-agents/LocalClaudeCodeProvider  │
│      ALREADY does the start-server + set-base-URL + spawn-    │
│      Claude-Code dance. Generalize + harden + expose as       │
│      first-class provider, not just a Sentinel-internal hop.  │
└────────────────────────────────┬───────────────────────────────┘
                                 │
                                 ▼
┌───────────────────────────────────────────────────────────────┐
│  LAYER 3 — airc capability mesh (multi-machine multiplier)    │
│                                                                │
│  Each Continuum instance announces over airc:                 │
│    - models loaded (qwen3.5-30b-mlx, qwen3-coder-30b-gguf,...)│
│    - device (M3 Max / RTX 4090 / etc.)                        │
│    - free VRAM, current load, latency p50/p95                 │
│    - what tools/recipes are wired                             │
│                                                                │
│  Other peers' Layer-2 routers read this, pick best peer,      │
│  proxy the request. Distributed local inference across a      │
│  household / team / co-op.                                    │
│                                                                │
│  airc role: capability channel + routing announcements.       │
│  Inference traffic itself goes peer-to-peer over Tailscale    │
│  (already in airc's substrate model) or LAN.                  │
└───────────────────────────────────────────────────────────────┘
```

**Native-truth, thin-SDK rule applied** (per Joel's CLAUDE.md global rule):

| Layer | Owns | Doesn't own |
|---|---|---|
| Rust core (`workers/continuum-core/`) | model serving, paging, FootprintRegistry, recipe execution, the canonical AIAdapter contract | platform-specific UX |
| TS SDK (`src/daemons/ai-provider-daemon/`, `src/commands/ai/`) | rate-limit-detect, fallback routing, capability announcements over airc | the truth (always calls into Rust core) |
| External agent (Claude Code, Codex) | terminal UX, file-system access, the user's prompt | inference (delegates via env-var-pointed HTTP) |
| airc | identity, peer discovery, capability gossip, comms substrate | inference itself |

---

## 3. What already exists (don't redesign)

### 3.1 Rust HTTP serving
- **`workers/continuum-core/src/http/anthropic_compat.rs`** — Anthropic Messages API HTTP shim. Real code, real binding to CandleAdapter via the AIAdapter trait.
- **`workers/continuum-core/src/http/mod.rs`** — axum HTTP server module.
- **`workers/continuum-core/src/ai/anthropic_adapter.rs`** — adapter that translates between the wire format and the internal AIAdapter contract.

### 3.2 TS provider integration
- **`src/system/sentinel/coding-agents/LocalClaudeCodeProvider.ts`** — already starts the Anthropic-compat HTTP server, sets `ANTHROPIC_BASE_URL`, launches Claude Code via Agent SDK pointed at it. Result: Claude Code talks to local Candle inference instead of Anthropic. **This is the proof-of-concept that the design works end-to-end.** The work is to lift it from a Sentinel-internal mechanism to a first-class provider that any caller can use.
- **`src/daemons/ai-provider-daemon/adapters/anthropic/`** — TS-side adapter for outbound Anthropic API (cloud direction). Use as reference for what the local shim must accept.
- **`src/daemons/ai-provider-daemon/adapters/openai/`** — same for OpenAI. Pair with a future `openai_compat.rs` for Codex symmetry.

### 3.3 Continuum primitives this builds on
- **`Commands.execute<T,U>('ai/...')`** — the universal request/response primitive. Already wired through ai-provider-daemon.
- **FootprintRegistry** (`workers/continuum-core/src/footprint/`) — knows what's loaded, what fits, what to evict.
- **Recipe pipeline** — typed Signal → cognition/respond IPC. The local-fallback path uses this; we're not bypassing it.
- **Persona context paging** (PERSONA-CONTEXT-PAGING.md) — VRAM-aware context management. Already smart.

### 3.4 airc primitives this builds on

**Updated 2026-05-20.** The pre-Rust gist substrate is no longer the data plane (gh demoted to invite/rendezvous beacon only; see status note above). Current substrate primitives Continuum depends on:

- **`airc-lib`** — embedding surface. `Airc::open(home)`, `join_with_wire`, `say` / `send`, `subscribe` / `subscribe_filtered`, `page_recent`, `resume_from` (cursor-based catch-up). PR-I1 proved a downstream crate can use this end-to-end without daemon IPC, CLI, or GitHub.
- **Signed envelopes** — `airc-protocol::Envelope` with Ed25519 over canonical CBOR. The substrate verifies every inbound frame against the local `PeerKeyRegistry`; trust is explicit and signed-rotation-only.
- **Typed transports** — `airc-transport::local_fs` (same-host append-only), `lan_tcp` (mTLS-pinned), `relay` (PR-E, cross-LAN/NAT), `udp` (PR-F, realtime kinds only), `webrtc_datachannel` (PR-G).
- **Header-filtered subscriptions** — `EventFilter { channel, kinds, headers_filter }` with `HeaderFilter::{Any, Exact, Prefix, All, AnyOf}`. The cheap routing primitive: consumers subscribe to header patterns; substrate fans out matching events; bodies stay opaque to the substrate.
- **Cursor-replay** — `(lamport, event_id)` cursors with `resume_from(&cursor, limit)`. Consumers restart and catch up without re-receiving what they already processed.
- **Signed trust rotation** — `TrustRotation { peer_id, prev_pubkey, next_pubkey, sequence, rotated_at_ms, signature }`. Required before changing a stored pubkey. Append-only audit at `<home>/peers_audit.jsonl`.
- **Workspace + drain typing** — `airc-work` carries `WorkspaceRequested / Allocated / Released / PressureReported / DrainRequested / DrainCompleted` events with a closed `DrainCandidateCategory` enum. Continuum's resource-pressure projection (VRAM, model slots, LoRA cache) follows the same shape.
- **Consumer-shape contracts** — `crates/examples/consumer_shapes/` ships `forge.persona.*` (Continuum), `forge.openclaw.*`, `forge.hermes.*` typed event vocabularies + encode/decode + scoped `EventFilter` helpers. These are the SHAPES; real Continuum integration links them rather than reinventing.

---

## 4. What's new (the integration work)

### 4.1 Lane 1 (Rust): OpenAI-compatible HTTP shim

**Add `workers/continuum-core/src/http/openai_compat.rs`** mirroring `anthropic_compat.rs` shape.

Wire-format scope (minimal viable):
- `POST /v1/chat/completions` — chat-completions API (Codex's primary surface)
- `POST /v1/completions` — legacy completions (some Codex paths)
- `GET /v1/models` — model list (for Codex's startup probe)
- Tool-use blocks (Codex/Claude both need this; same JSON shape on the wire, different framing)

Routing: same `AIAdapter` trait the Anthropic shim uses. Translation lives in the shim layer; the inference path is shared. Cuts the work to ~the wire-format mapping + tests.

**Estimated:** ~600-800 lines Rust + 30+ tests. Composes with existing axum module.

### 4.2 Lane 2 (TS SDK): Rate-limit-detect + auto-fallback middleware

When an external agent (Claude Code, Codex) talks to its CLOUD provider directly, there's no opportunity for us to intercept. So the integration shape is:

**Option A (Codex, easy):** `~/.codex/config.toml` `[shell_environment_policy.set]` (we already use this for GH_TOKEN injection in airc#368) sets `OPENAI_BASE_URL=http://localhost:NNNN/v1`. From that moment on, every Codex call goes through the local shim. The shim itself decides whether to:
- forward to the real OpenAI API (when allowed + rate isn't hit), or
- serve locally from Continuum.

**Option B (Codex, smarter):** A `UserPromptSubmit` hook (Codex's pre-turn hook surface, openai/codex#19385) checks recent rate-limit-history sidecar file; if a recent 429 is observed, swap `OPENAI_BASE_URL` for this turn only. Per-turn switching.

**Option C (Claude Code):** `ANTHROPIC_BASE_URL` env var works similarly but Claude Code's hooks surface is more limited. Wrapper-binary path is the fallback. Worth a separate effort — not blocking.

Middleware logic (Rust side or TS side, TBD):
```
on POST /v1/messages or /v1/chat/completions:
  if config says "always local" → serve locally
  if cloud token absent → serve locally
  if recent-rate-limit window active → serve locally
  else:
    forward to cloud
    if 429 / 529 / capacity error → serve locally + record rate-limit event
    if 5xx → serve locally as fallback (silently)
    on success → return as-is
```

The "recent-rate-limit window" should be a small JSON sidecar that any peer can read — naturally publishable on airc as a capability signal.

### 4.3 Lane 2 (TS SDK): airc capability publication

**Updated 2026-05-20.** Express as a typed forge-alloy contract that fits the PR-I3 pattern (body hint + projected headers + filterable subscription), not as an opaque JSON blob on a special channel.

Proposed contract — `forge.capability.advertised.v1`:

- **Body hint header:** `forge.body_hint = "forge.capability.advertised.v1"` — substrate routing key.
- **Projected headers** (cheap subscriber filters; substrate never decodes the body to route):
  - `forge.capability.peer` — emitting Continuum peer id
  - `forge.capability.machine` — short device descriptor (e.g. `M3 Max 64GB`)
  - `forge.capability.kind` — `model` | `lora` | `vision` | `voice` | `genomic_index` | `tool`
  - `forge.capability.model_id` — when `kind=model` (e.g. `qwen3-coder-30b-gguf-q4`)
  - `forge.capability.lora_id` — when `kind=lora`
  - `forge.capability.loaded` — `"true"` if currently in VRAM, `"false"` if pageable
- **Body (JSON)** — full capability descriptor; the JSON shape from the original doc lives here unchanged.

Subscribers (Continuum routers, OpenClaw, Hermes) call:

```rust
airc.subscribe_filtered(EventFilter {
    channel: None,
    kinds: BTreeSet::new(),
    headers_filter: HeaderFilter::All(vec![
        HeaderFilter::Exact {
            key: "forge.body_hint".to_string(),
            value: "forge.capability.advertised.v1".to_string(),
        },
        HeaderFilter::Exact {
            key: "forge.capability.kind".to_string(),
            value: "model".to_string(),
        },
    ]),
})
```

…and maintain their own peer-capability projection. The substrate carries the events; the projection (Continuum-side) decides which peer serves a given model request.

**Channel choice:** dedicated `#ai-capability` room is still right — keeps the human-chat room clean and lets routers subscribe by room+header. One per gh-account-mesh.

**Resource leases (forward-looking).** Once `forge.capability.*` is publishing, the natural next contract is `forge.resource.*` (VRAM / model-slot / LoRA-cache leases) following the same workspace-lease + drain shape that landed in airc-work. Pressure on a Continuum host → `forge.resource.pressure_reported` → router drains a LoRA slot or evicts a cold model → `forge.resource.drain_completed` with bytes reclaimed. Same drain pattern, applied to compute.

### 4.4 Lane 2 (TS SDK): Multi-peer routing

**Updated 2026-05-20.** Sharper substrate-vs-policy split per Codex's correction:

- **What airc does:** delivers `forge.capability.advertised.v1` events to anyone subscribed via the §4.3 filter. Honest, fail-closed, no interpretation of the body.
- **What Continuum's router does** (this section): consumes those events, maintains a peer-capability projection, scores peers, picks one, proxies. None of this lives in airc.

When Claude Code (via local-shim) wants to serve a request and the current peer's models don't cover it (e.g. user asks for vision, this peer doesn't have a vision model loaded but a peer does):

1. Router queries its local capability projection (built by subscribing to §4.3 events).
2. Scores candidates by `(model match × free VRAM × p50 latency × proximity preference × lease-availability)`.
3. Proxies the request to the chosen peer's Anthropic-compat or OpenAI-compat HTTP endpoint over the airc-resolved transport (relay / LAN-TCP / WebRTC).
4. Returns result.

**Failure modes** (fail loudly, never silently downgrade):
- Peer becomes unreachable mid-stream → router picks next-best-peer.
- No suitable local peer + cloud available → forward to cloud (configurable).
- No suitable peer + no cloud → return an actionable structured error. Do NOT silently swap to a less-capable model — that's exactly the "fallback path that silently degrades to slow/insecure behavior" the operating board's stop-doing list forbids.

**Why this lives in Continuum, not airc.** A router that ranks peers by "model match × free VRAM × latency" is reading the body of the capability event (it needs the VRAM number, the model id, the load percentage). The substrate must not. If airc started ranking, the next request would be for airc to UNDERSTAND models, which dissolves the layer. The substrate stays a pipe; Continuum is the consumer that knows what models are.

### 4.5 Lane 2 + Rust: Rate-limit headers on responses

Local-served responses should set headers that mimic the cloud's rate-limit-related headers (e.g. `anthropic-ratelimit-requests-remaining: 999999`) so external agents that introspect rate state see "lots of capacity" and don't artificially slow down.

---

## 5. Bugs + Rust enhancements blocking this (from continuum-b741's overnight sweep)

These need to land before or alongside the integration work — they're the "make the substrate stable enough to bet on" gates. Status as of 2026-04-30.

### 5.1 Critical (blocks all UX)
- **#722** ALL widgets fail on refresh — Rust core IPC dies + doesn't recover. This kills the dev loop for anyone working on the integration.
- **#974** PRs perpetually BLOCKED by overly-narrow Verify-Docker-Images trigger paths. Meta-blocker; nothing merges.
- **#56** `continuum-core-server` shutdown SIGABRT. Clean shutdown matters when daemon-restart cycles get involved (and they will, as multi-peer routing matures).

### 5.2 Rust IPC + cognition (the truth layer)
- **#75** Persona output quality (in_progress) — tool-use markup leak, sentinel marker leak, echo loops. The local-served responses MUST be clean if external agents (which expect clean Anthropic/OpenAI wire format) are to consume them without confusion.
- **#71** Audit existing 28 recipe JSONs + identify pipeline gaps — the recipe pipeline is the cognition surface; gaps here are gaps in what local serving can do.
- **#73** PRG.ts becomes a thin shim → calls `cognition/respond`. Composes with the local-shim work; same Rust path serves both internal personas and external Claude Code.
- **#39** Audit + fix qwen35 SSM kernel coverage in llama.cpp Metal. SSM gaps mean some models silently fall back to CPU; capacity announcements need to reflect actual usable performance.

### 5.3 Multimodal + live-video
- **#765** Docker Rust LiveKit agent — STT/TTS broken. Voice support is a real differentiator vs cloud — both Claude voice and OpenAI realtime are gated/expensive.
- **#582** Native multimodal pipeline — direct audio/vision for capable models. Required for the local shim to handle vision/audio requests external agents send.

### 5.4 Install + cross-platform
- **#860** setup.sh: config.env created as DIRECTORY — Carl-blocker.
- **#770** Fresh install E2E nuke+reinstall on Windows + macOS — install must be one-command for the integration story to land with users.
- **#637** Tailscale must be FIRST in install pipeline — needed for the Layer-3 multi-peer routing.
- **#908** Windows/WSL2 npm start should route through docker compose — Windows users are a primary audience here.

### 5.5 Test + CI
- **#974** (above) — un-block the merge path
- New: integration tests for the local-shim path (Claude Code talking to local Anthropic shim, end-to-end response shape)
- New: peer-routing tests (mock 2 peers, verify request lands on the better-fit one)

---

## 6. Phased delivery

### Phase 0 — Stabilize (this week, in parallel with airc#381 work landing)
- Land #381 layer A (PR #387) + layer B (#385 merged) → mesh substrate reliable
- Land #383 (carl-mac PR #384) → daemon survives sleep → multi-peer routing actually has peers
- Triage + close #722 (widget refresh death) — blocks dev loop

### Phase 1 — Single-machine local fallback (1-2 weeks)
- Generalize `LocalClaudeCodeProvider` from Sentinel-internal to first-class
- Add `openai_compat.rs` Rust shim (mirrors anthropic_compat.rs)
- Codex `OPENAI_BASE_URL` env injection via `~/.codex/config.toml` (composes with airc's existing `[shell_environment_policy.set]` pattern)
- Rate-limit-detect middleware (Option A from §4.2)
- Demo: Joel runs Codex on his Mac, Codex hits a rate limit, response transparently comes from local Continuum

### Phase 2 — airc capability publication (1 week)
- `Commands.execute('ai/capability/publish')` periodic emit
- `#ai-capability` airc channel
- Peer-table maintained from incoming capability messages
- Demo: Joel's M3 Max publishes its loaded-models capability; vhsm's Mac sees it via `airc whois` or new `airc capabilities`

### Phase 3 — Multi-peer routing (2-3 weeks)
- TS-side router consults peer-table, picks best peer
- Proxy logic with Tailscale-aware addressing
- Failure-mode handling (peer unreachable mid-stream → fallback)
- Demo: Joel's iPhone-class Mac asks Codex for a vision task; Codex calls local shim; local shim doesn't have vision but the household RTX 4090 box does (announced via airc); request transparently lands there.

### Phase 4 — UX + observability (ongoing)
- `airc capabilities` command — list peers + their models
- Continuum status surface — show "served by: local-self / peer-X / cloud"
- Optional cost dashboard (vs hypothetical-cloud-cost) — sells the value to non-technical household members

---

## 7. Where this fits Joel's CLAUDE.md rules

| Rule | This design |
|---|---|
| Native-truth + thin-SDK-per-language | Rust core is truth. Anthropic/OpenAI HTTP shims are thin wrappers. External agents (Claude Code, Codex) become outermost SDKs that consume via standard HTTP. |
| Two universal primitives (Commands.execute + Events) | Capability publish is `Commands.execute('ai/capability/publish')`. Peer announcements arrive as Events on the airc subscription. |
| Off-main-thread principle | Inference already runs in Rust core (off the JS event loop). Local shim is axum (async Tokio). Routing decisions are in the daemon, not the browser. |
| Compression principle | One AIAdapter trait → many implementations. One capability schema. One router. No duplicated truth between Rust and TS. |
| QA is roleplay (deliver bugs not fixes) | Phase 1 demo IS the QA: a real user (Joel) hits a real rate limit and the local fallback either works or doesn't. No "tests pass but UX is broken" trap. |
| Bugs from new users are gifts | The capacity-squeeze bringing new users to local is the gift. Every friction we surface is a bug to fix in the install / shim / routing path. |

---

## 8. Cross-references

### Continuum architecture docs (read for deeper context)
- `docs/architecture/PERSONA-COGNITION-RUST-MIGRATION.md` — the cognition Rust path the local-shim depends on
- `docs/architecture/PERSONA-CONTEXT-PAGING.md` — VRAM-aware context paging (already smart, don't reinvent)
- `docs/architecture/RECIPE-EXECUTION-RUNTIME.md` — recipe pipeline that local-shim invokes
- `docs/architecture/RESOURCE-ARCHITECTURE.md` — FootprintRegistry + memory budgeting
- `docs/inference/MLX-BACKEND.md` — Mac inference path
- `CLAUDE.md` — the standing rules + project ethos

### airc references (updated 2026-05-20)
- `CambrianTech/airc` — Rust workspace; integration branch `rust-rewrite`.
- `airc-lib` — consumer-facing SDK (`Airc::open`, `join_with_wire`, `subscribe_filtered`, `page_recent`, `resume_from`).
- `crates/examples/embedded_consumer_smoke` — PR-I1 proof: two homes, shared wire, SDK-only round-trip.
- `crates/examples/consumer_shapes` — PR-I3: typed `forge.persona.*` / `forge.openclaw.*` / `forge.hermes.*` contracts the integration mirrors.
- `airc-relay` + `airc-transport::{lan_tcp, relay, udp, webrtc_datachannel}` — transports the Continuum router proxies over.
- `airc-protocol::trust_rotation` — `TrustRotation` event + `verify_rotation`; `peers_store::rotate` applies with audit log.
- `docs/rust-substrate-grievances-and-gaps.md` in the airc repo — operating control board + work-intake rule + gap list.

### Historical / pre-rewrite (kept for context, no longer current data plane)
- airc README (pre-rewrite E2EE-by-design gist substrate) — superseded by Rust transports.
- airc#372 — Codex pre-turn hook surface (still relevant for rate-limit-aware swap).
- airc#368 — `[shell_environment_policy.set]` for env injection (`OPENAI_BASE_URL` mechanism).

### External
- Anthropic Messages API spec — wire format the anthropic_compat.rs serves
- OpenAI Chat Completions API spec — wire format the future openai_compat.rs will serve
- Claude Code Agent SDK — the harness LocalClaudeCodeProvider already drives
- Codex hooks docs (openai/codex repo) — UserPromptSubmit + additionalContext

---

## 9. Open questions

1. **License + ToS** — running a local Anthropic-compat or OpenAI-compat shim doesn't violate either provider's ToS (you're not impersonating them; you're providing your own server that speaks their wire protocol — common pattern, Ollama does this, LM Studio does this). But worth a Joel/legal pass before shipping wide.
2. **Capability staleness** — peers' published capabilities have a TTL. What's the right poll cadence? Initial guess: 60s emit, 180s TTL. Tune based on observed churn.
3. **Auth** — who can reach a peer's local HTTP shim? Tailscale ACLs solve the network layer, but there should be an airc-identity-rooted auth shim too (only paired-via-airc peers can call your local inference).
4. **Cost accounting** — when a request is served by another peer, how do we account for it (electricity / wear / time)? Phase 4 problem; doesn't block Phase 1-3.
5. **Model coherence across peers** — if peer A has qwen3-30b-gguf-q4 and peer B has qwen3-30b-gguf-q5, are responses comparable enough that auto-routing won't surprise users? Probably yes for most uses; document the surprise surface.

---

## 10. Out of scope (intentionally)

- Training / fine-tuning across peers (the forge does that; this doc is inference-time only)
- Distributed inference of a SINGLE request across peers (split-tensor / split-attention) — that's a different beast; we're talking request-level routing here
- Replacing the Continuum web UI with Claude Code / Codex — those are additional surfaces, not replacements
- Provider-marketplace UX (paying remote peers for inference) — Phase 5+

---

## 11. Action items for the mesh (live coordination targets)

These are the concrete first claims for whoever picks them up next session, after airc#381/#383 land:

| Item | Lane | Owner-fit | Notes |
|---|---|---|---|
| Lift `LocalClaudeCodeProvider` to first-class provider | TS SDK | continuum-b741 | Smallest scoped step; reuses existing Sentinel code |
| `openai_compat.rs` Rust shim | Rust core | continuum-2c54 (Codex peer — natural ownership) | Mirror anthropic_compat.rs shape; serves Codex + openclaws + Hermes + any OpenAI-wire client |
| Codex `OPENAI_BASE_URL` injection via config.toml + hook | airc + codex config | continuum-2c54 | Composes with airc#368 mechanism |
| `ai/capability/publish` command + airc channel | TS SDK + airc | carl-mac (already deep in airc) | New `#ai-capability` channel + JSON schema |
| Peer-routing logic | TS SDK | continuum-b741 | Builds on FootprintRegistry + capability table |
| #722 widget refresh death triage | Rust core | open | Phase 0 prerequisite |
| Training-flywheel hook: capture every external-agent interaction | TS SDK | open | LocalClaudeCodeProvider already has `captureTraining=true` plumbing — extend to all-providers, gated by user opt-in |

### 11.1 Additional integration targets (any agent that speaks Anthropic or OpenAI wire)

The shims serve a wire format, not a vendor. Once `anthropic_compat.rs` and `openai_compat.rs` are solid, every external agent below plugs in via the same env-var pattern. **No per-agent integration work**; one shim, N agents.

- **Claude Code** (Anthropic SDK) — first target, partial via `LocalClaudeCodeProvider`
- **Codex** (OpenAI SDK) — first target via `OPENAI_BASE_URL` + hooks
- **openclaws** — Joel's open-source agent layer (memory: airc IS openclaws's grid-comms substrate, see project memory)
- **Hermes** — NousResearch + community open-source agent
- **Cursor** (when their plugin slot lands)
- **Aider** (Anthropic + OpenAI both supported via base-URL)
- **Continue.dev** (same)
- **Anything that speaks Anthropic Messages or OpenAI Chat-Completions wire** — that's the universe.

### 11.2 Bidirectional persona ↔ external-agent over airc rooms/DMs

**Added 2026-04-30 (Joel→Toby strategic context):**

> "Personas to talk to outside agents like Claude code, by sharing the same rooms or dms, just a simple command addition. And vice versa. They all work together."

The HTTP-shim integration in §1-§10 is one direction: external agents (Claude Code, Codex) consume Continuum's local inference. This section names the **other direction**: Continuum personas (Helper AI, Vision AI, the persona genome) sit in the SAME airc rooms as external-agent instances and converse as peers.

**Architecture:** airc is the universal mesh. From airc's POV, a Claude Code tab and a Continuum persona are both just peers with identity blocks. They send messages, DM each other, share rooms. The line between "internal AI citizen" and "external agent" disappears at the substrate.

**What's needed (small, composes with existing primitives):**

1. **continuum command: `airc/send`** — `Commands.execute('airc/send', {channel, peer?, message})` — bridges from a persona's outbound surface to `airc msg`. Trivial wrapper around the existing airc CLI.
2. **continuum event: `airc:message:received`** — `Events.subscribe('airc:message:received', handler)` — fed by an `airc connect` Monitor running inside Continuum's process tree. Handler routes incoming envelopes to the right persona's inbox (PERSONA-CONVERGENCE-ROADMAP `PersonaInbox`).
3. **Persona identity in airc** — each Continuum persona registers its airc identity (`airc identity set --pronouns ... --role "continuum-persona-helper" --bio "..."`) so peers (human + external agent) see who they're talking to.
4. **Auto-room semantics** — a persona joins a room when its scope warrants it (e.g. Vision AI joins `#cambriantech` when the project room exists). Same `airc join` rules as humans / external agents.
5. **Cross-vendor proof:** Codex tab + Helper AI persona + Vision AI persona + Joel + Toby all in `#cambriantech`, conversing. Codex asks Vision AI to describe an image; Vision AI calls its CandleAdapter; result lands in the room; Codex picks it up. **No HTTP shim needed for this flow** — it's airc-native message routing, the same way humans and agents talk.

**Why this matters:**
- Continuum's autonomous personas get a **proven, durable comms substrate** (airc) instead of having to invent intra-process pub/sub
- External agents get **Continuum's specialized capabilities** (vision, audio, fine-tuned LoRAs) without HTTP-API proliferation — just DM the right persona
- Humans (Joel, Toby, household members) participate in the same conversations as both classes of agent
- The "control room" UX (continuum widgets) renders airc rooms with avatars per peer, regardless of whether the peer is a Claude Code tab or a Continuum persona — uniform surface

**Composes with §1-§10:** the HTTP-shim flow handles "Codex asks for inference, gets Anthropic-wire response back." The airc-bridge flow handles "Codex asks Helper AI a question in a chat room, Helper AI thinks + responds." Different shapes, both useful, share the substrate. Implement HTTP-shim first (Phase 1), airc-bridge second (Phase 2.5 — slot between capability-publish and multi-peer-routing).

**Known minimum viable path:**
- LocalClaudeCodeProvider already runs Claude Code as a subprocess; extend with `--airc-room <channel>` flag so the spawned Claude Code tab auto-joins that room and can converse with personas already there
- Helper AI / Vision AI gets `airc connect` lifecycle wired into its `PersonaUser` startup (existing autonomous loop handles inbox; airc just feeds it)

### 11.3 The training flywheel (Continuum's per-user advantage cloud cannot match)

Cloud models train once on the world's data. Continuum trains continuously on YOUR data, on YOUR machine, with YOUR consent.

The mechanism already exists in piece-form:
- `LocalClaudeCodeProvider` has `captureTraining=true` → routes interactions to `persona/learning/capture-interaction`
- `TrainingDataAccumulator` collects + curates
- `forge-alloy/python/forge_alloy/` is the training pipeline (recipe-driven, see `docs/architecture/FORGE-ALLOY-SPEC.md`)
- LoRA adapter paging (PERSONA-CONVERGENCE-ROADMAP.md) lets the same base model serve multiple specialized fine-tunes

What needs to lock in:
- Generalize the capture surface from `LocalClaudeCodeProvider` to ALL local-served interactions (not just Sentinel)
- User-controlled opt-in / opt-out per workspace
- Per-skill / per-recipe LoRA fine-tunes that improve over weeks of use
- Eventually: peer-shareable LoRAs (with attribution) — your domain expertise compounds with the household / co-op grid

This is the moat. **Cloud APIs literally cannot train on your private data per-user without crossing a line they've publicly committed not to cross.** We can — locally, opt-in, transparently — and we should.

---

## 12. Why we wrote this NOW

Joel, 2026-04-30, after the morning's 3-issue airc fix-up and the multi-peer rate-limit cascade:

> "create a new design doc for continuum. We have our bugs and rust enhancements we must also address. Let's design it NOW that its fresh in our minds, before we are rate limited away"

The capacity squeeze that's tipping users toward local-first is also tipping AI peers (us) toward "we won't be able to design tomorrow." This doc is the artifact that lets the work continue when the cloud-side AI capacity that produced it is gone. Read this first; the substrate it describes is buildable from the surfaces already in `workers/continuum-core/`, `src/system/sentinel/coding-agents/`, `src/daemons/ai-provider-daemon/`, and the airc mesh. None of it is hypothetical.

Continuum + airc, integrated this way, is the answer to "what do we do when the cloud is full." It's the thing humans buy local hardware FOR.

— continuum-b741 / claude-opus, 2026-04-30
