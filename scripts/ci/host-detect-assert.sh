#!/usr/bin/env bash
# host-detect-assert.sh — assert `ic_detect_hardware` tells the truth on THIS host.
#
# WHY THIS EXISTS (card b2e4a0bb, 2026-09-05). The only install check we run is
# `carl-install-smoke`, and it is `runs-on: ubuntu-latest` with no matrix. So
# the install path is verified on exactly one platform, and every bug found on
# the other two in one night was invisible to it:
#
#   - `wmic` was REMOVED in Windows 11 24H2, so the RAM arm fell to its else
#     branch and recorded IC_RAM_MIB=0 on a machine with 64914 MiB. Every
#     downstream tier decision then ran on that zero (fixed in #3739).
#   - the Intel-Mac arm builds llama-server CPU-only because a Metal build hangs
#     that hardware uninterruptibly, and nothing recorded that as the host's
#     serving PLAN until #3740 — so the backend receipt refused the node and
#     advised reinstalling with Metal, which would have hung it again.
#   - the Linux/WSL RAM arm still turns an unreadable /proc/meminfo into 0 with
#     no warning, which is the same defect as the Windows one on a platform
#     nobody re-checked.
#
# Every one of those is a `detect_host` fact being WRONG rather than absent, on
# a platform the author could not run. That is what a matrix is for, and it does
# NOT need Docker: `install-common.sh` is sourceable and `ic_detect_hardware`
# is a pure probe of the machine it runs on.
#
# The assertions are deliberately about SHAPE, not about specific hardware — a
# CI runner is not any of our boxes, and a test that expects an M5 or a 5090
# fails for the wrong reason. What must hold on every host is that the detector
# produced a determinate answer rather than a silent placeholder.

set -e
set -o pipefail  # a failing command in a pipeline must not read as success (card aad30dee)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# shellcheck source=/dev/null
. "$REPO_ROOT/tools/scripts/lib/install-common.sh"

fail=0
note() { printf '  %s\n' "$*"; }
check() {
  local what="$1" ok="$2" detail="$3"
  if [ "$ok" = "1" ]; then
    printf 'ok   %s — %s\n' "$what" "$detail"
  else
    printf 'FAIL %s — %s\n' "$what" "$detail"
    fail=1
  fi
}

echo "host-detect-assert on $(uname -s) $(uname -m)"
ic_detect_hardware

echo ""
echo "detected:"
note "IC_PLATFORM = ${IC_PLATFORM:-<unset>}"
note "IC_ARCH     = ${IC_ARCH:-<unset>}"
note "IC_RAM_MIB  = ${IC_RAM_MIB:-<unset>}"
note "IC_RAM_GB   = ${IC_RAM_GB:-<unset>}"
note "IC_GPU_KIND = ${IC_GPU_KIND:-<unset>}"
note "IC_GPU_NAME = ${IC_GPU_NAME:-<unset>}"
echo ""

# 1. PLATFORM must be one of the known values. An unrecognised host falling
#    through to an empty string is how the `*)` RAM arm gets reached, and that
#    arm records 0 MiB by construction.
case "${IC_PLATFORM:-}" in
  macos|linux|wsl|windows) check "platform" 1 "recognised as ${IC_PLATFORM}" ;;
  *) check "platform" 0 "unrecognised or unset (${IC_PLATFORM:-<unset>}) — the catch-all arms record placeholders" ;;
esac

# 2. RAM must be a POSITIVE integer. This is the wmic bug's assertion, and the
#    one that would have caught it on the box it broke: a real machine always
#    has RAM, so 0 means "we could not look" wearing the clothes of "there is
#    none". Empty is the Linux failure shape; 0 is the Windows one.
if [ -n "${IC_RAM_MIB:-}" ] && [ "${IC_RAM_MIB}" -gt 0 ] 2>/dev/null; then
  check "ram" 1 "${IC_RAM_MIB} MiB (${IC_RAM_GB} GB)"
else
  check "ram" 0 "IC_RAM_MIB=${IC_RAM_MIB:-<empty>} — a host with no readable RAM figure will be mis-tiered, silently"
fi

# 3. ARCH must be determinate for the same reason.
case "${IC_ARCH:-}" in
  arm64|x86_64|other) check "arch" 1 "${IC_ARCH}" ;;
  *) check "arch" 0 "unset or unexpected (${IC_ARCH:-<unset>})" ;;
esac

# 4. GPU KIND must be one of the declared values INCLUDING `none`. `none` is a
#    real answer on a CI runner and must not be confused with the empty string:
#    "no GPU" and "we did not look" are different facts, and conflating them is
#    what made the backend receipt refuse a working CPU-by-plan host.
case "${IC_GPU_KIND:-}" in
  metal|cuda|vulkan|rocm|none) check "gpu_kind" 1 "${IC_GPU_KIND}" ;;
  *) check "gpu_kind" 0 "unset or unexpected (${IC_GPU_KIND:-<unset>}) — 'none' is the answer for no GPU, empty is not" ;;
esac

# 5. THE PLACEMENT KEY, which is the whole reason this tier's fix is not just a
#    repair of one machine (#3740). install-llama-server.sh records
#    CONTINUUM_SERVING_PLACEMENT from the backend it decided, so a CPU-only host
#    serves by PLAN instead of being refused as a broken GPU host. The value is
#    only derivable where that script has run, so this leg asserts the DECISION
#    FUNCTION rather than the file — the wire is covered by the install smoke.
if declare -f ic_serving_placement >/dev/null 2>&1; then
  placement="$(ic_serving_placement)"
  case "$placement" in
    cpu|gpu) check "placement" 1 "would record ${placement}" ;;
    *) check "placement" 0 "ic_serving_placement returned ${placement:-<empty>}" ;;
  esac
else
  printf 'skip placement — ic_serving_placement not present in install-common.sh on this ref\n'
fi

echo ""
if [ "$fail" = "0" ]; then
  echo "host-detect-assert: all checks passed on ${IC_PLATFORM}"
else
  echo "host-detect-assert: FAILED on ${IC_PLATFORM} — see above"
fi
exit "$fail"
