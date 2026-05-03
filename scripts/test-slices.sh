#!/bin/bash
# test-slices.sh — Run slice tests against a continuum-core image
#
# Slices are small, fast, targeted behavior probes. They prove the assembled
# image ACTUALLY WORKS for its purpose, not just that `cargo build` exited
# zero. Run these on the dev machine BEFORE pushing so you catch issues
# before throwing them over the wall to CI.
#
# Each slice:
#   - Boots the image in its expected runtime config
#   - Exercises one specific capability
#   - Asserts on the observable output (sockets, logs, device enumeration)
#   - Exits non-zero on failure with a specific message
#
# Slices per variant:
#   core           — boot + socket + no-panic
#   cuda           — above + nvidia-smi visible + CUDA runtime linked
#   vulkan         — above + Vulkan ICD enumerates a device (via llvmpipe
#                    fallback on non-GPU hosts; via venus on krunkit; via
#                    venus/radv/anv on real Linux GPU hosts)
#   livekit-bridge — image-available + boot (no socket; this service exposes
#                    HTTP not the continuum-core IPC socket) + no-panic
#
# Usage:
#   scripts/test-slices.sh <variant> [image-tag]
#
#   image-tag defaults to ghcr.io/cambriantech/continuum-core-<variant>:<sha>
#   (or ghcr.io/cambriantech/continuum-livekit-bridge:<sha> for that variant)
#   where <sha> is the current git HEAD (7-char short).
#
# Exit codes:
#   0 = all slices pass
#   1 = usage / pre-flight error
#   2 = a slice failed (specific slice named in stderr)

set -uo pipefail  # NOT -e — we catch slice failures and report them

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

VARIANT="${1:-}"
if [[ -z "$VARIANT" ]]; then
  cat >&2 <<EOF
Usage: $0 <variant> [image-tag]
Variants: core | cuda | vulkan | livekit-bridge
EOF
  exit 1
fi

case "$VARIANT" in
  core|cuda|vulkan|livekit-bridge) ;;
  *) echo "ERROR: unknown variant '$VARIANT'" >&2; exit 1 ;;
esac

SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
case "$VARIANT" in
  livekit-bridge)
    DEFAULT_IMAGE="ghcr.io/cambriantech/continuum-livekit-bridge:$SHA"
    ;;
  *)
    DEFAULT_IMAGE="ghcr.io/cambriantech/continuum-core-$VARIANT:$SHA"
    ;;
esac
IMAGE_TAG="${2:-$DEFAULT_IMAGE}"

if ! command -v docker &>/dev/null; then
  echo "ERROR: docker CLI not found — can't run slice tests" >&2
  exit 1
fi

if ! docker info &>/dev/null; then
  echo "ERROR: docker daemon not reachable — can't run slice tests" >&2
  exit 1
fi

# Variant-specific docker run flags.
RUN_FLAGS=(--rm -d --name "continuum-slice-$VARIANT-$$")
case "$VARIANT" in
  cuda)
    # Requires NVIDIA Container Toolkit on the host. If absent, cuda slice
    # isn't actually testable here — document-and-skip rather than false-pass.
    if ! docker info 2>/dev/null | grep -qi "nvidia"; then
      echo "WARN: host has no NVIDIA Container Runtime; cuda slice tests cannot assert GPU visibility." >&2
      echo "      Run on a CUDA-capable host (BigMama) for real validation." >&2
    fi
    RUN_FLAGS+=(--gpus all)
    ;;
  vulkan)
    # /dev/dri is how krunkit exposes the virtio-GPU. On a Linux host with
    # a real GPU, same path. On a non-GPU host we still want to test — the
    # image should fall back to the llvmpipe ICD (software Vulkan) and
    # successfully enumerate a device, proving the binary and ICD loader
    # are wired correctly even if performance would be CPU-only.
    if [[ -e /dev/dri ]]; then
      RUN_FLAGS+=(--device /dev/dri:/dev/dri)
    fi
    ;;
esac

# ── Helpers ─────────────────────────────────────────────────────────
FAILS=()

pass() { echo "  ✓ $1"; }
fail() {
  echo "  ✗ $1: $2" >&2
  FAILS+=("$1")
}

cleanup() {
  if [[ -n "${CID:-}" ]]; then
    docker kill "$CID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

# ── Slice 1: image exists locally or pullable ──────────────────────
echo ""
echo "━━━ $VARIANT slice tests against $IMAGE_TAG ━━━"
echo ""

if ! docker image inspect "$IMAGE_TAG" &>/dev/null; then
  echo "→ Image not local, attempting pull..."
  if ! docker pull "$IMAGE_TAG" 2>&1 | tail -5; then
    fail "image-available" "docker pull $IMAGE_TAG failed"
    exit 2
  fi
fi
pass "image-available ($IMAGE_TAG)"

# ── Slice 2: boot ───────────────────────────────────────────────────
# Start the container and verify the IPC socket appears within a timeout.
# If this fails the binary is panicking or entrypoint is wrong.
BOOT_OK=false
CID="$(docker run "${RUN_FLAGS[@]}" "$IMAGE_TAG" 2>/dev/null || true)"
if [[ -z "$CID" ]]; then
  fail "boot" "docker run exited immediately"
  echo "  docker logs: $(docker logs "continuum-slice-$VARIANT-$$" 2>&1 | tail -10)" >&2
  exit 2
fi

# livekit-bridge doesn't expose the continuum-core IPC socket (it's an
# HTTP service), so socket-presence isn't a meaningful health signal.
# All we need is "container stayed up for 5s without crashing."
if [[ "$VARIANT" == "livekit-bridge" ]]; then
  sleep 5
  if docker inspect -f '{{.State.Running}}' "$CID" 2>/dev/null | grep -q true; then
    pass "boot (container running after 5s)"
    BOOT_OK=true
  else
    fail "boot" "container exited within 5s"
    echo "  docker logs:" >&2
    docker logs "$CID" 2>&1 | tail -20 | sed 's/^/    /' >&2
  fi
else
  # Wait up to 30s for the socket to appear. The healthcheck is identical.
  SOCKET_FOUND=false
  for _ in $(seq 1 30); do
    if docker exec "$CID" test -S /root/.continuum/sockets/continuum-core.sock 2>/dev/null; then
      SOCKET_FOUND=true
      break
    fi
    sleep 1
  done
  if $SOCKET_FOUND; then
    pass "boot (socket appeared within 30s)"
    BOOT_OK=true
  else
    fail "boot" "socket /root/.continuum/sockets/continuum-core.sock never appeared"
    echo "  docker logs:" >&2
    docker logs "$CID" 2>&1 | tail -20 | sed 's/^/    /' >&2
  fi
fi

# ── Slice 3: no panic ──────────────────────────────────────────────
# Even if the socket appeared, Rust can panic in a background task and
# still keep the process alive. Grep logs for panic signatures.
LOG_OUTPUT=$(docker logs "$CID" 2>&1)
if echo "$LOG_OUTPUT" | grep -iqE "panicked at|fatal runtime error|segmentation fault"; then
  fail "no-panic" "panic/fatal found in logs"
  echo "$LOG_OUTPUT" | grep -iE "panicked at|fatal|segv" | head -5 | sed 's/^/    /' >&2
else
  pass "no-panic"
fi

# ── Slice 4 (variant-specific): device visibility ──────────────────
if ! $BOOT_OK; then
  echo "  - runtime probes skipped: boot did not reach the expected ready state" >&2
else
  case "$VARIANT" in
    cuda)
      # nvidia-smi should list at least one device with any VRAM at all.
      if docker exec "$CID" nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null | grep -q .; then
        pass "cuda-device-visible"
      else
        fail "cuda-device-visible" "nvidia-smi produced no GPU rows (host NVIDIA runtime missing?)"
      fi
      # Check the binary was built with CUDA linkage — ldd should show libcudart.
      if docker exec "$CID" sh -c 'ldd $(which continuum-core-server) 2>/dev/null | grep -qE "libcudart|libcuda\.so"'; then
        pass "cuda-runtime-linked"
      else
        fail "cuda-runtime-linked" "continuum-core-server does not link libcudart — feature flag didn't propagate?"
      fi
      ;;
    vulkan)
      # vulkan-tools in the runtime image ships vulkaninfo. Expect at least one
      # device, even if it's llvmpipe (software). A device count of 0 means the
      # ICD loader couldn't find ANY driver — the image is broken.
      VKINFO=$(docker exec "$CID" vulkaninfo --summary 2>&1 || true)
      if echo "$VKINFO" | grep -qE "deviceName|deviceType"; then
        DEVNAME=$(echo "$VKINFO" | grep -E "deviceName" | head -1 | sed 's/.*= *//')
        pass "vulkan-device-visible ($DEVNAME)"
      else
        fail "vulkan-device-visible" "vulkaninfo enumerated no devices — ICD loader can't find a driver"
        echo "  vulkaninfo output: $(echo "$VKINFO" | head -10)" >&2
      fi
      # Check binary is linked against libvulkan.
      if docker exec "$CID" sh -c 'ldd $(which continuum-core-server) 2>/dev/null | grep -q libvulkan'; then
        pass "vulkan-runtime-linked"
      else
        fail "vulkan-runtime-linked" "continuum-core-server does not link libvulkan — feature flag didn't propagate?"
      fi
      ;;
    core)
      # CPU-only variant — just sanity that OpenMP runtime is present
      # (ggml-cpu uses it).
      if docker exec "$CID" sh -c 'ldconfig -p 2>/dev/null | grep -q libgomp'; then
        pass "openmp-runtime-present"
      else
        fail "openmp-runtime-present" "libgomp runtime package is missing from the image"
      fi
      if docker exec "$CID" sh -c 'ldd $(which continuum-core-server) 2>/dev/null | grep -q libgomp'; then
        pass "openmp-linked"
      else
        fail "openmp-linked" "continuum-core-server is not dynamically linked to libgomp"
      fi
      ;;
  esac
fi

# ── Summary ─────────────────────────────────────────────────────────
echo ""
if [[ ${#FAILS[@]} -eq 0 ]]; then
  echo "━━━ $VARIANT: ALL SLICES PASS ━━━"
  exit 0
else
  echo "━━━ $VARIANT: ${#FAILS[@]} SLICE(S) FAILED ━━━" >&2
  for f in "${FAILS[@]}"; do
    echo "  - $f" >&2
  done
  exit 2
fi
