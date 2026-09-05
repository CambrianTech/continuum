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
        BACKEND="cuda"; WIN_CUDA=1
        # arch=native → build for THIS machine's GPU (portable; NOT a hardcoded sm_120).
        BACKEND_DEFS=(-DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=native)
      fi
      # non-NVIDIA Windows falls through to the CPU build (llama.cpp CPU works on Win).
      ;;
  esac
fi
# Windows binaries carry .exe — apply the suffix to the installed path now, before the
# idempotency check and every downstream use.
INSTALL_BIN="${INSTALL_BIN}${EXE}"

# RECORD THE PLACEMENT DECISION WHERE THE CORE READS IT. The core's main lanes read
# CONTINUUM_SERVING_PLACEMENT from ~/.continuum/config.env (#3740): "cpu" pins every lane
# to the CPU and the backend receipt accepts a GPU-less server BY PLAN; anything else is
# the GPU and the receipt refuses a server that loads no GPU backend. The decision is
# made HERE, once, next to the build that implements it — an Intel Mac (Metal OFF, #3729)
# is the case: BigMama's review of #3740 — "a fix that needs a human to edit a file is
# not a fix"; IntelMac had written the key by hand an hour earlier and could not see it.
# Replace-or-append so re-running install is idempotent and other keys survive.
config_env_upsert() {
  local key="$1" value="$2" file="$HOME/.continuum/config.env"
  mkdir -p "$HOME/.continuum"
  touch "$file"
  if grep -qE "^[[:space:]]*${key}[[:space:]]*=" "$file"; then
    local tmp; tmp="$(mktemp)"
    sed -E "s|^[[:space:]]*${key}[[:space:]]*=.*|${key}=${value}|" "$file" > "$tmp" && mv "$tmp" "$file"
  else
    printf '%s=%s\n' "$key" "$value" >> "$file"
  fi
}
if [ "$BACKEND" = "cpu" ]; then
  config_env_upsert CONTINUUM_SERVING_PLACEMENT cpu
  echo "serving placement: cpu (backend=$BACKEND) — recorded in ~/.continuum/config.env"
else
  config_env_upsert CONTINUUM_SERVING_PLACEMENT gpu
  echo "serving placement: gpu (backend=$BACKEND) — recorded in ~/.continuum/config.env"
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
declare -a CMAKE_ARGS=(
  -DCMAKE_BUILD_TYPE=Release
  # Backends must be DYNAMICALLY LOADABLE, and that is not the ggml default.
  # ggml/CMakeLists.txt:86 defaults GGML_BACKEND_DL=OFF; ggml-backend-impl.h:241
  # only defines the exported `ggml_backend_init` under `#ifdef GGML_BACKEND_DL`.
  # With it OFF the loader in ggml-backend-reg.cpp:237 dlopens each ggml-*.dll,
  # finds no such symbol, and registers NOTHING — measured on BigMama 2026-09-05:
  #   load_backend: failed to find ggml_backend_init in ...\ggml-cuda.dll
  #   load_backend: failed to find ggml_backend_init in ...\ggml-cpu.dll
  #   Available devices: (none)
  # on a host with a working 5090. The serving receipt then correctly refuses the
  # lane, and the node hosts nobody. DL requires BUILD_SHARED_LIBS (enforced at
  # ggml/src/CMakeLists.txt:188), so both are set together and explicitly rather
  # than inherited from upstream defaults that our fork can change under us.
  # DL mode requires PORTABLE backends, so it is incompatible with GGML_NATIVE
  # (on by default): ggml-cpu/CMakeLists.txt:374 fails the configure outright with
  # "GGML_NATIVE is not compatible with GGML_BACKEND_DL, consider using
  # GGML_CPU_ALL_VARIANTS". Taking that suggestion rather than GGML_NATIVE=OFF:
  # OFF would build ONE lowest-common-denominator CPU backend for the whole fleet,
  # which silently degrades exactly the CPU-only tier (an Intel Mac serving its
  # citizens on Accelerate). ALL_VARIANTS builds each variant and picks the best
  # at runtime, so a portable build costs no CPU performance.
  -DBUILD_SHARED_LIBS=ON
  -DGGML_BACKEND_DL=ON
  -DGGML_CPU_ALL_VARIANTS=ON
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
    echo "\"$win_cmake\" --build \"$win_build\" --target llama-server llama-quantize --config Release -j $JOBS || exit /b 1"
  } > "$build_bat"
  cmd //c "$(cygpath -w "$build_bat")" >&2
else
  cmake -S "$SUBMODULE" -B "$BUILD_DIR" "${CMAKE_ARGS[@]}" >&2
  # BOTH targets. llama-quantize is not optional tooling: it carries our fork's
  # `--resident-only` + tier-manifest emit (fork #40), which is the ONLY way to
  # produce the resident tier a MoE pages around. Building only llama-server
  # vendored the capability in and never shipped the tool that exposes it — so
  # the forge custodian had nothing to call and MoE tiering silently had no
  # engine (glass-boxed 2026-08-05). Same built-≠-shipped class as #296.
  cmake --build "$BUILD_DIR" --target llama-server llama-quantize --config Release -j"$JOBS" >&2
fi

BUILT_BIN="$BUILD_DIR/bin/llama-server${EXE}"
if [ ! -x "$BUILT_BIN" ]; then
  echo "✗ FATAL: build finished but $BUILT_BIN is missing." >&2
  exit 1
fi

# Verify the quantize tool the SAME way, and fail just as loud. A soft warning
# here would recreate exactly the gap this change closes: for three days the
# fork carried `--resident-only` while the build emitted no tool to invoke it,
# and nothing said so — the forge custodian would have failed at RUN time, on a
# 91GB job, instead of at BUILD time in one second. If a platform genuinely
# cannot build this target we want to learn it now, explicitly, not discover it
# from a mysteriously empty tier manifest weeks later.
BUILT_QUANTIZE="$BUILD_DIR/bin/llama-quantize${EXE}"
if [ ! -x "$BUILT_QUANTIZE" ]; then
  echo "✗ FATAL: build finished but $BUILT_QUANTIZE is missing." >&2
  echo "  llama-quantize carries our fork's --resident-only + tier manifest (fork #40)." >&2
  echo "  Without it the forge custodian cannot produce a MoE resident tier." >&2
  exit 1
fi

# ── install + stamp ──────────────────────────────────────────────────
# ATOMIC replace (rm + temp + mv), never `cp -f` over the destination in place:
# macOS caches code signatures by inode, so overwriting an existing (or
# previously-executed) Mach-O yields Killed:9 on the next exec — which made the
# verify loop below false-FATAL a perfectly healthy build and delete it (live
# 2026-08-01; the "flaky verify" note below was this class, not memory
# pressure). Ad-hoc re-sign on macOS so the copied binary carries a valid
# signature of its own. Same discipline as start-server.sh's CLI install copy.
rm -f "$INSTALL_BIN"
cp "$BUILT_BIN" "$INSTALL_BIN.tmp.$$"
if [ "$(uname -s)" = "Darwin" ]; then
  codesign -s - --force "$INSTALL_BIN.tmp.$$" 2>/dev/null \
    || echo "⚠ codesign ad-hoc re-sign failed — proceeding (verify below is the gate)" >&2
fi
mv -f "$INSTALL_BIN.tmp.$$" "$INSTALL_BIN"
chmod +x "$INSTALL_BIN"

# Windows is a SHARED build: llama-server.exe is a thin launcher that loads its
# ggml*.dll / llama*.dll siblings from its own directory. Ship them alongside the exe or
# it can't start. (The CUDA RUNTIME dlls — cublas etc. — are NOT copied here; they come
# from the manifest runtime_path on PATH at serve time, one owner for that concern.)
if [ -n "$EXE" ]; then
  # The exe above is copied atomically and verified FATAL. These DLLs are the
  # artifacts it CANNOT RUN WITHOUT, and they used to be copied with
  # `2>/dev/null || true` — the loud failure guarded, the silent one not. That is
  # inverted: a missing exe fails at once, a STALE or unreplaced DLL produces a
  # server that starts and has no backend. On Windows the copy fails routinely,
  # because a running llama-server holds these open. Measured 2026-09-05: a
  # month-old ggml-cuda.dll survived every reinstall this way while the install
  # reported success, and the node hosted nobody. Fail loud instead.
  for _dll in "$BUILD_DIR/bin/"*.dll; do
    [ -e "$_dll" ] || continue
    cp -f "$_dll" "$INSTALL_DIR/" || {
      echo "✗ FATAL: could not install $(basename "$_dll") into $INSTALL_DIR." >&2
      echo "  On Windows this usually means a running llama-server holds it open." >&2
      echo "  Stop the core (\`continuum stop\`) and rerun; do NOT ship a partial set." >&2
      exit 1
    }
  done

  # The CUDA backend links cudart/cublas, which live in the toolkit rather than
  # beside the server. Windows searches the LOADING MODULE'S directory, so without
  # these ggml-cuda.dll fails to load at all — the empty-reason
  # "load_backend: failed to load ...ggml-cuda.dll" that hid a 5090 for weeks.
  if [ "$BACKEND" = "cuda" ]; then
    _cuda_bin="$(dirname "$nvcc_exe")"
    for _rt in cudart64_*.dll cublas64_*.dll cublasLt64_*.dll; do
      for _src in "$_cuda_bin"/$_rt; do
        [ -e "$_src" ] && cp -f "$_src" "$INSTALL_DIR/" 2>/dev/null || true
      done
    done
  fi
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
# ── POSTCONDITION: ASK THE BINARY WHAT IT CAN ACTUALLY DO ────────────
# Everything above verifies that FILES EXIST. None of it verifies the engine
# WORKS, and that gap is what cost this project a GPU node for weeks: the build
# succeeded, the files were all present, the install said ✓, and llama-server
# reported `Available devices: (none)` on a host with an RTX 5090 because the
# ggml backends could not be loaded. A precondition cannot catch that; only
# running the thing can. One command, one second.
#
# We only REFUSE when the build selected a GPU backend and the engine reports no
# GPU — the case where the two disagree. A cpu backend reporting no GPU device is
# correct and must stay silent (an Intel Mac deliberately builds CPU-only, #3729).
if [ "$BACKEND" != "cpu" ]; then
  _devices="$("$INSTALL_BIN" --list-devices 2>&1 || true)"
  if ! printf '%s' "$_devices" | grep -qE '^[[:space:]]*(CUDA|Metal|Vulkan|ROCm|SYCL)[0-9]*:'; then
    echo "✗ FATAL: built for '$BACKEND' but the installed server reports no $BACKEND device." >&2
    echo "  This install would serve every citizen on the CPU while a GPU sits idle," >&2
    echo "  and the serving receipt would refuse the lane outright. Its own words:" >&2
    printf '%s\n' "$_devices" | sed 's/^/      /' >&2
    echo "  NOT stamping — a broken engine must not be blessed as current." >&2
    exit 1
  fi
  echo "→ engine reports: $(printf '%s' "$_devices" | grep -E '^[[:space:]]*(CUDA|Metal|Vulkan|ROCm|SYCL)[0-9]*:' | head -1 | sed 's/^[[:space:]]*//')" >&2
fi

echo "$STAMP_WANT" > "$STAMP_FILE"   # stamp LAST — only a verified-good binary is blessed

echo "✓ llama-server installed: $INSTALL_BIN ($STAMP_WANT)" >&2
echo "$INSTALL_BIN"
