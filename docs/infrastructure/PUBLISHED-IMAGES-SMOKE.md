# Published Images Smoke Test

Smoke results from pulling each ghcr.io/cambriantech image to a clean M5
docker host and starting it with minimal config. Run 2026-04-15.

Validates that the images CI publishes are individually startable, surface
their failure modes gracefully when their dependencies are absent, and
don't have undocumented runtime requirements that would bite a fresh user.

## Pull stats

| Image | Size | Pull (M5 wifi) | Status |
|-------|------|----------------|--------|
| `continuum-core` | 269 MB | 9 s | ✅ OK |
| `continuum-model-init` | 73 MB | 2 s | ✅ OK |
| `continuum-widgets` | 1.2 GB | 40 s | ✅ OK (large) |
| `continuum-node` | 1.3 GB | 46 s | ✅ OK (large) |
| `continuum-livekit-bridge` | — | — | ❌ permission denied |

Total cold-pull payload (excluding livekit-bridge + cuda variant): ~3.0 GB.

## Findings

### `continuum-core` ✅ healthy boot

Started standalone with no socket mount, no DB. Boots cleanly. Notable:

- All 20 expected modules registered; "✅ Continuum Core Server fully started"
- Whisper STT model absent — graceful warning with HuggingFace download URL + target path
- TTS adapter initialized OK
- onnxruntime cosmetic warning: "Unknown CPU vendor" inside Rancher VM (harmless)

**Drift to fix**: registry warns "Unexpected module registered (not in EXPECTED_MODULES): sentinel" and same for `vision`. The `EXPECTED_MODULES` constant is out of date relative to the modules actually shipping. Cosmetic but noisy.

### `continuum-widgets` ✅ HTTP fallback works

Started standalone. Notable:

- `tailscale: not found` inside the image — graceful: "🔓 No TLS — serving HTTP" instead of crashing
- HTTP server listening on `:9000`
- WebSocket proxy `:9000 → :9001` advertised

**Image-size note**: 1.2 GB feels heavy for a widget HTTP server. If the layer breakdown shows large npm/node-modules from a transitive build, a multi-stage Dockerfile that strips dev deps could halve it.

### `continuum-node` ⚠️ requires sibling continuum-core

Started standalone. Notable:

- JTAG system readiness check sets up a file watcher
- "No valid signal file yet" — expects a shared volume containing the readiness signal from `continuum-core`
- Standalone hangs waiting (correct behavior for a wired-stack assumption, but undocumented for a casual pull-and-run)

**Doc gap**: `docker run ghcr.io/.../continuum-node` looks like it could be a stand-alone smoke, but it's actually a half of a paired stack. README should clarify or the image entrypoint should print "this image expects a shared `/.continuum/sockets/` volume + sibling continuum-core."

### `continuum-model-init` ✅ does its actual job

One-shot job. With `-v /tmp/smoke-models:/models`, downloads start:

- Pocket-TTS voice embeddings: skipped (no `HF_TOKEN` — graceful)
- Silero VAD (~2 MB): downloaded
- Orpheus TTS (3B GGUF + tokenizer + SNAC, ~2.5 GB total): tokenizer skipped (HF_TOKEN), SNAC downloaded, GGUF starts download

Smoke killed mid-Orpheus download — sufficient to confirm the script executes its plan. Failure modes (no `HF_TOKEN`, terms not accepted) surface as colored warnings with action steps.

### `continuum-livekit-bridge` ❌ ghcr permission denied

`docker pull ghcr.io/cambriantech/continuum-livekit-bridge:latest` returns
`error from registry: denied`.

Same package-permission gotcha that hit `continuum-model-init` earlier
this branch (fixed via the GitHub Packages → Settings → Manage Actions
access → grant the `continuum` repo Write role). Joel needs to apply the
same UI fix to `continuum-livekit-bridge`. Until then any user pulling
the gpu profile fails on the bridge image.

## Recommendations (priority order)

1. **`continuum-livekit-bridge` package permission** — apply the
   model-init fix to this package. Without it, `docker compose up`
   on the livekit bridge profile silently fails for everyone outside
   the org.
2. **`EXPECTED_MODULES` drift** in `continuum-core` — add `sentinel` +
   `vision` to the expected list (or remove the warning if the registry
   is intended to be open-ended).
3. **`continuum-node` + `continuum-widgets` size** — both >1 GB. Multi-
   stage build review for both could halve them. Not blocking.
4. **`continuum-node` standalone usability** — README or entrypoint hint
   that the image expects a paired `continuum-core` + shared socket
   volume.

## What this test does NOT cover

- `continuum-core-cuda` — requires NVIDIA Container Toolkit + GPU, M5
  doesn't have. Memento exercises this via BigMama validation.
- Inter-service wire (continuum-core ↔ node-server ↔ widgets) — needs
  full `docker compose up` with all services and shared volumes. The
  PR891-E2E-VALIDATION.md playbook covers that.
- Real persona inference end-to-end — single-image start cannot prove
  the substrate works under load.

This smoke test catches per-image failures BEFORE the e2e covers them,
so the e2e isn't fighting noisy image bugs while trying to prove
inference works.
