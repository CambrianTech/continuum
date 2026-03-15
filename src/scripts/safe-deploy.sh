#!/bin/bash
# Safe Deploy — External watchdog for autonomous deployments.
#
# Pure bash. No Node dependencies. Survives system failure.
# If the system destroys itself, this script is already running and will
# revert + restart from the last known-good state.
#
# Flow:
#   1. Record HEAD as rollback point
#   2. Stash WIP (if any uncommitted changes)
#   3. Stop running system
#   4. Compile check (npm run build:ts — fail fast)
#   5. Deploy (npm start)
#   6. Health check (./jtag ping, poll every 3s, max DEPLOY_HEALTH_TIMEOUT)
#   7. Healthy  → tag safe/latest, log success, exit 0
#      Unhealthy → stop, revert, restore stash, restart, verify recovery, exit 1
#
# Usage:
#   bash scripts/safe-deploy.sh                    # Normal deploy
#   DEPLOY_HEALTH_TIMEOUT=300 bash scripts/safe-deploy.sh  # Custom timeout
#   DEPLOY_REQUIRE_AI=true bash scripts/safe-deploy.sh     # Require AI personas healthy
#   DEPLOY_AUTO_REVERT=false bash scripts/safe-deploy.sh   # Skip auto-revert (debugging)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# ── Configuration (env var overrides) ──────────────────────────────────────

DEPLOY_HEALTH_TIMEOUT="${DEPLOY_HEALTH_TIMEOUT:-180}"
DEPLOY_HEALTH_POLL_INTERVAL="${DEPLOY_HEALTH_POLL_INTERVAL:-3}"
DEPLOY_REQUIRE_AI="${DEPLOY_REQUIRE_AI:-false}"
DEPLOY_AUTO_REVERT="${DEPLOY_AUTO_REVERT:-true}"
DEPLOY_HISTORY_DIR="${DEPLOY_HISTORY_DIR:-$HOME/.continuum/deploys}"

# ── Helpers ────────────────────────────────────────────────────────────────

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[safe-deploy]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[safe-deploy]${NC} $*"; }
log_error() { echo -e "${RED}[safe-deploy]${NC} $*"; }

# Append a JSON line to deploy history
log_deploy() {
  local status="$1"
  local commit="$2"
  shift 2
  # Remaining args are key=value pairs
  local extra=""
  while [ $# -gt 0 ]; do
    extra="${extra},\"$(echo "$1" | cut -d= -f1)\":\"$(echo "$1" | cut -d= -f2-)\""
    shift
  done
  mkdir -p "$DEPLOY_HISTORY_DIR"
  local timestamp
  timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "{\"timestamp\":\"${timestamp}\",\"commit\":\"${commit}\",\"status\":\"${status}\"${extra}}" \
    >> "$DEPLOY_HISTORY_DIR/history.jsonl"
}

# Check system health via ./jtag ping
check_health() {
  local output
  output=$(cd "$PROJECT_DIR" && ./jtag ping 2>/dev/null) || return 1

  # Check basic server response
  echo "$output" | grep -q '"success":true' || echo "$output" | grep -q '"success": true' || return 1

  # Check AI health if required
  if [ "$DEPLOY_REQUIRE_AI" = "true" ]; then
    # Verify at least one AI persona is healthy
    echo "$output" | grep -q '"healthy"' || return 1
    local healthy_count
    healthy_count=$(echo "$output" | grep -o '"healthy"' | wc -l | tr -d ' ')
    [ "$healthy_count" -gt 0 ] || return 1
  fi

  return 0
}

# Wait for health with timeout, returns 0 if healthy, 1 if timed out
wait_for_health() {
  local timeout="$1"
  local elapsed=0

  log_info "Waiting for health (timeout: ${timeout}s, poll: ${DEPLOY_HEALTH_POLL_INTERVAL}s)..."

  while [ "$elapsed" -lt "$timeout" ]; do
    if check_health; then
      log_info "Health check passed after ${elapsed}s"
      return 0
    fi
    sleep "$DEPLOY_HEALTH_POLL_INTERVAL"
    elapsed=$((elapsed + DEPLOY_HEALTH_POLL_INTERVAL))
    # Progress indicator every 30s
    if [ $((elapsed % 30)) -eq 0 ]; then
      log_info "Still waiting... ${elapsed}s / ${timeout}s"
    fi
  done

  log_error "Health check timed out after ${timeout}s"
  return 1
}

# ── Main Flow ──────────────────────────────────────────────────────────────

cd "$PROJECT_DIR"

log_info "Starting safe deploy from $(pwd)"

# 1. Record current state
ROLLBACK_COMMIT=$(git rev-parse HEAD)
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
log_info "Rollback point: ${ROLLBACK_COMMIT:0:12} on ${CURRENT_BRANCH}"

# 2. Stash WIP if any uncommitted changes
STASHED=false
if ! git diff --quiet || ! git diff --cached --quiet; then
  log_info "Stashing uncommitted changes..."
  git stash push -m "safe-deploy-$(date +%s)" --include-untracked
  STASHED=true
fi

# Track whether we need to clean up
DEPLOY_STARTED=false

cleanup_on_failure() {
  local reason="${1:-unknown}"
  log_error "Deploy failed: ${reason}"

  if [ "$DEPLOY_AUTO_REVERT" != "true" ]; then
    log_warn "Auto-revert disabled. Manual intervention required."
    log_deploy "failed" "$ROLLBACK_COMMIT" "reason=${reason}" "autoReverted=false"
    if [ "$STASHED" = true ]; then
      log_warn "You have stashed changes — run 'git stash pop' to restore."
    fi
    exit 1
  fi

  log_info "Auto-reverting to ${ROLLBACK_COMMIT:0:12}..."

  # Stop the broken system
  if [ "$DEPLOY_STARTED" = true ]; then
    log_info "Stopping failed deployment..."
    npm stop 2>/dev/null || bash scripts/system-stop.sh 2>/dev/null || true
    sleep 2
  fi

  # Revert: check if a new commit was made (vs rollback point)
  local current_head
  current_head=$(git rev-parse HEAD)
  if [ "$current_head" != "$ROLLBACK_COMMIT" ]; then
    log_info "Reverting commit ${current_head:0:12}..."
    git revert HEAD --no-edit --no-commit
    git checkout -- . 2>/dev/null || true
    git reset HEAD 2>/dev/null || true
    git checkout "$ROLLBACK_COMMIT" 2>/dev/null || true
  fi

  # If we have a safe/latest tag, use that as ultimate fallback
  if git rev-parse safe/latest >/dev/null 2>&1; then
    local safe_commit
    safe_commit=$(git rev-parse safe/latest)
    if [ "$safe_commit" != "$current_head" ]; then
      log_info "Checking out safe/latest (${safe_commit:0:12})..."
      git checkout "$safe_commit" 2>/dev/null || true
    fi
  fi

  # Restore stashed WIP
  if [ "$STASHED" = true ]; then
    log_info "Restoring stashed changes..."
    git stash pop 2>/dev/null || log_warn "Could not restore stash — resolve manually with 'git stash list'"
  fi

  # Restart with known-good code
  log_info "Restarting with reverted code..."
  npm start 2>&1 | tail -5 &
  local restart_pid=$!

  # Wait for recovery health check
  if wait_for_health "$DEPLOY_HEALTH_TIMEOUT"; then
    log_info "Recovery successful — system healthy on reverted code"
    log_deploy "failed" "$ROLLBACK_COMMIT" "reason=${reason}" "autoReverted=true" "recoveredTo=${ROLLBACK_COMMIT:0:12}"
  else
    log_error "CRITICAL: Recovery also failed. System may be down."
    log_deploy "critical" "$ROLLBACK_COMMIT" "reason=recovery_failed" "originalReason=${reason}"
  fi

  exit 1
}

# 3. Stop running system
log_info "Stopping current system..."
npm stop 2>/dev/null || bash scripts/system-stop.sh 2>/dev/null || true
sleep 2

# 4. Compile check — fail fast before deploying
log_info "Compile check (npm run build:ts)..."
if ! npm run build:ts 2>&1 | tail -20; then
  cleanup_on_failure "compile_failed"
fi

# 5. Deploy
log_info "Deploying (npm start)..."
DEPLOY_STARTED=true
npm start 2>&1 | tail -10 &
DEPLOY_PID=$!

# Wait for npm start to complete (it exits after system is UP)
wait "$DEPLOY_PID" || true

# 6. Health check
HEALTH_START=$(date +%s)

if wait_for_health "$DEPLOY_HEALTH_TIMEOUT"; then
  HEALTH_END=$(date +%s)
  HEALTH_MS=$(( (HEALTH_END - HEALTH_START) * 1000 ))

  # 7a. Success — tag and log
  log_info "Deploy successful!"

  # Tag this commit as known-good
  git tag -f safe/latest HEAD
  log_info "Tagged safe/latest → $(git rev-parse --short HEAD)"

  # Also tag with timestamp for history
  git tag "safe/$(date +%Y%m%d-%H%M%S)" HEAD 2>/dev/null || true

  # Restore stash if we had one
  if [ "$STASHED" = true ]; then
    log_info "Restoring stashed changes..."
    git stash pop 2>/dev/null || log_warn "Could not restore stash"
  fi

  log_deploy "success" "$(git rev-parse HEAD)" "healthCheckMs=${HEALTH_MS}"
  log_info "Deploy complete. History: ${DEPLOY_HISTORY_DIR}/history.jsonl"
  exit 0
else
  # 7b. Failure — revert and restart
  cleanup_on_failure "health_check_timeout"
fi
