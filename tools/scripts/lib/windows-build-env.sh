#!/usr/bin/env bash
# Windows MSVC/CUDA build-environment import — the ONE place it is decided.
#
# Source this before ANY cargo invocation on Windows (build, check, test,
# clippy). It exports INCLUDE/LIB, puts cl.exe + the Windows SDK + cmake +
# ninja on PATH, pins CMAKE_GENERATOR, and resolves CUDA_PATH/RUSTFLAGS.
#
# Why it is a lib and not inline in start-server.sh (where it lived until
# 2026-08-06): it was reachable only by booting a whole server. So on this
# platform "validate before you commit" degraded to "start the core and read
# the build log", and a plain `cargo test -p continuum-core` died in the
# llama build.rs with "is `cmake` not installed?" — cmake IS installed, the
# shell just had none of this. A build environment that only one script can
# enter is not an environment, it is a side effect of that script.
#
#   source tools/scripts/lib/windows-build-env.sh
#   cargo test -p continuum-core --lib
#
# Idempotent and self-guarding: no-ops off Windows, no-ops when cl.exe is
# already resolvable, and each PATH entry is prepended only if the directory
# really exists.

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) _mf_os=windows ;;
  Darwin)               _mf_os=macos ;;
  *)                    _mf_os=linux ;;
esac

# Locate the manifest relative to THIS file, not to whatever $SCRIPT_DIR the
# caller happens to have. A lib that reads the caller's variables only works
# for the one caller that defines them — which is how this block silently
# resolved to `/generated/manifest.windows.sh` in a plain shell, found
# nothing, and left nvcc off PATH so the MSVC import below skipped itself.
_wbe_lib_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_mf_runtime="$_wbe_lib_dir/../generated/manifest.${_mf_os}.sh"
# The generated manifest uses bash-4 associative arrays (`declare -A`). macOS ships bash 3.2,
# where those are a hard `invalid option` error — and under this script's `set -e` a mid-file
# failure ABORTS the whole boot (regression from #2046 "serve on Windows", which regenerated
# manifest.macos.sh with `declare -A`; it silently broke every macOS reboot until the last
# long-running core died). Only source it on bash 4+. The manifest solely feeds the
# runtime-PATH augmentation below (a Windows/CUDA concern), whose own guard already tolerates
# absence — so skipping it on bash 3.2 costs macOS nothing and the boot proceeds to the build.
if [ -f "$_mf_runtime" ] && [ "${BASH_VERSINFO[0]:-0}" -ge 4 ]; then
  # shellcheck source=/dev/null
  source "$_mf_runtime"
  if declare -p MOD_RUNTIME_PATH >/dev/null 2>&1; then
    for _mid in "${!MOD_RUNTIME_PATH[@]}"; do
      IFS=':' read -ra _rp_dirs <<<"${MOD_RUNTIME_PATH[$_mid]}"
      for _rp in "${_rp_dirs[@]}"; do
        # eval expands ~ and the version glob (cuda-*); prepend each existing match once.
        for _rp_hit in $(eval echo "$_rp"); do
          [ -d "$_rp_hit" ] || continue
          case ":$PATH:" in *":$_rp_hit:"*) ;; *) export PATH="$_rp_hit:$PATH" ;; esac
        done
      done
    done
  fi
fi

# ── Build drivers: cmake + ninja (EVERY platform, independent of CUDA) ──────────────────────
# These two used to live INSIDE the CUDA/MSVC block below, which gated them on
# `nvcc present AND cl.exe absent`. Neither has anything to do with CUDA:
#
#   * a CPU-only Windows box (no nvcc) skipped the block entirely and got no cmake
#     pin, so `cargo build` died with "is `cmake` not installed?" while cmake sat
#     installed at ~/.continuum/tools/cmake — a manifest-provisioned tool the build
#     could not see;
#   * a shell where cl.exe was ALREADY resolvable skipped it for the same reason.
#
# The PATH half is now the manifest's job (`[module.runtime_path]` on cmake/ninja,
# consumed by the generic loop above) — that is where the windows-vs-unix split
# belongs, because it IS a packaging fact: brew/apt put these on PATH, the Windows
# archives do not. What is left here is only what the manifest cannot express: the
# two env vars the cmake crate reads.

# cmake-rs (core/llama build.rs) resolves cmake from the CMAKE env var, else PATH.
# Point it at whatever the loop above (or the system package manager) resolved, so
# the crate never re-guesses. No fallback path is invented: if cmake is genuinely
# absent, the build fails loudly with the crate's own message rather than pointing
# at a file that isn't there.
if [ -z "$CMAKE" ] && command -v cmake >/dev/null 2>&1; then
  _wbe_cmake="$(command -v cmake)"
  if [ "$_mf_os" = windows ]; then
    export CMAKE="$(cygpath -w "$_wbe_cmake" 2>/dev/null || echo "$_wbe_cmake")"
  else
    export CMAKE="$_wbe_cmake"
  fi
fi

# WINDOWS ONLY: force a generator cmake actually knows. The cmake crate auto-picks
# the newest installed Visual Studio; on a VS18-2026 box that is "Visual Studio 18
# 2026", which cmake 3.30.x does not define ("Could not create named generator") —
# so the configure step fails on a machine where every tool is present and correct.
# Ninja is generator-version-agnostic and uses the MSVC env imported below.
# [[windows-build-env-drift]]
#
# Deliberately NOT applied on unix: cmake's default there (Unix Makefiles) is never
# broken, so pinning Ninja would change every Linux/macOS contributor's build to buy
# nothing. The asymmetry is the point — this is a Windows-specific defect, and the
# fix stays scoped to it.
if [ "$_mf_os" = windows ] && [ -z "$CMAKE_GENERATOR" ] && command -v ninja >/dev/null 2>&1; then
  # cmake ignores CMAKE_MAKE_PROGRAM from the env and searches PATH for ninja, which
  # the manifest runtime_path above already guarantees.
  export CMAKE_GENERATOR="Ninja"
fi

# ── Windows: import the MSVC toolchain so cargo's CUDA (candle) build finds cl.exe ──────────
# candle compiles CUDA kernels (affine.cu, ...) via nvcc, which needs cl.exe as its host
# compiler plus INCLUDE/LIB. The cargo builds below run in THIS bash shell (unlike the
# llama-server cmake build, which runs inside a vcvars .bat), so without this nvcc fails with
# "Cannot find compiler 'cl.exe' in PATH" and the whole core build dies. Import it once: export
# INCLUDE/LIB verbatim (only cl.exe reads them) and prepend the EXACT MSVC + Windows SDK bin
# dirs (from vcvars, converted to unix) to PATH — nvcc finds cl.exe while bash's own PATH
# resolution stays intact (we add unix dirs, never overwrite PATH with the Windows one). VS2022
# (14.4x) is selected explicitly: nvcc 12.x rejects the newer 14.5x/VS18 toolset.
if [ "$_mf_os" = windows ] && command -v nvcc >/dev/null 2>&1 && ! command -v cl.exe >/dev/null 2>&1; then
  _vswhere="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
  _vs=""
  [ -x "$_vswhere" ] && _vs="$("$_vswhere" -version "[17.0,18.0)" -products '*' \
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 \
        -property installationPath 2>/dev/null | head -1)"
  _vcvars_u=""
  [ -n "$_vs" ] && _vcvars_u="$(cygpath -u "$_vs\\VC\\Auxiliary\\Build\\vcvars64.bat" 2>/dev/null)"
  if [ -n "$_vcvars_u" ] && [ -f "$_vcvars_u" ]; then
    # Capture vcvars' env via a .bat FILE, not inline `cmd //c "call … && set"`: the vcvars path
    # has spaces AND parens, and escaping nested quotes through bash->cmd corrupts it (the same
    # reason install-llama-server.sh emits a .bat). The .bat calls vcvars then dumps `set`.
    _msbat="$(mktemp --suffix=.bat 2>/dev/null || echo "${TEMP:-/tmp}/_cc_vcvars_$$.bat")"
    printf '@echo off\r\ncall "%s" >nul 2>&1\r\nset\r\n' "$(cygpath -w "$_vcvars_u")" > "$_msbat"
    _msenv="$(cmd //c "$(cygpath -w "$_msbat")" 2>/dev/null | tr -d '\r')"
    rm -f "$_msbat"
    export INCLUDE="$(printf '%s\n' "$_msenv" | sed -n 's/^INCLUDE=//Ip' | head -1)"
    export LIB="$(printf '%s\n' "$_msenv" | sed -n 's/^LIB=//Ip' | head -1)"
    _vct="$(printf '%s\n' "$_msenv" | sed -n 's/^VCToolsInstallDir=//Ip' | head -1)"
    _sdkbin="$(printf '%s\n' "$_msenv" | sed -n 's/^WindowsSdkVerBinPath=//Ip' | head -1)"
    # Guard each conversion: only prepend a REAL existing directory. An empty/failed cygpath would
    # otherwise inject a bogus relative entry that corrupts the Windows-format PATH the cargo build
    # subprocess inherits, which silently breaks other tools' resolution (e.g. cmake "program not found").
    if [ -n "$_vct" ]; then _clb="$(cygpath -u "${_vct}bin\\Hostx64\\x64" 2>/dev/null)"; [ -d "$_clb" ] && PATH="$_clb:$PATH"; fi
    if [ -n "$_sdkbin" ]; then _sdb="$(cygpath -u "${_sdkbin}x64" 2>/dev/null)"; [ -d "$_sdb" ] && PATH="$_sdb:$PATH"; fi
    export PATH
    # (cmake + ninja are pinned ABOVE, outside this block — they are not CUDA concerns.
    # Gating them on nvcc is what left a CPU-only Windows box with no cmake at all.)
    # CUDA_PATH must be set: cudarc/candle/pocket-tts read it to emit their link-search; without it the
    # link has NO CUDA search path (measured: LNK1181 cuda.lib). Point it at a cuda-* whose import-lib
    # dir actually has the libs (a provisioning split can leave the crate-detected dir EMPTY - cuda-env
    # /Library/lib/x64=0 vs cuda-13.2=12; the #6 provisioning fix unifies them, and this node's cuda-env
    # was completed by copying the sibling's libs in). candle finds nvcc via PATH independently, so a
    # libs-only CUDA_PATH is fine (build proven). Real fix: provision ONE complete toolkit (#6).
    if [ -z "$CUDA_PATH" ]; then
      for _cand in "${CONTINUUM_HOME:-$HOME/.continuum}"/cuda-env "${CONTINUUM_HOME:-$HOME/.continuum}"/cuda-*; do
        for _sub in Library/lib/x64 lib/x64; do
          if [ -f "$_cand/$_sub/curand.lib" ] && [ -f "$_cand/$_sub/cuda.lib" ]; then
            export CUDA_PATH="$(cygpath -w "$_cand" 2>/dev/null || echo "$_cand")"
            _culibw="$(cygpath -w "$_cand/$_sub" 2>/dev/null || echo "$_cand/$_sub")"
            # pocket-tts links cuda.lib but (unlike cudarc) emits no rustc-link-search, relying on the
            # linker's own search. rustc links with the newest VS's linker + ITS OWN LIB (not ours), so
            # the dir must reach link.exe via RUSTFLAGS -L. RUSTFLAGS env REPLACES (not merges) the
            # .cargo/config target rustflags, so re-include +crt-static (the task-#4 /MT fix) or the whole
            # GPU stack mislinks LNK2038 MT-vs-MD.
            case " $RUSTFLAGS " in *"-L native=${_culibw} "*) : ;; *) export RUSTFLAGS="-C target-feature=+crt-static -L native=${_culibw} ${RUSTFLAGS}" ;; esac
            echo "▶ CUDA_PATH=$CUDA_PATH + RUSTFLAGS -L $_culibw (import libs in $_sub)"
            break 2
          fi
        done
      done
      [ -z "$CUDA_PATH" ] && echo "⚠ no complete CUDA import-lib dir found (cuda-*/**/{cuda,curand}.lib) - core link WILL fail. Provisioning gap (#6)." >&2
    fi
    if command -v cl.exe >/dev/null 2>&1; then
      echo "▶ MSVC toolchain imported for the CUDA cargo build (cl.exe on PATH for nvcc/candle)"
    else
      echo "⚠ MSVC import ran but cl.exe still unresolved — the CUDA cargo build will fail" >&2
    fi
  else
    echo "✗ Windows+CUDA needs VS2022 (14.4x) C++ x64 toolset for nvcc's host compiler; vswhere/vcvars not found — the core build will fail. Install via the 'msvc' module." >&2
  fi
fi
