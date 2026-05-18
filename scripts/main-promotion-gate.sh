#!/usr/bin/env bash
# main-promotion-gate.sh — per-host release receipt for canary -> main.
#
# Canary iteration should stay fast. Main promotion is where we require the
# full Carl/Docker/GPU matrix. Each capable machine runs this same script and
# leaves a receipt under .continuum/release-gate/receipts/.
#
# Usage:
#   scripts/main-promotion-gate.sh
#   scripts/main-promotion-gate.sh --check-receipts
#   CONTINUUM_RELEASE_PUSH_IMAGES=1 scripts/main-promotion-gate.sh
#
# Important env:
#   EXPECTED_SHA                  commit being promoted; defaults to HEAD
#   CONTINUUM_IMAGE_TAG           image tag for heartbeat/install gates
#   CONTINUUM_RELEASE_PUSH_IMAGES 1/true to build+push this host's slices
#   CONTINUUM_GATE_RUN_HEARTBEAT  1/true to run scripts/test-heartbeat.sh
#   CONTINUUM_GATE_RUN_INSTALL    1/true to run scripts/ci/install-and-run-gate.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

MODE="${1:-run}"
EXPECTED_SHA="${EXPECTED_SHA:-$(git rev-parse HEAD)}"
SHORT_SHA="${EXPECTED_SHA:0:7}"
IMAGE_TAG="${CONTINUUM_IMAGE_TAG:-$SHORT_SHA}"
PUSH_IMAGES="${CONTINUUM_RELEASE_PUSH_IMAGES:-0}"
RUN_HEARTBEAT="${CONTINUUM_GATE_RUN_HEARTBEAT:-0}"
RUN_INSTALL="${CONTINUUM_GATE_RUN_INSTALL:-0}"
RECEIPT_DIR="${CONTINUUM_GATE_RECEIPT_DIR:-$REPO_ROOT/.continuum/release-gate/receipts}"
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
HOSTNAME_VALUE="$(hostname 2>/dev/null || echo unknown-host)"
OS="$(uname -s)"
ARCH="$(uname -m)"
STATUS="pass"
FAILURES=()
NOTES=()
COMMANDS=()

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

json_array() {
  local first=1 item
  printf '['
  for item in "$@"; do
    if [ "$first" -eq 0 ]; then
      printf ','
    fi
    first=0
    printf '"%s"' "$(json_escape "$item")"
  done
  printf ']'
}

note() {
  NOTES+=("$1")
  echo "  - $1"
}

fail_gate() {
  STATUS="fail"
  FAILURES+=("$1")
  echo "  ✗ $1" >&2
}

run_gate_cmd() {
  local label="$1"
  shift
  COMMANDS+=("$label: $*")
  echo "→ $label"
  if "$@"; then
    echo "  ✓ $label"
  else
    fail_gate "$label"
  fi
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail_gate "missing command: $1"
  fi
}

is_true() {
  case "$1" in
    1|true|TRUE|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

receipt_value() {
  local file="$1"
  local key="$2"
  sed -n "s/.*\"$key\": \"\\([^\"]*\\)\".*/\\1/p" "$file" | head -1
}

check_receipts() {
  local missing=()
  local role receipt_status
  local matched

  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  main-promotion-gate receipt check"
  echo "  sha:      $EXPECTED_SHA"
  echo "  receipts: $RECEIPT_DIR"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  if [ ! -d "$RECEIPT_DIR" ]; then
    echo "✗ receipt directory missing: $RECEIPT_DIR" >&2
    exit 2
  fi

  for role in "${REQUIRED_RECEIPTS[@]}"; do
    matched=0
    while IFS= read -r receipt; do
      [ -f "$receipt" ] || continue
      if [ "$(receipt_value "$receipt" role)" = "$role" ] \
        && [ "$(receipt_value "$receipt" expected_sha)" = "$EXPECTED_SHA" ]; then
        matched=1
        receipt_status="$(receipt_value "$receipt" status)"
        if [ "$receipt_status" = "pass" ]; then
          echo "  ✓ $role: $receipt"
        else
          echo "  ✗ $role receipt failed: $receipt" >&2
          missing+=("$role failed")
        fi
        break
      fi
    done < <(find "$RECEIPT_DIR" -type f -name '*.json' 2>/dev/null | sort)

    if [ "$matched" -eq 0 ]; then
      echo "  ✗ missing receipt: $role" >&2
      missing+=("$role missing")
    fi
  done

  if [ "${#missing[@]}" -eq 0 ]; then
    echo "✓ all required main-promotion receipts present for $EXPECTED_SHA"
    exit 0
  fi

  echo "" >&2
  echo "Missing or failed required receipts:" >&2
  printf '  - %s\n' "${missing[@]}" >&2
  exit 2
}

GPU_CLASS="none"
HOST_ROLE="unsupported"
REQUIRED_RECEIPTS=(
  "darwin-arm64-metal"
  "linux-amd64-cuda"
  "linux-amd64-vulkan"
)

case "$MODE" in
  run) ;;
  --check-receipts|check-receipts) check_receipts ;;
  *)
    echo "Usage: $0 [--check-receipts]" >&2
    exit 1
    ;;
esac

if [ "$OS" = "Darwin" ] && [ "$ARCH" = "arm64" ]; then
  HOST_ROLE="darwin-arm64-metal"
  GPU_CLASS="metal"
elif [ "$OS" = "Linux" ] && [ "$ARCH" = "x86_64" ]; then
  HOST_ROLE="linux-amd64"
  if grep -qi microsoft /proc/version 2>/dev/null; then
    note "WSL2 host detected; receipt still counts as linux/amd64 for the release matrix."
  fi

  if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi >/dev/null 2>&1; then
    HOST_ROLE="$HOST_ROLE-cuda"
    GPU_CLASS="cuda"
  elif [ -e /dev/dri ]; then
    HOST_ROLE="$HOST_ROLE-vulkan"
    GPU_CLASS="vulkan"
  else
    HOST_ROLE="$HOST_ROLE-no-gpu"
    GPU_CLASS="none"
  fi
elif [ "$OS" = "Linux" ] && { [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; }; then
  HOST_ROLE="linux-arm64-core"
  GPU_CLASS="native-arm64"
fi

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  main-promotion-gate"
echo "  host:       $HOSTNAME_VALUE"
echo "  role:       $HOST_ROLE"
echo "  os/arch:    $OS/$ARCH"
echo "  gpu:        $GPU_CLASS"
echo "  sha:        $EXPECTED_SHA"
echo "  image tag:  $IMAGE_TAG"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

require_cmd git
require_cmd bash

if [ "$EXPECTED_SHA" != "$(git rev-parse HEAD)" ]; then
  note "EXPECTED_SHA differs from checkout HEAD; build scripts will pin to EXPECTED_SHA where supported."
fi

case "$HOST_ROLE" in
  darwin-arm64-metal)
    require_cmd cargo
    require_cmd docker
    note "Mac receipt proves native Rust/Metal support and arm64 Docker slices; CUDA/Vulkan receipts must come from Linux/WSL2 GPU hosts."
    ;;
  *cuda)
    require_cmd docker
    require_cmd nvidia-smi
    if ! docker info 2>/dev/null | grep -qi nvidia; then
      fail_gate "docker NVIDIA runtime not visible"
    fi
    ;;
  *vulkan)
    require_cmd docker
    if [ ! -e /dev/dri ]; then
      fail_gate "/dev/dri missing for Vulkan GPU receipt"
    fi
    if command -v vulkaninfo >/dev/null 2>&1; then
      if vulkaninfo --summary 2>/dev/null | grep -qi llvmpipe; then
        fail_gate "vulkaninfo reports llvmpipe; hardware Vulkan receipt required"
      fi
    else
      note "vulkaninfo not installed; Docker slice test must prove Vulkan device visibility."
    fi
    ;;
  linux-arm64-core)
    require_cmd docker
    note "Linux arm64 receipt covers core/livekit arm64 only; not a CUDA/Vulkan substitute."
    ;;
  *)
    fail_gate "unsupported or no-GPU host role for main promotion: $HOST_ROLE"
    ;;
esac

if is_true "$PUSH_IMAGES"; then
  run_gate_cmd "push native image slices" env EXPECTED_SHA="$EXPECTED_SHA" scripts/push-current-arch.sh
else
  note "image push skipped; set CONTINUUM_RELEASE_PUSH_IMAGES=1 to build+push this host's native slices."
fi

if is_true "$RUN_HEARTBEAT"; then
  run_gate_cmd "heartbeat" scripts/test-heartbeat.sh "$IMAGE_TAG"
else
  note "heartbeat skipped; set CONTINUUM_GATE_RUN_HEARTBEAT=1 to run stack/persona heartbeat."
fi

if is_true "$RUN_INSTALL"; then
  run_gate_cmd "Carl install gate" env CONTINUUM_IMAGE_TAG="$IMAGE_TAG" scripts/ci/install-and-run-gate.sh
else
  note "Carl install gate skipped; set CONTINUUM_GATE_RUN_INSTALL=1 to run install-and-run gate."
fi

mkdir -p "$RECEIPT_DIR"
RECEIPT="$RECEIPT_DIR/${HOST_ROLE}-${HOSTNAME_VALUE}-${SHORT_SHA}-$(date -u +%Y%m%dT%H%M%SZ).json"
ENDED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
REQUIRED_RECEIPTS_JSON="$(json_array "${REQUIRED_RECEIPTS[@]}")"
if [ "${#COMMANDS[@]}" -eq 0 ]; then
  COMMANDS_JSON="[]"
else
  COMMANDS_JSON="$(json_array "${COMMANDS[@]}")"
fi
if [ "${#NOTES[@]}" -eq 0 ]; then
  NOTES_JSON="[]"
else
  NOTES_JSON="$(json_array "${NOTES[@]}")"
fi
if [ "${#FAILURES[@]}" -eq 0 ]; then
  FAILURES_JSON="[]"
else
  FAILURES_JSON="$(json_array "${FAILURES[@]}")"
fi

cat >"$RECEIPT" <<EOF
{
  "schema": "continuum.main-promotion-gate.v1",
  "status": "$(json_escape "$STATUS")",
  "host": "$(json_escape "$HOSTNAME_VALUE")",
  "role": "$(json_escape "$HOST_ROLE")",
  "os": "$(json_escape "$OS")",
  "arch": "$(json_escape "$ARCH")",
  "gpu_class": "$(json_escape "$GPU_CLASS")",
  "expected_sha": "$(json_escape "$EXPECTED_SHA")",
  "image_tag": "$(json_escape "$IMAGE_TAG")",
  "started_at": "$(json_escape "$STARTED_AT")",
  "ended_at": "$(json_escape "$ENDED_AT")",
  "required_receipts": $REQUIRED_RECEIPTS_JSON,
  "commands": $COMMANDS_JSON,
  "notes": $NOTES_JSON,
  "failures": $FAILURES_JSON
}
EOF

echo ""
echo "Receipt: $RECEIPT"

if [ "$STATUS" = "pass" ]; then
  echo "✓ main-promotion-gate local receipt complete"
  exit 0
fi

echo "✗ main-promotion-gate failed; see receipt failures" >&2
exit 2
