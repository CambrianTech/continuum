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
#   7 — chat/send accepted, but node-server logged a fatal persona response
#       failure before a reply could be posted

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
  # Capture per-container docker logs BEFORE `docker compose down` kills
  # the containers and makes their logs unrecoverable. Without this the
  # workflow's `if: failure()` step fires after smoke exit when containers
  # are already gone — exactly the silent-evidence-loss the per-container
  # logs are supposed to prevent. Capture on every exit (success or
  # failure) since the file glob in the workflow upload is failure-only.
  if [ -d "$CARL_INSTALL_DIR" ] && [ -f "$CARL_INSTALL_DIR/docker-compose.yml" ]; then
    for svc in continuum-core node-server model-init widget-server livekit-bridge; do
      ( cd "$CARL_INSTALL_DIR" && docker compose logs --no-color --timestamps "$svc" \
        > "${CARL_INSTALL_DIR}.${svc}.log" 2>&1 ) || true
    done
    ( cd "$CARL_INSTALL_DIR" && docker compose ps -a > "${CARL_INSTALL_DIR}.compose-ps.log" 2>&1 ) || true
    if [ -d "$HOME/.continuum" ]; then
      tar -C "$HOME/.continuum" -czf "${CARL_INSTALL_DIR}.continuum-logs.tgz" logs personas 2>/dev/null || true
    fi
  fi
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
# Pass CONTINUUM_REF so install.sh clones the PR's src/ tree, not main.
# Pre-2026-05-03 install.sh always cloned main → PR src/ changes never
# got validated by carl-install-smoke. This made Carl-install testing
# limited to install.sh-internal changes only — every src/ fix had to
# merge to main before the smoke could test it. Real-world impact:
# months of "the smoke is broken because main's broken" loop with no
# way to validate PR fixes. CONTINUUM_REF closes the loop.
INSTALL_START=$(date +%s)
if ! timeout "$CARL_INSTALL_TIMEOUT_SEC" bash -c \
     "CONTINUUM_DIR='$CARL_INSTALL_DIR' CONTINUUM_REF='$CARL_INSTALL_REF' bash <(curl -fsSL '$INSTALL_URL')" \
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

# ── 3b. Headless screenshot — what Carl ACTUALLY sees in the browser ──
# curl gives the server-rendered HTML shell. The chat UI itself loads via
# JS — could be a blank chat with no personas or an empty room and curl
# wouldn't catch it. Use chromium headless to capture what a real browser
# renders. Wait a few seconds for the JS to populate tabs, personas,
# rooms before snapping. Continue on screenshot failure (chrome may not
# be on the PATH for non-CI runs); this is diagnostic, not gating.
PAGE_PNG="${CARL_INSTALL_DIR}.page.png"
CHROME_BIN="$(command -v google-chrome || command -v chromium || command -v chromium-browser || true)"
if [ -n "$CHROME_BIN" ]; then
  echo ""
  echo "━━━ headless screenshot via $CHROME_BIN (waits 8s for JS to render) ━━━"
  sleep 8
  "$CHROME_BIN" --headless --disable-gpu --no-sandbox --hide-scrollbars \
    --window-size=1280,1024 \
    --screenshot="$PAGE_PNG" \
    --virtual-time-budget=8000 \
    "http://localhost:9003/" >/dev/null 2>&1 || true
  if [ -f "$PAGE_PNG" ]; then
    echo "  ✓ screenshot saved: $PAGE_PNG ($(stat -c%s "$PAGE_PNG" 2>/dev/null || stat -f%z "$PAGE_PNG") bytes)"
  else
    echo "  ⚠ screenshot capture failed (non-fatal)"
  fi
else
  echo "  ⚠ no chromium/chrome on PATH — skipping browser screenshot"
fi

# ── 4. End-to-end chat: Carl types a message, expects an AI reply ─────
# Per Joel's "OOTB on MacBook Air, free, accessible" + "canary e2e
# working from curl, Carl's case" — page-render is necessary but not
# sufficient. The actual user-facing target is "Carl can chat with the
# AI." This step closes that gap: send a message via jtag/chat/send
# (which goes through the same code path the widget uses), poll
# chat/export for an AI reply, fail loudly if none arrives.
#
# Exit codes for this section:
#   4 — chat/send didn't accept the message (system not ready for chat)
#   5 — no AI reply within CARL_CHAT_TIMEOUT_SEC (default 90s)
#       — root cause: no personas seeded, persona allocation failed,
#         model not loaded, or inference path broken (DMR not running,
#         GPU EP misconfigured, etc.). Each of those should now hard-
#         fail with an actionable error per the #964 + #980 series.
#   6 — chat/send accepted but the warning marker from #994 fires
#       (no listener) — distinguishes "no AI" from "AI didn't respond"
echo ""
echo "━━ end-to-end chat: send message, expect AI reply ━━"
CARL_CHAT_TIMEOUT_SEC="${CARL_CHAT_TIMEOUT_SEC:-90}"
CHAT_PROBE_ID="carl-smoke-probe-$(date +%s)"
CHAT_PROBE_MSG="@Helper AI please reply with one short sentence including ${CHAT_PROBE_ID}."
CHAT_LOG="${CARL_INSTALL_DIR}.chat.log"

# Locate jtag — install.sh symlinks it into BIN_DIR for the user
# (typically $HOME/.local/bin/jtag). Carl's install used CONTINUUM_DIR.
JTAG_BIN=""
for cand in \
  "$CARL_INSTALL_DIR/src/jtag" \
  "$HOME/.local/bin/jtag" \
  "$(command -v jtag 2>/dev/null)"; do
  if [ -n "$cand" ] && [ -x "$cand" ]; then
    JTAG_BIN="$cand"; break
  fi
done

if [ -z "$JTAG_BIN" ]; then
  echo "❌ chat probe: couldn't locate jtag binary"
  echo "  Searched: \$CARL_INSTALL_DIR/src/jtag, \$HOME/.local/bin/jtag, PATH"
  echo "  CARL_INSTALL_DIR=$CARL_INSTALL_DIR"
  exit 4
fi
echo "  jtag binary: $JTAG_BIN"

# Send. The jtag/chat/send command returns a JSON envelope; we extract
# the messageId from the response to track the thread.
echo "  → sending probe: '$CHAT_PROBE_MSG'"
CHAT_SENT_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SEND_OUT=$("$JTAG_BIN" collaboration/chat/send --room=general --message="$CHAT_PROBE_MSG" 2>&1)
SEND_RC=$?
echo "$SEND_OUT" | sed 's/^/    /' > "$CHAT_LOG"
if [ $SEND_RC -ne 0 ]; then
  echo "❌ chat probe: chat/send command FAILED (exit $SEND_RC)"
  echo "  Output:"
  echo "$SEND_OUT" | head -10 | sed 's/^/    /'
  exit 4
fi

# Detect the no-listener warning (#994). If chat/send accepted but
# warned about no AI personas, that's a distinct failure mode from
# "AI silent" — surface the difference.
if echo "$SEND_OUT" | grep -q "No AI personas in system"; then
  echo "❌ chat probe: chat/send accepted, but reported NO PERSONAS in system"
  echo "  This means seed didn't successfully allocate persona-users."
  echo "  Cascades from a failed install seed (#980 Bug 3) or a"
  echo "  continuum-core that didn't register commands in time."
  echo "  Diagnose: $JTAG_BIN data/list --collection=users --filter='{\"type\":\"persona\"}'"
  exit 6
fi

echo "  ✓ chat/send accepted (some persona is listening)"

# Poll chat/export for an AI reply. The probe id is unique;
# we look for any message in the room AFTER our probe whose exported
# sender heading is not the human sender.
echo "  → polling for AI reply (timeout ${CARL_CHAT_TIMEOUT_SEC}s)…"
REPLY_OK=0
REPLY_LATENCY=0
for i in $(seq 1 "$CARL_CHAT_TIMEOUT_SEC"); do
  EXPORT_OUT=$("$JTAG_BIN" collaboration/chat/export --room=general --limit=20 2>/dev/null || true)
  # Find the first message AFTER our probe that's NOT from the human sender.
  # Current chat/export headings are "## #shortId - Sender"; older exports
  # used "**Sender**", so support both while still requiring a post-probe
  # sender heading.
  if echo "$EXPORT_OUT" | awk -v probe="$CHAT_PROBE_ID" '
      $0 ~ probe { found_probe=1; next }
      found_probe && /^## #[[:alnum:]]+ - / && !/ - (Developer|Joel|joel|human)$/ { print; exit }
      found_probe && /^\*\*[a-zA-Z0-9_ -]+\*\*/ && !/Joel|joel|human|Developer/ { print; exit }
    ' | grep -q .; then
    REPLY_OK=1
    REPLY_LATENCY=$i
    echo "  ✓ AI reply detected after ${i}s"
    break
  fi

  # Fail fast on fatal persona response errors. Without this, CI burns the
  # whole chat timeout polling a room that cannot receive a reply.
  if [ "$i" -le 5 ] || [ $((i % 5)) -eq 0 ]; then
    NODE_FATAL=$(
      ( cd "$CARL_INSTALL_DIR" && docker compose logs --no-color --since "$CHAT_SENT_AT" node-server 2>/dev/null ) \
        | grep -E "PersonaResponseGenerator.*response failed|Local AI is unavailable|cognition/respond.*(FAILED|failed|error)" \
        | tail -5 || true
    )
    if [ -n "$NODE_FATAL" ]; then
      echo "❌ chat probe: persona response failed before any AI reply"
      echo "$NODE_FATAL" | sed 's/^/    /'
      exit 7
    fi
  fi
  sleep 1
done

if [ $REPLY_OK -ne 1 ]; then
  # Architecture rule: "lack of GPU integration is forbidden." A no-GPU CI
  # runner with only llvmpipe may exercise the command path, but it has NOT
  # proven Carl can chat with an AI. Do not convert that into a pass.
  NO_GPU_HOST=0
  if [ "$(uname -s)" = "Darwin" ]; then
    : # macOS always has Metal.
  elif [ -d /proc/driver/nvidia ] || ls /dev/nvidia* >/dev/null 2>&1 || command -v nvidia-smi >/dev/null 2>&1; then
    : # NVIDIA present somewhere.
  elif command -v vulkaninfo >/dev/null 2>&1; then
    VK_DEVICES=$(vulkaninfo --summary 2>/dev/null | grep -i deviceName || true)
    if echo "$VK_DEVICES" | grep -qi "llvmpipe" && \
       ! echo "$VK_DEVICES" | grep -qiE "GeForce|Radeon|Intel.*(Iris|HD|Arc)|Apple|Mali|Adreno"; then
      NO_GPU_HOST=1
    fi
  else
    # No NVIDIA, no vulkaninfo on host PATH — almost certainly a CI runner
    # with neither GPU passthrough nor a graphics stack installed. Carl
    # can't run in this state architecturally.
    NO_GPU_HOST=1
  fi

  echo "❌ chat probe: no AI reply within ${CARL_CHAT_TIMEOUT_SEC}s"
  if [ "$NO_GPU_HOST" = "1" ]; then
    echo "  Host appears to have no real GPU path. That is still a failure:"
    echo "  Carl-install smoke only passes when the installed system produces"
    echo "  an actual AI reply."
  fi
  echo ""
  echo "  This is the classic Carl-blocker: chat goes silent."
  echo "  Likely root causes (post-#980 series):"
  echo "    - continuum-core inference path not reaching DMR (check #997's"
  echo "      'local' default actually routes correctly)"
  echo "    - DMR not running (Docker Model Runner needs Docker Desktop 4.62+)"
  echo "    - GPU EP not configured (#985 / #991 cfg fixes — verify metal feature)"
  echo "    - Persona model not pulled into DMR (install.sh's docker model pull)"
  echo "    - SIGABRT in continuum-core (NEW-A — upstream llama.cpp bug,"
  echo "      tracked at ggml-org/llama.cpp#22593)"
  echo ""
  echo "  Last 30 lines of room export:"
  echo "$EXPORT_OUT" | tail -30 | sed 's/^/    /'
  echo ""
  echo "  Diagnose:"
  echo "    $JTAG_BIN ai/providers/status"
  echo "    $JTAG_BIN ai/local-inference/status"
  echo "    docker compose -f $CARL_INSTALL_DIR/docker-compose.yml logs --tail=100 continuum-core"
  exit 5
fi

# ── Done ──────────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  ✅ carl-install-smoke PASSED — Carl can install + chat with AI"
echo "  Install duration: ${INSTALL_DUR}s"
echo "  Health latency:   $(( $(date +%s) - INSTALL_START - INSTALL_DUR ))s after install"
echo "  Chat reply latency: ${REPLY_LATENCY}s after first message"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
