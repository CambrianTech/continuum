#!/bin/bash
# The cargo feature sets for the core's bins on one platform: ONE place, read by the
# on-node build (start-server.sh), by the deploy consumer judging a published core
# (continuum.rs local_core_features), by the Windows developer install (win-modules.ps1
# Mod-BuildCore) and by CI's published binaries (core-binaries.yml). Two copies of this
# choice would drift, and a published binary built with a different set than the node
# would build is a different program (card 30a8b3ac).
#
# A feature set is TWO choices, made by two different facts:
#   - the platform GPU set (metal,accelerate / cuda / vulkan / ...): what the HARDWARE
#     runs, detected here and in shared/cargo-features.sh;
#   - the NODE PROFILE: what this node HOSTS beyond the headless core — calls
#     (`livekit-webrtc`) and 3D avatars (`avatar-3d`). It is the operator's statement in
#     ~/.continuum/config.env, never a guess:
#         CONTINUUM_NODE_FEATURES=livekit-webrtc,avatar-3d
#     The crate's default is headless (`default = []`, Joel 2026-10-19: "headless with
#     desktop clients, so you can operate more easily in things like AWS"), so a node with
#     no profile builds headless and this file SAYS so on every build.
#
# Usage: source tools/scripts/lib/core-features.sh
#        select_core_features            # this machine
#        select_core_features "Darwin x86_64"   # a named platform, as `uname -sm` prints it
#        select_core_features "Windows-NVIDIA x86_64"   # CI's Windows build (no GPU to detect)
#        resolve_node_features           # this node's profile, canonical, or refuse
#        published_node_features "Darwin arm64"   # the profile CI publishes for a platform
# Sets CONTINUUM_FEATURES (core-server, continuum-mcp, forge-custodian),
# CONTINUUM_CLI_FEATURES (the `continuum` CLI) and CONTINUUM_HARDWARE_FEATURES (what a
# published core is judged against). Returns nonzero, naming the cause, on a profile it
# does not recognize: a typo must never build a node without the calls it asked for.
_core_features_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# The features a node profile may name, in the ONE order every string is written in (a
# published core is judged by string equality, so the node and CI must spell a set alike).
CONTINUUM_NODE_FEATURE_VOCABULARY="livekit-webrtc avatar-3d"

# The node profile CI publishes per platform: what that platform's fleet node hosts, so a
# deployed core keeps every capability its node had before `default = []` (#5180 plan line
# 15). Today's published sets, unchanged: the M5 and the Windows 5090 host calls and 3D
# avatars; the IntelMac hosts calls without Bevy (card 0bff1a0a). A platform with no
# published core prints nothing.
published_node_features() {
  case "$1" in
    "Darwin arm64"|"Windows-NVIDIA x86_64") printf '%s' "livekit-webrtc,avatar-3d" ;;
    "Darwin x86_64")                        printf '%s' "livekit-webrtc" ;;
  esac
}

# This node's profile: CONTINUUM_NODE_FEATURES from the environment when the caller set it
# (start-server.sh has sourced config.env; CI names the published profile), else from
# ~/.continuum/config.env, read by `source` exactly as start-server.sh reads it. Printed
# canonical (vocabulary order, comma-joined); empty = headless. Nonzero on a word outside
# the vocabulary, naming it.
resolve_node_features() {
  local raw=""
  if [ -n "${CONTINUUM_NODE_FEATURES+set}" ]; then
    raw="$CONTINUUM_NODE_FEATURES"
  elif [ -f "$HOME/.continuum/config.env" ]; then
    raw="$(set -a; . "$HOME/.continuum/config.env" >/dev/null 2>&1; printf '%s' "${CONTINUUM_NODE_FEATURES:-}")"
  fi
  local word known out=""
  for word in $(printf '%s' "$raw" | tr ',' ' '); do
    case " $CONTINUUM_NODE_FEATURE_VOCABULARY " in
      *" $word "*) ;;
      *)
        echo "✗ CONTINUUM_NODE_FEATURES names \`$word\`; a node profile is drawn from: $CONTINUUM_NODE_FEATURE_VOCABULARY" >&2
        return 1
        ;;
    esac
  done
  for known in $CONTINUUM_NODE_FEATURE_VOCABULARY; do
    case ",$(printf '%s' "$raw" | tr -d ' ')," in
      *",$known,"*) out="${out:+$out,}$known" ;;
    esac
  done
  printf '%s' "$out"
}

# "--features a,b" from comma lists (empty lists skipped); empty when there is nothing.
_core_feature_flags() {
  local list="" part
  for part in "$@"; do
    [ -n "$part" ] && list="${list:+$list,}$part"
  done
  [ -n "$list" ] && printf -- '--features %s' "$list"
  return 0
}

# The comma list inside a cargo-features.sh flag string ("--features cuda,directml").
_core_feature_list() {
  local word
  for word in $1; do
    case "$word" in --features) ;; *) printf '%s' "$word" ;; esac
  done
}

# Is every word of comma list $1 in comma list $2?
_core_features_subset() {
  local word
  for word in $(printf '%s' "$1" | tr ',' ' '); do
    case ",$2," in *",$word,"*) ;; *) return 1 ;; esac
  done
  return 0
}

# Mac Intel can't use Metal (task #131 — ggml_metal_device_init hangs on
# Intel + AMD discrete). Force mac-cpu-only on Intel Mac.
#
# CONTINUUM_CLI_FEATURES is the GPU-FREE set for the `continuum` CLI, which is a
# socket client and must never link a GPU runtime (see the CLI build in start-server.sh for why
# that made it unlaunchable on Windows). It is platform-shaped for the same reason
# the core's set is: an empty set is NOT GPU-free-and-buildable everywhere. On macOS
# the unconditional `llama` dependency fires
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
  local node published gpu hardware_gpu
  node="$(resolve_node_features)" || return 1
  published="$(published_node_features "$platform")"
  # CONTINUUM_HARDWARE_FEATURES: the core set this machine's HARDWARE runs, which a
  # published (prebuilt) core is judged against. Its GPU part equals the build's except on
  # a Windows NVIDIA box without MSVC on PATH: it cannot compile CUDA, but runs CI's CUDA
  # core (cargo-features.sh, CARGO_GPU_HARDWARE_FEATURES).
  case "$platform" in
    "Darwin x86_64")
      # Not `avatar-3d` in the published set (the Bevy 3D renderer): a CPU-only Intel Mac
      # doesn't render 3D avatars, and bevy was ~16% of the core binary (card 0bff1a0a).
      # Requests for a 3D face fail loudly naming the feature.
      gpu="llama/mac-cpu-only"
      hardware_gpu="$gpu"
      ;;
    "Darwin arm64")
      gpu="metal,accelerate"
      hardware_gpu="$gpu"
      ;;
    *)
      # Source the existing detector for Linux/Windows.
      source "$_core_features_dir/../shared/cargo-features.sh"
      gpu="$(_core_feature_list "$CARGO_GPU_FEATURES")"
      hardware_gpu="$(_core_feature_list "$CARGO_GPU_HARDWARE_FEATURES")"
      # CI publishes the Windows + NVIDIA build from a runner with no GPU and no device to
      # probe, so it names the flavor; the node's detector reaches the same constant.
      if [ "$platform" = "Windows-NVIDIA x86_64" ]; then
        gpu="$(_core_feature_list "$WINDOWS_NVIDIA_FEATURES")"
        hardware_gpu="$gpu"
      fi
      # A Windows node judges CI's Windows-NVIDIA core by the profile CI publishes for it.
      case "$platform" in MINGW*|MSYS*|CYGWIN*) published="$(published_node_features "Windows-NVIDIA x86_64")" ;; esac
      ;;
  esac
  CONTINUUM_FEATURES="$(_core_feature_flags "$gpu" "$node")"
  # A published core carries its platform's published profile. A node hosting a subset of
  # it (headless included) runs that core as is — compiled-in calls it does not host cost
  # it nothing — so it judges by the published string. A node hosting MORE than CI
  # publishes is judged by its own set: no published core matches it, and it builds its
  # own rather than run one that silently lacks what it hosts.
  if [ -n "$published" ] && _core_features_subset "$node" "$published"; then
    CONTINUUM_HARDWARE_FEATURES="$(_core_feature_flags "$hardware_gpu" "$published")"
  else
    CONTINUUM_HARDWARE_FEATURES="$(_core_feature_flags "$hardware_gpu" "$node")"
  fi
  # ONE library compile per deploy (card 9174fc83, measured on the IntelMac 2026-09-20:
  # 18 + 18 min of second lib builds; on the M5 ~10 min, 2026-09-13): a CLI feature set
  # that differs from the core's makes cargo compile continuum-core twice. The CLI carries
  # its own GPU-free set ONLY where the core's set links a GPU runtime the loader must
  # find before main() — cuda (cublas/cudart), rocm, vulkan (libvulkan): the measured
  # Windows failure (see the CLI notes at start-server.sh's build) and its Linux twins. It
  # is then headless too: the socket client neither renders nor joins calls. Everywhere
  # else (both Macs, a CPU-only box, DirectML-only Windows, where ort loads onnxruntime by
  # name at run time) the CLI shares the core's one library build.
  case ",$gpu," in
    *,cuda,*|*,rocm,*|*,vulkan,*) CONTINUUM_CLI_FEATURES="" ;;
    *)                            CONTINUUM_CLI_FEATURES="$CONTINUUM_FEATURES" ;;
  esac
  if [ -n "$node" ]; then
    echo "node profile: $node (CONTINUUM_NODE_FEATURES) — this build hosts it" >&2
  else
    echo "node profile: headless — CONTINUUM_NODE_FEATURES is not set (~/.continuum/config.env), so this build carries no calls (livekit-webrtc) and no 3D avatars (avatar-3d). A node that hosts them sets CONTINUUM_NODE_FEATURES=livekit-webrtc,avatar-3d" >&2
  fi
}
