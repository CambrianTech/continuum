# apps/desktop — an unbuilt Tauri wrapper (NOT where the UI lives)

> **Looking for the Continuum UI? It is not in this directory and never has been.**
>
> | what | where |
> |---|---|
> | The UI itself — chat, rooms, kanban, serving console, wall, nav | **`core/continuum-positron/`** (Rust, ~6.4k lines across 19 modules) |
> | The renderer that draws it | **`apps/web/src/`** — `ChatWidget`, `RoomsPanel`, `SysPanel`, `ServingPanel`, `renderLive` / `renderArena` / `renderGrid` / `renderPersona`, with `.spec.ts` tests alongside |
> | A built, runnable bundle | **`apps/web/dist/`** (PWA manifest + icon) |
>
> This directory is a **reservation for a future native shell only**. It is
> empty of source on purpose. If you read "empty slot" here and concluded the
> desktop UI does not exist, that is this file's fault — it happened, it cost a
> real scare, and this header is the fix.

## Status

Unbuilt. There is no Tauri config, no `src/`, and no entry point.

`package.json` is nonetheless a member of the root npm workspace and declares
`@tauri-apps/*` dependencies, so `npm install` resolves it and `npm run dev -w
@continuum/desktop` will fail on a missing Tauri project — expected, not a
regression.

## Intent, when it is built

A first-party desktop embodiment that wraps Continuum without a browser or
terminal: system tray, OS notifications, window management, local hotkeys. It
would be a **thin client** — one of several equal clients over the headless
Rust core — and would host no substrate logic of its own.

| Stack | Pros | Cons |
|-------|------|------|
| Tauri | small bundle, reuses the `apps/web` UI directly | another JS runtime in process |
| Bevy / native Rust | links `client/continuum-client` directly, zero JS | UI primitives less mature |

Tauri is the better fit for visual parity with `apps/web`. Bevy becomes
interesting if `apps/vr` matures and a shared Rust scene graph is worth having.

(The old note here claimed the current desktop path was `src/server-index.ts`
under Electron-like assumptions. That file is now under `legacy/` and is not
the desktop path for anything.)
