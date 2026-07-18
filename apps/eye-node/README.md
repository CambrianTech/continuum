# @continuum/eye-node

**An opt-in worker that gives personas eyes.** (#187 Perception Surface · #29 client SDK)

A headless core cannot render or capture — no browser, no display on a rack. So
`perception/observe` (and `interface/screenshot`) are **Provided** commands: one
name, fulfilled by a connected adapter. The eye-node is that adapter for the web.

It connects to the core over the IPC socket, registers as the provider of
`perception/observe` (via `@continuum/sdk-typescript`'s `NodeSocketTransport`),
and fulfils each call by driving a real browser (`@continuum/perception`).

## What a persona sees

`perception/observe { target }` returns pixels **and** structure:

- **image** — the rendered frame as a `data:` URL (SEE / JUDGE).
- **structure** — the tree of named, boxed nodes (REASON / aim actions at an
  element, not a pixel).
- **url / title** — the surface's identity.

`target` is uniform — it's just a URL:

| A persona wants to see… | `target` |
|---|---|
| Continuum's own interface | the positron UI URL (e.g. `http://localhost:<port>`) |
| An interface they built in a project | that project's dev-server URL |
| A benchmark harness | the benchmark's URL |
| A room / recipe / activity | its route in the positron UI |

## Run

```bash
# from repo root (workspaces linked): start an eye-node against the local core
CONTINUUM_CORE_SOCKET=/tmp/continuum-core.sock npm --workspace @continuum/eye-node start
# or directly
cd apps/eye-node && npx tsx src/index.ts
```

Env:

- `CONTINUUM_CORE_SOCKET` — core IPC socket path or `tcp://host:port`
  (default `/tmp/continuum-core.sock`, matching `cu`).
- `EYE_NODE_LABEL` — provider label shown in the core's logs.

**Opt-in, browserless-core principle:** not every core runs a browser. Start an
eye-node on a browser-capable node (a laptop, a render worker that chose to
install Chromium). While one is connected, every persona on that core can see;
when none is, `perception/observe` fails loud ("no eye-node connected") rather
than fabricating an observation.

## Shape

```
index.ts        entry — resolve socket, start, stay alive
eyeNode.ts      EyeNode — connect, provide('perception/observe'), flush
observeAdapter  ObserveParams → PerceptionSession.openWeb → observe → ObserveResult
```

The wire contract (`ObserveResult`, `ProbeNode`, …) is single-sourced from Rust
(`protocol/typescript/perception`); the adapter maps `@continuum/perception`'s
internal `Observation` onto it at the boundary.

## Next

- CV-aid ladder for non-VLM personas (YOLO / OCR / layout+contrast → text).
- `SceneSurface`/`BevySurface` targets (3D) — same `perception/observe`, the
  adapter just renders a scene instead of a page.
