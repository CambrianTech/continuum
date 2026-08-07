# apps/ — UI shells per embodiment

Thin app shells, one per env (CBAR pattern). Each app is UI-only and
consumes either a per-language SDK from `sdk/` or `client/continuum-client`
directly.

Per `[[citizens-have-envs-not-the-other-way-around]]`: a person and a
persona are the same kind of citizen; `apps/<env>/` is what an
embodiment looks like for that env.

**Status is per-app and half of these are empty.** The table says which, so
nobody has to open a directory to find out. Line counts are source only
(`.rs`/`.ts`/`.dart`, excluding `node_modules`), measured 2026-08-06.

| App | Embodiment | Status | Consumes |
|-----|------------|--------|----------|
| `web/` | browser | **built** — 28 files, ~8.9k lines, `dist/` present | `sdk/typescript` (TS over rust core via IPC) |
| `mobile/` | iOS + Android (Flutter, one codebase) | **in progress** — 14 files, ~1.1k lines | `sdk/flutter` |
| `tui/` | terminal UI | **in progress** — 7 files, ~665 lines | `client/continuum-client` |
| `cli/` | terminal (TTY) | **in progress** — 2 files, ~577 lines | `client/continuum-client` (rust, direct) |
| `eye-node/` | headless observer node | **in progress** — 3 files, ~205 lines | `sdk/typescript` |
| `ar/` | AR headsets (Quest / Vision Pro / etc.) | *empty — no source* | `sdk/flutter` or `sdk/{swift,kotlin}` |
| `vr/` | VR worlds (the Grid embodied) | *empty — no source* | `sdk/flutter` or Unity FFI |
| `desktop/` | native desktop shell | *empty — no source* | `sdk/typescript` (Tauri) or `client/continuum-client` (Bevy) |
| `mcp/` | MCP protocol server | *empty here* — the working crate is `core/jtag-mcp/`, which has not graduated into `apps/` yet | `client/continuum-client` or `sdk/typescript` |

## Where the UI actually is

`apps/web/` is the **renderer**. The UI itself — chat, rooms, kanban, serving
console, wall, nav — lives in **`core/continuum-positron/`** (Rust, ~6.4k lines
across 19 modules), which projects state that `apps/web/src/` draws.

So "`apps/desktop/` is empty" is true and says nothing about whether Continuum
has a UI. It does. Reading the empty slot as the answer to "where is the
desktop UI" is a mistake this file used to invite; see
`apps/desktop/README.md`.

Remaining empty slots get filled slice by slice — #29 tracks the Node-side
client SDK rebuild.
