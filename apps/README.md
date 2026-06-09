# apps/ — UI shells per embodiment

Thin app shells, one per env (CBAR pattern). Each app is UI-only and
consumes either a per-language SDK from `sdk/` or `client/continuum-client`
directly.

Per `[[citizens-have-envs-not-the-other-way-around]]`: a person and a
persona are the same kind of citizen; `apps/<env>/` is what an
embodiment looks like for that env.

| App | Embodiment | Consumes |
|-----|------------|----------|
| `cli/` | terminal (TTY) | `client/continuum-client` (rust, direct) |
| `web/` | browser | `sdk/typescript` (TS over rust core via IPC) |
| `mcp/` | MCP protocol server | `client/continuum-client` or `sdk/typescript` |
| `mobile/` | iOS + Android (Flutter, one codebase) | `sdk/flutter` |
| `ar/` | AR headsets (Quest / Vision Pro / etc.) | `sdk/flutter` or `sdk/{swift,kotlin}` |
| `vr/` | VR worlds (the Grid embodied) | `sdk/flutter` or Unity FFI |
| `desktop/` | Native desktop | `sdk/typescript` (Tauri) or `client/continuum-client` (Bevy) |

Most apps are placeholders today. Filled in slice by slice — task #143
rewrites the legacy `src/jtag` + `src/cli.ts` as `apps/cli/`; #215 tracks
the Node-side rebuild.
