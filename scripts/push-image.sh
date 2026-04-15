#!/bin/bash
# push-image.sh — Build a continuum-core image locally and push to ghcr.io
#
# The premise: your laptop is ~10x faster than a GHA runner. Build on metal,
# push to the registry, let CI do what CI is good at (checking slices, not
# assembling images). A full CI rebuild of the CUDA variant is ~1h42m;
# native build on BigMama amd64 is ~20min. Same story for vulkan on Mac
# arm64 (native) vs GHA arm64 (qemu emulated from amd64 runner).
#
# Usage:
#   scripts/push-image.sh <variant> [platforms]
#
#   variant: one of `core`, `cuda`, `vulkan`
#   platforms: optional, defaults to the natural platform for the variant
#              (cuda=linux/amd64, vulkan=linux/arm64,linux/amd64, core=both)
#
# Examples:
#   # On BigMama WSL2, build CUDA for the 5090 hosts:
#   scripts/push-image.sh cuda
#
#   # On Mac M1, build Vulkan natively for Carl-on-Mac:
#   scripts/push-image.sh vulkan linux/arm64
#
#   # Full multi-arch (uses qemu for non-native, slow):
#   scripts/push-image.sh vulkan linux/amd64,linux/arm64
#
# Auth: you must be logged into ghcr.io with `docker login ghcr.io`
#       (use a PAT with `write:packages` scope).
#
# Tags applied:
#   ghcr.io/cambriantech/continuum-core-<variant>:<short-sha>
#   ghcr.io/cambriantech/continuum-core-<variant>:<branch>
#   ghcr.io/cambriantech/continuum-core-<variant>:latest  (only on main)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Parse args ──────────────────────────────────────────────────────
VARIANT="${1:-}"
PLATFORMS="${2:-}"

if [[ -z "$VARIANT" ]]; then
  cat >&2 <<EOF
Usage: $0 <variant> [platforms]

Variants:
  core    — CPU-only (Ares bootloader exception; not a Carl default)
  cuda    — Nvidia GPU via CUDA (BigMama, Nvidia Linux hosts)
  vulkan  — GPU via Vulkan (Mac Carl via Podman+krunkit+MoltenVK, also
            valid on Nvidia/AMD/Intel Linux hosts with libvulkan)

Platforms (optional): linux/amd64, linux/arm64, or comma-separated both.
  Default per variant:
    core    → linux/amd64,linux/arm64
    cuda    → linux/amd64   (CUDA is x86-only in practice)
    vulkan  → linux/amd64,linux/arm64
EOF
  exit 1
fi

case "$VARIANT" in
  core)   DOCKERFILE="docker/continuum-core.Dockerfile"; IMAGE="continuum-core"
          GPU_FEATURES="--no-default-features --features load-dynamic-ort"
          DEFAULT_PLATFORMS="linux/amd64,linux/arm64"
          ;;
  cuda)   DOCKERFILE="docker/continuum-core-cuda.Dockerfile"; IMAGE="continuum-core-cuda"
          GPU_FEATURES="--no-default-features --features load-dynamic-ort,cuda"
          DEFAULT_PLATFORMS="linux/amd64"
          ;;
  vulkan) DOCKERFILE="docker/continuum-core-vulkan.Dockerfile"; IMAGE="continuum-core-vulkan"
          GPU_FEATURES="--no-default-features --features load-dynamic-ort,vulkan"
          DEFAULT_PLATFORMS="linux/amd64,linux/arm64"
          ;;
  *) echo "ERROR: unknown variant '$VARIANT' (core|cuda|vulkan)" >&2; exit 1 ;;
esac

PLATFORMS="${PLATFORMS:-$DEFAULT_PLATFORMS}"
SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
BRANCH="$(git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD)"
# Sanitize branch name for docker tag (no slashes, etc).
BRANCH_TAG="$(echo "$BRANCH" | tr '/' '-')"

REGISTRY="ghcr.io/cambriantech"
TAG_SHA="$REGISTRY/$IMAGE:$SHA"
TAG_BRANCH="$REGISTRY/$IMAGE:$BRANCH_TAG"
TAGS=(--tag "$TAG_SHA" --tag "$TAG_BRANCH")

# Only push :latest if we're on main (mirrors CI behavior).
if [[ "$BRANCH" == "main" ]]; then
  TAG_LATEST="$REGISTRY/$IMAGE:latest"
  TAGS+=(--tag "$TAG_LATEST")
fi

# ── Pre-flight ──────────────────────────────────────────────────────
cd "$REPO_ROOT"

if [[ ! -f "$DOCKERFILE" ]]; then
  echo "ERROR: $DOCKERFILE not found" >&2
  exit 1
fi

if [[ ! -f "src/workers/vendor/llama.cpp/CMakeLists.txt" ]]; then
  echo "ERROR: vendor/llama.cpp submodule not initialized." >&2
  echo "       Run: git submodule update --init --recursive" >&2
  exit 1
fi

if ! docker info &>/dev/null; then
  echo "ERROR: docker daemon not reachable (start Docker Desktop / Rancher / podman machine)" >&2
  exit 1
fi

# buildx is required for multi-platform + push in one step
if ! docker buildx version &>/dev/null; then
  echo "ERROR: docker buildx not installed" >&2
  exit 1
fi

# Ensure we have a buildx builder that supports multi-platform
if ! docker buildx inspect continuum-builder &>/dev/null; then
  echo "→ Creating buildx builder 'continuum-builder'..."
  docker buildx create --name continuum-builder --use
else
  docker buildx use continuum-builder
fi

# ── Build + push ────────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  variant:    $VARIANT"
echo "  dockerfile: $DOCKERFILE"
echo "  platforms:  $PLATFORMS"
echo "  tags:"
for t in "${TAGS[@]}"; do
  [[ "$t" == "--tag" ]] && continue
  echo "    $t"
done
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

docker buildx build \
  --platform "$PLATFORMS" \
  --file "$DOCKERFILE" \
  --build-arg "GPU_FEATURES=$GPU_FEATURES" \
  "${TAGS[@]}" \
  --cache-from "type=registry,ref=$REGISTRY/$IMAGE:buildcache" \
  --cache-to   "type=registry,ref=$REGISTRY/$IMAGE:buildcache,mode=max" \
  --push \
  src/workers

echo ""
echo "✓ Pushed: $TAG_SHA"
echo "✓ Pushed: $TAG_BRANCH"
[[ "$BRANCH" == "main" ]] && echo "✓ Pushed: $TAG_LATEST"
echo ""
echo "To use on another machine:"
echo "  docker pull $TAG_SHA"
echo ""
