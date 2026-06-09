# apps/cli — `ctm` rust CLI binary

Successor to `./jtag` (per task #143). Links `client/continuum-client`
directly — every command goes through the same `Connection` /
`CommandClient` seam the substrate's integration tests exercise. No
Node middleware, no JTAG-daemon IPC dance.

## Status

Two subcommands wired end-to-end through the substrate: `metrics` and
`generate`. Each future subcommand is a small slice that migrates a
`./jtag <command>` to a `ctm <command>` call. As subcommands land, the
Node `./jtag` shrinks; eventually only its install footprint is left.

## Install

The binary builds out of the Cargo workspace at repo root:

```bash
export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"
cargo build -p continuum-cli --release
# Binary lands at $CARGO_TARGET_DIR/release/ctm
```

A future slice adds a `cargo install --path apps/cli` flow + a shell
shim so `jtag` aliases `ctm`.

## Usage

```bash
# Required: substrate peer UUID. Find it on the substrate host:
#   airc status   # peer_id: …
# Set once via env, or pass --peer every call.
export CONTINUUM_PEER_ID=9bb24964-1a1a-43e2-a5aa-8140362bab63

ctm metrics                      # fetch runtime/metrics/all
ctm metrics --peer <UUID>        # override env
ctm --home ~/.airc-alt metrics   # override default $HOME/.airc

ctm generate --prompt "explain HandleRef"
ctm generate --prompt "..." --model "qwen3.5-4b-code-forged"
ctm generate --prompt "..." --json   # raw JSON instead of plain text

# Coverage battery against the target peer:
ctm grid-smoke                       # runs default battery; nonzero exit on any fail

# Tracing:
CONTINUUM_CLI_LOG=debug ctm metrics
```

## Commands today

| Command      | Substrate call(s)                                          | Notes                                                                                                  |
|--------------|------------------------------------------------------------|--------------------------------------------------------------------------------------------------------|
| `metrics`    | `runtime/metrics/all`                                      | Pretty-prints JSON for all modules                                                                     |
| `generate`   | `ai/generate`                                              | Dispatches inference; substrate's adapter registry picks the model. With PR #1560 (AircRemoteInferenceAdapter) the inference may transparently run on a remote peer — CLI doesn't know or care. |
| `grid-smoke` | `runtime/metrics/all`, `ai/providers/list`, `ai/generate`  | Coverage battery; per-row ✅/❌ + ms; nonzero exit on any failure. v1 single-hop only; multi-hop composition (M → A → B → C), fan-out, and mixed-modality chains land in v2 with probe-trace ingestion. |

## Architecture

```text
  user @ shell
       │
       ▼
  ctm <subcommand>          (this crate — apps/cli/)
       │
       ▼
  Connection::connect(airc, substrate_peer_id)
       │
       ▼
  CommandClient<AircIpcTransport>
       │
       ▼
  airc-lib request/await_reply   (LAN socket → substrate peer)
       │
       ▼
  continuum-core-server / CommandRequestHandler
       │
       ▼
  module dispatch → AircCommandResponse → reply over airc
       │
       ▼ (back up the stack)
  serde_json::Value → ctm prints to stdout
```

Same seam the `core/continuum-core/tests/airc_ipc_roundtrip.rs`
integration test exercises end-to-end.

## Pending follow-ups

- **Auto peer discovery** — current slice requires `--peer`. A future
  slice reads `~/.continuum/peer.json` or uses an `airc ipc-endpoint`
  lookup so the operator doesn't have to type a UUID.
- **More subcommands** — `chat/send`, `gpu/stats`, `data/list`,
  `cognition/admit-inbox-message`, etc. Each is a small slice.
- **Shell shim install** — `tools/scripts/install-cli.sh` to put `ctm`
  on PATH and alias `jtag` → `ctm` for backwards muscle memory.
