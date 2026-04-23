#!/bin/bash
# push-current-arch.sh — single-line entry point for pre-push hook AND
# manual use. Detects the host's native OS+arch and delegates to
# push-image.sh for the slices THIS machine can build natively.
#
# The whole point: the CI story for multi-arch Docker builds is broken
# (QEMU emulation from amd64 GHA runners to linux/arm64 = 5-6 hour
# timeouts on every PR — see verify-architectures failures on PR #950).
# Instead, each dev machine pushes its native arch:
#
#   Mac M-series (arm64)  → linux/arm64 slice of core + livekit-bridge
#   Linux amd64           → linux/amd64 slices of core + vulkan + livekit-bridge
#   Linux amd64 + Nvidia  → + cuda variant (linux/amd64 only)
#
# Note: vulkan is amd64-only. Mac Docker Desktop has no GPU passthrough,
# and arm64 vulkan has no realistic consumer use case (Asahi/Pi users
# build native, not in Docker). BigMama (linux/amd64, also Windows WSL2
# capable) owns the vulkan slice.
#
# CI's job shrinks to: build the amd64 slice on a GHA runner (native,
# fast) if it's not already in the registry, then combine arch slices
# into a multi-arch manifest, then verify-architectures gates merge.
# See docker-images.yml for the workflow changes that pair with this.
#
# Usage:
#   scripts/push-current-arch.sh
#
# Env overrides:
#   SKIP_PHASE_0=1   — skip the cargo test gate (push-image.sh's Phase 0).
#                      Useful when iterating on Docker/CI config with
#                      no Rust changes. Default: gate enabled.
#   VARIANT=<name>   — only push this variant (core | cuda | vulkan).
#                      Default: all variants the host supports natively.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

OS="$(uname -s)"
ARCH="$(uname -m)"

# What variants does this host build natively for its own arch?
# "Natively" means: Docker's build runs without QEMU emulation for the
# target platform, AND the GPU toolkit (CUDA / Vulkan) is available in
# the builder image's repo tree (vendored or pullable).
case "$OS/$ARCH" in
  Darwin/arm64)
    # Mac M-series: linux/arm64 is natively buildable via Docker Desktop's
    # Linux VM. Mac uses Metal natively (continuum-core base, not vulkan)
    # and Docker Desktop has no GPU passthrough — there's no point shipping
    # vulkan/arm64 from this host. Core + livekit-bridge cover the arm64
    # leg. Vulkan + CUDA come from BigMama (linux/amd64).
    HOST_PLATFORM="linux/arm64"
    HEAVY_VARIANTS=("core" "livekit-bridge")
    ;;
  Linux/x86_64)
    # Linux amd64 (BigMama, Windows WSL2): native platform. Core + vulkan
    # + livekit-bridge always; CUDA only when Nvidia driver is present
    # (nvidia-smi reports a GPU). Vulkan here covers Linux + Windows WSL2
    # consumer GPU users.
    HOST_PLATFORM="linux/amd64"
    HEAVY_VARIANTS=("core" "vulkan" "livekit-bridge")
    if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi >/dev/null 2>&1; then
      HEAVY_VARIANTS+=("cuda")
    fi
    ;;
  Linux/aarch64 | Linux/arm64)
    # Linux arm64 (e.g. Raspberry Pi, Nvidia Jetson, ARM cloud host).
    # Same logic as Mac: no realistic vulkan/arm64 consumer story, so
    # core + livekit-bridge only.
    HOST_PLATFORM="linux/arm64"
    HEAVY_VARIANTS=("core" "livekit-bridge")
    ;;
  *)
    echo "ERROR: push-current-arch.sh — unsupported host $OS/$ARCH" >&2
    echo "       Supported: Darwin/arm64, Linux/x86_64, Linux/aarch64" >&2
    exit 1
    ;;
esac

# Light (TS-only) images: node-server, model-init, widget-server.
# These are small Node.js / static-content Dockerfiles with no Rust
# compile, so they build in <2 min even via QEMU. Multi-arch in one
# pass is fine. We push them on every dev-machine run so both arches
# stay current — last push wins for the manifest, but since builds are
# fast and fully reproducible from source, "last wins" is fine.
LIGHT_IMAGES=(
  "continuum-node:docker/node-server.Dockerfile:./src"
  "continuum-model-init:docker/model-init.Dockerfile:./src"
  "continuum-widgets:docker/widget-server.Dockerfile:./src"
)

# VARIANT env var lets a caller override the default heavy set (useful
# for iterating on one variant without the full ~20+ min cost).
if [[ -n "${VARIANT:-}" ]]; then
  HEAVY_VARIANTS=("$VARIANT")
fi

# SKIP_LIGHT=1 skips the TS-only image push (e.g. iterating on Rust only).
# SKIP_HEAVY=1 skips the Rust-heavy push (e.g. only updating widgets).
SKIP_LIGHT="${SKIP_LIGHT:-0}"
SKIP_HEAVY="${SKIP_HEAVY:-0}"

cd "$REPO_ROOT"

REGISTRY="ghcr.io/cambriantech"
SHA="$(git rev-parse --short HEAD)"
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
BRANCH_TAG="$(echo "$BRANCH" | tr '/' '-')"
PR_NUMBER="${PR_NUMBER:-}"
if [[ -z "$PR_NUMBER" ]] && command -v gh >/dev/null 2>&1; then
  PR_NUMBER="$(gh pr list --head "$BRANCH" --json number --jq '.[0].number // empty' 2>/dev/null || true)"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  push-current-arch: $OS/$ARCH → $HOST_PLATFORM"
echo "  heavy:  ${HEAVY_VARIANTS[*]}"
echo "  light:  $(if [[ "$SKIP_LIGHT" -eq 0 ]]; then echo "node + model-init + widgets"; else echo "(skipped)"; fi)"
echo "  branch: $BRANCH"
echo "  sha:    $SHA"
[[ -n "$PR_NUMBER" ]] && echo "  pr:     #$PR_NUMBER"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# ── Heavy variants (Rust-compiling, native arch only) ───────────────
if [[ "$SKIP_HEAVY" -eq 0 ]]; then
  for V in "${HEAVY_VARIANTS[@]}"; do
    case "$V" in
      cuda)
        # CUDA variant is always linux/amd64. If HOST_PLATFORM is arm64,
        # this machine can't build cuda natively — skip with a note.
        if [[ "$HOST_PLATFORM" != "linux/amd64" ]]; then
          echo "→ Skipping cuda (requires linux/amd64 host; this is $HOST_PLATFORM)"
          continue
        fi
        echo "→ scripts/push-image.sh cuda"
        "$SCRIPT_DIR/push-image.sh" cuda
        ;;
      core|vulkan|livekit-bridge)
        echo "→ scripts/push-image.sh $V $HOST_PLATFORM"
        "$SCRIPT_DIR/push-image.sh" "$V" "$HOST_PLATFORM"
        ;;
      *)
        echo "WARN: unknown heavy variant '$V' — skipped" >&2
        ;;
    esac
  done
fi

# ── Light variants (TS-only, multi-arch via QEMU is fast) ───────────
# These are direct `docker buildx build --push` invocations rather than
# going through push-image.sh — the script's Rust-shaped phases (cargo
# test gate, slice tests) don't apply to TS-only Dockerfiles.
if [[ "$SKIP_LIGHT" -eq 0 ]]; then
  echo ""
  echo "→ Building light TS images (multi-arch via QEMU; fast, no Rust)"

  if ! docker buildx inspect continuum-builder &>/dev/null; then
    docker buildx create --name continuum-builder --use >/dev/null
  else
    docker buildx use continuum-builder >/dev/null
  fi

  for ENTRY in "${LIGHT_IMAGES[@]}"; do
    IFS=':' read -r IMAGE DOCKERFILE CONTEXT <<< "$ENTRY"
    TAG_SHA="$REGISTRY/$IMAGE:$SHA"
    TAG_BRANCH="$REGISTRY/$IMAGE:$BRANCH_TAG"
    LIGHT_TAGS=(--tag "$TAG_SHA" --tag "$TAG_BRANCH")
    [[ "$BRANCH" == "main" ]] && LIGHT_TAGS+=(--tag "$REGISTRY/$IMAGE:latest")
    [[ -n "$PR_NUMBER" ]] && LIGHT_TAGS+=(--tag "$REGISTRY/$IMAGE:pr-$PR_NUMBER")

    echo ""
    echo "→ docker buildx build --push  $IMAGE  (multi-arch)"
    docker buildx build \
      --platform "linux/amd64,linux/arm64" \
      --file "$DOCKERFILE" \
      "${LIGHT_TAGS[@]}" \
      --cache-from "type=registry,ref=$REGISTRY/$IMAGE:buildcache" \
      --cache-to   "type=registry,ref=$REGISTRY/$IMAGE:buildcache,mode=max" \
      --push \
      "$CONTEXT"
    echo "✓ Pushed: $TAG_SHA"
  done
fi

echo ""
echo "✓ push-current-arch: complete"
echo "  Heavy variants ($HOST_PLATFORM): ${HEAVY_VARIANTS[*]}"
[[ "$SKIP_LIGHT" -eq 0 ]] && echo "  Light variants (multi-arch): node, model-init, widgets"
echo ""
echo "  CI's verify-architectures gates merge. If a required image is missing,"
echo "  CI's error message tells you which machine/script to run."
