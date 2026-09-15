#!/usr/bin/env bash
# Regression: warm deploy must not reap a live core, engine, or AIRC daemon.
# Exercise the actual launcher in a scratch repo; no native builds or services.
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
while [ "$1" != --bin ]; do shift; done
bin="$2"
if [ "$bin" = continuum-core-server ] && [ "${FAIL_CORE_BUILD:-0}" = 1 ]; then
  echo 'fixture: mapped output Access is denied (os error 5)' >&2
  exit 37
fi
mkdir -p "$CARGO_TARGET_DIR/release"
if [ "$bin" = continuum-core-server ] && [ "${FIXTURE_PLATFORM:-}" = MINGW64_NT-10.0 ]; then bin="$bin.exe"; fi
printf '#!/usr/bin/env bash\necho LAUNCHED >> "$FIXTURE_TRACE"\nexit 99\n' > "$CARGO_TARGET_DIR/release/$bin"
chmod +x "$CARGO_TARGET_DIR/release/$bin"
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
    echo "PASS $platform build-only (core build failure=$failure)"
  done
done
