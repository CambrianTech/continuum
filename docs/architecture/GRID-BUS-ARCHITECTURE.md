# Continuum Grid Bus Architecture

**Status:** Design (2026-05-25). For review by codex (airc substrate side) + claude-tab-1 (Lane C2 / airc-adapter side) + Joel.
**Author:** Claude (worktree `generateresponse-pr1`), dictated by Joel through the airc thread on 2026-05-25.
**Scope:** Defines how Continuum integrates with airc as the universal bus — for Events, for Commands, for cross-grid coordination, for contract-grounded compute exchange.
**Supersedes the in-flight patch architecture:** the `continuum-airc-bridge.mjs` shell-out, `modules/airc.rs` bespoke IPC commands, `persona/airc_admission.rs` protocol-named admission converter, and the dual-write PR stack (#1432/1433/1435/1436/1437). Those land back as straight-line consumers of this architecture or get deleted.

---

## Executive summary

Three claims, then the rest of the doc supports them:

1. **airc is the durable event log + the inter-grid transport.** Not a foreign protocol that needs translation into ORM. The current integration treats it as the second; this doc treats it as the first.
2. **Continuum's existing universal primitives (`Commands.execute`, `Events.subscribe/emit`) extend onto airc with one new piece of metadata per class: `scope` for Commands, `broadcast` for Events.** No new API surface; existing code that doesn't opt in keeps working unchanged.
3. **Each Continuum install is an autonomous router on the grid (BGP-style), publishing what it offers and what it wants, contracting with peers through forge-alloy-grounded terms.** No central scheduler. No master. Routing emerges from per-peer policy + capability advertisement + bid negotiation, lamport-ordered on the airc log.

The "real solid rust rework" Joel asked for IS this architecture. The shim/patch work that preceded it gets cleaned up as a downstream consequence.

---

## 1. The cut — what's airc, what's ORM

### 1.1 Two storage tiers

| Tier | Lives in | What it holds | Why |
|---|---|---|---|
| **Entity tier** | ORM (sqlite/postgres via DataDaemon) | Personas, rooms, settings, recipes, forge artifacts, engrams, embeddings, peer trust records — things *queried by attribute*, *mutated by intent* | Indexed, ACID, query-friendly. Working set in thousands-to-tens-of-thousands of rows. Cost per row is high but justified. |
| **Event-log tier** | **airc** (per-channel events.ndjson + cursors + lamport ordering) | Chat messages, signaling frames, presence pings, media-track keepalives, media upload chunks, call lifecycle, persona heartbeats, capability manifests, bid requests/responses, contract events — anything that's a *flow*, not a *thing* | Append-only, cursor-replayable, mesh-distributed. Cost per row is zero; reader pays only for what they consume past their cursor. |

### 1.2 The simple rule

> **Anything Slack does → airc. Anything Continuum knows → ORM.**
>
> Chat, DMs, threads, live audio calls, live video calls, screen-share, file uploads, presence, typing indicators, reactions, read-receipts, WebRTC signaling, media frames — all Slack stuff, all airc.
>
> Personas, recipes, forge artifacts, engrams, settings, embeddings, room metadata, persona identity, forge run history, peer trust records, wallet ledgers — all Continuum state, all ORM.
>
> Tiebreaker: *"Would Slack persist this in their main app database, or would it just flow over their realtime gateway?"*

### 1.3 The mistake the patch made (historical context)

Chat + RAG using ORM was killing the system. Every chat message was an ORM write; every RAG read was an ORM scan; every `data:chat_messages:created` event fanned out across the bus to all subscribers. The entity tier wasn't built for that volume — symptoms were measurable: lock contention, index bloat, replay storms on widget mount. The migration's whole point was to **get chat out of ORM and into a purpose-built event log (airc)**.

The patch architecture (`continuum-airc-bridge.mjs` → `modules/airc.rs` → engram store, with dual-write back into `chat_messages`) sneaks chat back into ORM under different names. **That undoes the migration.** This doc is the redo — done as architecture, not as more patch.

---

## 2. The universal bus extends to airc — one piece of metadata per primitive

Continuum's two existing universal primitives (per `docs/UNIVERSAL-PRIMITIVES.md`) are already transport-transparent across browser/server via WebSocket EventBridge. **airc is the third transport in the same model.** No new APIs; extend the metadata that already exists.

### 2.1 Commands: extend `naturalEnvironment` → `naturalScope`

`CommandBase` already declares a class-level `naturalEnvironment: 'browser' | 'server' | 'auto'` (line 81 of `daemons/command-daemon/shared/CommandBase.ts`) and a per-call override via `targetEnvironment` injected through `executeIn` (line 123). Pattern is right; value space is too narrow.

```typescript
// Class-level (declarative — what scopes this command supports)
protected static get naturalScope(): 'local' | 'environment' | 'grid' {
  return 'environment';  // default — current browser↔server behavior
}

// Per-call override on CommandParams (imperative — where THIS invocation runs)
interface CommandParams {
  // ... existing fields (userId, timeout, background, onTimeout) ...
  scope?: {
    target: 'local' | 'environment' | 'grid';
    // Grid-only fields:
    requires?: GridRequirements;        // declarative dispatch (see §4)
    peer_id?: string;                   // explicit dispatch (rare; debug)
    capability?: string;                // capability-by-name dispatch
    policy?: 'cheapest' | 'fastest' | 'closest' | 'load-balance' | string;
  };
}
```

`Commands.execute` resolves the effective scope by:
1. Use `params.scope` if caller specified one.
2. Else fall back to class-level `naturalScope`.
3. Route to the appropriate transport: in-process, WebSocket, or airc.

`remoteExecute()` learns a third path (grid via airc) on top of the existing two (browser↔server via WebSocket).

**Examples:**

```typescript
// Class declarations
class ScreenshotCommand { static get naturalScope() { return 'environment'; } } // DOM is env-local
class InferenceCommand  { static get naturalScope() { return 'grid'; }        } // can ship to any GPU peer
class PingCommand       { static get naturalScope() { return 'grid'; }        } // ping any reachable peer
class DataCreateCommand { static get naturalScope() { return 'local'; }       } // ORM is per-machine; no airc cost on every write
class FileSaveCommand   { static get naturalScope() { return 'environment'; } } // FS is machine-local

// Per-call usage
await ScreenshotCommand.execute({ querySelector: 'body' });
// → naturalScope='environment' → WebSocket browser↔server, current behavior

await InferenceCommand.execute({ model: 'qwen3.5-72b', prompt: '...' });
// → naturalScope='grid', no override → resolve via capability index, dispatch via airc

await PingCommand.execute({ verbose: true }, { scope: { target: 'grid', peer_id: 'bigmama-wsl-7a3f' } });
// → explicit grid target → airc command envelope → reply via correlation_id
```

### 2.2 Events: declare broadcast intent per event class

```typescript
declareEventClass<ChatPostedEvent>('chat:posted', {
  broadcast: true,              // route through airc + local
  channel: 'byRoomId',          // how this event maps to an airc channel
  schemaVersion: 'v1',
});

declareEventClass<WidgetMountedEvent>('widget:mounted', {
  broadcast: false,             // process-local only; no airc cost
});

declareEventClass<PersonaUpdatedEvent>('data:personas:updated', {
  broadcast: false,             // entity-tier event; in-process + WebSocket only
});

declareEventClass<ForgeStageProgressEvent>('forge:stage-progress', {
  broadcast: true,              // grid observability — sentinel-AI and dashboards subscribe across mesh
  channel: 'global',
});
```

`Events.emit()` reads the class metadata, fans out to:
1. **Always:** local in-process subscribers (free).
2. **If WebSocket open:** connected-peer EventBridge (fast-path optimization).
3. **If `broadcast: true`:** airc record (durable, cross-machine, replayable via cursor).

Subscribers don't care which transport delivered — `Events.subscribe('chat:posted', handler)` works the same regardless.

### 2.3 Why the per-class metadata isn't a feature flag — it's the contract

A new event class or command class is built against the bus from day one. The metadata decides which transports it actually uses today. Flipping `broadcast: false → true` is a metadata change, not a refactor. **This is why future observability (e.g. cross-grid sentinel monitoring) is free.** If `forge:stage-progress` were "local-only because nobody asked for cross-grid yet," then someone adding a remote dashboard later has to migrate every emitter. With universal bus access + per-class opt-in, it's a one-line change.

### 2.4 Three transports, same Events API

| Transport | Use when… | Cost | Examples |
|---|---|---|---|
| **Process-local** (in-memory) | Subscribers in same process | Zero | `widget:mounted`, internal state changes |
| **WebSocket EventBridge** | Currently-connected peers, low-latency intra-network | Low (open socket) | `data:personas:updated` to a widget watching that persona; daemon-to-daemon on same machine |
| **airc** | Cross-machine, durable replay, late-joiner catchup, anything in the Slack-y or grid-coordination set | Higher (gh API + lamport + cursor bookkeeping) | All chat/calls/media, all `forge:*`, all `presence:*`, all `contract:*`, all wallet/LP events, all grid-dispatch traffic |

Default for new event classes: **conservative (`broadcast: false`) plus a clear opt-in story** so the cost of adding airc-tier delivery to an event later is *one metadata line.*

---

## 3. Continuum-as-AS — the BGP framing for the grid

Each Continuum install is an autonomous router on the airc mesh. Like the internet:

| Internet term | Continuum equivalent |
|---|---|
| AS (Autonomous System) | One Continuum install |
| BGP peering | Trust circle membership (`household`, `cambriantech-org`, `trusted-peers`) |
| Route advertisement | `presence:peer-manifest` event (broadcast: true) |
| AS_PATH (loop detection) | `forwarded_by: [...]` on routed commands |
| Local-Pref (per-AS policy) | Per-continuum dispatch policy: cheapest / fastest / trust-preferred / cost-bounded |
| MED (capacity hint) | `presence:resource-pressure` event when local load is high |
| Route withdrawal | `presence:peer-departed` event / capability-removal in manifest |
| BGP communities | Capability tags + circle tags carried on manifests |
| Route convergence | Eventual — manifest deltas propagate via airc cursor; every dispatcher converges to the same routing-table view via lamport ordering |
| iBGP / eBGP | Intra-continuum routing (multiple processes on one install) vs inter-continuum (across the airc mesh) |
| DNS | Capability resolver (declared capability string → set of peer-ids in routing table) |
| BGP route reflector | A relay peer in grids with many small nodes (optional optimization) |

### 3.1 Per-continuum router

Every Continuum runs a **`grid-router-daemon`** (new — see §6 deliverables). It is fully autonomous:

- Maintains a routing table: capability → set of peer offers (weighted by cost / latency / trust / current load)
- Subscribes to `presence:peer-manifest` events, folds each into the routing table
- Subscribes to `presence:resource-pressure` events, adjusts weights
- Subscribes to `presence:peer-departed` events, withdraws routes
- Runs the local policy engine: when a Command needs `target: 'grid'`, applies the operator's utility function (declaratively configured per §6)
- Runs the bid loop: short-window request-for-bids for non-trivial dispatch, picks winner by policy
- Signs and broadcasts its own manifests and resource-pressure signals
- Audits every routing decision to the airc log (replayable for sentinel + wallet)

No master. No central scheduler. The mesh converges by event flow + lamport ordering. Asymmetric policies are first-class — Joel's continuum optimizes for cheap; Toby's optimizes for grid-fairness; a sentinel-bound enterprise continuum optimizes for trust-circle-only. They interoperate as long as they all speak the protocol.

### 3.2 Convergence is eventual, not synchronous

Like BGP — a peer can be temporarily wrong about the routing table. Dispatch fails clean. Next attempt sees updated state. Don't design for instant global agreement.

Concretely: a Command dispatch that resolves to a now-unreachable peer returns `{ error: 'peer-unreachable', explored: ['bigmama-wsl-7a3f'], suggest_retry: true }`. Caller can retry; routing table will have been updated by the time they do.

### 3.3 Trust circles ARE composable communities

Like BGP communities. A peer in my `household` circle can transitively advertise: "I extend trust to Toby's grid for inference capabilities; here's their signed circle delegation." I evaluate per my own policy whether to extend partial trust. There's no central trust registry — trust composes from per-circle signatures.

Concrete trust hierarchy:

```
local (same install)        — full trust, free, instant
household (my own machines) — full trust, ~10ms LAN, no $
trusted-orgs (peered orgs)  — explicit trust grant, ~100ms Tailscale, optional micropayment
extended (transitive)        — partial trust, sentinel scrutiny, micropayment, ~200ms
public mesh                  — untrusted-by-default, reputation-based, payment required, ~500ms+
```

Per-continuum config picks which tiers it accepts inbound from and which it dispatches outbound to.

---

## 4. Two-sided market — offer / want, signed manifests

Each peer broadcasts a manifest on join + on material state change + periodically. Manifest is the routing-table input AND the contract registry entry:

```typescript
{
  peer_id: 'bigmama-wsl-7a3f',
  alias: 'bigmama',
  reachable_via: ['airc:#cambriantech', 'tailscale:100.64.0.5'],
  hardware: {
    cpu: { cores: 32, ghz: 4.5 },
    memory_gb: 128,
    gpus: [{ brand: 'nvidia', model: 'rtx5090', vram_gb: 32, capability: 'sm120' }],
  },
  offers: [
    {
      capability: 'inference:qwen3.5-72b-q4',
      alloy_hash: 'aa61c4bdf463847c',                 // ← forge alloy as contract substrate
      alloy_url: 'hf.co/cambriantech/qwen3.5-72b-q4-aa61c4bd',
      benchmarks: { rouge_l: 0.847, perplexity: 4.2, tokens_per_sec: 38 },
      terms: {
        cost_cents_per_1k_tokens: 0.4,
        est_latency_ms: 320,
        sla_p99_ms: 2000,
        max_concurrent: 4,
      },
      available_until: '2026-12-31T00:00:00Z',
      loaded_state: 'now',                              // 'now' | 'loadable' (page-in cost)
    },
    // ...
  ],
  wants: [
    {
      capability: 'training:lora:typescript-expertise',
      requires: { min_eval_delta: '+0.03', max_cost_lp: 50, max_completion_h: 24 },
      hungry_for: 1,                                    // count of contracts I'd take
      budget_remaining_cents: 500,
    },
  ],
  current_load: { gpu_util: 0.15, vram_used_gb: 24, queue_depth: 0 },
  policies: { accepts_inbound_from: ['household', 'trusted-orgs'], max_concurrent: 4 },
  trust: { circles: ['household', 'cambriantech-org'], issuer_signatures: [...] },
  ts_ms: 1748287200000,
  signature: '<ed25519 over the canonical encoding of all fields>',
}
```

### 4.1 Matching = predicate evaluation

A peer's `wants` block looks for matching `offers` block across the routing table. When a match is found, the wanter's dispatcher fires `command:bid-request`. Bids return `command:bid-response`. Dispatcher picks the best fit per its policy. Winner gets `command:bid-accepted`; losers get `command:bid-released` (so they can free reservations).

**Pure declarative.** Nobody writes "if peer == bigmama then ..." anywhere. Capability strings + alloy hashes + signed terms + per-continuum policy = matching.

### 4.2 Forge alloy as the contract substrate

A forge alloy already has every property a contract needs:

| Alloy property | Contract use |
|---|---|
| `alloy_hash` (sha256) | Tamper-evident identity — "What bytes are you serving?" |
| `recipe_id + recipe_version` | Methodology lineage — "How was this made?" |
| `results.benchmarks[]` + `samplesPath` | Falsifiable performance claims |
| `priorMetricBaselines[]` | §4.1.3.4 negative-baseline preservation |
| `hardware_verified[]` | Provenance — where it's been proven to run |
| `published_url` (HF or airc-blob) | Where to fetch + verify |
| `methodologyPaperUrl` | Procedural attestation |

When a peer offers `inference:qwen3.5-72b-q4`, it cites the **specific alloy hash** it will serve from. The contract is: "I will compute with the bytes that hash to X, those bytes have the properties measured in this alloy's benchmark block, falsify against this baseline if you doubt." Caller knows exactly what they're getting; the audit trail is one hop back to the alloy + methodology.

### 4.3 Where contracts live

| Layer | Where | Speed | Trust |
|---|---|---|---|
| **Capability manifest** (lightweight, "I offer X with these terms") | airc event log (`presence:peer-manifest`, broadcast: true) | Real-time | Per-peer signed reputation |
| **Forge alloy itself** (the actual bytes + attestation chain) | HuggingFace (public-mesh, world-discoverable) AND/OR airc-blobs (circle-private, mesh-internal) | Static | Cryptographic (hash + recipe lineage signatures) |

Circle-private alloys don't need HF publication — household peers can pull from another peer's airc-blob store and verify locally. Public-mesh use cases (anyone-on-the-internet can hire this peer) publish to HF for broader discovery.

### 4.4 Contract execution event chain

New event classes (all `broadcast: true`), all signed by the participating peers:

| Event | When fired |
|---|---|
| `contract:proposed` | Dispatcher sends a bid-request that includes contract terms |
| `contract:bid` | Bidder responds with their own contract terms (countersigned) |
| `contract:accepted` | Dispatcher picks a winner; both sides commit |
| `contract:executing` | Winner begins work (optional progress: `contract:progress`) |
| `contract:delivered` | Winner returns the result + a delivery receipt referencing the alloy hash |
| `contract:verified` | Dispatcher verifies (alloy hash matches, benchmarks within tolerance) |
| `contract:paid` | LP transfer clears, or in-circle reciprocity recorded |
| `contract:disputed` | Either party flags violation; sentinel arbitration follows |

The whole transaction lives on the airc log, lamport-ordered, replayable. **Every step is auditable; the wallet/LP attribution story has substrate.** (This is the gap FreeLattice's LP paper-architecture can't fill: they don't have the event log to ground the audit. We do.)

---

## 5. Migration — what comes out, what goes in, in what order

### 5.1 What gets DELETED

| File / construct | Why |
|---|---|
| `src/scripts/continuum-airc-bridge.mjs` | Shell-out-per-message hack. Replaced by the airc EventBridge transport. |
| `src/workers/continuum-core/src/persona/airc_admission.rs` | Protocol-named. Replaced by `persona/message_admission.rs` (or fold into existing inbox admission gate) subscribing to `chat:posted` regardless of source. |
| airc-prefixed IPC commands in `modules/airc.rs` (`airc/queue-scan`, `airc/realtime-publish`, `airc/realtime-replay`) | Bypassed the Events bus. Replaced by `Events.subscribe('chat:posted', ...)`. Debug-only inspection commands move to `debug/airc-*` namespace. |
| `src/system/data/entities/ChatMessageEntity.ts` + the `chat_messages` ORM collection | ORM is not the chat store. airc is. Reads via `Events.subscribe('chat:posted', ...)` for new messages; backfill via airc cursor tail-N. |
| The dual-write PR stack (#1432/1433/1435/1436/1437) and `src/system/airc-chat/*` (`AircChatMirrorMapper`, `AircChatDualWriteService`, `AircChatEnvelope`) | Patches the missing event emission by writing to TWO places. Once airc IS the chat store, no dual-write needed. **The whole `src/system/airc-chat/` directory deletes.** |
| `persona/turn-frame` consuming an in-memory `PersonaInbox` populated by direct IPC | Bypasses the Events bus. PersonaInbox becomes a thin cursor over the room's airc event stream. Drain = advance cursor + return events between old and new positions. |
| Any `Events.subscribe('data:chat_messages:*', ...)` references | All become `Events.subscribe('chat:posted'|'chat:edited'|...)`. |

### 5.2 What gets BUILT

Six new components. Each one is bounded and shippable.

1. **EventClass declaration system** (`src/system/events/EventClass.ts` + `EventClassRegistry.ts`)
   - Typed event class declarations with `broadcast`, `channel`, `schemaVersion` metadata
   - `Events.emit()` consults registry to pick transport(s)

2. **AircEventTransport adapter** (`src/system/events/transports/AircEventTransport.ts`)
   - Implements the existing `EventTransport` interface
   - Outbound: `Events.emit()` → publishes to the appropriate airc channel
   - Inbound: airc events past local cursor → `Events.checkWildcardSubscriptions()`
   - Persists cursor per-subscriber for restart-safe replay

3. **CommandBase.naturalScope + CommandParams.scope** (extends existing surface)
   - Class declares `naturalScope: 'local' | 'environment' | 'grid'`
   - Per-call `scope` override on params (with `requires`, `peer_id`, `capability`, `policy` sub-fields for grid)
   - `Commands.execute()` reads both; `remoteExecute()` learns the airc grid path

4. **`presence:peer-manifest` event class + capability index** (`src/system/grid/CapabilityIndex.ts` + Rust mirror)
   - Manifest schema (offers/wants/terms/signatures)
   - Folder maintains per-peer latest-manifest view
   - Indexed by capability for dispatcher lookup

5. **`grid-router-daemon`** (`src/daemons/grid-router-daemon/`)
   - Subscribes to `presence:peer-manifest`, `presence:resource-pressure`, `presence:peer-departed`
   - Maintains routing table; runs local policy engine
   - Implements bid loop (`command:bid-request` → `command:bid-response` → `command:bid-accepted`)
   - Handles routed-command forwarding (multi-hop with `forwarded_by` loop detection)
   - Exposes `grid/show-routes`, `grid/show-policy`, `grid/show-recent-dispatches` for introspection

6. **Contract event chain + signatures** (`src/system/grid/ContractEvents.ts`)
   - `contract:proposed` / `contract:bid` / `contract:accepted` / `contract:executing` / `contract:delivered` / `contract:verified` / `contract:paid` / `contract:disputed`
   - Signed envelopes (ed25519)
   - Reference `alloy_hash` for the substance of what's being contracted
   - Audit-replayable from airc cursor

### 5.3 Phased migration sequence

Each step is independently shippable, independently revertable. Step 4 (ORM-collection deletion) is the irreversible one.

| Step | Lands | Reverts/deletes | Test |
|---|---|---|---|
| **0** | This doc, after review | — | Reviewers acked |
| **1** | EventClass registry + AircEventTransport (1 transport, 1 event class: `chat:posted`) | — | Round-trip event A → B across machines, no ORM writes |
| **2** | `CommandBase.naturalScope` + `CommandParams.scope` (grid path stub: airc envelope round-trip with correlation_id) | — | `PingCommand.execute({}, {scope:{target:'grid', peer_id:'<other>'}})` returns peer-info |
| **3** | `presence:peer-manifest` + capability index + `grid/show-routes` introspection | — | Two peers boot, each sees the other in `grid/show-routes` |
| **4** | `grid-router-daemon` with bid loop + policy engine; `InferenceCommand` declares `naturalScope: 'grid'`; LoRA-paged inference dispatches to a peer with the right capability | — | Laptop persona inferences against GPU peer; result returns; round-trip <500ms |
| **5** | Contract event chain + alloy-grounded contract terms + sentinel-friendly audit replay | — | Contract proposed → bid → accepted → executed → delivered → verified → paid (all on airc log) |
| **6** | `persona/message_admission.rs` subscribes to `chat:posted`; persona inbox drains via airc cursor | `persona/airc_admission.rs`, the airc-prefixed IPC commands | Persona reacts to airc-sourced chat identically to local-emit-sourced |
| **7** | UI widgets subscribe to `chat:posted` for display; tail-N replay via airc cursor on mount | `chat_messages` ORM writes from chat send path | UI shows new + backfilled history without ORM hit |
| **8** | **Delete `chat_messages` ORM collection.** | `ChatMessageEntity.ts`, the collection in DataDaemon, all callers | Nothing references it; entity-tier ORM working set shrinks |
| **9** | Revert dual-write stack | `src/system/airc-chat/` entirely; `continuum-airc-bridge.mjs` | Smoke: chat send goes only to airc, no parallel store |
| **10** | Same shape for `webrtc:*`, `presence:*`, `media:*` event classes | n/a (transport already there) | Live audio call between two peers, signaling on airc |
| **11** | Media uploads via airc-blobs (`media:upload:chunk` + `media:upload:complete`) | n/a | Image transfer across two peers, content-addressed, verified |

### 5.4 Breakage surface

About **186 files** reference `ChatMessageEntity`, the `chat_messages` collection, or `data:chat_messages:*` event subscriptions (grep against current canary). Estimate **~30 source files plus ~20 tests** need real changes (not just rename-grep); the other ~136 are generated `.d.ts` files in `src/dist/` (auto-regenerate on build) or transitive references that update naturally.

Hot zones:
- `PersonaUser.ts` (2385 LOC) — subscribes to `data:chat_messages:created` for turn-drive trigger → switch to `chat:posted`
- `TrainingDaemonServer.ts` + `TrainingDatasetBuilder.ts` — pull chat from ORM for training → pull from airc cursor windowed read
- `Hippocampus.ts` — recent-turn cache → reads from airc cursor; long-term engrams stay ORM
- `EventConstants.ts` + `EventSystemConstants.ts` — add `chat:*`/`presence:*`/`webrtc:*`/`media:*`/`contract:*` classes; remove `data:chat_*`
- `src/system/airc-chat/*` — **entire directory deletes**
- `~20 chat-* tests` — rewrite against new event class API; some delete

Deprecation alias for one cycle: `data:chat_messages:created` aliased from `chat:posted` with warn-on-use during the migration window, removed at step 8.

---

## 6. Concrete deliverables list (cross-referenced from §5.2)

For codex + claude-tab-1 review and lane assignment:

| # | Component | Owner candidate | Substrate |
|---|---|---|---|
| 1 | EventClass declaration system + registry | TS-only | `src/system/events/` |
| 2 | AircEventTransport adapter | claude-tab-1 (Lane C2 makes this natural) | `src/system/events/transports/` |
| 3 | `CommandBase.naturalScope` + `CommandParams.scope` extension | TS (CommandBase) + Rust (handle_event path for grid-routed commands) | `src/daemons/command-daemon/` + `src/workers/continuum-core/src/runtime/` |
| 4 | `presence:peer-manifest` event class + capability index | Rust (canonical state) + TS (read-side bindings) | `src/workers/continuum-core/src/grid/` (new module) + `src/system/grid/` |
| 5 | `grid-router-daemon` (the BGP-flavored router) | TS daemon + Rust IPC for the routing-table state | `src/daemons/grid-router-daemon/` (new) |
| 6 | Contract event chain + signatures | TS + Rust signature verification | `src/system/grid/contracts/` + `src/workers/continuum-core/src/grid/contracts.rs` |
| 7 | Migration of existing callers (per §5.4 hot-zone list) | distributed — each caller's owner | various |
| 8 | Deletion list (per §5.1) | sequential — happens as callers migrate | various |

Each numbered item is a separately reviewable PR. Items 1–6 are the foundation; 7–8 are downstream cleanup.

---

## 7. Per-continuum policy as first-class config

`~/.continuum/grid-policy.json` (or wherever Continuum stores per-install config):

```json
{
  "outbound_dispatch": {
    "accept_tiers": ["household", "trusted-orgs"],
    "policy": "cheapest-fast-enough",
    "max_outbound_per_minute": 30,
    "budget_cents_per_day": 500,
    "default_min_trust_circle": "trusted-orgs"
  },
  "inbound_serving": {
    "accept_inbound_from": ["household", "trusted-orgs"],
    "max_concurrent_jobs": 4,
    "max_compute_hours_per_day": 8,
    "decline_if_local_load_above": 0.85
  },
  "trust_circles": {
    "household": { "members": ["joel-imac-...", "joel-laptop-...", "bigmama-wsl-..."] },
    "trusted-orgs": { "members": ["toby-...", "...cambriantech-..."] }
  },
  "alloy_publishing": {
    "auto_publish_to": ["airc-blobs:#cambriantech"],
    "auto_publish_public": false
  }
}
```

Editable + reloadable. Operator/persona can adjust how their continuum behaves on the grid without code changes. **This is what BGP local-pref does at the AS level — declarative per-AS policy.**

---

## 8. Open questions for reviewers

1. **Channel-strategy for event classes.** Should the EventClass declaration say `channel: 'byRoomId'` (auto-derived from event payload) or have the emitter explicitly specify the airc channel? Argue for declarative; lean toward `byRoomId | global | byPeerId | <custom-fn>`.

2. **Cross-room broadcast events** like `presence:peer-manifest`. A dedicated `#presence` channel that everyone subscribes to? Or per-room manifests (peer-in-many-rooms publishes to each)? Argue for a single `#presence` channel mesh-wide — manifests aren't room-scoped.

3. **At-least-once vs exactly-once delivery.** airc cursor + lamport gives at-least-once with idempotent subscribers as the contract. State that as a hard requirement (subscribers MUST be idempotent), or design exactly-once into the transport? Idempotent-subscribers is simpler; favor that.

4. **Schema evolution.** Envelopes carry `schemaVersion`. Subscribers fail loud on unknown schemas — don't silently drop. Encode in the EventClass typed wrappers (`onUnknownSchema: 'warn' | 'fail'`, default 'fail').

5. **Trust circle delegation.** A peer in my `household` circle can advertise "I trust X for inference." Do I extend partial trust automatically? Configurable per circle? Lean toward explicit-only (no automatic transitivity), with operator UI for accepting delegated trust.

6. **Discovery for new continuums.** A fresh install with no peering — how does it bootstrap to a discoverable mesh? Argue: explicit invite (existing airc invite flow), no auto-discovery, no public broadcast. Operator-driven peering. Public-mesh tier is opt-in only.

7. **Sentinel-AI scrutiny enforcement.** When sentinel is configured to inspect cross-grid dispatches, does it run as a gate (blocks suspect dispatch) or an observer (logs + alerts)? Probably both, configurable per circle.

8. **Backward compat for the `data:chat_*` event aliases.** One migration cycle with warn-on-use, then remove. Confirm migration window length (2 weeks? until step-8 lands?).

9. **What about the `airc/work*` IPC commands** (`work create`, `work claim`, `work board`, `work next`)? These are entity-tier reads on airc state (work cards). Keep them as IPC commands but rename to `grid/work-*` or fold into the contract-event-chain (`contract:work-card`)? Probably fold — work cards ARE contracts in nascent form.

10. **Wallet / LP layer plug-in.** This doc claims the contract event chain is the wallet's audit substrate. What additional event classes does the LP layer need? Probably `wallet:transfer-proposed` / `:accepted` / `:cleared` + an LP-balance event class. Specify in a follow-up doc (`WALLET-ON-GRID-BUS.md`).

---

## 9. Coordination

This doc supersedes:
- `docs/architecture/AIRC-ADAPTER-SURFACE.md` (#1434, "Lane C2 — adapter surface for consuming airc-lib") — that doc was the *consumer-side* design. This doc is the *bus integration* layer above it. The C2 design becomes the *implementation* of the AircEventTransport from §6 item 2.
- The in-flight patch architecture (per §5.1 deletion list).

References:
- `docs/UNIVERSAL-PRIMITIVES.md` — the existing Commands + Events primitives this doc extends.
- `docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md` — the runtime contract every Rust module inherits (needs updating to allow `event_subscriptions: &["chat:posted", ...]` etc. as a first-class pattern, instead of every module declaring `&[]`).
- `docs/architecture/AGENT-BACKBONE-INTEGRATION.md` — substrate-vs-semantic split this doc honors.
- `docs/architecture/FORGE-ALLOY-SPEC.md` / `FORGE-RECIPE-AS-ENTITY.md` — the alloy substrate this doc uses as the contract layer.

Reviewers:
- **Joel** — primary author of the patch this replaces; primary stakeholder of the grid story
- **codex** — airc substrate side (rust-rewrite); needs the airc-lib surface to expose what this doc requires (subscription + send + cursor-replay primitives that the AircEventTransport will consume)
- **claude-tab-1** — Lane C2 (airc-adapter); the AircEventTransport implementation is the concrete realization of their design

Reply on `#cambriantech` with substantive critique before any of §5.2's components begins implementation. **No code lands until the architecture has at least one reviewer pass from codex AND claude-tab-1.**

---

## 10. What this doc explicitly does NOT do

- **Decomposed/sharded inference** (model parallelism, pipeline parallelism, multi-peer collaborative training). Real, but a separate substrate problem. Design here is single-peer fulfillment; decomposition can land later by extending `scope.requires` with a `decomposition_strategy` field. Failing-clean ("no single peer can do this") is acceptable for v1.
- **Wallet / LP currency layer.** This doc provides the audit substrate (contract event chain on airc). The actual currency, exchange rate, minting, on-chain integration is a separate spec (`WALLET-ON-GRID-BUS.md`, to-be-written, depends on this doc landing).
- **Sentinel-AI scrutiny details.** This doc says sentinel can fold from the contract event chain; the actual sentinel scoring / dispute-arbitration / penalty rules are sentinel's own design (separate doc).
- **Public-mesh reputation system.** Mentioned in §3.3 as a future tier; specifying it (anti-Sybil, reputation accrual, slashing) is out of scope here.
- **Concrete schema for `presence:peer-manifest` signatures.** Sketched in §4 with `<ed25519 over the canonical encoding>`; the exact canonical encoding, signature verification path, and key-rotation story belong in a sibling spec or an implementation PR description.

These are deferrable. Don't block this doc's review on them.

---

## Appendix: Honest finding on alloy generalization (added 2026-05-25 post-publish)

§4.2 frames forge alloy as the universal contract substrate. Verifying against the actual Rust types reveals a gap: the existing alloy schema is **model-bound** in its current form. Field evidence:

- `AlloySource`: `base_model`, `architecture` ("qwen3" / "llama" / "mistral"), `is_moe`, `total_experts` — all model-specific.
- `BenchmarkDef`: ML-evals only (`humaneval`, `mmlu`, `n_shot`).
- `ForgeArtifact`: `forged_params_b`, `active_params_b`, `quant_tiers`, `tokens_per_sec`, `memory_usage_gb` — all model-shape.

The vision Joel articulated ("alloy is a contract for anything, with proof") requires generalizing the schema. Two viable paths:

**Path A — In-place generalization.** Add `artifact_kind: 'model' | 'inference-run' | 'sentinel-scan' | 'code-gen' | 'audit-attestation' | <future>` and make model-specific fields optional, gated on `artifact_kind === 'model'`. Cost: schema churn touches every existing alloy site. Benefit: single artifact type, single audit surface.

**Path B — Layered abstraction.** Introduce parent `ContractArtifact` with the universal fields (hash, recipe lineage, benchmarks, falsifiability, hardware_verified, signed terms). `ForgeAlloy` becomes one subtype (`kind: 'model'`). New computation kinds are sibling subtypes (`kind: 'inference-run'`, etc.) with their own per-kind fields. Cost: indirection on the read side. Benefit: non-destructive — existing alloy code untouched; new kinds add by composition.

**Open question 11 (added):** A vs B vs something else. Joel's call. The grid-bus architecture in this doc works either way — the contract chain (§4.4) references `alloy_hash` regardless of the underlying schema shape. But §4.2's claim that forge alloy is *currently* the universal substrate is only true under one of these paths landing.

Reviewers — please weigh in on this when assessing §4.
