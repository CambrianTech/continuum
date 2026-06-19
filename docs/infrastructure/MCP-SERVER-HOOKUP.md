# MCP Server Hookup (continuum-mcp) — Runbook

**What:** `continuum-mcp` is the headless-Rust MCP server. An MCP client (unsloth
Studio, Claude Code, …) spawns it over stdio; it auto-discovers the running local
core and exposes continuum's commands as MCP tools. It **replaces the Node
`src/mcp-server.ts`** — no Node in the loop.

**Design:** [UNSLOTH-INTEGRATION.md](../architecture/UNSLOTH-INTEGRATION.md) §5
(Seam 1). It's a *client* of the core ([persona-is-a-client]): `tools/list` →
`mcp/list-tools`, `tools/call` → the command path, all through the same gate any
caller hits.

---

## 1. Build

`continuum-mcp` builds as part of `npm start` (per the all-bins-build-via-start
rule — `tools/scripts/start-server.sh` builds it before launching the core):

```bash
cd src && npm start          # builds continuum-mcp + launches the core
```

The binary lands at `<cargo-target>/debug/continuum-mcp` (or `release/` when
`CONTINUUM_RELEASE` is set). With the shared cache that's
`~/.continuum/cache/cargo-target/debug/continuum-mcp`. Find the exact path:

```bash
ls "${CARGO_TARGET_DIR:-core/target}"/debug/continuum-mcp
```

## 2. Hook up an MCP client

**Turnkey — no config needed.** With the core running, `continuum-mcp`
auto-discovers it (airc liveness probe). The MCP client config is just the binary
path:

```jsonc
// MCP client config (Claude Code ~/.claude.json, or unsloth Studio → Settings →
// Connections → MCP). Use the ABSOLUTE path to the built binary.
{
  "mcpServers": {
    "continuum": {
      "command": "/Users/<you>/.continuum/cache/cargo-target/debug/continuum-mcp"
    }
  }
}
```

That's it — no peer-id or socket to look up (that lookup was the friction that
bred unreliability). Override only if you need to target a non-default core:

```jsonc
{
  "mcpServers": {
    "continuum": {
      "command": ".../continuum-mcp",
      "env": {
        "CONTINUUM_SOCKET": "/path/to/airc-daemon.sock",  // default: discovered
        "CONTINUUM_PEER":   "<core-airc-peer-uuid>",       // default: discovered
        "CONTINUUM_HOME":   "/path/to/.airc",              // default: $AIRC_HOME, else $HOME/.airc
        "CONTINUUM_AGENT":  "continuum-mcp"                // this client's agent name
      }
    }
  }
}
```

## 3. Verify

In the MCP client, confirm the `continuum` server connects and lists tools, then
call one:
- `tools/list` shows continuum commands (e.g. `interface_screenshot`,
  `collaboration_chat_send`, `mcp_search_tools`).
- `tools/call` `mcp_search_tools {"query":"chat"}` returns matches.
- A real command (e.g. `collaboration_chat_send`) runs and returns content.

Manual smoke (stdio, no client) — initialize + list:

```bash
printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | "${CARGO_TARGET_DIR:-core/target}"/debug/continuum-mcp
# Expect two JSON-RPC response lines: initialize result, then the tools array.
```

## 4. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `could not discover a healthy local core` on startup | Core isn't running — `cd src && npm start`. Or the airc daemon socket is stale (`airc doctor --fix`). Or override `CONTINUUM_SOCKET` + `CONTINUUM_PEER`. |
| Tools list empty | Catalog (`mcp/list-tools`) returned nothing — check the core started its module registry. |
| A `tools/call` returns `isError` content | The command was refused (auth/params) — that's a tool error surfaced to the model, not a transport failure; read the content. |
| Diagnostics polluting the client | They shouldn't — `continuum-mcp` writes diagnostics to **stderr**; only JSON-RPC goes to stdout. |

---

## 5. Retiring `src/mcp-server.ts` (gated on green)

Once §3 verifies `continuum-mcp` against a real MCP client (unsloth Studio at
`127.0.0.1:8888` and/or Claude Code), retire the Node server it replaces:

1. Delete `src/mcp-server.ts` (871 lines of Node + `@modelcontextprotocol/sdk` +
   `sharp`).
2. Drop its `package.json` entries: `bin.mcp` (`./mcp-server.ts`) and the
   `mcp` / `mcp:setup` scripts (and `@modelcontextprotocol/sdk` / `sharp` from
   deps if unused elsewhere).
3. Repoint any MCP client config / `tools/scripts/setup-mcp.sh` from
   `npx tsx mcp-server.ts` to the `continuum-mcp` binary (§2).
4. Confirm no remaining importers of `mcp-server.ts`.

Do this as its own commit **after** the live green, so the cutover is reversible
and never leaves the repo without a working MCP server.
