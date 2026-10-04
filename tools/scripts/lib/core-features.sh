#!/bin/bash
# The cargo feature sets for the core's bins on one platform: ONE place, read by the
# on-node build (start-server.sh) and by CI's published binaries (core-binaries.yml).
# Two copies of this choice would drift, and a published binary built with a different
# set than the node would build is a different program (card 30a8b3ac).
#
# Usage: source tools/scripts/lib/core-features.sh
#        select_core_features            # this machine
#        select_core_features "Darwin x86_64"   # a named platform, as `uname -sm` prints it
# Sets CONTINUUM_FEATURES (core-server, continuum-mcp, forge-custodian) and
# CONTINUUM_CLI_FEATURES (the `continuum` CLI).

_core_features_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Mac Intel can't use Metal (task #131 — ggml_metal_device_init hangs on
# Intel + AMD discrete). Force mac-cpu-only on Intel Mac.
#
# CONTINUUM_CLI_FEATURES is the GPU-FREE set for the `continuum` CLI, which is a
# socket client and must never link a GPU runtime (see the CLI build in start-server.sh for why
# that made it unlaunchable on Windows). It is platform-shaped for the same reason
# the core's set is: a bare `--no-default-features` is NOT GPU-free-and-buildable
# everywhere. On macOS the unconditional `llama` dependency fires
#
#   compile_error!("llama crate built on macOS WITHOUT `--features metal`")
#
# so the plain flag has NEVER produced a CLI on a Mac — every `npm start` since it
# landed has hit the loud "⚠ GPU-free continuum build failed — retrying with the
# full feature set … Please report this" fallback, and shipped a GPU-linked CLI
# while reporting an anomaly nobody reported. `llama/mac-cpu-only` is that guard's
# OWN declared opt-in for a deliberately CPU-only build, which is exactly what a
# socket client wants.
select_core_features() {
  local platform="${1:-$(uname -sm)}"
  case "$platform" in
    "Darwin x86_64")
      # `--no-default-features` also drops `avatar-3d` (the Bevy 3D renderer): a CPU-only
      # Intel Mac doesn't render 3D avatars, and bevy was ~16% of the core binary (card
      # 0bff1a0a). Requests for a 3D face fail loudly naming the feature.
      CONTINUUM_FEATURES="--no-default-features --features livekit-webrtc,llama/mac-cpu-only"
      # ONE library compile per deploy — the arm64 rule below, which this arm never got
      # (card 7d1b3660). With the CLI lacking `livekit-webrtc`, cargo's per-invocation
      # feature unification recompiled the WHOLE continuum-core lib for the CLI and then
      # again for the next bin: measured on the IntelMac deploy of fd1960e52 (2026-09-20),
      # the post-stop pass spent 18 + 18 min on two lib rebuilds after the warm pass had
      # already built every bin — the core was DOWN 41 minutes. The GPU-free reason for a
      # smaller CLI set (a box without a CUDA runtime) does not apply to a Mac; the CLI is
      # still CPU-only through `llama/mac-cpu-only`, the same as the core here.
      CONTINUUM_CLI_FEATURES="$CONTINUUM_FEATURES"
      ;;
    "Darwin arm64")
      CONTINUUM_FEATURES="--features metal,accelerate"
      # ONE library compile per deploy (2026-09-13): a CLI feature set that differs from the
      # core's makes cargo compile continuum-core TWICE per deploy (measured: the "pure
      # relaunch" spent ~10 min in a second full lib build). On Apple silicon the featured
      # build links Metal, which every Mac has — the GPU-free reason (a box without a CUDA
      # runtime) does not apply here. Same features → the CLI shares the core's lib.
      CONTINUUM_CLI_FEATURES="$CONTINUUM_FEATURES"
      ;;
    *)
      # Source the existing detector for Linux/Windows.
      source "$_core_features_dir/../shared/cargo-features.sh"
      CONTINUUM_FEATURES="$CARGO_GPU_FEATURES"
      # ONE library compile per deploy here too (card 9174fc83). The CLI carries its
      # own GPU-free set ONLY where the core's set links a GPU runtime the loader must
      # find before main() — cuda (cublas/cudart), rocm, vulkan (libvulkan): the
      # measured Windows failure (see the CLI notes at start-server.sh's build) and its Linux twins.
      # A CPU-only box (empty set → the crate defaults) and a DirectML-only Windows
      # box (ort loads onnxruntime by name at run time; nothing binds at load) get the
      # same set as the core, so the CLI shares the core's one library build. Before
      # this every Linux/Windows deploy paid a second full lib compile for a CLI whose
      # only difference was dropping `livekit-webrtc` — no launchability gained.
      case " $CONTINUUM_FEATURES " in
        # (`--no-default-features` also keeps `avatar-3d`/bevy out of the socket-client
        # CLI — ~24% of its binary, never used by it.)
        *cuda*|*rocm*|*vulkan*) CONTINUUM_CLI_FEATURES="--no-default-features" ;;
        *)                      CONTINUUM_CLI_FEATURES="$CONTINUUM_FEATURES" ;;
      esac
      ;;
  esac
}
