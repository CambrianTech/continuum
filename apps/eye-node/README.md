# @continuum/eye-node

**An opt-in worker that gives personas eyes.** (#187 Perception Surface · #29 client SDK)

A headless core cannot render or capture — no browser, no display on a rack. So
`perception/observe` (and `interface/screenshot`) are **Provided** commands: one
name, fulfilled by a connected adapter. The eye-node is that adapter for the web.

It connects to the core over the IPC socket, registers as the provider of
`perception/observe` and `perception/hot-edit` (via `@continuum/sdk-typescript`'s
`NodeSocketTransport`), and fulfils each call by driving a real browser
(`@continuum/perception`).

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

## Hot css, no deployments

`perception/hot-edit { target, css, viewport?, selector? }` is the TWEAK verb of
the design loop (render → observe → hot-edit → re-grade): open `target`, apply
`css` as the page's single hot-patch layer (`<style data-continuum-hot-edit>`,
**replaced wholesale** each call — empty `css` clears it), re-observe, and
return the same observation shape observe does, plus `appliedCss` and a `delta`
(fraction of pixels the patch moved). The page is re-opened fresh per call
today, so a persona passes its FULL accumulated stylesheet each time; a
persistent live session is the next step and changes only the adapter's session
lifetime, never the wire.

## Drive a live page: `perception/interact`

`perception/interact { session?, target?, viewport?, actions, selector? }` keeps a
page open across calls (card 3569675f). The first call passes `target` and gets a
`session` handle back with the observation; later calls pass `session` plus
`actions` (`click`, `type`, `press`, `hover`, `goto`, with CSS selectors aimed at the
returned tree) and get the page after them plus a `delta`. That lets a persona click
through a flow on her own dev server and capture evidence of each step.
`perception/session-close { session }` releases a browser early. Sessions also
close after 10 idle minutes, and at most 8 are open per eye-node (a ninth is
refused with the reason, never an eviction of a live one).

## Run

```bash
# from repo root (workspaces linked): start an eye-node against the local core
CONTINUUM_CORE_SOCKET=/tmp/continuum-core.sock npm --workspace @continuum/eye-node start
# or directly (the endpoint is required; use the path your core reports)
cd apps/eye-node && CONTINUUM_CORE_SOCKET=/tmp/continuum-core.sock npx tsx src/index.ts
```

Env:

- `CONTINUUM_CORE_SOCKET` — core IPC socket path or `tcp://host:port`. Required, with no
  default: the endpoint differs by platform (a Unix socket path, or on Windows a local TCP
  listener, `tcp://127.0.0.1:<port>`), so the launcher passes the one the core's endpoint
  resolver reports. Without it the eye-node exits and
  names the variable.
- `EYE_NODE_LABEL` — provider label shown in the core's logs.

**Opt-in, browserless-core principle:** not every core runs a browser. Start an
eye-node on a browser-capable node (a laptop, a render worker that chose to
install Chromium). While one is connected, every persona on that core can see;
when none is, `perception/observe` fails loud ("no eye-node connected") rather
than fabricating an observation.

## Shape

### PDF documents

`perception/observe` also accepts a local `file:///.../document.pdf#page=1`
target. Page numbers are one-based; omission selects page 1. The file must exist
on the provider node. Remote PDF fetching and OCR are not implemented.

Install Poppler on that node and make `pdfinfo`, `pdftotext`, and `pdftoppm`
available on the **eye-node process PATH** (an interactive terminal's PATH may
differ from its service). Missing decoders produce an explicit failure.

The reusable `PdfSurface` returns page text and rendered PNG pixels from the
same immutable snapshot, with source SHA-256, page number and total pages in
the structure. The existing capture-retention/native-image pipeline handles
the result unchanged. A scanned page can have an empty text layer while still
providing its pixels; empty text is not a claim of OCR or document comprehension.

Each call observes one page, caps source size at 32 MiB, text at 1 MiB, PNG at
12 MiB, and decoder time at 30 seconds total. The viewport bounds rendering
(aspect ratio preserved, longest side bounded by the smaller viewport dimension;
default 1440, maximum 4096). Temporary snapshots are cleaned on close. Documents
are read-only; selectors/CSS editing and browser interaction do not apply.

Example: `continuum perception/observe --target=file:///C:/documents/resume.pdf#page=2`

This is a second provider-side Surface implementation, not a new command bus or
plugin installation system. Additional formats should implement the same Surface
contract and use the existing command provider and evidence delivery boundaries.

```
index.ts        entry — take the core endpoint (coreEndpoint.ts), start, stay alive
eyeNode.ts      EyeNode — connect, provide(observe, hot-edit, interact, session-close), flush
observeAdapter  ObserveParams → PerceptionSession.openWeb → observe → ObserveResult
hotEditAdapter  HotEditParams → openWeb → observe → hotPatchCss → re-observe (+Delta) → HotEditResult
interactAdapter InteractSessions: handle → live PerceptionSession; interact (+Delta), close, idle sweep
```

The wire contract (`ObserveResult`, `ProbeNode`, …) is single-sourced from Rust
(`protocol/typescript/perception`); the adapter maps `@continuum/perception`'s
internal `Observation` onto it at the boundary.

## Next

- CV-aid ladder for non-VLM personas (YOLO / OCR / layout+contrast → text).
- `SceneSurface`/`BevySurface` targets (3D) — same `perception/observe`, the
  adapter just renders a scene instead of a page.
