#!/bin/bash
# Postinstall wrapper: skip the heavyweight model download in agent
# worktrees / explicit-skip contexts. The actual voice/avatar bytes are
# only needed by the running stack; per-worktree npm install in an agent
# lane wastes 30s+ + several GB of disk per lane.
#
# Skip conditions (any one is sufficient):
#   1. CONTINUUM_SKIP_MODEL_DOWNLOAD=1 in the env
#   2. pwd is under an airc lane worktree (~/.airc-worktrees/...)
#   3. CI=true or GITHUB_ACTIONS=true (CI runners don't need the bytes;
#      tests that need them download on demand)
#
# Otherwise, delegate to the existing download-voice-models.sh.
#
# See continuum#1172 for the issue + rationale.

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

skip_reason=""

if [ "${CONTINUUM_SKIP_MODEL_DOWNLOAD:-0}" = "1" ]; then
  skip_reason="CONTINUUM_SKIP_MODEL_DOWNLOAD=1"
fi

if [ -z "$skip_reason" ] && [[ "$PWD" == *".airc-worktrees"* ]]; then
  skip_reason="airc lane worktree detected (PWD=$PWD)"
fi

if [ -z "$skip_reason" ] && { [ "${CI:-}" = "true" ] || [ "${GITHUB_ACTIONS:-}" = "true" ]; }; then
  skip_reason="CI environment detected"
fi

if [ -n "$skip_reason" ]; then
  echo "⏭️  Skipping voice/avatar model download (~3.9GB) — $skip_reason"
  echo "    To force download: unset CONTINUUM_SKIP_MODEL_DOWNLOAD and run:"
  echo "    npm run worker:models"
  exit 0
fi

# Delegate to the real download script. Honor its non-fatal contract
# (the original postinstall wrapped this in `|| echo …` so the install
# itself never failed on missing models).
if ! "$SCRIPT_DIR/download-voice-models.sh"; then
  echo "⚠️  Voice model download failed (non-fatal — system starts without STT/TTS)"
  exit 0
fi
