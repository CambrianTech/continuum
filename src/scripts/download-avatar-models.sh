#!/bin/bash
# Download VRM avatar models for AI persona video rendering
# All models are CC0 (public domain) — no attribution required, commercial use OK
#
# Sources:
#   - VRoid Studio CC0 samples (OpenGameArt) — anime style, full blend shapes + spring bones
#   - 100Avatars by Polygonal Mind (Arweave) — low-poly stylized, CC0
#
# Called automatically by npm start if models don't exist

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/shared/preflight.sh"

MODELS_DIR="models/avatars"
mkdir -p "$MODELS_DIR"

# Track how many we download vs already have
DOWNLOADED=0
EXISTING=0

download_vrm() {
  local name="$1"
  local url="$2"
  local dest="$MODELS_DIR/${name}.vrm"

  if [ -f "$dest" ] && [ "$(wc -c < "$dest")" -gt 10000 ]; then
    EXISTING=$((EXISTING + 1))
    return
  fi

  echo -e "  ${YELLOW}Downloading ${name}...${NC}"
  if command -v curl &> /dev/null; then
    curl -sL --progress-bar -o "$dest" "$url"
  elif command -v wget &> /dev/null; then
    wget -q --show-progress -O "$dest" "$url"
  fi

  if [ -f "$dest" ] && [ "$(wc -c < "$dest")" -gt 10000 ]; then
    DOWNLOADED=$((DOWNLOADED + 1))
  else
    echo -e "  ${RED}Failed to download ${name}${NC}"
    rm -f "$dest"
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

  local tmpzip=$(mktemp /tmp/vrm_XXXXXX.zip)
  local tmpdir=$(mktemp -d /tmp/vrm_extract_XXXXXX)

  echo -e "  ${YELLOW}Downloading ${name} (zip)...${NC}"
  if command -v curl &> /dev/null; then
    curl -sL --progress-bar -o "$tmpzip" "$url"
  elif command -v wget &> /dev/null; then
    wget -q --show-progress -O "$tmpzip" "$url"
  fi

  # Verify download is a valid zip (must be > 10KB and start with PK signature)
  local filesize=$(wc -c < "$tmpzip" 2>/dev/null || echo 0)
  if [ "$filesize" -lt 10000 ]; then
    echo -e "  ${RED}Downloaded file too small (${filesize} bytes) for ${name} — likely a 404 or empty response${NC}"
    rm -rf "$tmpzip" "$tmpdir"
    return
  fi

  # Extract zip — use python3 (always available) so we don't need unzip installed
  if ! python3 -c "
import zipfile, sys
try:
    with zipfile.ZipFile('$tmpzip', 'r') as z:
        z.extractall('$tmpdir')
except (zipfile.BadZipFile, Exception) as e:
    print(f'Extract failed: {e}', file=sys.stderr)
    sys.exit(1)
"; then
    echo -e "  ${RED}Failed to extract ${name}: file may be corrupt or not a zip${NC}"
    rm -rf "$tmpzip" "$tmpdir"
    return
  fi
  local vrm_file=$(find "$tmpdir" -iname "*.vrm" -type f | head -1)

  if [ -n "$vrm_file" ] && [ -f "$vrm_file" ]; then
    mv "$vrm_file" "$dest"
    DOWNLOADED=$((DOWNLOADED + 1))
  else
    echo -e "  ${RED}No .vrm found in ${name} zip${NC}"
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
if [ "$DOWNLOADED" -gt 0 ]; then
  echo -e "${GREEN}Avatar models: ${DOWNLOADED} downloaded, ${EXISTING} already existed (${TOTAL}/8 total)${NC}"
elif [ "$EXISTING" -eq 8 ]; then
  echo -e "${GREEN}All 8 avatar models already exist${NC}"
else
  echo -e "${YELLOW}Avatar models: ${TOTAL}/8 present${NC}"
fi
