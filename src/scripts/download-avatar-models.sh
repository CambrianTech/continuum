#!/bin/bash
# Download VRM avatar models for AI persona video rendering
# All models are CC0 (public domain) — no attribution required, commercial use OK
#
# Sources:
#   - VRoid Studio CC0 samples (OpenGameArt) — anime style, full blend shapes + spring bones
#   - 100Avatars by Polygonal Mind (Arweave) — low-poly stylized, CC0
#
# Called automatically by npm start if models don't exist
#
# Failure policy (continuum#1087): per-VRM download failure is NON-FATAL.
# Third-party CDN flakes (OpenGameArt has been observed returning curl exit 11
# = CURLE_FTP_WEIRD_PASS_REPLY) must NOT block the model-init container from
# completing — every other model in the chain (Qwen, voice, embeddings) has
# already downloaded by the time this script runs, and a partial-avatar set is
# strictly better than blocking the install. Each per-VRM failure logs a
# structured warning so the operator sees the actual exit code (Joel's "never
# swallow errors" rule); the run summary at the end reports failed-vs-total
# count, but the script returns 0 so the model-init container is healthy.

set -eu  # NOTE: no pipefail and no -e on the per-VRM curl/extract calls

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/shared/preflight.sh"

# Override root with MODELS_DIR env var for Docker volume mounts
MODELS_DIR="${MODELS_DIR:-models}/avatars"
mkdir -p "$MODELS_DIR"

# Track how many we download vs already have vs failed
DOWNLOADED=0
EXISTING=0
FAILED=0
FAILED_NAMES=()

download_vrm() {
  local name="$1"
  local url="$2"
  local dest="$MODELS_DIR/${name}.vrm"

  if [ -f "$dest" ] && [ "$(wc -c < "$dest")" -gt 10000 ]; then
    EXISTING=$((EXISTING + 1))
    return
  fi

  echo -e "  ${YELLOW}Downloading ${name}...${NC}"
  # set +e for the curl/wget call: per-VRM failure is non-fatal (continuum#1087).
  # Capture the exit code so we can log it — never swallow silently.
  local curl_ec=0
  if command -v curl &> /dev/null; then
    set +e
    curl -sL --progress-bar -o "$dest" "$url"
    curl_ec=$?
    set -e
  elif command -v wget &> /dev/null; then
    set +e
    wget -q --show-progress -O "$dest" "$url"
    curl_ec=$?
    set -e
  fi

  if [ -f "$dest" ] && [ "$(wc -c < "$dest")" -gt 10000 ]; then
    DOWNLOADED=$((DOWNLOADED + 1))
  else
    echo -e "  ${RED}⚠ Failed to download ${name} (curl exit ${curl_ec}, source: ${url}) — continuing${NC}" >&2
    rm -f "$dest"
    FAILED=$((FAILED + 1))
    FAILED_NAMES+=("$name")
  fi
}

# Download a VRoid zip, extract the .vrm, clean up
download_vroid_zip() {
  local name="$1"
  local url="$2"
  local dest="$MODELS_DIR/${name}.vrm"

  if [ -f "$dest" ] && [ "$(wc -c < "$dest")" -gt 10000 ]; then
    EXISTING=$((EXISTING + 1))
    return
  fi

  local tmpzip
  tmpzip=$(mktemp /tmp/vrm_XXXXXX.zip)
  local tmpdir
  tmpdir=$(mktemp -d /tmp/vrm_extract_XXXXXX)

  echo -e "  ${YELLOW}Downloading ${name} (zip)...${NC}"
  # set +e for curl: per-VRM failure non-fatal (continuum#1087). OpenGameArt has
  # been observed returning curl exit 11 (CURLE_FTP_WEIRD_PASS_REPLY) on this
  # endpoint; capture the code, log it, move on.
  local curl_ec=0
  if command -v curl &> /dev/null; then
    set +e
    curl -sL --progress-bar -o "$tmpzip" "$url"
    curl_ec=$?
    set -e
  elif command -v wget &> /dev/null; then
    set +e
    wget -q --show-progress -O "$tmpzip" "$url"
    curl_ec=$?
    set -e
  fi

  if [ "$curl_ec" -ne 0 ]; then
    echo -e "  ${RED}⚠ Download failed for ${name} (curl exit ${curl_ec}, source: ${url}) — continuing${NC}" >&2
    rm -rf "$tmpzip" "$tmpdir"
    FAILED=$((FAILED + 1))
    FAILED_NAMES+=("$name")
    return
  fi

  # Verify download is a valid zip (must be > 10KB and start with PK signature)
  local filesize
  filesize=$(wc -c < "$tmpzip" 2>/dev/null || echo 0)
  if [ "$filesize" -lt 10000 ]; then
    echo -e "  ${RED}⚠ Downloaded file too small (${filesize} bytes) for ${name} — likely a 404 or empty response${NC}" >&2
    rm -rf "$tmpzip" "$tmpdir"
    FAILED=$((FAILED + 1))
    FAILED_NAMES+=("$name")
    return
  fi

  # Extract zip. model-init images include unzip; local dev machines often
  # have python3. Require one explicit extractor and report which path failed.
  if command -v unzip >/dev/null 2>&1; then
    if ! unzip -q "$tmpzip" -d "$tmpdir"; then
      echo -e "  ${RED}⚠ Failed to extract ${name}: unzip rejected archive${NC}" >&2
      rm -rf "$tmpzip" "$tmpdir"
      FAILED=$((FAILED + 1))
      FAILED_NAMES+=("$name")
      return
    fi
  elif command -v python3 >/dev/null 2>&1; then
    if ! python3 -c "
import zipfile, sys
try:
    with zipfile.ZipFile('$tmpzip', 'r') as z:
        z.extractall('$tmpdir')
except (zipfile.BadZipFile, Exception) as e:
    print(f'Extract failed: {e}', file=sys.stderr)
    sys.exit(1)
"; then
      echo -e "  ${RED}⚠ Failed to extract ${name}: python3 rejected archive${NC}" >&2
      rm -rf "$tmpzip" "$tmpdir"
      FAILED=$((FAILED + 1))
      FAILED_NAMES+=("$name")
      return
    fi
  else
    echo -e "  ${RED}⚠ Failed to extract ${name}: no unzip or python3 available${NC}" >&2
    rm -rf "$tmpzip" "$tmpdir"
    FAILED=$((FAILED + 1))
    FAILED_NAMES+=("$name")
    return
  fi
  local vrm_file
  vrm_file=$(find "$tmpdir" -iname "*.vrm" -type f | head -1)

  if [ -n "$vrm_file" ] && [ -f "$vrm_file" ]; then
    mv "$vrm_file" "$dest"
    DOWNLOADED=$((DOWNLOADED + 1))
  else
    echo -e "  ${RED}⚠ No .vrm found in ${name} zip — continuing${NC}" >&2
    FAILED=$((FAILED + 1))
    FAILED_NAMES+=("$name")
  fi

  rm -rf "$tmpzip" "$tmpdir"
}

echo -e "${YELLOW}Checking VRM avatar models (8 CC0 models)...${NC}"

# ============================================================================
# VRoid Studio CC0 Models (anime style, full VRM features)
# Source: https://opengameart.org/content/vroid-studio-cc0-models
# ============================================================================

echo -e "${YELLOW}VRoid Studio anime avatars (8 models):${NC}"

download_vroid_zip "vroid-female-base" \
  "https://opengameart.org/sites/default/files/base_female.zip"

download_vroid_zip "vroid-male-base" \
  "https://opengameart.org/sites/default/files/base_male.zip"

download_vroid_zip "vroid-sakurada" \
  "https://opengameart.org/sites/default/files/sakurada_fumiriya.zip"

download_vroid_zip "vroid-shino" \
  "https://opengameart.org/sites/default/files/sendagaya_shino.zip"

download_vroid_zip "vroid-darkness" \
  "https://opengameart.org/sites/default/files/avatarsample_d_darkness.zip"

download_vroid_zip "vroid-sample-d" \
  "https://opengameart.org/sites/default/files/avatarsample_d_0.zip"

download_vroid_zip "vroid-sample-e" \
  "https://opengameart.org/sites/default/files/avatarsample_e.zip"

download_vroid_zip "vroid-sample-f" \
  "https://opengameart.org/sites/default/files/avatarsample_f.zip"

# 100Avatars REMOVED — 2D flat models, look terrible next to 3D VRoid models.
# Need proper 3D CC0 models to expand the catalog beyond 8.

# ============================================================================
# Summary
# ============================================================================

TOTAL=$((DOWNLOADED + EXISTING))
EXPECTED=8
if [ "$FAILED" -gt 0 ]; then
  # Degraded summary — script still returns 0 (continuum#1087) so model-init
  # container is healthy, but the operator sees exactly which avatars failed.
  echo -e "${YELLOW}━━ avatar download DEGRADED — ${FAILED} of ${EXPECTED} failed ━━${NC}" >&2
  echo -e "${YELLOW}  failed: ${FAILED_NAMES[*]}${NC}" >&2
  echo -e "${YELLOW}  succeeded: ${TOTAL}/${EXPECTED} (downloaded=${DOWNLOADED}, cached=${EXISTING})${NC}" >&2
  echo -e "${YELLOW}  cause is upstream (CDN flake / 404 / rate limit) — not a Continuum bug${NC}" >&2
  echo -e "${YELLOW}  re-run: docker compose run model-init    (or: ./scripts/download-avatar-models.sh)${NC}" >&2
elif [ "$DOWNLOADED" -gt 0 ]; then
  echo -e "${GREEN}Avatar models: ${DOWNLOADED} downloaded, ${EXISTING} already existed (${TOTAL}/${EXPECTED} total)${NC}"
elif [ "$EXISTING" -eq "$EXPECTED" ]; then
  echo -e "${GREEN}All ${EXPECTED} avatar models already exist${NC}"
else
  echo -e "${YELLOW}Avatar models: ${TOTAL}/${EXPECTED} present${NC}"
fi

# Always exit 0 (continuum#1087): partial avatar set is acceptable; downstream
# (Bevy live mode) gracefully degrades to whatever VRMs are present. Failing
# the model-init container blocks the whole install for a third-party CDN
# blip — that trade is wrong. The summary above carries the diagnostic.
exit 0
