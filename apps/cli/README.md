# apps/cli — rust CLI binary (placeholder)

**Status:** empty slot. Tracked by task #143 (rewrite `src/jtag` + `src/cli.ts` as rust binary).

## Intent

Replace the current Node-based `./jtag` CLI with a single rust binary that
links `client/continuum-client` directly (no IPC over an extra Node process).

When this lands:
- `apps/cli/Cargo.toml` adds itself to the workspace.
- `apps/cli/src/main.rs` parses args (clap), constructs `Connection::connect(airc, peer_uuid)`,
  routes to typed `CommandClient::execute` calls.
- Per-command argument parsing reuses the same `CommandRequest`/`CommandResponse`
  shapes as the substrate (no manual deserialization).
- Drop-in replacement for `./jtag` at the shell level; `tools/scripts/start-server.sh`
  invokes the new binary in place of the current `tsx cli.ts`.

Until then: use the Node CLI in `src/jtag/`.
