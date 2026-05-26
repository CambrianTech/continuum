# Continuum → airc-ipc: direct IPC dep (no subprocess, no JSON transcode)

**Status:** dep landed; consumer impl pending follow-up PRs.
**Pairs with:** [`AIRC-CONTINUUM-BRIDGE.md`](AIRC-CONTINUUM-BRIDGE.md) — long-term architecture.
**Roadmap:** kanban card `156770cf-95f9-4945-88da-5dcce795ceb7`.

## Why

The grid-event hot path moves typed envelopes (chat:posted, presence:peer-manifest, contract:*, future media-signal events) between Continuum personas and the airc substrate at high rate. Three transport shapes are possible; only one is correct under load.

| Shape | Per-event cost | Sig stability | Verdict |
|---|---|---|---|
| Subprocess `airc publish` + parse JSON of `airc inbox --json` | spawn + serde_json round-trip × 2 per event | canonical bytes mutated by re-encode → ed25519 sig verify **breaks** | Wrong. Inhibits L1-6 signed envelopes. |
| Direct Unix-socket IPC via `airc-ipc::DaemonClient` (CBOR) | 1 CBOR encode + 1 framed write per event | canonical bytes preserved end-to-end | **Correct.** |
| Continuum embeds the daemon | conflated lifetimes, mixed substrates | sig stable but two daemons would race over the same wire | Wrong shape. |

The IPC ABI version (`airc_ipc::IPC_PROTOCOL_VERSION`) pinning is what makes shape 2 safe across redeploys: Continuum and the daemon negotiate the same version or refuse to connect.

## What this PR lands

Workspace-level git deps in `src/workers/Cargo.toml`:

```toml
airc-core    = { git = "https://github.com/CambrianTech/airc", rev = "ef6eced…" }
airc-protocol = { git = "https://github.com/CambrianTech/airc", rev = "ef6eced…" }
airc-ipc      = { git = "https://github.com/CambrianTech/airc", rev = "ef6eced…" }
```

`continuum-core/Cargo.toml` picks up `airc-ipc.workspace = true` + `airc-protocol.workspace = true`. (`airc-core` is pulled transitively; not redeclared.)

**Zero new code, zero behavior change.** The existing `InMemoryAircRealtimeStore` stays the default. The dep addition is purely the architectural commitment — every follow-up PR consumes types from `airc_ipc::` / `airc_protocol::` directly instead of subprocess + parse.

## Why no consumer impl in this PR

Two design questions block writing the `DaemonAircRealtimeStore` cleanly today:

### Q1 — room-id boundary

Continuum's `AircRealtimeEnvelope` carries `room_id: String`. airc's `PublishRequest` carries `channel: Uuid` + `wire: PathBuf`. The deterministic mapping (`airc room <name>` derives both from the name) lives in `airc-lib::room::Room::from_name` + `airc-lib::subscriptions::derive_room_id`.

Three options:

| Option | What | Cost |
|---|---|---|
| A | Continuum depends on `airc-lib` too, calls `derive_room_id` directly | Bigger dep surface (airc-identity + airc-store come along) |
| B | Continuum keeps string room-ids; daemon translates at the IPC boundary | Requires adding a translation hop to airc-ipc's `PublishRequest` shape (accept name string OR uuid) |
| C | Continuum maintains its own room-id↔channel-uuid map, populated at room-join time | Cleanest dep boundary; one-time setup cost per room |

Recommend C.

### Q2 — wire path

`PublishRequest::wire` is the per-room wire directory. airc maintains this; Continuum doesn't need to know its filesystem path, only that it exists. The daemon already knows from prior `Subscribe` calls.

Two options:

| Option | What | Cost |
|---|---|---|
| α | Add a `wire-by-channel-uuid` lookup to `airc-ipc` (daemon resolves) | Tiny airc PR; clean shape on continuum side |
| β | Continuum tracks wire paths per room (subscribe step) | More state on continuum side; requires `airc subscribe` round-trip per room-join |

Recommend α — `airc-ipc` exposing the lookup is consistent with its role as "the typed ABI for talking to the daemon."

## Follow-up PRs

1. **continuum**: `DaemonAircRealtimeStore` impl (this PR's deps + Q1=C decision). Replaces `InMemoryAircRealtimeStore` as default. Feature-gated fallback to in-memory for unit-test paths.
2. **airc**: `airc-ipc::ResolveWireRequest` + corresponding daemon handler (Q2=α decision).
3. **continuum**: airc-side inbound stream — long-lived `Request::Attach` poller that drains `Response::Event` frames + dispatches as local `Events.subscribe` callbacks. The reverse direction.
4. **continuum**: L1-6 Phase B — peer-pubkey lookup via L1-4's `presence:peer-manifest` (needs card `290f64b7-5837-42ff-9844-570088fbb01a` resolved first — `signing_pubkey_hex` field on `AircPeerManifest`).
