#!/usr/bin/env bash
# Regression: warm deploy must not reap a live core, engine, or AIRC daemon.
# Exercise the actual launcher in a scratch repo; no native builds or services.
#
# Also (card 9174fc83): A DEPLOY COMPILES THE LIBRARY ONCE. The launcher builds
# every bin of the crate on ONE cargo command line with ONE feature set. Four
# per-bin `cargo build` lines cost the IntelMac four "Compiling continuum-core"
# per deploy (18m16s + 5m41s + 5m14s + 7m46s = 37 min; 25 min on the M5) with the
# core dark for all of it under the stop-first path. The checks below pin it from
# both sides: the script text carries exactly one literal invocation, and the
# trace of a run shows one cargo line naming all four bins.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch_root="$(cd "${TMPDIR:-/tmp}" && pwd -P)"
scratch="$(mktemp -d "$scratch_root/continuum-warm-build.XXXXXX")"
scratch="$(cd "$scratch" && pwd -P)"
case "$scratch" in "$scratch_root"/continuum-warm-build.*) ;; *) echo "Unsafe scratch path" >&2; exit 1 ;; esac
fixture_home="$scratch/home"
trap 'status=$?; if [ "$status" != 0 ]; then cat "$scratch/output" "$scratch/trace" >&2; fi; rm -rf "$scratch"' EXIT
mkdir -p "$scratch/repo/tools/scripts/lib" "$scratch/repo/tools/scripts/shared" "$scratch/repo/core/continuum-core/src" "$scratch/home/.cargo/bin"
cp "$script_dir/../start-server.sh" "$scratch/repo/tools/scripts/start-server.sh"
# Toolchain setup is orthogonal to lifecycle; isolate it from the host machine.
printf ':\n' > "$scratch/repo/tools/scripts/lib/windows-build-env.sh"
printf 'CARGO_GPU_FEATURES=--no-default-features\n' > "$scratch/repo/tools/scripts/shared/cargo-features.sh"
printf '// fixture source\n' > "$scratch/repo/core/continuum-core/src/lib.rs"
export CARGO_TARGET_DIR="$scratch/build cache 雪"
export CONTINUUM_BUILD_RECEIPT="$scratch/artifact.receipt"
# The real Windows CLI passes a native path in this environment variable.
if command -v cygpath >/dev/null 2>&1; then
  CONTINUUM_BUILD_RECEIPT="$(cygpath -am "$CONTINUUM_BUILD_RECEIPT")"
fi
export FIXTURE_TRACE="$scratch/trace" CONTINUUM_BUILD_ONLY=1
export CONTINUUM_SKIP_SELF_BUILD=1 CONTINUUM_TRACK_BRANCH=canary
unset CONTINUUM_DEBUG CONTINUUM_RELEASE BASH_ENV
cat > "$fixture_home/.cargo/bin/cargo" <<'SH'
#!/usr/bin/env bash
set -eu
printf 'cargo %s\n' "$*" >> "$FIXTURE_TRACE"
# One command line names every bin (`--bin a --bin b …`), like real cargo.
bins=()
while [ $# -gt 0 ]; do
  if [ "$1" = --bin ]; then bins+=("$2"); shift; fi
  shift
done
[ "${#bins[@]}" -gt 0 ] || { echo 'fixture: cargo build without --bin' >&2; exit 2; }
for bin in "${bins[@]}"; do
  if [ "$bin" = continuum-core-server ] && [ "${FAIL_CORE_BUILD:-0}" = 1 ]; then
    echo 'fixture: mapped output Access is denied (os error 5)' >&2
    exit 37
  fi
done
mkdir -p "$CARGO_TARGET_DIR/release"
for bin in "${bins[@]}"; do
  if [ "$bin" = continuum-core-server ] && [ "${FIXTURE_PLATFORM:-}" = MINGW64_NT-10.0 ]; then bin="$bin.exe"; fi
  printf '#!/usr/bin/env bash\necho LAUNCHED >> "$FIXTURE_TRACE"\nexit 99\n' > "$CARGO_TARGET_DIR/release/$bin"
  chmod +x "$CARGO_TARGET_DIR/release/$bin"
done
SH
chmod +x "$fixture_home/.cargo/bin/cargo"
# Even a suppressed command failure must leave evidence. Never call real tools.
cat > "$scratch/forbidden" <<'SH'
#!/usr/bin/env bash
echo "FORBIDDEN $0 $*" >> "$FIXTURE_TRACE"
exit 99
SH
chmod +x "$scratch/forbidden"
for tool in taskkill tasklist pkill pgrep airc llama-server curl powershell.exe; do
  cp "$scratch/forbidden" "$fixture_home/.cargo/bin/$tool"
done
for tool in install-llama-server.sh track-canary.sh; do
  cp "$scratch/forbidden" "$scratch/repo/tools/scripts/$tool"
done
# Builtin kill cannot be shadowed with a PATH entry.
kill() { echo "FORBIDDEN kill $*" >> "$FIXTURE_TRACE"; return 99; }
export -f kill
# Linux CI simulates Windows control flow; Windows CI uses real native conversion.
if ! command -v cygpath >/dev/null 2>&1; then
  printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$2"\n' > "$fixture_home/.cargo/bin/cygpath"
  chmod +x "$fixture_home/.cargo/bin/cygpath"
fi
for platform in MINGW64_NT-10.0 Linux Darwin; do
  printf '#!/usr/bin/env bash\necho %s\n' "$platform" > "$fixture_home/.cargo/bin/uname"
  chmod +x "$fixture_home/.cargo/bin/uname"
  for failure in 0 1; do
    : > "$FIXTURE_TRACE"
    : > "$CONTINUUM_BUILD_RECEIPT"
    status=0
    HOME="$fixture_home" FIXTURE_PLATFORM="$platform" FAIL_CORE_BUILD="$failure" bash "$scratch/repo/tools/scripts/start-server.sh" > "$scratch/output" 2>&1 || status=$?
    if grep -Eq 'FORBIDDEN|LAUNCHED' "$FIXTURE_TRACE"; then
      cat "$FIXTURE_TRACE" >&2; exit 1
    fi
    grep -q -- '--bin continuum-core-server' "$FIXTURE_TRACE"
    if [ "$failure" = 0 ]; then
      [ "$status" = 0 ]
      grep -q 'warm build complete' "$scratch/output"
      artifact="$CARGO_TARGET_DIR/release/continuum-core-server"
      if [ "$platform" = MINGW64_NT-10.0 ]; then
        artifact="$artifact.exe"
        if command -v cygpath >/dev/null 2>&1; then artifact="$(cygpath -am "$artifact")"; fi
      fi
      [ "$(cat "$CONTINUUM_BUILD_RECEIPT")" = "$artifact" ]
    else
      [ "$status" != 0 ]
      grep -q 'mapped output Access is denied' "$scratch/output"
      grep -q 'leaving the running core untouched' "$scratch/output"
      [ ! -s "$CONTINUUM_BUILD_RECEIPT" ]
      if grep -q 'warm build complete' "$scratch/output"; then exit 1; fi
    fi
    # One library compile: the FIRST cargo line names the core and both sidecars
    # together (the self-build is skipped here, so the CLI is off it), and no
    # later line builds the core again on the successful path. The failing path
    # is allowed exactly one more: the core-alone diagnosis. (A later `--bin
    # continuum` line is the #296 restore of a swept CLI — the fixture cache
    # starts empty — not a fourth build.)
    core_lines="$(grep -c -- '--bin continuum-core-server' "$FIXTURE_TRACE" || true)"
    head -1 "$FIXTURE_TRACE" | grep -q -- '--bin continuum-core-server --bin continuum-mcp --bin forge-custodian --release --no-default-features$'
    if [ "$failure" = 0 ]; then [ "$core_lines" = 1 ]; else [ "$core_lines" = 2 ]; fi
    echo "PASS $platform build-only (core build failure=$failure)"
  done
done

# ── A deploy compiles the library once (card 9174fc83) ───────────────────────
# Static: the launcher spells `cargo build` against the crate manifest ONCE, inside
# build_core_bins. A second literal is the defect returning — every path (deploy,
# #296 restore, #194 forced rebuild, the GPU-free CLI arm) must call the function.
launcher="$scratch/repo/tools/scripts/start-server.sh"
literal_builds="$(grep -cE '^[^#]*cargo build --manifest-path "\$CORE_MANIFEST"' "$launcher" || true)"
if [ "$literal_builds" != 1 ]; then
  echo "start-server.sh has $literal_builds literal 'cargo build --manifest-path \"\$CORE_MANIFEST\"' lines; the law is ONE (build_core_bins)" >&2
  exit 1
fi
grep -qF 'build_core_bins "$CONTINUUM_FEATURES" $core_build_bins' "$launcher"
echo "PASS static: one literal cargo build for the crate's bins"

# Behavioural: with the CLI's self-build allowed and a feature set that links no GPU
# runtime, ALL FOUR bins ride one cargo line with the core's feature set.
printf '#!/usr/bin/env bash\necho Linux\n' > "$fixture_home/.cargo/bin/uname"
: > "$FIXTURE_TRACE"; : > "$CONTINUUM_BUILD_RECEIPT"
HOME="$fixture_home" FIXTURE_PLATFORM=Linux FAIL_CORE_BUILD=0 CONTINUUM_SKIP_SELF_BUILD= \
  bash "$launcher" > "$scratch/output" 2>&1
if grep -Eq 'FORBIDDEN|LAUNCHED' "$FIXTURE_TRACE"; then cat "$FIXTURE_TRACE" >&2; exit 1; fi
[ "$(grep -c '^cargo build ' "$FIXTURE_TRACE")" = 1 ]
grep -q -- '--bin continuum-core-server --bin continuum-mcp --bin forge-custodian --bin continuum --release --no-default-features$' "$FIXTURE_TRACE"
grep -q 'the library compiles once' "$scratch/output"
echo "PASS one cargo invocation names all four bins with one feature set"

# The one sanctioned exception, pinned to its smallest shape: a set that links a GPU
# runtime (cuda) gives the socket-client CLI its own GPU-free set — a SECOND line, the
# only one, carrying `--bin continuum` alone; the other three still share one line.
printf 'CARGO_GPU_FEATURES="--features cuda,load-dynamic-ort"\n' > "$scratch/repo/tools/scripts/shared/cargo-features.sh"
: > "$FIXTURE_TRACE"; : > "$CONTINUUM_BUILD_RECEIPT"
HOME="$fixture_home" FIXTURE_PLATFORM=Linux FAIL_CORE_BUILD=0 CONTINUUM_SKIP_SELF_BUILD= \
  bash "$launcher" > "$scratch/output" 2>&1
if grep -Eq 'FORBIDDEN|LAUNCHED' "$FIXTURE_TRACE"; then cat "$FIXTURE_TRACE" >&2; exit 1; fi
[ "$(grep -c '^cargo build ' "$FIXTURE_TRACE")" = 2 ]
grep -q -- '--bin continuum-core-server --bin continuum-mcp --bin forge-custodian --release --features cuda,load-dynamic-ort$' "$FIXTURE_TRACE"
grep -q -- '--bin continuum --release --no-default-features$' "$FIXTURE_TRACE"
echo "PASS a GPU-runtime box separates only the CLI, on exactly one extra line"
