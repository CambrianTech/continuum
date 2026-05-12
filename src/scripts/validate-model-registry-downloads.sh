#!/bin/bash
# Validate that registry auto_download artifacts resolve on HuggingFace.
# This is a fast preflight for model-init: it catches stale repos/filenames
# before the install smoke spends minutes booting a system with no persona LLM.
# Validation is intentionally anonymous: default install models must be public.

set -euo pipefail

REGISTRY="${REGISTRY:-src/shared/models.json}"
TIER="${TIER:-full}"
VALIDATE_ALL_TIERS="${VALIDATE_ALL_TIERS:-0}"

if [[ ! -f "$REGISTRY" ]]; then
  echo "ERROR: registry file not found: $REGISTRY" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "ERROR: jq is required" >&2
  exit 1
fi

MODEL_KEYS=()
if [[ "$VALIDATE_ALL_TIERS" == "1" ]]; then
  while IFS= read -r key; do
    MODEL_KEYS+=("$key")
  done < <(jq -r '
    [
      .auto_download.always[],
      (.auto_download.by_tier[] // [])[]
    ] | unique | .[]
  ' "$REGISTRY")
else
  case "$TIER" in
    mba|mid|full) ;;
    *)
      echo "ERROR: TIER='$TIER' is not valid; use mba, mid, or full" >&2
      exit 1
      ;;
  esac
  while IFS= read -r key; do
    MODEL_KEYS+=("$key")
  done < <(jq -r --arg tier "$TIER" '
    [
      .auto_download.always[],
      (.auto_download.by_tier[$tier] // [])[]
    ] | unique | .[]
  ' "$REGISTRY")
fi

FAILED=0

for KEY in "${MODEL_KEYS[@]}"; do
  FORMAT=$(jq -r --arg k "$KEY" '.models[$k].format // ""' "$REGISTRY")
  REPO=$(jq -r --arg k "$KEY" '.models[$k].hf_repo // ""' "$REGISTRY")
  REVISION=$(jq -r --arg k "$KEY" '.models[$k].hf_revision // "main"' "$REGISTRY")
  FILES=()
  while IFS= read -r file; do
    FILES+=("$file")
  done < <(jq -r --arg k "$KEY" '.models[$k].files // [] | .[]' "$REGISTRY")

  if [[ "$FORMAT" == "candle-builtin" ]]; then
    continue
  fi

  if [[ -z "$REPO" ]]; then
    echo "ERROR: $KEY has no hf_repo" >&2
    FAILED=$((FAILED + 1))
    continue
  fi

  if [[ ${#FILES[@]} -eq 0 ]]; then
    echo "ERROR: $KEY has no files[]" >&2
    FAILED=$((FAILED + 1))
    continue
  fi

  for FILE in "${FILES[@]}"; do
    URL="https://huggingface.co/${REPO}/resolve/${REVISION}/${FILE}"
    CURL_ARGS=(-fsSIL --retry 2 --retry-delay 1 --retry-all-errors)
    if curl "${CURL_ARGS[@]}" "$URL" >/dev/null; then
      echo "OK $KEY $FILE"
    else
      echo "ERROR: missing artifact: $URL" >&2
      FAILED=$((FAILED + 1))
    fi
  done
done

if [[ "$FAILED" -gt 0 ]]; then
  echo "model registry validation failed: $FAILED missing/broken artifact(s)" >&2
  exit 1
fi

echo "model registry validation passed (${#MODEL_KEYS[@]} model keys, tier=${TIER}, all_tiers=${VALIDATE_ALL_TIERS})"
