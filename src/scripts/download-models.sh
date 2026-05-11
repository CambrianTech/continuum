#!/bin/bash
# download-models.sh — Reads src/shared/models.json and downloads every
# model listed in `auto_download.always` plus the tier-specific set. Runs
# in the model-init container.
#
# Replaces the previous Mac-only `docker model pull` flow + the hardcoded
# URL list in download-voice-models.sh. ONE source of truth (models.json)
# means swapping a model is a single edit there — this script and all
# other consumers pick it up automatically.
#
# Per Joel's rule (2026-05-04): "all the models must download and run on
# GPU" — no DMR dependency. Continuum-core loads everything via its
# built-in llama.cpp via the host GPU (Metal / CUDA / Vulkan ICD).
#
# Env:
#   MODELS_DIR=/models  (the volume mount; default /models)
#   TIER=full           (mba | mid | full; defaults to full if RAM ≥ 32GB)
#   REGISTRY=/app/shared/models.json  (path to registry inside container)

set -euo pipefail

MODELS_DIR="${MODELS_DIR:-/models}"
REGISTRY="${REGISTRY:-/app/shared/models.json}"

# Auto-detect tier from total RAM if not set. Mirrors install.sh tier
# logic + ModelRegistry.tierFromRamGB() — keep consistent.
if [[ -z "${TIER:-}" ]]; then
  if [[ -f /proc/meminfo ]]; then
    RAM_KB=$(grep MemTotal /proc/meminfo | awk '{print $2}')
    RAM_GB=$((RAM_KB / 1024 / 1024))
  else
    RAM_GB=32  # fallback assume full tier
  fi
  if   [[ "$RAM_GB" -ge 32 ]]; then TIER=full
  elif [[ "$RAM_GB" -ge 24 ]]; then TIER=mid
  else                              TIER=mba
  fi
fi

YELLOW='\033[1;33m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

mkdir -p "$MODELS_DIR"

echo -e "${YELLOW}━━━ download-models.sh — registry-driven model download ━━━${NC}"
echo "  REGISTRY: $REGISTRY"
echo "  MODELS_DIR: $MODELS_DIR"
echo "  TIER: $TIER"
echo ""

if [[ ! -f "$REGISTRY" ]]; then
  echo -e "${RED}ERROR: registry file $REGISTRY not found in container.${NC}" >&2
  echo "  Check model-init.Dockerfile COPY of src/shared/models.json." >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo -e "${RED}ERROR: jq not installed in this image.${NC}" >&2
  echo "  Add 'jq' to the apt-get line in model-init.Dockerfile." >&2
  exit 1
fi

# Validate TIER against the canonical set BEFORE the jq lookup. Without
# this, an unknown tier (e.g. legacy `primary` from older install.sh)
# would silently produce an empty `by_tier` set — install ships only
# voice models and personas have no local Qwen at runtime. That was the
# 2026-05-11 RTX 5090 silent-no-replies root cause. Fail loud per Joel's
# 'no silent fallback to placeholder models' rule.
case "$TIER" in
  mba|mid|full) ;;
  *)
    echo -e "${RED}ERROR: TIER='${TIER}' is not a canonical tier name.${NC}" >&2
    echo "  Valid: mba | mid | full (canon: src/shared/models.json auto_download.by_tier keys)." >&2
    echo "  Likely cause: install.sh CONTINUUM_TIER (e.g. legacy 'primary') diverged from registry. Align both ends." >&2
    exit 1
    ;;
esac

# Compute the download set: always[] + by_tier[$TIER][]
mapfile -t MODEL_KEYS < <(jq -r --arg tier "$TIER" '
  [
    .auto_download.always[],
    (.auto_download.by_tier[$tier] // [])[]
  ] | unique | .[]
' "$REGISTRY")

echo -e "${YELLOW}Models to download (${#MODEL_KEYS[@]}): ${MODEL_KEYS[*]}${NC}"
echo ""

# Download via huggingface direct-URL pattern: each model has files[].
# We resolve to https://huggingface.co/<repo>/resolve/main/<file> and curl.
# The huggingface-cli would be cleaner but adds Python+pip to model-init
# (currently a tiny node:slim image, ~120MB). Direct curl keeps it lean.
for KEY in "${MODEL_KEYS[@]}"; do
  KIND=$(jq -r --arg k "$KEY" '.models[$k].kind // "unknown"' "$REGISTRY")
  REPO=$(jq -r --arg k "$KEY" '.models[$k].hf_repo // ""' "$REGISTRY")
  FORMAT=$(jq -r --arg k "$KEY" '.models[$k].format // ""' "$REGISTRY")
  SIZE=$(jq -r --arg k "$KEY" '.models[$k].size_gb // "?"' "$REGISTRY")

  if [[ -z "$REPO" ]]; then
    echo -e "${YELLOW}  SKIP $KEY — no hf_repo in registry${NC}"
    continue
  fi
  # Skip candle-builtin formats (continuum-core loads from rust-bert / candle direct)
  if [[ "$FORMAT" == "candle-builtin" ]]; then
    echo -e "${GREEN}  SKIP $KEY — format=candle-builtin (loaded in-process by continuum-core)${NC}"
    continue
  fi

  TARGET_DIR="$MODELS_DIR/$KEY"
  mkdir -p "$TARGET_DIR"

  # Get files list. Some entries omit files (huggingface-cli style); skip those.
  mapfile -t FILES < <(jq -r --arg k "$KEY" '.models[$k].files // [] | .[]' "$REGISTRY")
  if [[ ${#FILES[@]} -eq 0 ]]; then
    echo -e "${YELLOW}  SKIP $KEY — no files[] specified (huggingface-cli pull required)${NC}"
    continue
  fi

  echo -e "${YELLOW}━━ $KEY (kind=$KIND, ~${SIZE}GB) ━━${NC}"
  for FILE in "${FILES[@]}"; do
    DEST="$TARGET_DIR/$(basename "$FILE")"
    if [[ -f "$DEST" ]]; then
      echo -e "${GREEN}  ✓ already cached: $(basename "$FILE")${NC}"
      continue
    fi
    URL="https://huggingface.co/${REPO}/resolve/main/${FILE}"
    echo "  ↓ $URL"
    if curl -fsSL --retry 3 --retry-delay 2 -o "$DEST.partial" "$URL"; then
      mv "$DEST.partial" "$DEST"
      echo -e "${GREEN}  ✓ $(basename "$FILE") ($(du -h "$DEST" | cut -f1))${NC}"
    else
      rm -f "$DEST.partial"
      echo -e "${RED}  ✗ FAILED to download $FILE${NC}" >&2
      # Continue rather than fail-the-container — partial models is better
      # than no models. continuum-core will report missing-file at load time.
    fi
  done
done

echo ""
echo -e "${GREEN}━━ download-models.sh complete (TIER=$TIER) ━━${NC}"
echo "  Total in $MODELS_DIR: $(du -sh "$MODELS_DIR" 2>/dev/null | cut -f1)"
