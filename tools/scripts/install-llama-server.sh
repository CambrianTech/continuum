#!/usr/bin/env bash
# install-llama-server.sh — build the inference engine WE OWN.
#
# continuum's serving daemon launches `llama-server` (llama.cpp's OpenAI-/v1
# gateway). That binary is OURS now, not a borrowed one: we excised unsloth, so
# depending on `~/.unsloth/llama.cpp/build/bin/llama-server` made our inference
# hostage to a tool we no longer ship — and a freshly-cloned public machine has
# no `~/.unsloth` at all. This script builds llama-server from the llama.cpp
# submodule we already vendor and build for the in-process FFI lib
# (core/vendor/llama.cpp — our fork, github.com/CambrianTech/llama.cpp), into a
# continuum-owned path. ONE llama.cpp source of truth for both the lib and the
# server; no external dependency.
#
# Idempotent: if the installed binary's stamp matches the submodule HEAD + the
# detected backend, it's a no-op. Pass --force to rebuild regardless.
#
# Backend is detected the same way core/llama/build.rs gates the FFI build:
#   macOS arm64 → Metal · Linux + nvcc → CUDA · otherwise → CPU.
set -euo pipefail

FORCE=0
[ "${1:-}" = "--force" ] && FORCE=1

# ── paths ────────────────────────────────────────────────────────────
# Repo root is two up from tools/scripts/.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SUBMODULE="$REPO_ROOT/core/vendor/llama.cpp"

CONTINUUM_HOME="${CONTINUUM_HOME:-$HOME/.continuum}"
INSTALL_DIR="$CONTINUUM_HOME/bin"
INSTALL_BIN="$INSTALL_DIR/llama-server"
STAMP_FILE="$INSTALL_DIR/.llama-server.stamp"
# Build dir lives in the shared cache, reused across runs for incremental builds.
BUILD_DIR="$CONTINUUM_HOME/cache/llama-server-build"

# ── toolchain ────────────────────────────────────────────────────────
if ! command -v cmake >/dev/null 2>&1; then
  echo "✗ FATAL: cmake not found — required to build llama-server from source." >&2
  echo "  macOS: brew install cmake · Debian/Ubuntu: apt-get install cmake" >&2
  exit 1
fi

# ── submodule presence ───────────────────────────────────────────────
# A fresh clone may not have the submodule checked out yet. Init it from our
# fork rather than failing — this is the public-user path.
if [ ! -f "$SUBMODULE/tools/server/CMakeLists.txt" ]; then
  echo "→ llama.cpp submodule not populated; initializing core/vendor/llama.cpp …" >&2
  git -C "$REPO_ROOT" submodule update --init core/vendor/llama.cpp >&2
fi
if [ ! -f "$SUBMODULE/tools/server/CMakeLists.txt" ]; then
  echo "✗ FATAL: llama.cpp submodule missing at $SUBMODULE even after init." >&2
  exit 1
fi

SUBMODULE_HEAD="$(git -C "$SUBMODULE" rev-parse --short HEAD 2>/dev/null || echo unknown)"

# ── backend detection (mirrors core/llama/build.rs) ──────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"
BACKEND="cpu"
declare -a BACKEND_DEFS=()
if [ "$OS" = "Darwin" ] && [ "$ARCH" = "arm64" ]; then
  BACKEND="metal"
  BACKEND_DEFS=(-DGGML_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON)
elif [ "$OS" = "Linux" ] && command -v nvcc >/dev/null 2>&1; then
  BACKEND="cuda"
  BACKEND_DEFS=(-DGGML_CUDA=ON)
fi

STAMP_WANT="$SUBMODULE_HEAD:$BACKEND"

# ── idempotency ──────────────────────────────────────────────────────
if [ "$FORCE" -eq 0 ] && [ -x "$INSTALL_BIN" ] && [ -f "$STAMP_FILE" ] \
   && [ "$(cat "$STAMP_FILE" 2>/dev/null)" = "$STAMP_WANT" ]; then
  echo "✓ llama-server already current at $INSTALL_BIN ($STAMP_WANT)" >&2
  echo "$INSTALL_BIN"
  exit 0
fi

# ── build ────────────────────────────────────────────────────────────
echo "→ building llama-server ($BACKEND, llama.cpp@$SUBMODULE_HEAD) from $SUBMODULE" >&2
mkdir -p "$BUILD_DIR" "$INSTALL_DIR"

# Server-only build. LLAMA_CURL=OFF: we serve local GGUF paths (-m), never fetch
# models by URL, so the libcurl dependency is dead weight + a portability snag.
cmake -S "$SUBMODULE" -B "$BUILD_DIR" \
  -DCMAKE_BUILD_TYPE=Release \
  -DLLAMA_BUILD_SERVER=ON \
  -DLLAMA_BUILD_TOOLS=ON \
  -DLLAMA_BUILD_COMMON=ON \
  -DLLAMA_BUILD_TESTS=OFF \
  -DLLAMA_BUILD_EXAMPLES=OFF \
  -DLLAMA_CURL=OFF \
  "${BACKEND_DEFS[@]}" >&2

# Parallelism: nproc (Linux) / sysctl (macOS), default 4.
JOBS="$( (command -v nproc >/dev/null 2>&1 && nproc) || sysctl -n hw.ncpu 2>/dev/null || echo 4)"
cmake --build "$BUILD_DIR" --target llama-server --config Release -j"$JOBS" >&2

BUILT_BIN="$BUILD_DIR/bin/llama-server"
if [ ! -x "$BUILT_BIN" ]; then
  echo "✗ FATAL: build finished but $BUILT_BIN is missing." >&2
  exit 1
fi

# ── install + stamp ──────────────────────────────────────────────────
cp -f "$BUILT_BIN" "$INSTALL_BIN"
chmod +x "$INSTALL_BIN"

# Verify the installed binary runs BEFORE stamping it (2026-07-26, M5+BigMama
# two-box dogfood). The old order (stamp THEN verify) had two failure modes:
#   1. STAMP-BEFORE-VERIFY (BigMama): a genuinely-broken build still got its
#      stamp written, so the next run saw stamp==SUBMODULE_HEAD, decided it was
#      "already current", skipped the rebuild, and served the broken binary
#      forever. The stamp must be the LAST step, written only after proof.
#   2. FLAKY VERIFY (M5): a fresh Mach-O's first exec can transiently SIGKILL
#      (Killed:9) under concurrent build+serve memory/Metal pressure even though
#      the binary is perfectly healthy — a single --version check then false-
#      FATALs a good install. Retry once before declaring failure.
# On genuine failure, remove the binary so it can't masquerade as installed.
verify_ok=0
for _attempt in 1 2; do
  if "$INSTALL_BIN" --version >/dev/null 2>&1; then verify_ok=1; break; fi
  sleep 1
done
if [ "$verify_ok" -ne 1 ]; then
  rm -f "$INSTALL_BIN"
  echo "✗ FATAL: built llama-server does not run (--version failed after retry)." >&2
  exit 1
fi
echo "$STAMP_WANT" > "$STAMP_FILE"   # stamp LAST — only a verified-good binary is blessed

echo "✓ llama-server installed: $INSTALL_BIN ($STAMP_WANT)" >&2
echo "$INSTALL_BIN"
