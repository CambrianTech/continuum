# apps/mcp — MCP protocol server (placeholder)

**Status:** rust crate `core/jtag-mcp/` exists today; will graduate here
when task #143 lands the rust client rewrite. Node MCP shim at
`src/mcp-server.ts` will be retired.

## Intent

Expose continuum's command surface to MCP-speaking AI clients (Claude
Desktop, Cursor, etc.) as MCP tools. Each substrate command becomes an
MCP tool with typed params + result, dispatched through
`client/continuum-client` instead of through `./jtag` shell-outs.

Two viable shapes:
- **Rust binary** linking `client/continuum-client` (matches `apps/cli`).
- **Node MCP server** consuming `sdk/typescript` (preserves the existing
  MCP TS ergonomics; thinner glue).

The choice depends on which has lower latency under a real client
session — empirically tracked in the latency campaign (#195).
