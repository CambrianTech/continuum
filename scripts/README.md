# The factory — dev-loop automation

> Build the factory *with* the product, keep it **state of the art**, and make it run
> **everywhere the product does** (Windows / macOS / Linux). These tools exist so the
> repetitive loops cost one command, not eleven, and so the same leverage is available to
> every citizen — human or persona ([[build-the-factory-as-you-build-the-car]]). Cross-platform
> by rule: Node or Rust, never bash + OS-specific paths ([[solve-for-public-users]]).

## The two loops, one command each

| Loop | Command | What it does |
|---|---|---|
| **Edit → canary** | `node scripts/ship.mjs ["title" ["body"]]` | Push the current feature branch → open a PR into canary → wait for CI → squash-merge → delete branch → sync local canary. Refuses canary/main; a red PR **cannot** be merged (blocks on branch protection; never `--no-verify`/`--admin`). |
| **See it (never blind)** | `node scripts/shot.mjs [url] [out.png]` | Headless screenshot of a running URL → PNG. OS-detected Chrome/Chromium/Edge; wall-clock guard so a live-WebSocket page can't hang the capture. Default URL `http://localhost:5173/`. |
| **Layout truth** | `node scripts/inspect.mjs <url> <selector>` | Pixels lie; this prints the DOM box model + computed styles for a selector and its ancestor chain (pierces shadow DOM), flagging where `scrollWidth > clientWidth` — the real overflow source. Forces the viewport via CDP `Emulation` (`SHOT_SIZE=390,844` = true phone width). Built the moment `shot.mjs`'s pixels-only blind spot bit a real bug. |

Typical brick: make the change → validate (cargo test / the live app) → `node scripts/shot.mjs …` to *see* it → `node scripts/ship.mjs "feat(x): …"`. Both are dogfooded (each has shipped itself).

### shot.mjs — seeing the live app

```
node scripts/shot.mjs "http://localhost:5173/?core=ws://localhost:8974&me=<uuid>" /tmp/app.png
```
The web app fails loud if `?core=ws://host:port` (the core WS) or `?me=<uuid>` (sender identity)
is missing — that error boundary IS the diagnostic. Env: `CHROME`, `SHOT_SIZE` (default `1600,1000`),
`SHOT_BUDGET_MS` (SPA settle, default `6000`).

## SOTA roadmap — the factory improves toward state of the art

The bar keeps rising; never let a tool sit at "good enough." Known next steps:

1. **`npm run ship` / `npm run shot`** aliases at the root for discoverability (the CLAUDE.md
   "generators create discoverable systems" ethos).
2. **A `boot` leg** — one command to build + start the core + web + serving so the see-it loop is
   runnable from a cold clone (today the stack must already be up). The missing third leg.
3. **`shot` → CDP (like `inspect`)** — two weaknesses to close together: (a) replace the wall-clock
   guard with a real readiness signal (CDP `Page.loadEventFired` / a DOM marker) so captures are
   deterministic, not time-boxed; (b) `--window-size` does NOT set the real viewport (inspect found it
   renders ~500px when asked for 390) — use `Emulation.setDeviceMetricsOverride` so a "mobile" shot is
   a *true* phone width, matching what `inspect` already does. Shared CDP helper between shot + inspect.
4. **cu-native — converge to the JTAG/feedback port** (the endgame, [[feedback-is-a-first-class-cross-modality-dimension-jtag-cu]]). Feedback (screenshot / inspect / perf / log) is a first-class dimension that must be uniform + easy across **every** modality (web / mobile / ARVR / rag-persona) through the one `uu` port, so a **persona (Asha) runs the same verb a human does**. The substrate already has the pattern — the `Screenshotter` trait (`commands/interface/capture/{web,ios,android}.rs`, #94: one trait, N targets, fails loud persona-actionably). So: (a) `shot.mjs` reinvents the *web slice* of that adapter → route it through `uu interface/capture` (dev-external is the pre-core-boot fallback); (b) `inspect.mjs` is a NEW capability → make it a sibling **Inspector adapter family** (`uu interface/inspect`; web=CDP, mobile=layout/accessibility tree); (c) close the **modality gaps** — ARVR + rag/persona capture, and the same first-class treatment for **performance** + **logging** across all surfaces.

5. **Port the rest of the factory off bash** — the existing npm scripts (`start`, `stop`,
   `install:continuum`, `setup:git-hooks`, `docker:ensure`) all shell out to `bash tools/scripts/*.sh`,
   which needs Git Bash/WSL on Windows. The whole factory layer should follow `ship`/`shot` to
   cross-platform Node (or `uu`), so a cold Windows clone works with zero bash. Broader Windows debt.

Discoverable: `npm run ship` / `npm run shot` (cross-platform aliases for the `.mjs` tools).
Retired: `ship.sh`, `shot.sh` (macOS-only bash — replaced by the portable `.mjs` above).
