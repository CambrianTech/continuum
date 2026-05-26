# Continuum → airc-ipc: direct IPC dep (no subprocess, no JSON transcode)

**Status:** direct IPC dep landed; daemon-backed publish/replay bridge landed; inbound attach stream in progress.
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

## What the dependency PR landed

Workspace-level git deps in `src/workers/Cargo.toml`:

```toml
airc-core     = { git = "https://github.com/CambrianTech/airc", rev = "428f928…" }
airc-protocol = { git = "https://github.com/CambrianTech/airc", rev = "428f928…" }
airc-ipc      = { git = "https://github.com/CambrianTech/airc", rev = "428f928…" }
```

`continuum-core/Cargo.toml` picks up `airc-ipc.workspace = true`, `airc-protocol.workspace = true`, and `airc-core.workspace = true`.

The first dependency-only PR had zero behavior change. The bridge now consumes the typed ABI directly: `AircModule::new()` publishes through the daemon-backed event transport for the current project `.airc` scope, while the in-memory store remains an explicit test fixture path.

The inbound half is the same direct-IPC rule in reverse: `AircModule::initialize()` attaches to the daemon's `Response::Event` stream, accepts only `forge.body_hint = continuum.airc.realtime.envelope.v1`, decodes the shared envelope contract, and republishes valid `EventBridgePayload` events into Continuum's `MessageBus`. No subprocess, no stdout contract, no separate JSON command surface.

## Why no consumer impl in this PR

Two design questions blocked writing the daemon-backed transport cleanly; both are resolved:

### Q1 — room-id boundary

Continuum's `AircRealtimeEnvelope` carries `room_id: Uuid`. airc's `PublishRequest` carries `channel: Uuid` + `wire: PathBuf`.

Three options:

| Option | What | Cost |
|---|---|---|
| A | Continuum depends on `airc-lib` too, calls `derive_room_id` directly | Bigger dep surface (airc-identity + airc-store come along) |
| B | Continuum keeps string room-ids; daemon translates at the IPC boundary | Requires adding a translation hop to airc-ipc's `PublishRequest` shape (accept name string OR uuid) |
| C | Continuum maintains its own room-id↔channel-uuid map, populated at room-join time | Cleanest dep boundary; one-time setup cost per room |

Decision: C, now implemented at the type boundary. Continuum carries the channel UUID it received from room/join context; it does not ask the daemon to translate room names on every publish.

### Q2 — wire path

`PublishRequest::wire` is the per-room wire directory. airc maintains this; Continuum doesn't need to know its filesystem path, only that it exists. The daemon already knows from prior `Subscribe` calls.

Two options:

| Option | What | Cost |
|---|---|---|
| α | Add a `wire-by-channel-uuid` lookup to `airc-ipc` (daemon resolves) | Tiny airc PR; clean shape on continuum side |
| β | Continuum tracks wire paths per room (subscribe step) | More state on continuum side; requires `airc subscribe` round-trip per room-join |

Decision: α. airc exposes `ResolveWireRequest { channel: Uuid }` over `airc-ipc`; Continuum resolves the daemon-owned wire path immediately before publish and fails loud when the channel is not joined.

## Follow-up PRs

1. **continuum**: L1-6 Phase B landed — replayed contract events verify the signed envelope and bind the signer pubkey to L1-4's `presence:peer-manifest.signing_pubkey_hex`.
2. **continuum/airc**: cursor contract upgrade. `airc-ipc::InboxRequest` is lamport-cursor-native; Continuum's public replay API now accepts `afterCursor` and returns a cursor shaped as `(lamport, event_id)` so high-rate Continuum event streams resume from the substrate position instead of fetching a bounded page and filtering by event id.
3. **continuum**: runtime e2e proof. Start a daemon for a temp project `.airc`, publish a Continuum realtime envelope through `AircModule::new()`, observe the attach stream republish it into `MessageBus`, and prove no CLI/stdout path participates.
