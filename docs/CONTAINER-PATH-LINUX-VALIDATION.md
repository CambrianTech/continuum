# Container path — Linux validation work (estimate)

The container-first install is **config-validated on macOS** (`docker compose config` green for base /
mac / gpu overlays; old-Node UI chain removed). But macOS has **no Docker GPU passthrough**, so the actual
image **builds** and a real `docker compose up` can only be verified on a **Linux + NVIDIA** box (or WSL2
for the Windows path). This file is the punch list for that session.

Everything below is *build-time / run-time* — `docker compose config` cannot catch it (it parses the
compose, it does not resolve build contexts or run containers).

## 1. Core image build — ✅ RESOLVED (build-proven on macOS, was a false alarm)
The earlier worry was that `entity_schemas.json` is a generated file absent from a clean checkout. It
turned out to be **vestigial scaffolding**: `modules/entity_schemas.rs` embeds the schema at compile time via
`include_str!("../../../../protocol/typescript/entity_schemas.json")` — a **source-relative** path, and that
file is **checked in** at `protocol/typescript/`, so the Dockerfile's `COPY . .` already provides it. The
`COPY --from=shared-generated …/shared/generated/…` targeted a path the code never reads, and `models.json`
has **zero** references in the Rust core. So the `additional_contexts` block (compose) + both
`COPY --from=shared*` lines (all three core Dockerfiles) were **deleted**. Build-verified on macOS with a
throwaway image that the contexts resolved, and by the fact the native core has compiled all session reading
exactly that `include_str!` path. Nothing to do on Linux here beyond the normal full build (item 3).

## 2. model-init image — ✅ build-proven on macOS
The download scripts had moved to `tools/scripts/` (not `legacy/src/scripts/`), so the build broke on the
script COPYs (caught by an actual `docker build`). Fixed: context → `./tools/scripts`, `models.json` via a
`models` additional-context (`./legacy/src/shared`), dead `generate-scene-models.ts` + `package.json` COPYs
dropped. Image builds clean (`docker build` on macOS — `node:20-slim` is multi-arch, native arm64). The model
DOWNLOAD is a runtime CMD, so only a real `up` on a network-connected box exercises the actual fetch (item 3).

## 3. Full `docker compose --profile gpu up` on Linux+NVIDIA — ~half-day, iterate
Bring up `continuum-core-vulkan` (or `-cuda` via the gpu overlay) + `model-init` + `livekit` (live profile)
+ `forge-worker` + `inference`. Verify: core comes up healthy (socket + IPC ping), **GPU passthrough works**
(nvidia runtime, `--gpus`), the resource daemons see the real GPU/VRAM (holistic view — never an isolated
sandbox), and a persona can generate. This is the real "reliable upstart in a container" gate.

## 4. Desktop → containerized-core WS (grid remote access) — ~2–4h + a 2-machine grid test
The containerized core uses a **unix socket**; a remote Tauri desktop needs the core's **WS ingress on a
TCP port** + (grid mode) Tailscale proxying it. `tailscale-serve.json` now proxies only livekit signaling
(the old widget-server/node-server web proxies were removed). The desktop→core-WS grid proxy is **unbuilt** —
expose the core's WS port in the container + add a Tailscale serve handler for it, then test from a second
machine.

## 5. `install.sh` on a fresh Ubuntu / WSL2 — ~2–3h, iterate
Run the 9-step installer end-to-end on a clean box: system deps → node → rust → python ML → (postgres
opt-in) → livekit → native engine note (now: llama-server + mlx, no Unsloth) → (tailscale opt-in). Confirm
the final `npm start` + `continuum ping` actually work (the message this PR fixed). WSL2 additionally exercises the
`install.ps1` → `bootstrap.sh` handoff.

## 6. postgres profile — ~1h
Verify `docker compose --profile postgres up` + the core's `DATABASE_URL=postgres://…` path still works for
the multi-writer grid case (default is SQLite; postgres is opt-in).

## Rough total
The blocking item (1) is gone. What's left is a real `up` on a Linux+NVIDIA box (item 3) with GPU
passthrough, the model-init build (2), the grid desktop→core-WS proxy (4), a fresh-box `install.sh` (5), and
the postgres profile (6) — none of which can be exercised from a Mac with no Docker GPU. Call it a focused
afternoon of iterating against real container failures once the box is available, plus a WSL2 pass for the
Windows install path.

## What's already done (macOS — config-validated + build-proven where possible)
- **Item 1 resolved + build-proven**: deleted the vestigial `additional_contexts` (compose) + `COPY
  --from=shared*` lines (all three core Dockerfiles). `entity_schemas.json` comes via `COPY . .`
  (`include_str!` from `protocol/typescript/`, checked in); `models.json` is unreferenced by the core.
- Removed the old-Node UI chain: `node-server` + `widget-server` services (+ the Mac override + the
  Tailscale-serve web proxies that fronted them). Desktop-only.
- `model-init` build context repointed `./src` → `./legacy/src` (it legitimately reads `models.json`).
- `docker compose config` green for base / `+mac` / `+gpu`; 0 old-Node services; core-build contexts proven
  to resolve with a throwaway `docker build`.
