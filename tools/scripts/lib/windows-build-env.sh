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
# ── ONE declared CUDA tree, chosen before anything touches PATH (#6) ────────────────────────
#
# THIS is where the multi-tree bug actually lived. The manifest's runtime-PATH entries contain
# a `cuda-*` glob; expanding it prepended EVERY provisioned tree, so the last one prepended won
# PATH and therefore decided which `cublas.lib` the linker opened. `CUDA_PATH` named a tree but
# had no say in it.
#
# MEASURED on BigMama 2026-08-07: four trees at two majors, CUDA_PATH resolved to a CUDA 12
# tree, and `dumpbin //DEPENDENTS` on the linked binary reported cublas64_13.dll — CUDA 13. The
# declaration and the binding disagreed, and the first symptom was a user-facing
# "cublas64_12.dll was not found" at launch, naming a major the binary does not even use.
#
#   cuda-13.2        34 libs  170 dlls  cuda-13   complete
#   cuda-env          9 libs  123 dlls  cuda-12   complete
#   cuda-toolkit     12 libs    9 dlls  cuda-12   import libs, no runtime
#   cuda-build-venv   0 libs    0 dlls  --        empty
#
# Warning about this was the previous behaviour and it was not enough: a warning leaves the
# next build binding by search order anyway. Choose ONE, deterministically, and let only that
# one onto PATH — then CUDA_PATH, the link, and the runtime all name the same tree by
# construction rather than by luck.
#
# SELECTION, in order, all derived from what is on disk (never a hardcoded tree name):
#   1. linkable  — has cuda.lib AND curand.lib, or it cannot satisfy the link at all
#   2. runnable  — ships cublas64_<major>.dll, or you link fine and fail at load (cuda-toolkit
#                  above is exactly this trap: 12 import libs, 9 DLLs, no usable runtime)
#   3. highest major, then most DLLs, then lexicographic — so two machines with the same trees
#      always choose the same one, and adding a tree never silently re-points an existing node
#      unless it is genuinely newer and complete.
_wbe_cuda_tree=""
_wbe_cuda_major=""
_wbe_cuda_rejected=""
if [ "$_mf_os" = windows ]; then
  _wbe_best_rank=""
  for _t in "${CONTINUUM_HOME:-$HOME/.continuum}"/cuda-*; do
    [ -d "$_t" ] || continue
    _has_lib=""; _has_dll=""; _maj=""
    for _sub in "Library/lib/x64" "lib/x64"; do
      [ -f "$_t/$_sub/cuda.lib" ] && [ -f "$_t/$_sub/curand.lib" ] && _has_lib="$_t/$_sub"
    done
    for _sub in "Library/bin" "bin"; do
      for _c in "$_t/$_sub"/cublas64_*.dll; do
        [ -f "$_c" ] || continue
        _maj="$(basename "$_c" | sed -n 's/^cublas64_\([0-9]\+\)\.dll$/\1/p')"
        [ -n "$_maj" ] && _has_dll="$_t/$_sub" && break
      done
      [ -n "$_has_dll" ] && break
    done
    if [ -z "$_has_lib" ] || [ -z "$_has_dll" ]; then
      _wbe_cuda_rejected="$_wbe_cuda_rejected $(basename "$_t")($([ -z "$_has_lib" ] && echo no-import-libs || echo no-runtime-dll))"
      continue
    fi
    _ndll=$(find "$_t" -maxdepth 3 -name '*.dll' 2>/dev/null | wc -l | tr -d ' ')
    # Zero-padded so string compare orders numerically — no arithmetic on possibly-empty vars.
    _rank="$(printf '%03d-%06d-%s' "$_maj" "$_ndll" "$(basename "$_t")")"
    if [ -z "$_wbe_best_rank" ] || [ "$_rank" \> "$_wbe_best_rank" ]; then
      _wbe_best_rank="$_rank"; _wbe_cuda_tree="$_t"; _wbe_cuda_major="$_maj"
    fi
  done
  if [ -n "$_wbe_cuda_tree" ]; then
    echo "▶ CUDA tree: $(basename "$_wbe_cuda_tree") (cuda-$_wbe_cuda_major) — the ONE declared tree${_wbe_cuda_rejected:+; rejected:$_wbe_cuda_rejected}"
    # CUDA 13's bundled CCCL headers REFUSE MSVC's traditional preprocessor:
    #
    #   cuda/std/__cccl/preprocessor.h(23): fatal error C1189: #error: MSVC/cl.exe with
    #   traditional preprocessor is used ... pass `/Zc:preprocessor` to cl.exe
    #   Error: CompilationFailed { path: "src\reduce.cu" }
    #
    # MEASURED: this is what the first build on cuda-13 hit. CUDA 12's headers had no such
    # check, so the requirement only appears the moment the chooser above correctly prefers the
    # newer complete tree — i.e. picking the right tree is what EXPOSED it, not what caused it.
    #
    # `NVCC_PREPEND_FLAGS` is nvcc's own environment variable, so it reaches EVERY nvcc
    # invocation — candle-kernels, cudarc, llama's cmake — without each crate needing its own
    # flag plumbing. That matters here because there is no single build.rs to patch: several
    # independent crates spawn nvcc, and a per-crate fix would leave the next one broken.
    #
    # Applied only on cuda-13+; harmless on 12 but conditioning it documents WHOSE requirement
    # this is, so nobody later removes it as a mystery flag. Appends rather than overwrites, so
    # an operator's own NVCC_PREPEND_FLAGS survives.
    if [ "${_wbe_cuda_major:-0}" -ge 13 ] 2>/dev/null; then
      case " $NVCC_PREPEND_FLAGS " in
        *"/Zc:preprocessor"*) : ;;
        *) export NVCC_PREPEND_FLAGS="${NVCC_PREPEND_FLAGS:+$NVCC_PREPEND_FLAGS }-Xcompiler /Zc:preprocessor" ;;
      esac
      echo "▶ nvcc: -Xcompiler /Zc:preprocessor (cuda-$_wbe_cuda_major CCCL requires the conforming MSVC preprocessor)"
    fi
  elif [ -n "$_wbe_cuda_rejected" ]; then
    echo "⚠ no COMPLETE CUDA tree (needs cuda.lib+curand.lib AND a cublas64_*.dll); rejected:$_wbe_cuda_rejected" >&2
  fi
fi

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
          # A cuda-* dir that is NOT the chosen tree never reaches PATH. Without this the glob
          # puts every major on PATH and the linker picks by position — the whole bug.
          case "$_rp_hit" in
            *"/cuda-"*)
              if [ -n "$_wbe_cuda_tree" ] && [ "${_rp_hit#"$_wbe_cuda_tree"}" = "$_rp_hit" ]; then
                continue
              fi
              ;;
          esac
          case ":$PATH:" in *":$_rp_hit:"*) ;; *) export PATH="$_rp_hit:$PATH" ;; esac
        done
      done
    done
  fi
fi

# ── Windows: pin cmake + ninja + the generator ──────────────────────────────────────────────
# UNCONDITIONAL on Windows, and deliberately so. This block used to live nested inside the
# CUDA/MSVC import below, behind `command -v nvcc && ! command -v cl.exe`. cmake is a
# core/llama concern, NOT a CUDA one: every cargo build that touches core/llama runs the cmake
# crate, CUDA or not. So on a box where nvcc was not (yet) on PATH — or in a shell that had
# already imported cl.exe — the whole block skipped, CMAKE/CMAKE_GENERATOR never got set, and
# the build died TWELVE MINUTES LATER inside llama's build.rs with "is `cmake` not installed?"
# Measured tonight: cmake IS installed; the guard for a different tool had silently disowned it.
# A precondition for X must not be gated on the presence of Y.
if [ "$_mf_os" = windows ]; then
  # The cmake crate (core/llama build.rs) resolves cmake via the CMAKE env var or PATH, but a
  # manifest-provisioned cmake lives at ~/.continuum/tools/cmake/bin and is not guaranteed on a
  # clean shell's PATH (measured: absent in a clean subshell → "cmake not found"). Point CMAKE at
  # the known install (same resolution as install-llama-server.sh) and put its dir on PATH for
  # cmake's own sub-tools.
  _ccmk="$(command -v cmake 2>/dev/null || echo "${CONTINUUM_HOME:-$HOME/.continuum}/tools/cmake/bin/cmake.exe")"
  if [ -x "$_ccmk" ]; then
    export CMAKE="$(cygpath -w "$_ccmk" 2>/dev/null || echo "$_ccmk")"
    case ":$PATH:" in *":$(dirname "$_ccmk"):"*) ;; *) PATH="$(dirname "$_ccmk"):$PATH"; export PATH ;; esac
  fi
  # Force a generator cmake actually knows. The cmake crate auto-picks the newest installed VS; on
  # a VS18-2026 box that is "Visual Studio 18 2026" — a generator cmake 3.30.x does NOT define
  # ("Could not create named generator"). Ninja is version-agnostic, uses the MSVC env imported
  # below, and matches the llama-server build. [[windows-build-env-drift]]
  _cninja="$(command -v ninja 2>/dev/null || echo "${CONTINUUM_HOME:-$HOME/.continuum}/tools/ninja/ninja.exe")"
  if [ -x "$_cninja" ]; then
    export CMAKE_GENERATOR="Ninja"
    # ninja must be ON PATH: the cmake crate ignores CMAKE_MAKE_PROGRAM env, so with -G Ninja it
    # searches PATH ("unable to find Ninja / CMAKE_MAKE_PROGRAM is not set" otherwise).
    case ":$PATH:" in *":$(dirname "$_cninja"):"*) ;; *) PATH="$(dirname "$_cninja"):$PATH"; export PATH ;; esac
  fi
fi

# ── Windows: import the MSVC toolchain (cl.exe + INCLUDE/LIB) ───────────────────────────────
# Needed by BOTH consumers, which is why the guard is presence-of-cl.exe and nothing else:
#   - nvcc (candle's CUDA kernels) uses cl.exe as its host compiler;
#   - the Ninja generator pinned above needs a C/C++ compiler on PATH for ANY llama build.
# The guard used to also require `command -v nvcc`, which meant a Windows box without CUDA got
# no cl.exe and its Ninja build had no compiler at all — CUDA's absence disabling the non-CUDA
# path. The cargo builds run in THIS bash shell (unlike the llama-server cmake build, which runs
# inside a vcvars .bat), so without this the build dies with "Cannot find compiler 'cl.exe'".
# Import once: export INCLUDE/LIB verbatim (only cl.exe reads them) and prepend the EXACT MSVC +
# Windows SDK bin dirs (from vcvars, converted to unix) to PATH — bash's own PATH resolution
# stays intact (we add unix dirs, never overwrite PATH with the Windows one). VS2022 (14.4x) is
# selected explicitly: nvcc 12.x rejects the newer 14.5x/VS18 toolset.
if [ "$_mf_os" = windows ] && ! command -v cl.exe >/dev/null 2>&1; then
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
    # CUDA_PATH must be set: cudarc/candle/pocket-tts read it to emit their link-search; without it the
    # link has NO CUDA search path (measured: LNK1181 cuda.lib). Point it at a cuda-* whose import-lib
    # dir actually has the libs (a provisioning split can leave the crate-detected dir EMPTY - cuda-env
    # /Library/lib/x64=0 vs cuda-13.2=12; the #6 provisioning fix unifies them, and this node's cuda-env
    # was completed by copying the sibling's libs in). candle finds nvcc via PATH independently, so a
    # libs-only CUDA_PATH is fine (build proven). Real fix: provision ONE complete toolkit (#6).
    # Use THE tree chosen above — never a second, independent scan. The old loop listed
    # `cuda-env` first and took the first linkable hit, so CUDA_PATH named a CUDA 12 tree while
    # PATH (globbed, unordered) handed the linker CUDA 13. One selection, one answer: if these
    # two ever disagree again it is a bug in the chooser, not a race between two searches.
    if [ -z "$CUDA_PATH" ]; then
      for _cand in ${_wbe_cuda_tree:+"$_wbe_cuda_tree"}; do
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
      # Only a CUDA build can be hurt by a missing CUDA_PATH. Warning unconditionally would cry
      # wolf on every CPU-only Windows box now that this block is no longer behind an nvcc guard.
      if [ -z "$CUDA_PATH" ] && command -v nvcc >/dev/null 2>&1; then
        echo "⚠ nvcc present but no complete CUDA import-lib dir found (cuda-*/**/{cuda,curand}.lib) — the CUDA core link WILL fail. Provisioning gap (#6)." >&2
      fi
    fi
    # CONSISTENCY ASSERTION, replacing the warning this used to print.
    #
    # The old block DETECTED that several majors were provisioned and told the operator to go
    # check the binary with dumpbin. That was honest and useless: it named the hazard and left
    # the next build binding by search order anyway. Now that exactly one tree reaches PATH,
    # the invariant is checkable directly — PATH must offer the SAME major CUDA_PATH declares.
    #
    # If these disagree, something re-ordered PATH after this file ran (a caller's own export,
    # a stale shell, a second toolchain from an installer), and that is the precise condition
    # that produced "cublas64_12.dll was not found" for a binary that actually wanted 13. Fail
    # loud HERE, where it is one line to read, instead of at load time on someone else's box.
    if [ -n "$CUDA_PATH" ] && [ -n "$_wbe_cuda_major" ]; then
      _path_major=""
      _IFS_SAVE="$IFS"; IFS=':'
      for _pd in $PATH; do
        for _c in "$_pd"/cublas64_*.dll; do
          [ -f "$_c" ] || continue
          _path_major="$(basename "$_c" | sed -n 's/^cublas64_\([0-9]\+\)\.dll$/\1/p')"
          break 2
        done
      done
      IFS="$_IFS_SAVE"
      if [ -n "$_path_major" ] && [ "$_path_major" != "$_wbe_cuda_major" ]; then
        echo "✗ CUDA MAJOR MISMATCH: declared cuda-$_wbe_cuda_major ($(basename "$_wbe_cuda_tree")) but PATH offers cublas64_$_path_major first." >&2
        echo "  The linker binds what PATH offers, so the binary would import cuda-$_path_major while" >&2
        echo "  CUDA_PATH promises cuda-$_wbe_cuda_major — the exact split that surfaces later as" >&2
        echo "  'cublas64_XX.dll was not found' naming a major the binary does not use." >&2
        echo "  Something re-ordered PATH after windows-build-env.sh ran. Fix that, do not build." >&2
        (return 0 2>/dev/null) && return 1 || exit 1
      fi
      unset _path_major _pd _c _IFS_SAVE
    fi
    if command -v cl.exe >/dev/null 2>&1; then
      echo "▶ MSVC toolchain imported (cl.exe on PATH for ninja/nvcc/candle)"
    else
      echo "⚠ MSVC import ran but cl.exe still unresolved — the cargo build will fail" >&2
    fi
  else
    echo "✗ Windows needs the VS2022 (14.4x) C++ x64 toolset — it is ninja's compiler and nvcc's host compiler; vswhere/vcvars not found, so the core build will fail. Install via the 'msvc' module." >&2
  fi
fi

# ── Postcondition: the environment either IS usable or says exactly why not ──────────────────
# The whole point of this file is that `source it, then cargo` works. Until now it could complete
# with cmake unresolvable and print nothing, so the first sign of trouble was a build.rs panic
# twelve minutes and several hundred log lines downstream — "is `cmake` not installed?" when
# cmake was installed the whole time. That is the silently-unwired shape: the work ran, the
# outcome was never checked, and the failure surfaced somewhere that named the wrong cause.
#
# Verify what the next cargo invocation actually needs, name each missing piece at the seam, and
# return non-zero so a caller that checks `$?` gets a signal. Sourced (the documented usage), a
# bare `return` sets $? without killing the caller's shell; guard it so an accidental direct
# execution still exits cleanly instead of erroring on `return` outside a function.
if [ "$_mf_os" = windows ]; then
  _wbe_missing=""
  command -v cmake  >/dev/null 2>&1 || _wbe_missing="$_wbe_missing cmake"
  command -v cl.exe >/dev/null 2>&1 || _wbe_missing="$_wbe_missing cl.exe"
  [ -n "$CMAKE_GENERATOR" ]         || _wbe_missing="$_wbe_missing CMAKE_GENERATOR(ninja)"
  if [ -n "$_wbe_missing" ]; then
    echo "✗ windows-build-env: environment INCOMPLETE — missing:$_wbe_missing" >&2
    echo "  cargo will fail later with a misleading error. Run the 'cmake'/'ninja'/'msvc' install modules." >&2
    unset _wbe_missing
    (return 0 2>/dev/null) && return 1 || exit 1
  fi
  unset _wbe_missing
fi
