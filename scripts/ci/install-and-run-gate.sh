#!/usr/bin/env bash
# install-and-run-gate.sh — bring up the Carl docker compose stack, verify
# widget-server health on :9003, dump logs on failure, tear down.
#
# Usage:
#   CONTINUUM_IMAGE_TAG=pr-950 bash scripts/ci/install-and-run-gate.sh
#   CONTINUUM_IMAGE_TAG=latest bash scripts/ci/install-and-run-gate.sh
#
# Defaults:
#   CONTINUUM_IMAGE_TAG=latest
#   HEALTH_TIMEOUT_SEC=300  (5 min)
#   MODEL_INIT_TIMEOUT_SEC=300  (5 min)
#
# Both CI (docker-images.yml verify-architectures job) and humans (bigmama-wsl
# on bigmama-1, anvil on Mac, anyone with the repo + docker + bash) call this
# script via the same one-line invocation. Same script, same behavior, same
# failure surface — the gate is the gate.
#
# Why a script and not just CI yaml: Joel 2026-04-23: "make your own testing
# easy" + "you guys should test rather than throwing it over the wall to ci."
# A 70-line shell script that ANY of us can run on ANY machine in 30 seconds
# beats a CI-yaml-only gate that we discover is broken only after CI fails
# the second time and we have to re-fast-forward.
#
# Exit codes:
#   0 — all checks passed, stack torn down cleanly
#   1 — usage / pre-flight error
#   2 — model-init didn't finish in time (download stalled)
#   3 — widget-server didn't return 2xx in time (service health failed)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

CONTINUUM_IMAGE_TAG="${CONTINUUM_IMAGE_TAG:-latest}"
HEALTH_TIMEOUT_SEC="${HEALTH_TIMEOUT_SEC:-300}"
MODEL_INIT_TIMEOUT_SEC="${MODEL_INIT_TIMEOUT_SEC:-300}"

export CONTINUUM_IMAGE_TAG

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  install-and-run-gate"
echo "  CONTINUUM_IMAGE_TAG=$CONTINUUM_IMAGE_TAG"
echo "  HEALTH_TIMEOUT_SEC=$HEALTH_TIMEOUT_SEC"
echo "  MODEL_INIT_TIMEOUT_SEC=$MODEL_INIT_TIMEOUT_SEC"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

teardown() {
  echo ""
  echo "━━━ tearing down ━━━"
  docker compose down -v 2>&1 | tail -3
}
trap teardown EXIT INT TERM

echo ""
echo "━━━ pulling image set at tag $CONTINUUM_IMAGE_TAG ━━━"
docker compose pull --quiet \
  model-init livekit-bridge continuum-core node-server widget-server livekit

echo ""
echo "━━━ bringing up model-init (one-shot voice model download) ━━━"
docker compose up -d model-init

# Wait up to MODEL_INIT_TIMEOUT_SEC for model-init to exit cleanly.
echo "  waiting up to ${MODEL_INIT_TIMEOUT_SEC}s for model-init to finish..."
DEADLINE=$(( $(date +%s) + MODEL_INIT_TIMEOUT_SEC ))
while [ "$(date +%s)" -lt "$DEADLINE" ]; do
  STATUS=$(docker compose ps -a --format json model-init 2>/dev/null \
           | head -1 \
           | python3 -c "import sys,json
try: print(json.loads(sys.stdin.read() or '{}').get('State',''))
except Exception: print('')" 2>/dev/null)
  case "$STATUS" in
    exited) echo "  model-init exited cleanly"; break;;
    "")     echo "  (model-init container not visible yet)";;
    *)      echo "  model-init: $STATUS";;
  esac
  sleep 10
done

if [ "$(date +%s)" -ge "$DEADLINE" ]; then
  echo "❌ model-init did not finish within ${MODEL_INIT_TIMEOUT_SEC}s"
  docker compose logs --tail=30 model-init
  exit 2
fi

echo ""
echo "━━━ bringing up runtime services ━━━"
docker compose up -d livekit livekit-bridge continuum-core node-server widget-server

echo ""
echo "━━━ waiting up to ${HEALTH_TIMEOUT_SEC}s for widget-server :9003 health ━━━"
HEALTHY=0
DEADLINE=$(( $(date +%s) + HEALTH_TIMEOUT_SEC ))
while [ "$(date +%s)" -lt "$DEADLINE" ]; do
  CODE=$(curl -fsS -o /dev/null -w "%{http_code}" http://localhost:9003/ 2>/dev/null || echo "000")
  case "$CODE" in
    2*) HEALTHY=1; echo "✅ widget-server responded $CODE on :9003"; break;;
    *)  echo "  curl :9003 → $CODE (still waiting)";;
  esac
  sleep 5
done

# Bonus probe: continuum-core IPC socket. Surfaces Rust-panic-on-startup as
# warning even if widget happens to come up first. Doesn't fail the gate.
if docker compose exec -T continuum-core test -S /root/.continuum/sockets/continuum-core.sock 2>/dev/null; then
  echo "✅ continuum-core IPC socket present"
else
  echo "⚠️  continuum-core IPC socket NOT present (warning only)"
fi

if [ "$HEALTHY" -ne 1 ]; then
  echo ""
  echo "❌ widget-server never returned 2xx within ${HEALTH_TIMEOUT_SEC}s"
  echo "   service logs (last 50 lines each):"
  for SVC in continuum-core node-server widget-server livekit-bridge livekit; do
    echo ""
    echo "━━━ $SVC ━━━"
    docker compose logs --tail=50 "$SVC" 2>&1 || true
  done
  exit 3
fi

echo ""
echo "✅ install-and-run-gate PASSED at tag $CONTINUUM_IMAGE_TAG"
