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
  core           — CPU-only (Ares bootloader exception; not a Carl default)
  cuda           — Nvidia GPU via CUDA (BigMama, Nvidia Linux hosts)
  vulkan         — GPU via Vulkan (Mac Carl via Podman+krunkit+MoltenVK,
                   also valid on Nvidia/AMD/Intel Linux hosts with libvulkan)
  livekit-bridge — Rust WebRTC bridge to LiveKit SFU (separate process)

Platforms (optional): linux/amd64, linux/arm64, or comma-separated both.
  Default per variant:
    core           → linux/amd64,linux/arm64
    cuda           → linux/amd64   (CUDA is x86-only in practice)
    vulkan         → linux/amd64,linux/arm64
    livekit-bridge → linux/amd64,linux/arm64
EOF
  exit 1
fi

case "$VARIANT" in
  core)        DOCKERFILE="docker/continuum-core.Dockerfile"; IMAGE="continuum-core"
               GPU_FEATURES="--no-default-features --features load-dynamic-ort"
               DEFAULT_PLATFORMS="linux/amd64,linux/arm64"
               ;;
  cuda)        DOCKERFILE="docker/continuum-core-cuda.Dockerfile"; IMAGE="continuum-core-cuda"
               GPU_FEATURES="--no-default-features --features load-dynamic-ort,cuda"
               DEFAULT_PLATFORMS="linux/amd64"
               ;;
  vulkan)      DOCKERFILE="docker/continuum-core-vulkan.Dockerfile"; IMAGE="continuum-core-vulkan"
               GPU_FEATURES="--no-default-features --features load-dynamic-ort,vulkan"
               DEFAULT_PLATFORMS="linux/amd64,linux/arm64"
               ;;
  livekit-bridge)
               DOCKERFILE="docker/livekit-bridge.Dockerfile"; IMAGE="continuum-livekit-bridge"
               # WebRTC + LiveKit bridge — separate Rust binary in src/workers/.
               # Same workspace, different Cargo binary. Uses default features
               # (livekit-webrtc enabled) since this IS the livekit-webrtc consumer.
               GPU_FEATURES=""
               DEFAULT_PLATFORMS="linux/amd64,linux/arm64"
               ;;
  *) echo "ERROR: unknown variant '$VARIANT' (core|cuda|vulkan|livekit-bridge)" >&2; exit 1 ;;
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

# Add :pr-<N> tag if there's an open PR for this branch — keeps the
# `CONTINUUM_IMAGE_TAG=pr-<N> curl install.sh | bash` reviewer flow
# working when we build locally instead of via CI. Mirrors the CI
# `type=ref,event=pr,prefix=pr-` rule. PR_NUMBER env override exists
# for hosts where `gh` isn't available (e.g. SSH on BigMama).
PR_NUMBER="${PR_NUMBER:-}"
if [[ -z "$PR_NUMBER" ]] && command -v gh >/dev/null 2>&1; then
  PR_NUMBER="$(gh pr list --head "$BRANCH" --json number --jq '.[0].number // empty' 2>/dev/null || true)"
fi
if [[ -n "$PR_NUMBER" ]]; then
  TAG_PR="$REGISTRY/$IMAGE:pr-$PR_NUMBER"
  TAGS+=(--tag "$TAG_PR")
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

# ── Phase 0: native cargo test ──────────────────────────────────────
# Prove the Rust code is sound against the backend BEFORE we spin up a
# Docker build. Fails in seconds instead of minutes when the regression
# is in the Rust source. Runs only when the host can natively build the
# feature set — otherwise skipped (not faked) so Phase 1+2 remains the
# authoritative gate on skippable hosts.
HOST_OS="$(uname -s)"
HOST_ARCH="$(uname -m)"
NATIVE_FEATURE=""
case "$VARIANT:$HOST_OS" in
  cuda:Linux)
    # Needs nvcc (CUDA dev toolkit) + Nvidia driver. nvidia-smi alone is
    # the DRIVER — common state on WSL2 hosts where the GPU is passed
    # through but the dev toolkit was never installed. nvcc is what
    # actually compiles the Rust+CUDA build, so it's the real prereq.
    # Skip Phase 0 cleanly when only the driver is present rather than
    # failing late inside cargo with a confusing nvcc-not-found error.
    if command -v nvcc &>/dev/null; then
      NATIVE_FEATURE="cuda"
    elif command -v nvidia-smi &>/dev/null && nvidia-smi &>/dev/null; then
      echo "→ Phase 0 skipped: variant=cuda — nvidia-smi present but nvcc missing"
      echo "  (driver passthrough without CUDA dev toolkit; install cuda-toolkit-nvcc to enable native Phase 0)"
      echo "  Phase 1+ docker build inside the container has its own CUDA toolkit, so the image build itself is fine."
    else
      echo "→ Phase 0 skipped: variant=cuda but no working nvidia-smi or nvcc on host"
    fi
    ;;
  vulkan:Linux)
    # Needs libvulkan. Detect vulkaninfo or pkg-config.
    if command -v vulkaninfo &>/dev/null || pkg-config --exists vulkan 2>/dev/null; then
      NATIVE_FEATURE="vulkan"
    else
      echo "→ Phase 0 skipped: variant=vulkan but libvulkan not installed on host"
    fi
    ;;
  core:Darwin)
    # Mac + core: Metal is the native backend AND required by llama
    # crate's compile_error guard (commit 7f32bc04e) — without
    # --features metal, cargo test fails at compile time. The old
    # `core:*` branch below erroneously caught core:Darwin first and
    # left NATIVE_FEATURE empty → Phase 0 crashed with compile_error
    # instead of running tests. Explicit core:Darwin branch placed
    # before core:* so Mac gets the feature set it needs.
    # Phase 0 runs `cargo test -p llama`, so features must be llama-crate-
    # scoped (metal|cuda|vulkan). `accelerate` belongs to continuum-core
    # and is not a valid llama feature — passing it here fails with
    # "package llama does not contain this feature accelerate".
    NATIVE_FEATURE="metal"
    echo "→ Phase 0 using --features=metal on Mac (variant=core)"
    ;;
  core:*)
    # Non-Mac + core: Default features, no GPU required — always runnable.
    NATIVE_FEATURE=""  # Empty means default features (no --features flag)
    ;;
  *:Darwin)
    # Mac + any other variant (livekit-bridge, etc): still Metal for host-
    # side Phase 0 validation. Docker build inside container uses its own
    # feature set (cuda for continuum-core-cuda, vulkan for continuum-core-
    # vulkan — those don't build natively on Mac anyway). llama-crate-
    # scoped feature only (see core:Darwin note above).
    NATIVE_FEATURE="metal"
    echo "→ Phase 0 using --features=metal on Mac (variant=$VARIANT builds in container)"
    ;;
esac

if [[ -n "${NATIVE_FEATURE+x}" ]]; then
  echo ""
  echo "→ Phase 0: cargo test -p llama ${NATIVE_FEATURE:+--features=$NATIVE_FEATURE}"
  pushd "$REPO_ROOT/src/workers" >/dev/null
  if [[ -n "$NATIVE_FEATURE" ]]; then
    cargo test -p llama --features="$NATIVE_FEATURE" --release -- --test-threads=1
  else
    cargo test -p llama --release -- --test-threads=1
  fi
  TEST_RC=$?
  popd >/dev/null
  if [[ $TEST_RC -ne 0 ]]; then
    echo "" >&2
    echo "✗ Phase 0 (native cargo test) failed — NOT building docker image." >&2
    echo "  Rust code regression in llama crate. Fix locally, re-run." >&2
    exit 2
  fi
  echo "✓ Phase 0 passed"
fi

# ── Phase 1-4: docker build + slice + push ──────────────────────────
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

# Two-phase: build-and-load first (to the local daemon, single platform),
# run slice tests against the real binary, THEN build-and-push (all
# requested platforms). If the slice tests fail, NOTHING is pushed —
# we don't throw half-working images over the wall to CI.
LOCAL_PLATFORM="$(docker version --format '{{.Server.Os}}/{{.Server.Arch}}' 2>/dev/null || echo linux/amd64)"

# Capture the build-time HEAD SHA so the resulting image carries it as a
# label. Verify-architectures asserts this label matches the PR HEAD SHA;
# without it a stale-tagged image (alias of an older sha) would silently
# pass the gate. Issue #957/#959/#964 paired QA cycle proved we need this
# to detect "the tag exists but the binary is from before the fix landed."
BUILD_SHA="$(git rev-parse HEAD)"

echo "→ Phase 1: local build + slice test on $LOCAL_PLATFORM"
docker buildx build \
  --platform "$LOCAL_PLATFORM" \
  --file "$DOCKERFILE" \
  --build-arg "GPU_FEATURES=$GPU_FEATURES" \
  --build-arg "GIT_SHA=$BUILD_SHA" \
  --build-context "shared-generated=src/shared/generated" \
  --tag "$TAG_SHA" \
  --label "org.opencontainers.image.revision=$BUILD_SHA" \
  --cache-from "type=registry,ref=$REGISTRY/$IMAGE:buildcache" \
  --load \
  src/workers

echo ""
echo "→ Phase 2: slice tests"
if ! "$SCRIPT_DIR/test-slices.sh" "$VARIANT" "$TAG_SHA"; then
  echo ""
  echo "✗ Slice tests failed — NOT pushing to registry." >&2
  echo "  Fix the issue, re-run this script." >&2
  exit 2
fi

echo ""
echo "→ Phase 3: multi-platform build + push ($PLATFORMS)"
docker buildx build \
  --platform "$PLATFORMS" \
  --file "$DOCKERFILE" \
  --build-arg "GPU_FEATURES=$GPU_FEATURES" \
  --build-arg "GIT_SHA=$BUILD_SHA" \
  --build-context "shared-generated=src/shared/generated" \
  "${TAGS[@]}" \
  --label "org.opencontainers.image.revision=$BUILD_SHA" \
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
