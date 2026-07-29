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
# cmake on PATH, else the manifest install location — a user who provisioned via
# install-manifest has cmake at ~/.continuum/tools/cmake/bin but may not have added it to
# PATH (Windows especially). Discover it rather than falsely failing. (CONTINUUM_HOME is
# already resolved above.)
if ! command -v cmake >/dev/null 2>&1; then
  for c in "$CONTINUUM_HOME/tools/cmake/bin/cmake" "$CONTINUUM_HOME/tools/cmake/bin/cmake.exe"; do
    if [ -x "$c" ]; then PATH="$(dirname "$c"):$PATH"; export PATH; break; fi
  done
fi
if ! command -v cmake >/dev/null 2>&1; then
  echo "✗ FATAL: cmake not found — required to build llama-server from source." >&2
  echo "  macOS: brew install cmake · Debian/Ubuntu: apt-get install cmake · Windows: install-manifest 'cmake' module" >&2
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
EXE=""          # binary suffix — ".exe" on Windows, load-bearing for every path below
WIN_CUDA=0      # Windows+CUDA needs a distinct build path (MSVC env + Ninja)
declare -a BACKEND_DEFS=()
if [ "$OS" = "Darwin" ] && [ "$ARCH" = "arm64" ]; then
  BACKEND="metal"
  BACKEND_DEFS=(-DGGML_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON)
elif [ "$OS" = "Linux" ] && command -v nvcc >/dev/null 2>&1; then
  BACKEND="cuda"
  BACKEND_DEFS=(-DGGML_CUDA=ON)
else
  case "$OS" in
    MINGW*|MSYS*|CYGWIN*)
      EXE=".exe"
      if command -v nvcc >/dev/null 2>&1 || [ -x "$CONTINUUM_HOME/cuda-toolkit/bin/nvcc.exe" ]; then
        BACKEND="cuda-static"; WIN_CUDA=1
        # arch=native → build for THIS machine's GPU (portable; NOT a hardcoded sm_120).
        # BUILD_SHARED_LIBS=OFF is LOAD-BEARING (2026-07-28, BigMama): the shared build's
        # ggml-cuda.dll fails CUDA init at runtime ("no usable GPU found") while passing
        # --version — so a GPU-blind engine got stamped verified-good and every generation
        # 500'd (the serving daemon's decode-ready probe then never admits personas). The
        # static build (ggml linked into the exe) initializes CUDA correctly on the same
        # box. Backend renamed cuda→cuda-static so existing broken installs fail the stamp
        # check and rebuild on next run. CUDA RUNTIME (cudart/cublas dlls) stays dynamic
        # via the manifest runtime_path — only ggml/llama are static.
        BACKEND_DEFS=(-DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=native -DBUILD_SHARED_LIBS=OFF)
      fi
      # non-NVIDIA Windows falls through to the CPU build (llama.cpp CPU works on Win).
      ;;
  esac
fi
# Windows binaries carry .exe — apply the suffix to the installed path now, before the
# idempotency check and every downstream use.
INSTALL_BIN="${INSTALL_BIN}${EXE}"

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
declare -a CMAKE_ARGS=(
  -DCMAKE_BUILD_TYPE=Release
  -DLLAMA_BUILD_SERVER=ON
  -DLLAMA_BUILD_TOOLS=ON
  -DLLAMA_BUILD_COMMON=ON
  -DLLAMA_BUILD_TESTS=OFF
  -DLLAMA_BUILD_EXAMPLES=OFF
  -DLLAMA_CURL=OFF
  "${BACKEND_DEFS[@]}"
)

# Parallelism: nproc (Linux) / sysctl (macOS), default 4.
JOBS="$( (command -v nproc >/dev/null 2>&1 && nproc) || sysctl -n hw.ncpu 2>/dev/null || echo 4)"

if [ "$WIN_CUDA" -eq 1 ]; then
  # Windows + CUDA is its own build path. Two hard requirements the Unix path lacks:
  #   1. The MSVC toolchain env (cl.exe as nvcc's host compiler + INCLUDE/LIB). We run
  #      cmake INSIDE a VS2022 developer shell via `cmd /c call vcvars64` so bash never
  #      has to import Windows-style PATH/INCLUDE/LIB (which would corrupt its own PATH).
  #   2. The Ninja generator. The Visual Studio generator mangles nvcc's `-ccbin` when the
  #      host-compiler path contains spaces (`-ccbin=C:Program Files...`), so we use Ninja
  #      and let nvcc auto-find cl.exe from the dev-shell PATH (never pass a spaced
  #      CMAKE_CUDA_HOST_COMPILER — that reintroduces the mangling).
  # VS2022 (14.4x) is selected EXPLICITLY: nvcc 12.x rejects the newer 14.5x/VS18 toolset,
  # and a machine may have both installed.
  vswhere="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
  [ -x "$vswhere" ] || { echo "✗ FATAL: vswhere not found — install VS2022 BuildTools (install-manifest 'msvc' module)." >&2; exit 1; }
  vs_path="$("$vswhere" -version "[17.0,18.0)" -products '*' \
              -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 \
              -property installationPath 2>/dev/null | head -1)"
  [ -n "$vs_path" ] || { echo "✗ FATAL: VS2022 (14.4x) with the C++ x64 toolset not found — nvcc needs it, not VS18/14.5x." >&2; exit 1; }
  vcvars="$vs_path\\VC\\Auxiliary\\Build\\vcvars64.bat"
  # Ninja + nvcc: on PATH, else the manifest install locations (a manifest-provisioned box
  # has these under ~/.continuum but may not export them on PATH).
  ninja_exe="$(command -v ninja 2>/dev/null || echo "$CONTINUUM_HOME/tools/ninja/ninja.exe")"
  [ -x "$ninja_exe" ] || { echo "✗ FATAL: ninja not found (install-manifest toolchain)." >&2; exit 1; }
  nvcc_exe="$(command -v nvcc 2>/dev/null || echo "$CONTINUUM_HOME/cuda-toolkit/bin/nvcc.exe")"
  [ -x "$nvcc_exe" ] || { echo "✗ FATAL: nvcc not found (install-manifest 'cuda' module)." >&2; exit 1; }
  cmake_exe="$(command -v cmake)"
  # Windows-native paths for the cmd context.
  win_sub="$(cygpath -w "$SUBMODULE")"
  win_build="$(cygpath -w "$BUILD_DIR")"
  win_ninja="$(cygpath -w "$ninja_exe")"
  win_nvcc="$(cygpath -w "$nvcc_exe")"
  win_cmake="$(cygpath -w "$cmake_exe")"
  # CMAKE_CUDA_COMPILER is passed EXPLICITLY (not left to PATH discovery) — nvcc may not be
  # on PATH, and its path locates the whole CUDA toolkit for cmake (cublas at link time).
  # We emit the configure+build into a .bat and run THAT, rather than an inline
  # `cmd /c "call vcvars && cmake …"`: the vcvars/cmake paths contain spaces AND parens, and
  # escaping nested quotes through bash→cmd corrupts them ('vcvars64.bat' is not recognized).
  # A .bat carries native Windows quoting cleanly, and `call ... && …` in one script keeps
  # the vcvars env live across configure and build. CMAKE_ARGS are plain -DKEY=VALUE.
  build_bat="$BUILD_DIR/_win_build.bat"
  {
    echo "@echo off"
    echo "call \"$vcvars\" >nul || exit /b 1"
    echo "\"$win_cmake\" -S \"$win_sub\" -B \"$win_build\" -G Ninja -DCMAKE_MAKE_PROGRAM=\"$win_ninja\" -DCMAKE_CUDA_COMPILER=\"$win_nvcc\" ${CMAKE_ARGS[*]} || exit /b 1"
    echo "\"$win_cmake\" --build \"$win_build\" --target llama-server --config Release -j $JOBS || exit /b 1"
  } > "$build_bat"
  cmd //c "$(cygpath -w "$build_bat")" >&2
else
  cmake -S "$SUBMODULE" -B "$BUILD_DIR" "${CMAKE_ARGS[@]}" >&2
  cmake --build "$BUILD_DIR" --target llama-server --config Release -j"$JOBS" >&2
fi

BUILT_BIN="$BUILD_DIR/bin/llama-server${EXE}"
if [ ! -x "$BUILT_BIN" ]; then
  echo "✗ FATAL: build finished but $BUILT_BIN is missing." >&2
  exit 1
fi

# ── install + stamp ──────────────────────────────────────────────────
cp -f "$BUILT_BIN" "$INSTALL_BIN"
chmod +x "$INSTALL_BIN"

# Windows is a SHARED build: llama-server.exe is a thin launcher that loads its
# ggml*.dll / llama*.dll siblings from its own directory. Ship them alongside the exe or
# it can't start. (The CUDA RUNTIME dlls — cublas etc. — are NOT copied here; they come
# from the manifest runtime_path on PATH at serve time, one owner for that concern.)
if [ -n "$EXE" ]; then
  cp -f "$BUILD_DIR/bin/"*.dll "$INSTALL_DIR/" 2>/dev/null || true
fi

# The verify below runs the binary standalone. On Windows+CUDA it dynamically loads the
# CUDA runtime (ggml-cuda.dll → cublas), which lives outside INSTALL_DIR — put the manifest
# CUDA runtime dirs on PATH just for the verify (same glob the runtime_path module uses,
# so this stays in step with it rather than hardcoding a version).
if [ "$WIN_CUDA" -eq 1 ]; then
  for d in "$CONTINUUM_HOME"/cuda-*/bin "$CONTINUUM_HOME"/cuda-*/Library/bin; do
    [ -d "$d" ] && PATH="$d:$PATH"
  done
  export PATH
fi

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

# On Windows+CUDA, --version is NOT enough: the GPU-blind shared build passed it while
# unable to init CUDA (the 2026-07-28 regression). Require the engine to actually SEE a
# CUDA device before stamping — this is the difference between "binary runs" and "binary
# can serve". A build that can't see the GPU is removed, not blessed.
if [ "$WIN_CUDA" -eq 1 ]; then
  if ! "$INSTALL_BIN" --list-devices 2>&1 | grep -q "CUDA0"; then
    rm -f "$INSTALL_BIN"
    echo "✗ FATAL: built llama-server cannot see a CUDA device (--list-devices has no CUDA0)." >&2
    echo "  A GPU-blind engine must never be stamped — it serves decode-dead lanes." >&2
    exit 1
  fi
  echo "✓ CUDA verify: engine sees CUDA0" >&2
fi
echo "$STAMP_WANT" > "$STAMP_FILE"   # stamp LAST — only a verified-good binary is blessed

echo "✓ llama-server installed: $INSTALL_BIN ($STAMP_WANT)" >&2
echo "$INSTALL_BIN"
