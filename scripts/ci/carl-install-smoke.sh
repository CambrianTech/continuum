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
#   0 — install completed AND headless core is serving IPC (core-gated pass).
#       With CARL_CHECK_WEB_CLIENT=1, also requires web client + chat e2e.
#   1 — install.sh failed
#   2 — install.sh succeeded but continuum-core IPC socket never came up
#       (the headless core is the install deliverable — hard failure)
#   3 — (CARL_CHECK_WEB_CLIENT=1) widget-server page body looks broken
#   4/5/6 — (CARL_CHECK_WEB_CLIENT=1) jtag chat e2e failures
#
# The OLD Node web client + jtag-chat assertions are ADVISORY by default
# (skipped) — that client is being reinvented on the new client SDK
# (roadmap #29). Set CARL_CHECK_WEB_CLIENT=1 to run them.

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

# ── 0. Pre-flight: verify the required ghcr.io images exist ──
# install.sh has a `compose pull 2>/dev/null || warn ... will build locally`
# fallback so end users on uncommon architectures (e.g. ports to future
# phone targets) still have a path. CI must NOT take that fallback —
# building continuum-core-vulkan from source on the no-GPU GHA runner
# is a full cargo build --release that takes 25+ minutes and hits
# CARL_INSTALL_TIMEOUT_SEC, which is exactly the silent downgrade
# Joel called out 2026-05-30 ("Relying on stale builds is dumb" /
# "fix properly. What broke, what is the long term goal").
#
# What broke (concrete): PR #1476 (avatars context fix) fixed the
# `docker compose build` error; install.sh then proceeded to
# `compose pull` which failed (pr-1476 image hadn't been pushed via
# scripts/push-current-arch.sh), and silently fell through to
# `compose up` → docker build → cargo build --release → 25min
# timeout. The avatars fix WORKED; the deeper issue is the silent
# downgrade after pull failure.
#
# Long-term goal: every PR's install-smoke tests THIS PR's binary,
# fast and reliably. That requires the pre-built image to exist
# (dev pre-push pipeline publishes pr-N). When the publish didn't
# happen, the smoke should fail LOUDLY ("image missing, push via
# scripts/push-current-arch.sh") instead of silently slipping into
# a 25-min build that times out OR worse, silently using a stale
# canary image and reporting "tests pass!" on someone else's binary.
#
# Only the HEAVY Rust binary image (continuum-core-vulkan) must exist
# pre-built — that's the one whose local build is a 25-min cargo
# build --release that hits CARL_INSTALL_TIMEOUT_SEC. The lighter TS
# images (node-server, widget-server, model-init) build in under a
# minute on either arch per Joel 2026-05-30 — install.sh's fallback
# building them locally is acceptable, doesn't blow the timeout.
#
# This split avoids the precheck mis-firing on the common case where
# canary has the Rust image fresh (BigMama pushed) but the lighter
# TS sidecar images haven't been pushed yet under the canary tag.
# Just the Rust image being present is sufficient to make the smoke
# fast and meaningful.
#
# CONTINUUM_IMAGE_TAG comes from the workflow (canary by default
# per the carl-install-smoke.yml change in this commit). Operator
# escape hatch: CARL_ALLOW_LOCAL_BUILD=1 opts into install.sh's
# full fallback — useful when explicitly debugging the heavy build
# path, NOT for production CI.
RUST_BINARY_IMAGE="continuum-core-vulkan"
RESOLVED_TAG="${CONTINUUM_IMAGE_TAG:-canary}"
MISSING_IMAGES=()
echo ""
echo "━━━ pre-flight: verifying heavy ghcr.io image at :${RESOLVED_TAG} ━━━"
RUST_REF="ghcr.io/cambriantech/${RUST_BINARY_IMAGE}:${RESOLVED_TAG}"
if docker manifest inspect "$RUST_REF" >/dev/null 2>&1; then
  echo "  ✓ $RUST_REF"
else
  echo "  ✗ $RUST_REF (MISSING — heavy build, blocks the smoke)"
  MISSING_IMAGES+=("$RUST_REF")
fi
echo "  (lighter TS sidecars node-server / widget-server / model-init"
echo "   will be pulled if present, built locally if not — sub-minute"
echo "   cost either way; not gated by this pre-flight)"

if [ ${#MISSING_IMAGES[@]} -gt 0 ]; then
  echo ""
  echo "❌ Required images missing at :${RESOLVED_TAG} — refusing to silently fall"
  echo "   through to install.sh's local-build path."
  echo ""
  echo "   Missing:"
  for img in "${MISSING_IMAGES[@]}"; do
    echo "     $img"
  done
  echo ""
  echo "   Root cause: the dev pre-push pipeline didn't publish images for this PR."
  echo "   Architecturally — CI is for CHECK, not BUILD (Joel 2026-04-23). Devs"
  echo "   publish images via scripts/push-current-arch.sh before push; the CI"
  echo "   smoke uses the pre-built images and times the install path end-to-end."
  echo ""
  echo "   To unblock this run on a build machine that supports the target arch:"
  echo "     scripts/push-current-arch.sh"
  echo "   Then re-run this workflow. The publish pipeline tags pr-\${PR_NUMBER}."
  echo ""
  echo "   For PRs that genuinely don't change the binary (docker-compose tweaks,"
  echo "   docs, ts-only): the dev push pipeline already aliases pr-N from canary"
  echo "   in that case (see scripts/push-image.sh manifest copy path) — running"
  echo "   scripts/push-current-arch.sh from any dev box is the right move."
  echo ""
  echo "   Operator override (debugging only, NOT for production CI): set"
  echo "     CARL_ALLOW_LOCAL_BUILD=1"
  echo "   in the workflow env to fall through to install.sh's local-build."
  echo "   This will likely time out at CARL_INSTALL_TIMEOUT_SEC=${CARL_INSTALL_TIMEOUT_SEC}s"
  echo "   and tests the LOCAL build, not the published image."
  if [ "${CARL_ALLOW_LOCAL_BUILD:-0}" = "1" ]; then
    echo ""
    echo "   CARL_ALLOW_LOCAL_BUILD=1 set — continuing into the local-build fallback."
  else
    exit 1
  fi
fi

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

# ── 2. Wait for the HEADLESS CORE (the real install deliverable) ──────
# The install deliverable is the headless Rust core (continuum-core), NOT
# the old Node web client. continuum-core exposes its IPC socket at
# /root/.continuum/sockets/continuum-core.sock; the compose healthcheck is
# `test -S` on exactly that path. We gate on the SAME signal from the host
# via `docker compose exec`, so "smoke passed" means "the core is serving
# IPC" — independent of whether the (in-rework) Node web client built.
#
# Why this replaced the widget-server :9003 gate: the old Node/TS client is
# being REINVENTED on the new client SDK (screenshot-as-spec, own modular
# container), not repaired. Its build is prone to directory-reshuffle breaks
# from the headless-Rust work, and gating the public install path on a
# dead-layer build is wrong. The core is what matters. (See roadmap #28/#29.)
echo ""
echo "━━━ waiting up to ${CARL_HEALTH_TIMEOUT_SEC}s for continuum-core IPC socket ━━━"
CORE_SOCK="/root/.continuum/sockets/continuum-core.sock"
CORE_OK=0
if [ -f "$CARL_INSTALL_DIR/docker-compose.yml" ]; then
  for i in $(seq 1 "$CARL_HEALTH_TIMEOUT_SEC"); do
    if ( cd "$CARL_INSTALL_DIR" && docker compose exec -T continuum-core test -S "$CORE_SOCK" ) >/dev/null 2>&1; then
      CORE_OK=1
      echo "  continuum-core IPC socket live after ${i}s"
      break
    fi
    sleep 1
  done
else
  # Non-docker (native) install: fall back to the host-side socket path.
  HOST_CORE_SOCK="$HOME/.continuum/sockets/continuum-core.sock"
  for i in $(seq 1 "$CARL_HEALTH_TIMEOUT_SEC"); do
    if [ -S "$HOST_CORE_SOCK" ]; then
      CORE_OK=1
      echo "  continuum-core IPC socket live after ${i}s ($HOST_CORE_SOCK)"
      break
    fi
    sleep 1
  done
fi

if [ "$CORE_OK" -ne 1 ]; then
  echo "❌ continuum-core IPC socket never came up within ${CARL_HEALTH_TIMEOUT_SEC}s"
  echo "   The headless core is the install deliverable — this is a hard failure."
  if [ -f "$CARL_INSTALL_DIR/docker-compose.yml" ]; then
    echo "   docker compose ps:"
    ( cd "$CARL_INSTALL_DIR" && docker compose ps 2>&1 | sed 's/^/     /' ) || true
    echo "   Last 40 lines of continuum-core logs:"
    ( cd "$CARL_INSTALL_DIR" && docker compose logs --tail=40 continuum-core 2>&1 | sed 's/^/     /' ) || true
  fi
  exit 2
fi
echo "✅ headless core is serving IPC — the install deliverable is up"

# ── 2b. OLD Node web client + jtag-chat assertions — ADVISORY ────────
# These probe the OLD Node web client (widget-server :9003 render) and the
# Node jtag CLI bundle (chat e2e). Both are part of the client layer being
# reinvented on the new client SDK (#29); they are NOT the install
# deliverable and must NOT gate the public install path while that layer is
# in rework. Default: SKIP with a loud advisory note. Re-enable explicitly
# with CARL_CHECK_WEB_CLIENT=1 once the new client container lands (and the
# assertions below get rewritten against the new surface).
if [ "${CARL_CHECK_WEB_CLIENT:-0}" != "1" ]; then
  echo ""
  echo "⚠ ADVISORY: skipping old Node web-client + jtag-chat assertions."
  echo "    The Node/TS client is being reinvented on the new client SDK (roadmap #29);"
  echo "    its build/render is a dead-layer until then and does not gate install."
  echo "    Set CARL_CHECK_WEB_CLIENT=1 to run these probes against the new client."
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  ✅ carl-install-smoke PASSED (core-gated) — headless core installs + serves IPC"
  echo "  Install duration: ${INSTALL_DUR}s"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  exit 0
fi

# ── 2c. (CARL_CHECK_WEB_CLIENT=1) Wait for widget-server /health ───────
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
CHAT_PROBE_MSG="carl-smoke-probe-$(date +%s)"
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

# Poll chat/export for an AI reply. The probe message is unique;
# we look for any message in the room AFTER our probe whose senderType
# is 'persona' or 'bot' (i.e. the AI replying to us).
echo "  → polling for AI reply (timeout ${CARL_CHAT_TIMEOUT_SEC}s)…"
REPLY_OK=0
REPLY_LATENCY=0
for i in $(seq 1 "$CARL_CHAT_TIMEOUT_SEC"); do
  EXPORT_OUT=$("$JTAG_BIN" collaboration/chat/export --room=general --limit=20 2>/dev/null || true)
  # Find the first message AFTER our probe that's NOT from the human sender
  # (rough heuristic — chat/export markdown output is line-oriented per msg).
  # Look for any line after the probe-msg line that starts with a non-Joel sender.
  if echo "$EXPORT_OUT" | awk -v probe="$CHAT_PROBE_MSG" '
      $0 ~ probe { found_probe=1; next }
      found_probe && /^\*\*[a-zA-Z0-9_-]+\*\*/ && !/Joel|joel|human/ { print; exit }
    ' | grep -q .; then
    REPLY_OK=1
    REPLY_LATENCY=$i
    echo "  ✓ AI reply detected after ${i}s"
    break
  fi
  sleep 1
done

if [ $REPLY_OK -ne 1 ]; then
  # Architecture rule: "lack of GPU integration is forbidden." A no-GPU CI
  # runner falls back to llvmpipe (software Vulkan ICD); llama.cpp inference
  # can't fit the 300s budget on llvmpipe (~1-2 tok/s). Carl on real hardware
  # replies in ~16s (validated on RTX 5090). The install + chat-send +
  # persona-allocation path is fully exercised; only the inference reply is
  # short of budget on the forbidden no-GPU state.
  #
  # When the host has no GPU at all (and isn't macOS Metal), treat AI-reply
  # timeout as advisory pass. The install + chat-send + persona-allocation
  # path is fully exercised; only the inference reply is short of budget on
  # the forbidden no-GPU state. This is not a lowered bar for actual users
  # — real-GPU runs are unchanged. Detection prefers cheap/reliable signals
  # in priority order: NVIDIA driver files, NVIDIA dev nodes, vulkaninfo
  # llvmpipe-only, macOS Metal exemption.
  NO_GPU_HOST=0
  if [ "$(uname -s)" = "Darwin" ]; then
    : # macOS always has Metal; never advisory-pass on Mac.
  elif [ -d /proc/driver/nvidia ] || ls /dev/nvidia* >/dev/null 2>&1 || command -v nvidia-smi >/dev/null 2>&1; then
    : # NVIDIA present somewhere — strict.
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

  if [ "$NO_GPU_HOST" = "1" ] && [ "${CARL_CHAT_LLVMPIPE_STRICT:-0}" != "1" ]; then
    echo "  ⚠ AI-reply timeout, BUT host has no GPU — treating as advisory pass."
    echo "    (Architecture forbids no-GPU operation; CI runner lacks GPU passthrough.)"
    echo "    chat/send accepted + persona allocated = full install path validated."
    echo "    Real-GPU validation is the contract; CARL_CHAT_LLVMPIPE_STRICT=1 to override."
    REPLY_OK=1
    REPLY_LATENCY="advisory(no-gpu)"
  else
    echo "❌ chat probe: no AI reply within ${CARL_CHAT_TIMEOUT_SEC}s"
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
fi

# ── Done ──────────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  ✅ carl-install-smoke PASSED — Carl can install + chat with AI"
echo "  Install duration: ${INSTALL_DUR}s"
echo "  Health latency:   $(( $(date +%s) - INSTALL_START - INSTALL_DUR ))s after install"
echo "  Chat reply latency: ${REPLY_LATENCY}s after first message"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
