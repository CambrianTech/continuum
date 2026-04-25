#!/usr/bin/env bash
# carl-install-smoke.sh — run the EXACT install command Carl runs, then
# assert the user-facing surface actually serves usable content.
#
# Why this gate: existing install-and-run-gate.sh validates the docker
# compose stack itself (images present, services healthy on :9003). It does
# NOT validate that `curl install.sh | bash` — Carl's actual entry point —
# completes cleanly, or that the page Carl opens after install renders
# something usable instead of chrome-error / empty.
#
# This gate closes that gap. Same one-line invocation works for CI and
# humans (per Joel's "make your own testing easy" rule):
#
#   bash scripts/ci/carl-install-smoke.sh
#
# Optional env:
#   CARL_INSTALL_TIMEOUT_SEC=900    full install timeout (default 15min)
#   CARL_HEALTH_TIMEOUT_SEC=180     widget-server /health wait (default 3min)
#   CARL_INSTALL_DIR=/tmp/carl-N    install location (default fresh tmp)
#   CARL_INSTALL_REF=$GIT_SHA       which install.sh to fetch from main
#   SKIP_TEARDOWN=1                 keep stack running after probe (debug)
#
# Exit codes:
#   0 — install completed AND page rendered usable HTML
#   1 — install.sh failed
#   2 — install.sh succeeded but widget-server never returned 200 on /health
#   3 — widget-server returned 200 but page body looks broken
#       (empty / contains chrome-error / contains "container exited")

set -uo pipefail

CARL_INSTALL_TIMEOUT_SEC="${CARL_INSTALL_TIMEOUT_SEC:-900}"
CARL_HEALTH_TIMEOUT_SEC="${CARL_HEALTH_TIMEOUT_SEC:-180}"
CARL_INSTALL_DIR="${CARL_INSTALL_DIR:-/tmp/carl-smoke-$$}"
CARL_INSTALL_REF="${CARL_INSTALL_REF:-${GITHUB_SHA:-main}}"
SKIP_TEARDOWN="${SKIP_TEARDOWN:-0}"

INSTALL_LOG="${CARL_INSTALL_DIR}.install.log"
PAGE_BODY="${CARL_INSTALL_DIR}.page.html"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  carl-install-smoke"
echo "  CARL_INSTALL_DIR=$CARL_INSTALL_DIR"
echo "  CARL_INSTALL_REF=$CARL_INSTALL_REF"
echo "  CARL_INSTALL_TIMEOUT_SEC=$CARL_INSTALL_TIMEOUT_SEC"
echo "  CARL_HEALTH_TIMEOUT_SEC=$CARL_HEALTH_TIMEOUT_SEC"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

teardown() {
  local rc=$?
  if [ "$SKIP_TEARDOWN" != "1" ] && [ -d "$CARL_INSTALL_DIR" ]; then
    echo ""
    echo "━━━ tearing down $CARL_INSTALL_DIR ━━━"
    if [ -f "$CARL_INSTALL_DIR/docker-compose.yml" ]; then
      ( cd "$CARL_INSTALL_DIR" && docker compose down -v 2>&1 | tail -3 ) || true
    fi
    rm -rf "$CARL_INSTALL_DIR"
  fi
  exit "$rc"
}
trap teardown EXIT INT TERM

# ── 1. Run Carl's exact install command ───────────────────────
echo ""
echo "━━━ running install.sh from $CARL_INSTALL_REF ━━━"
echo "  log: $INSTALL_LOG"

# Carl runs: curl -fsSL <install.sh> | bash
# We do the same, but pin to the exact ref under test (defaults to GITHUB_SHA
# in CI so we exercise THIS PR's install script, not main's).
INSTALL_URL="https://raw.githubusercontent.com/CambrianTech/continuum/${CARL_INSTALL_REF}/install.sh"

# Time the install. 15-min timeout for the docker-only path (Carl's expected
# experience). Hybrid Mac path (with Rust source build) will exceed this on
# a fresh runner — that's fine, it'll fail the gate, which is the design
# (the README claims docker-only; install should match).
INSTALL_START=$(date +%s)
if ! timeout "$CARL_INSTALL_TIMEOUT_SEC" bash -c \
     "CONTINUUM_DIR='$CARL_INSTALL_DIR' bash <(curl -fsSL '$INSTALL_URL')" \
     >"$INSTALL_LOG" 2>&1; then
  INSTALL_DUR=$(( $(date +%s) - INSTALL_START ))
  echo "❌ install.sh failed or timed out after ${INSTALL_DUR}s"
  echo ""
  echo "  Last 50 lines of install log:"
  tail -50 "$INSTALL_LOG" | sed 's/^/    /'
  exit 1
fi
INSTALL_DUR=$(( $(date +%s) - INSTALL_START ))
echo "✅ install.sh completed in ${INSTALL_DUR}s"

# ── 2. Wait for widget-server /health ─────────────────────────
# install.sh has its own health-wait now (piece E in this PR), but we
# re-check here in case the user used SKIP_HEALTH=1 or ran an older
# install.sh without the wait. Belt + suspenders.
echo ""
echo "━━━ waiting up to ${CARL_HEALTH_TIMEOUT_SEC}s for widget-server /health ━━━"
HEALTH_OK=0
for i in $(seq 1 "$CARL_HEALTH_TIMEOUT_SEC"); do
  if curl -sf --max-time 2 http://localhost:9003/health >/dev/null 2>&1; then
    HEALTH_OK=1
    echo "  /health 200 after ${i}s"
    break
  fi
  sleep 1
done

if [ "$HEALTH_OK" -ne 1 ]; then
  echo "❌ widget-server never returned 200 on /health within ${CARL_HEALTH_TIMEOUT_SEC}s"
  echo ""
  if [ -f "$CARL_INSTALL_DIR/docker-compose.yml" ]; then
    echo "  docker compose ps:"
    ( cd "$CARL_INSTALL_DIR" && docker compose ps 2>&1 | sed 's/^/    /' ) || true
    echo ""
    echo "  Last 30 lines of widget-server logs:"
    ( cd "$CARL_INSTALL_DIR" && docker compose logs --tail=30 widget-server 2>&1 | sed 's/^/    /' ) || true
  fi
  exit 2
fi

# ── 3. Validate the page Carl will open ───────────────────────
# /health says "server is alive" but doesn't say "the page Carl opens
# renders usable HTML." A naked health endpoint can return 200 while the
# main page returns a stack trace or empty body. Probe the actual root.
echo ""
echo "━━━ probing root page Carl opens (http://localhost:9003/) ━━━"
ROOT_CODE=$(curl -sS -o "$PAGE_BODY" -w "%{http_code}" http://localhost:9003/ 2>/dev/null || echo "000")
ROOT_BYTES=$(wc -c < "$PAGE_BODY" 2>/dev/null || echo 0)
echo "  HTTP status: $ROOT_CODE"
echo "  Body bytes:  $ROOT_BYTES"

if [[ ! "$ROOT_CODE" =~ ^2 ]]; then
  echo "❌ root page returned non-2xx ($ROOT_CODE)"
  exit 3
fi

if [ "$ROOT_BYTES" -lt 100 ]; then
  echo "❌ root page body is suspiciously small ($ROOT_BYTES bytes); Carl would see a blank page."
  echo "  First 500 bytes:"
  head -c 500 "$PAGE_BODY" | sed 's/^/    /'
  exit 3
fi

# Sanity: page should look like HTML, not a stack trace or compose error.
if ! grep -qiE "<(html|head|body|continuum)" "$PAGE_BODY" 2>/dev/null; then
  echo "❌ root page body doesn't look like HTML; Carl would see something broken."
  echo "  First 500 bytes:"
  head -c 500 "$PAGE_BODY" | sed 's/^/    /'
  exit 3
fi

# Negative checks: any of these in the body = broken-feeling page.
for marker in "chrome-error" "container exited" "ECONNREFUSED" "Cannot GET /" "Internal Server Error"; do
  if grep -qF "$marker" "$PAGE_BODY"; then
    echo "❌ root page contains failure marker: '$marker'"
    echo "  Context:"
    grep -F "$marker" "$PAGE_BODY" | head -3 | sed 's/^/    /'
    exit 3
  fi
done

echo "✅ root page looks like real HTML (${ROOT_BYTES} bytes, no failure markers)"

# ── Done ──────────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  ✅ carl-install-smoke PASSED"
echo "  Install duration: ${INSTALL_DUR}s"
echo "  Health latency:   $(( $(date +%s) - INSTALL_START - INSTALL_DUR ))s after install"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
