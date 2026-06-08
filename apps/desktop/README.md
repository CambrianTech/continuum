# apps/desktop — native desktop shell (placeholder)

**Status:** empty slot. Two candidate stacks:

| Stack | Pros | Cons |
|-------|------|------|
| Tauri | small bundle, web UI reuse from `apps/web` | another JS runtime in process |
| Bevy / native rust | links `client/continuum-client` directly, zero JS | UI primitives less mature |

## Intent

A first-party desktop embodiment that wraps continuum without a browser
or terminal — system tray, OS notifications, window management, local
hotkeys. Today the only "desktop" path is `src/server-index.ts` running
under Electron-like assumptions; that's a transitional crutch.

Tauri makes the most sense for visual parity with `apps/web`; Bevy
becomes interesting when `apps/vr` matures (shared rust scene graph).
