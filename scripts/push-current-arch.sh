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
#   Mac M-series (arm64)  → linux/arm64 slices of core + vulkan
#   Linux amd64           → linux/amd64 slices of core + vulkan
#   Linux amd64 + Nvidia  → + cuda variant (linux/amd64 only)
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
    # Linux VM. Vulkan is the Carl-on-Mac backend. Core is the CPU-only
    # baseline. CUDA requires Nvidia hardware — skipped on Mac.
    HOST_PLATFORM="linux/arm64"
    DEFAULT_VARIANTS=("vulkan" "core")
    ;;
  Linux/x86_64)
    # Linux amd64: native platform. Core + vulkan always; CUDA only when
    # Nvidia driver is present (nvidia-smi reports a GPU). nvcc isn't
    # required here — push-image.sh's Phase 0 handles its own detection.
    HOST_PLATFORM="linux/amd64"
    DEFAULT_VARIANTS=("core" "vulkan")
    if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi >/dev/null 2>&1; then
      DEFAULT_VARIANTS+=("cuda")
    fi
    ;;
  Linux/aarch64 | Linux/arm64)
    # Linux arm64 (e.g. a Raspberry Pi, Nvidia Jetson, or ARM cloud host).
    # Native linux/arm64 slices of core + vulkan.
    HOST_PLATFORM="linux/arm64"
    DEFAULT_VARIANTS=("core" "vulkan")
    ;;
  *)
    echo "ERROR: push-current-arch.sh — unsupported host $OS/$ARCH" >&2
    echo "       Supported: Darwin/arm64, Linux/x86_64, Linux/aarch64" >&2
    exit 1
    ;;
esac

# VARIANT env var lets a caller override the default set (useful for
# iterating on one variant without the full ~20+ min for all three).
if [[ -n "${VARIANT:-}" ]]; then
  VARIANTS=("$VARIANT")
else
  VARIANTS=("${DEFAULT_VARIANTS[@]}")
fi

cd "$REPO_ROOT"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  push-current-arch: $OS/$ARCH → $HOST_PLATFORM"
echo "  variants: ${VARIANTS[*]}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Phase 0 opt-out (cargo test gate inside push-image.sh). Propagated via
# a simple wrapper — push-image.sh doesn't read this env var itself but
# editing it to would be a bigger change than we want here.
export SKIP_PHASE_0="${SKIP_PHASE_0:-0}"

for V in "${VARIANTS[@]}"; do
  case "$V" in
    cuda)
      # CUDA variant is always linux/amd64. If HOST_PLATFORM is arm64,
      # this machine can't build cuda natively — skip with a note.
      if [[ "$HOST_PLATFORM" != "linux/amd64" ]]; then
        echo "→ Skipping cuda (requires linux/amd64 host; this is $HOST_PLATFORM)"
        continue
      fi
      echo "→ scripts/push-image.sh cuda  (linux/amd64 default)"
      "$SCRIPT_DIR/push-image.sh" cuda
      ;;
    core|vulkan)
      echo "→ scripts/push-image.sh $V $HOST_PLATFORM"
      "$SCRIPT_DIR/push-image.sh" "$V" "$HOST_PLATFORM"
      ;;
    *)
      echo "WARN: unknown variant '$V' — skipped" >&2
      ;;
  esac
done

echo ""
echo "✓ push-current-arch: done — pushed ${VARIANTS[*]} for $HOST_PLATFORM"
echo "  CI will verify coverage across both arches at merge time."
echo "  If the OTHER arch is missing, a dev on that machine runs the same script."
