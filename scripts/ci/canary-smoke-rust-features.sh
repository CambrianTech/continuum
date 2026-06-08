#!/usr/bin/env bash
# canary-smoke-rust-features.sh — Rust feature-boundary slice of the
# canary end-to-end smoke matrix (continuum#1132).
#
# This is intentionally narrower than a full build. It proves that the Rust
# workspace still advertises the feature contracts our install/docker paths
# depend on, then runs a small cargo-check slice that is valid for the current
# host. GPU-specific checks skip when the host cannot prove that backend.

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKERS_DIR="$ROOT_DIR/core"
RUN_CARGO_CHECK="${RUN_CARGO_CHECK:-1}"
SMOKE_VERBOSE="${SMOKE_VERBOSE:-0}"

PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0
FAILED_STEPS=()

pass() {
  PASS_COUNT=$((PASS_COUNT + 1))
  printf '  ✓ %s\n' "$1"
}

skip() {
  SKIP_COUNT=$((SKIP_COUNT + 1))
  printf '  - %s — %s\n' "$1" "$2"
}

fail() {
  FAIL_COUNT=$((FAIL_COUNT + 1))
  FAILED_STEPS+=("$1: $2")
  printf '  ✗ %s — %s\n' "$1" "$2"
}

run_step() {
  local name="$1"
  shift

  local out rc
  out=$("$@" 2>&1)
  rc=$?

  if [ "$rc" -eq 0 ]; then
    pass "$name"
    if [ "$SMOKE_VERBOSE" -eq 1 ]; then
      printf '%s\n' "$out" | sed 's/^/      /'
    fi
  else
    fail "$name" "exit=$rc"
    printf '%s\n' "$out" | tail -80 | sed 's/^/      /'
  fi
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail "preflight: $1" "command not found"
    return 1
  fi
  pass "preflight: $1"
}

printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'
printf '  canary-smoke-rust-features (continuum#1132)\n'
printf '  workspace=%s\n' "$WORKERS_DIR"
printf '  RUN_CARGO_CHECK=%s\n' "$RUN_CARGO_CHECK"
printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'

require_cmd cargo || true
require_cmd python3 || true

if [ "$FAIL_COUNT" -ne 0 ]; then
  printf '\nFAILED preflight; cannot continue.\n' >&2
  exit 2
fi

METADATA_JSON="$(mktemp -t continuum-rust-metadata.XXXXXX)"
trap 'rm -f "$METADATA_JSON"' EXIT

run_step "cargo metadata parses workspace" \
  cargo metadata --manifest-path "$WORKERS_DIR/Cargo.toml" --format-version 1 --no-deps

if cargo metadata --manifest-path "$WORKERS_DIR/Cargo.toml" --format-version 1 --no-deps >"$METADATA_JSON" 2>/dev/null; then
  python3 - "$METADATA_JSON" <<'PY'
import json
import sys

metadata_path = sys.argv[1]
data = json.load(open(metadata_path))
packages = {pkg["name"]: pkg for pkg in data["packages"]}

checks = [
    ("continuum-core", "metal", ["candle-core/metal", "llama/metal", "ort/coreml"]),
    ("continuum-core", "cuda", ["candle-core/cuda", "llama/cuda", "ort/cuda"]),
    ("continuum-core", "vulkan", ["llama/vulkan"]),
    ("continuum-core", "load-dynamic-ort", ["ort/load-dynamic"]),
    ("continuum-core", "livekit-webrtc", ["dep:livekit", "dep:livekit-api"]),
    ("llama", "metal", []),
    ("llama", "cuda", []),
    ("llama", "vulkan", []),
    ("inference-grpc", "metal", ["candle-core/metal"]),
    ("inference-grpc", "cuda", ["candle-core/cuda"]),
]

errors = []
for crate, feature, required_edges in checks:
    pkg = packages.get(crate)
    if not pkg:
        errors.append(f"missing package {crate}")
        continue
    features = pkg.get("features", {})
    if feature not in features:
        errors.append(f"{crate} missing feature {feature}")
        continue
    edges = set(features[feature])
    for edge in required_edges:
        if edge not in edges:
            errors.append(f"{crate}/{feature} missing edge {edge}")

default_features = set(packages["continuum-core"].get("features", {}).get("default", []))
for forbidden in ("metal", "cuda", "vulkan"):
    if forbidden in default_features:
        errors.append(f"continuum-core default must not enable {forbidden}")

if "livekit-webrtc" not in default_features:
    errors.append("continuum-core default must include livekit-webrtc until bridge migration removes it")

if errors:
    for error in errors:
        print(f"ERROR: {error}")
    sys.exit(1)

print("Rust feature contract OK")
PY
  if [ "$?" -eq 0 ]; then
    pass "Rust feature contract matches install/docker matrix"
  else
    fail "Rust feature contract matches install/docker matrix" "metadata contract mismatch"
  fi
else
  fail "Rust feature contract matches install/docker matrix" "metadata unavailable"
fi

if [ "$RUN_CARGO_CHECK" = "0" ]; then
  skip "cargo check slices" "RUN_CARGO_CHECK=0"
else
  run_step "cargo check bridge protocol" \
    cargo check --manifest-path "$WORKERS_DIR/Cargo.toml" -p continuum-bridge-protocol

  case "$(uname -s)" in
    Darwin)
      skip "cargo check llama default" "macOS intentionally rejects CPU-only llama builds"
      run_step "cargo check llama metal on macOS" \
        cargo check --manifest-path "$WORKERS_DIR/Cargo.toml" -p llama --features metal
      ;;
    Linux)
      run_step "cargo check llama default" \
        cargo check --manifest-path "$WORKERS_DIR/Cargo.toml" -p llama

      if command -v nvidia-smi >/dev/null 2>&1 && command -v nvcc >/dev/null 2>&1; then
        run_step "cargo check llama cuda on NVIDIA Linux" \
          cargo check --manifest-path "$WORKERS_DIR/Cargo.toml" -p llama --features cuda
      else
        skip "cargo check llama cuda on NVIDIA Linux" "nvidia-smi or nvcc unavailable"
      fi

      if command -v vulkaninfo >/dev/null 2>&1; then
        run_step "cargo check llama vulkan on Linux" \
          cargo check --manifest-path "$WORKERS_DIR/Cargo.toml" -p llama --features vulkan
      else
        skip "cargo check llama vulkan on Linux" "vulkaninfo unavailable"
      fi
      ;;
    *)
      skip "GPU cargo check slices" "unsupported host $(uname -s)"
      ;;
  esac
fi

printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'
printf '  result: %s passed, %s skipped, %s failed\n' "$PASS_COUNT" "$SKIP_COUNT" "$FAIL_COUNT"
printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'

if [ "$FAIL_COUNT" -ne 0 ]; then
  printf '\nFailed steps:\n' >&2
  for step in "${FAILED_STEPS[@]}"; do
    printf '  - %s\n' "$step" >&2
  done
  exit 2
fi
