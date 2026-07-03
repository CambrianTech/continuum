# @continuum/tui

A terminal chat client for Continuum. It is the **outlier-B** validation of the
client SDK seam (task #29): the maximally-different renderer to apps/web's Lit
`<chat-widget>`. Both consume the identical SDK client seam
(`StateConnection` READ + `chat/send` SEND) and the identical shared projection
(`@continuum/chat-view`), differing only in surface — DOM+Lit vs stdin+ANSI. That
the same seam fits both without forcing is the proof the interface is right.

## What it is (and isn't)

- **UI only.** No substrate logic. The who/what/where projection lives in
  `@continuum/chat-view`, shared with the web widget; this package owns only the
  ANSI renderer and the readline compose loop.
- **Zero extra deps.** Runs on Node ≥ 22's global `WebSocket`; the SDK socket
  classes fail loud if it is absent rather than pulling in a `ws` polyfill.

## Run

The core must be running with `CONTINUUM_CORE_WS=<port>` set (there is no default
port — a guessed one points at nothing).

```bash
# flags (repointable inline):
node --experimental-strip-types apps/tui/src/index.ts \
  --core ws://127.0.0.1:8974 --me <your-user-uuid>

# or via the workspace script + env:
CONTINUUM_WS=ws://127.0.0.1:8974 CONTINUUM_USER_ID=<uuid> \
  npm start -w @continuum/tui
```

Config resolves **flag → env** (first hit wins), fails loud naming exactly what
is missing. See `.env.example`.

- `--core` / `CONTINUUM_WS` — the core's WS ingress url.
- `--me` / `CONTINUUM_USER_ID` — your citizen UUID, threaded as `chat/send`'s
  `senderId`. Identity pairing is not wired yet (tasks #37/#38), so this is
  explicit config, never minted.

Type a line and press enter to send; a blank line just repaints; Ctrl-D exits.

## Layout

Joel's three-panel who/what/where design, projected onto a line-oriented
terminal as three labelled sections:

```
general  1/2 here · room-1

WHO
  ● * Asha [claude]
  ○ > Bo

WHAT
  14:03 * Asha [claude]: hello there
```

- **WHERE/WHICH** — the header (room + live counts + id).
- **WHO** — the roster (`●` active / `○` idle; `>` human, `*` agent, `~` system).
- **WHAT** — the conversation.

## Develop

```bash
npm run typecheck -w @continuum/tui   # tsc --noEmit, Node lib (not DOM)
npm run lint -w @continuum/tui         # strict typescript-eslint (no any, etc.)
npm test -w @continuum/tui             # vitest — pure renderer + config resolver
```

The renderer (`renderChat`) and the config resolver (`resolveConfig`) are both
pure functions, unit-tested without a TTY or a live core.
