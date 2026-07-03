# apps/web — browser UI shell

Thin browser client for Continuum, reinvented on `@continuum/sdk-typescript`
(NOT a repair of the legacy `src/{browser,widgets,server,daemons}/` Node
monolith — that tree stays reference-only and gets pruned once this reaches
parity, task #83). UI-only: DOM rendering, state subscription, command dispatch
through the SDK. No business logic lives here; every substrate decision stays in
`core/` ([[headless-core-many-clients]]).

## What's built: the three-panel chat surface (task #29)

`<chat-widget>` renders Joel's **who / what / where-which** layout from one live
positron `ChatViewState` snapshot:

```
┌─────────────────────────────────────────────┐
│ header — WHERE/WHICH (room name + N/M here)  │
├───────────────┬─────────────────────────────┤
│ roster — WHO  │ messages — WHAT             │
│ (presence)    │ (the conversation)          │
├───────────────┴─────────────────────────────┤
│ compose — talk to Asha normally             │
└─────────────────────────────────────────────┘
```

Two sockets to the same core WS ingress (`src/index.ts` is the only file that
touches both the SDK and the DOM):

- **READ** — a `StateConnection` subscribed to `kind="chat"`; each envelope
  becomes a `ChatState` pushed onto `widget.state`, Lit re-renders. This is the
  same positron read surface (#84) the persona itself observes.
- **SEND** — a `Continuum` command client; the compose bar issues one
  `chat/send` into the room on screen. Asha's reply arrives back through the READ
  stream (no optimistic local append to drift).

Layering (each layer testable in isolation):
`chatViewModel` (pure projection, unit-tested browser-free) → `renderChat` (pure
Lit template) → `ChatWidget` (the Lit host: reactive state + compose/send) →
`index.ts` (SDK wiring).

## Run it

The core must be running with its thin-client WS ingress enabled — it only opens
the listener when `CONTINUUM_CORE_WS=<port>` is set (there is **no** hardcoded
port), binding `127.0.0.1:<port>`.

```bash
# 1. copy the config template and fill BOTH values (no invented defaults — fail-loud)
cp apps/web/.env.example apps/web/.env
#    VITE_CONTINUUM_WS      = ws://127.0.0.1:<CONTINUUM_CORE_WS>
#    VITE_CONTINUUM_USER_ID = a real seeded/registered user UUID (identity pairing
#                             is not wired yet — tasks #37/#38)

# 2. dev server
npm run dev -w @continuum/web
```

Either value can be overridden per-tab without a rebuild via query params:
`…/?core=ws://127.0.0.1:8974&me=<uuid>`.

## Verify

```bash
npm run typecheck -w @continuum/web   # tsc --noEmit (strict)
npm run test      -w @continuum/web   # vitest — the pure view-model spec
```

Validation is pure TS (`tsc` + `vitest`) — never `npm start`/`./jtag`
([[validate-via-pure-rust-not-npm-jtag]]).
