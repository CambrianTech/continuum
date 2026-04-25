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
STARTUP_SHA_FULL="$(git rev-parse HEAD)"
SHA="$(git rev-parse --short "$STARTUP_SHA_FULL")"
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
BRANCH_TAG="$(echo "$BRANCH" | tr '/' '-')"
PR_NUMBER="${PR_NUMBER:-}"
if [[ -z "$PR_NUMBER" ]] && command -v gh >/dev/null 2>&1; then
  PR_NUMBER="$(gh pr list --head "$BRANCH" --json number --jq '.[0].number // empty' 2>/dev/null || true)"
fi

# ── Working-tree cleanliness guard ───────────────────────────────────
# git worktree add checks out the committed tree at $STARTUP_SHA_FULL, so
# ANY uncommitted modifications to tracked files would silently NOT make
# it into the build. Forbid the situation up front so the contributor sees
# the right error ("commit or stash") instead of "why isn't my fix in the
# image?" 30 minutes later.
if ! git diff --quiet HEAD -- 2>/dev/null; then
  echo "ERROR: Working tree has modified tracked files. Push would mix source states." >&2
  echo "       Commit or stash first:  git status" >&2
  exit 1
fi

# ── Frozen build context via git worktree (replaces TOCTOU guard) ────
# 2026-04-24: contributor pushed at SHA A, made follow-up commits during the
# 20-min image build, prepush hook's per-variant assert_sha_unchanged fired,
# killed the push partway through. Result: stale image at :A pushed for
# some variants, others unpushed, refs not pushed at all, contributor needs
# `git reset --hard A` (lossy) or rerun (race fires again on next commit).
#
# The fix is structural: pin the build to a checkout that CAN'T move. git
# worktree gives us exactly that — a separate working directory at a frozen
# commit, sharing the .git database (so creation is fast, ~5-10s + a file
# materialization pass). The main checkout stays free to receive new
# commits during the long docker build; this one doesn't see them.
#
# Submodules: `git worktree add` materializes superproject files only —
# submodule directories appear as empty placeholders. We `submodule update
# --init --recursive` inside the worktree so vendor/llama.cpp + vendor/
# whisper.cpp are populated for the cmake step.
#
# Cleanup: trap on EXIT removes the worktree (force-remove tolerates the
# dirty state docker leaves behind in target/). Layer cache lives in the
# registry, so removal doesn't lose any work.
WORKTREE_DIR="${WORKTREE_DIR:-/tmp/continuum-build-${STARTUP_SHA_FULL:0:12}}"

if [ -e "$WORKTREE_DIR" ]; then
  # Stale worktree from a previous run that crashed. Try the clean removal
  # first, fall back to rm -rf + worktree prune. Either way the path is gone
  # before we add a new one.
  echo "→ Cleaning stale worktree at $WORKTREE_DIR"
  git -C "$REPO_ROOT" worktree remove --force "$WORKTREE_DIR" 2>/dev/null || true
  rm -rf "$WORKTREE_DIR"
  git -C "$REPO_ROOT" worktree prune 2>/dev/null || true
fi

echo "→ Creating frozen worktree at $WORKTREE_DIR (pinned at $STARTUP_SHA_FULL)"
git -C "$REPO_ROOT" worktree add --detach "$WORKTREE_DIR" "$STARTUP_SHA_FULL" >/dev/null

# Capture the original $REPO_ROOT so the cleanup trap can find the .git
# database after we re-point $REPO_ROOT at the worktree below.
ORIGINAL_REPO_ROOT="$REPO_ROOT"

cleanup_worktree() {
  local rc=$?
  if [ -d "$WORKTREE_DIR" ]; then
    echo "→ Cleaning up worktree $WORKTREE_DIR"
    # -C "$ORIGINAL_REPO_ROOT" so the cleanup operates on the main .git db
    # regardless of cwd or any inherited GIT_DIR.
    git -C "$ORIGINAL_REPO_ROOT" worktree remove --force "$WORKTREE_DIR" 2>/dev/null \
      || rm -rf "$WORKTREE_DIR"
    git -C "$ORIGINAL_REPO_ROOT" worktree prune 2>/dev/null || true
  fi
  exit "$rc"
}
trap cleanup_worktree EXIT

# Drop the inherited GIT_DIR / GIT_WORK_TREE that the pre-push hook set up
# pointing at the main repo. Inside the worktree we want git to discover the
# correct context via parent-directory walk (worktree's .git is a file
# pointing back at the shared db). Without this, `git submodule update` runs
# against the main repo's GIT_DIR but cwd of the worktree, which trips
# "git-submodule cannot be used without a working tree" — the exact failure
# Joel hit on the first push attempt with this script.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_PREFIX

# Initialize submodules INSIDE the worktree (git worktree doesn't auto-init).
# Without this, vendor/llama.cpp/CMakeLists.txt is missing and the cmake
# build fails ~15 min in with the wrong error (the existing fast-fail check
# in continuum-core.Dockerfile catches it but only inside docker — better
# to fail at the host before we burn buildkit cycles).
echo "→ Initializing submodules in worktree (vendor/llama.cpp + vendor/whisper.cpp)"
( cd "$WORKTREE_DIR" && git submodule update --init --recursive --depth 1 ) >/dev/null

# All build steps from here run from the worktree, not $REPO_ROOT. The main
# checkout is now free to receive new commits during the build — they won't
# leak into the docker context. SCRIPT_DIR moves with us so the inner
# push-image.sh derives its own REPO_ROOT from $WORKTREE_DIR/scripts/.
REPO_ROOT="$WORKTREE_DIR"
SCRIPT_DIR="$WORKTREE_DIR/scripts"
cd "$WORKTREE_DIR"

# ── Stop in-flight stale builds (energy + correctness) ────────────────
# A push that fires while a previous push is still building wastes CPU
# (two concurrent builds compete for cores) AND ships the wrong bits if
# the OLDER build finishes second and its alias step overwrites the
# newer image. 2026-04: we observed buildkit at 2300% CPU + 10GB RAM
# from a stale build that started 30+ min earlier at an older SHA while
# new fixes had landed.
#
# Strategy: when a build is already running, restart the buildkit
# container before kicking off the new one. Layer cache is preserved
# (it lives in the registry via --cache-from/--cache-to, not inside the
# buildkit container) so the new build benefits from anything the
# old one already pushed to buildcache. Net effect: kill in-flight
# wasted work, keep the layer cache, build at the current SHA only.
#
# Skip if STOP_PRIOR=0 (e.g., parallel-test scenarios that genuinely
# want concurrent builds; default is to be conservative).
STOP_PRIOR="${STOP_PRIOR:-1}"
if [ "$STOP_PRIOR" = "1" ] && command -v docker >/dev/null 2>&1; then
  BUILDKIT_CONTAINER="$(docker ps --filter "name=buildx_buildkit_continuum-builder0" --format '{{.Names}}' 2>/dev/null | head -1)"
  if [ -n "$BUILDKIT_CONTAINER" ]; then
    # Check if there's actual build work running (rustc / cargo / sh -c) —
    # idle buildkit is fine to leave alone.
    INFLIGHT="$(docker exec "$BUILDKIT_CONTAINER" sh -c "pgrep -f 'rustc|cargo' | wc -l" 2>/dev/null || echo 0)"
    INFLIGHT="$(echo "$INFLIGHT" | tr -d ' ')"
    if [ "$INFLIGHT" -gt 0 ] 2>/dev/null; then
      echo "→ Stopping in-flight buildkit work ($INFLIGHT rustc/cargo procs from a previous push)..."
      docker restart "$BUILDKIT_CONTAINER" >/dev/null 2>&1 || true
      # Brief settle so the next buildx invocation doesn't race the
      # restarting container. Layer cache stays in the registry.
      sleep 2
      echo "  ✓ Cleared. Registry layer cache preserved — new build will reuse unchanged layers."
    fi
  fi
fi
# assert_sha_unchanged() is now a no-op: the worktree is pinned at
# $STARTUP_SHA_FULL and can't move, so HEAD movement in the main checkout
# (the original race) doesn't affect the build context. Kept as a stub so
# any future re-introduction of the check fails loudly rather than silently
# being undefined.
assert_sha_unchanged() {
  : # no-op — worktree-pinned build, see header
}

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
    assert_sha_unchanged
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
    assert_sha_unchanged
    IFS=':' read -r IMAGE DOCKERFILE CONTEXT <<< "$ENTRY"
    TAG_SHA="$REGISTRY/$IMAGE:$SHA"
    TAG_BRANCH="$REGISTRY/$IMAGE:$BRANCH_TAG"
    LIGHT_TAGS=(--tag "$TAG_SHA" --tag "$TAG_BRANCH")
    [[ "$BRANCH" == "main" ]] && LIGHT_TAGS+=(--tag "$REGISTRY/$IMAGE:latest")
    [[ -n "$PR_NUMBER" ]] && LIGHT_TAGS+=(--tag "$REGISTRY/$IMAGE:pr-$PR_NUMBER")

    echo ""
    echo "→ docker buildx build --push  $IMAGE  (multi-arch)"
    # --label org.opencontainers.image.revision parity with push-image.sh
    # heavy builds. Without this, light images (node/model-init/widgets)
    # ship tagged :<sha> but carry no `revision` label — the stale-image
    # gate in verify-image-revisions.sh then reports them as pre-gate
    # pushes and blocks merge. Caught empirically 2026-04-24 after the
    # paired amd64/arm64 rebuild at 0c6d62ad5: heavy variants passed the
    # gate, light variants failed "no revision label." Same $STARTUP_SHA_FULL
    # already captured at script start for the TOCTOU guard.
    docker buildx build \
      --platform "linux/amd64,linux/arm64" \
      --file "$DOCKERFILE" \
      "${LIGHT_TAGS[@]}" \
      --label "org.opencontainers.image.revision=$STARTUP_SHA_FULL" \
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
