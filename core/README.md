# core/ — rust substrate workspace

The headless substrate. Every server-side crate that runs inside the
`continuum-core-server` binary lives here, plus the few supporting
crates that ship alongside it (vendored AI/ML deps, shared protocol
types, derive macros).

## Crates

| Crate | Role |
|-------|------|
| `continuum-core/` | The substrate binary (`continuum-core-server`) and library. Persona / cognition / inference / airc / paging / system_resources / routing / runtime. |
| `continuum-airc-protocol/` | Shared wire types for the substrate's airc command + event protocols. Consumed by `core/continuum-core` (server-side) AND `client/continuum-client` (client-side) so the wire bytes can't drift. |
| `continuum-orm-derive/` | `#[derive(Entity)]` + `#[entity(...)]` proc macros. The rust analogue of TS entity decorators. |
| `inference-grpc/` | gRPC-protocol shim for AI inference (separate binary). |
| `jtag-mcp/` | Rust MCP server (will graduate to `apps/mcp/` per task #143). |
| `livekit-bridge/`, `livekit-protocol/` | WebRTC + LiveKit integration crates. |
| `llama/` | Wrapper around the vendored `llama.cpp` library (`core/vendor/llama.cpp`). |
| `archive/` | Cold-storage archival utilities. |
| `shared/` | Cross-crate shared types kept local to `core/`. |

## Vendored

- `core/vendor/llama.cpp/` — submodule, built via `core/llama/build.rs` and CMake.
- `core/vendor/whisper.cpp/` — submodule for the audio adapters.

## Read first if you're new

The substrate doctrine docs in `docs/architecture/`:

- `CBAR-SUBSTRATE-ARCHITECTURE.md` — RTOS-style runtime contract every rust module inherits.
- `CONCURRENCY-STYLE-GUIDE.md` — canonical concurrent shape (own task + `tokio::time::interval` + `watch::Sender<Snapshot>` + atomic gate + 100ms timeout + quarantine).
- `PERSONA-COGNITION-PIPELINE.md` — what a persona IS and the cognition cycle.
- `RTOS-DEBUGGER-PROBES.md` — `probe!` / `time_sync!` / `time_probe!` macros as RTOS-style breakpoints.

The CLAUDE.md at repo root carries the hot-path "stop, read this first"
gates for the most amnesia-prone surfaces (persona, cognition,
service_loop, monitors, pressure pools, test infrastructure).

## Build

```bash
export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"
cargo check --features metal,accelerate -p continuum-core
```

The workspace root is the repo root (`../Cargo.toml`); members are
declared per-tier (`core/*`, `client/*`, future `apps/*`).
