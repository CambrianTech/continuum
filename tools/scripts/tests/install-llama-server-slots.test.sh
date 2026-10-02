#!/usr/bin/env bash
# Regression (card 7a6a033a, the installer half of #4491): a deploy installs the engine into
# the IDLE slot the core names and promotes it; it never replaces what a live lane runs.
# Exercises the real install-llama-server.sh in a scratch repo with a fake `continuum` CLI
# and a fake cmake: no native build, no service, no core.
#
# What each case catches:
#   old CLI      the first deploy is driven by a CLI without the verbs: pre-slot install, said so
#   migrate      a pre-slot engine already verified at the pin moves into a slot with NO rebuild
#   current      the current slot at the pin is a no-op: no idle-slot query, no promote
#   none idle    exit 3 skips the engine build, exits 0, promotes nothing
#   refused      any other idle-slot failure fails loud, never guesses a slot
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch_root="$(cd "${TMPDIR:-/tmp}" && pwd -P)"
scratch="$(mktemp -d "$scratch_root/continuum-engine-slots.XXXXXX")"
case "$scratch" in "$scratch_root"/continuum-engine-slots.*) ;; *) echo "Unsafe scratch path" >&2; exit 1 ;; esac
trap 'status=$?; if [ "$status" != 0 ]; then cat "$scratch/err" >&2 2>/dev/null || true; fi; rm -rf "$scratch"' EXIT

case "$(uname -s)" in Darwin|Linux) ;; *) echo "skip: the slot path is macOS/Linux (Windows has its PowerShell arm)"; exit 0 ;; esac

repo="$scratch/repo"
mkdir -p "$repo/tools/scripts" "$repo/core/vendor/llama.cpp/tools/server" "$scratch/bin"
cp "$script_dir/../install-llama-server.sh" "$repo/tools/scripts/"
mkdir -p "$repo/tools/scripts/lib"
cp "$script_dir/../lib/payload-paths.sh" "$repo/tools/scripts/lib/"
sub="$repo/core/vendor/llama.cpp"
printf '# fixture\n' > "$sub/tools/server/CMakeLists.txt"
git -C "$sub" init -q && git -C "$sub" -c user.email=t@t -c user.name=t add -A \
  && git -C "$sub" -c user.email=t@t -c user.name=t commit -qm fixture
head="$(git -C "$sub" rev-parse --short HEAD)"
backend=cpu
if [ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ]; then backend=metal
elif [ "$(uname -s)" = Linux ] && command -v nvcc >/dev/null 2>&1; then backend=cuda; fi
want="$head:$backend"

# Fake cmake: the slot cases must never reach a build; if one does, it fails loud.
printf '#!/usr/bin/env bash\necho "cmake must not run in this case" >&2\nexit 97\n' > "$scratch/bin/cmake"
# Fake CLI: `--help` advertises the verbs unless OLD_CLI; `engine` answers from the fixture.
cat > "$scratch/bin/continuum" <<'SH'
#!/usr/bin/env bash
if [ "${1:-}" = "--help" ]; then
  echo "usage: continuum ..."
  [ -z "${OLD_CLI:-}" ] && echo "       continuum engine idle-slot           print the engine slot"
  exit 0
fi
[ "${1:-}" = "engine" ] || exit 64
shift
source "$FIXTURE_PAYLOAD_LIB"
root="$(managed_payload_root "$CONTINUUM_HOME")/bin" || exit 1
echo "$*" >> "$FIXTURE_LOG"
case "$1" in
  idle-slot) [ "$IDLE_RC" = 0 ] && echo "$root/$IDLE_SLOT"; exit "$IDLE_RC" ;;
  promote) printf '%s\n' "$2" > "$root/current"; exit 0 ;;
esac
exit 64
SH
chmod +x "$scratch/bin/cmake" "$scratch/bin/continuum"
export PATH="$scratch/bin:$PATH" CONTINUUM_CLI="$scratch/bin/continuum" FIXTURE_LOG="$scratch/log"
export FIXTURE_PAYLOAD_LIB="$repo/tools/scripts/lib/payload-paths.sh"

fresh_home() {
  rm -rf "$scratch/home" "$scratch/log"; : > "$scratch/log"
  export HOME="$scratch/home" CONTINUUM_HOME="$scratch/home/.continuum"
  mkdir -p "$CONTINUUM_HOME/bin"
}
engine_at() { # dir stamp: a runnable engine stamped at `stamp`
  mkdir -p "$1"; printf '#!/bin/sh\nexit 0\n' > "$1/llama-server"; chmod +x "$1/llama-server"
  printf '%s\n' "$2" > "$1/.llama-server.stamp"
}
run() { bash "$repo/tools/scripts/install-llama-server.sh" > "$scratch/out" 2> "$scratch/err"; }
fail() { echo "FAIL: $*" >&2; exit 1; }

# old CLI: the pre-slot install stands, and the log says why.
fresh_home; engine_at "$CONTINUUM_HOME/bin" "$want"
OLD_CLI=1 IDLE_RC=0 IDLE_SLOT=engine-a run || fail "old CLI: exit $?"
[ "$(cat "$scratch/out")" = "$CONTINUUM_HOME/bin/llama-server" ] || fail "old CLI: $(cat "$scratch/out")"
grep -q "no engine slots yet" "$scratch/err" || fail "old CLI: the skew is not said"
[ ! -s "$scratch/log" ] || fail "old CLI: engine verbs were called"

# migrate: the verified pre-slot engine is copied into the idle slot and promoted, no build.
fresh_home; engine_at "$CONTINUUM_HOME/bin" "$want"
IDLE_RC=0 IDLE_SLOT=engine-a run || fail "migrate: exit $?"
[ "$(cat "$scratch/out")" = "$CONTINUUM_HOME/bin/engine-a/llama-server" ] || fail "migrate: $(cat "$scratch/out")"
[ "$(cat "$CONTINUUM_HOME/bin/engine-a/.llama-server.stamp")" = "$want" ] || fail "migrate: slot not stamped"
grep -qx "promote engine-a $want" "$scratch/log" || fail "migrate: not promoted with the pin"
[ -x "$CONTINUUM_HOME/bin/llama-server" ] || fail "migrate: the pre-slot engine a lane may run was removed"

# current: the active slot already at the pin is a no-op.
fresh_home; engine_at "$CONTINUUM_HOME/bin/engine-b" "$want"; echo engine-b > "$CONTINUUM_HOME/bin/current"
IDLE_RC=0 IDLE_SLOT=engine-a run || fail "current: exit $?"
[ "$(cat "$scratch/out")" = "$CONTINUUM_HOME/bin/engine-b/llama-server" ] || fail "current: $(cat "$scratch/out")"
[ ! -s "$scratch/log" ] || fail "current: queried or promoted: $(cat "$scratch/log")"

# already built: a non-current slot holding the pin is promoted with no build and no idle-slot
# question, even when every slot is busy (card 6d5bacab: the 5090's cancelled install).
fresh_home; engine_at "$CONTINUUM_HOME/bin/engine-b" "old000:$backend"; echo engine-b > "$CONTINUUM_HOME/bin/current"
engine_at "$CONTINUUM_HOME/bin/engine-c" "$want"
IDLE_RC=3 IDLE_SLOT= run || fail "already built: exit $?"
[ "$(cat "$scratch/out")" = "$CONTINUUM_HOME/bin/engine-c/llama-server" ] || fail "already built: $(cat "$scratch/out")"
grep -qx "promote engine-c $want" "$scratch/log" || fail "already built: not promoted"
! grep -q "idle-slot" "$scratch/log" || fail "already built: asked for an idle slot it did not need"

# none idle: exit 3 skips the build this deploy, exits 0, promotes nothing.
fresh_home; engine_at "$CONTINUUM_HOME/bin/engine-b" "old000:$backend"; echo engine-b > "$CONTINUUM_HOME/bin/current"
IDLE_RC=3 IDLE_SLOT= run || fail "none idle: exit $?"
grep -q "engine build is skipped" "$scratch/err" || fail "none idle: the skip is not said"
! grep -q promote "$scratch/log" || fail "none idle: promoted"

# refused: any other idle-slot failure is FATAL, nothing built or promoted.
fresh_home; engine_at "$CONTINUUM_HOME/bin/engine-b" "old000:$backend"; echo engine-b > "$CONTINUUM_HOME/bin/current"
if IDLE_RC=1 IDLE_SLOT= run; then fail "refused: exited 0"; fi
grep -q "no slot can be proven idle" "$scratch/err" || fail "refused: the cause is not named"
! grep -q promote "$scratch/log" || fail "refused: promoted"

# A recorded cold payload owns the active engine, even with a stale legacy bin.
fresh_home
cold="$scratch/cold payload"
engine_at "$cold/bin/engine-b" "$want"
printf '%s\n' "$cold" > "$CONTINUUM_HOME/payload-root"
echo engine-b > "$cold/bin/current"
IDLE_RC=0 IDLE_SLOT=engine-a run || fail "cold current: exit $?"
[ "$(cat "$scratch/out")" = "$cold/bin/engine-b/llama-server" ] || fail "cold current: wrong engine"
[ ! -s "$scratch/log" ] || fail "cold current: rebuilt or promoted unnecessarily"

echo "install-llama-server slots: legacy and cold cases pass"
