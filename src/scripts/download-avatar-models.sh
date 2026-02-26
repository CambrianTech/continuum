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

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

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

  # Extract and find the .vrm file
  unzip -q -o "$tmpzip" -d "$tmpdir" 2>/dev/null || true
  local vrm_file=$(find "$tmpdir" -name "*.vrm" -type f | head -1)

  if [ -n "$vrm_file" ] && [ -f "$vrm_file" ]; then
    mv "$vrm_file" "$dest"
    DOWNLOADED=$((DOWNLOADED + 1))
  else
    echo -e "  ${RED}No .vrm found in ${name} zip${NC}"
  fi

  rm -rf "$tmpzip" "$tmpdir"
}

echo -e "${YELLOW}Checking VRM avatar models (14 CC0 models)...${NC}"

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

# ============================================================================
# 100Avatars by Polygonal Mind (low-poly stylized, CC0)
# Source: https://github.com/ToxSam/open-source-avatars
# ============================================================================

echo -e "${YELLOW}100Avatars stylized models (6 models):${NC}"

download_vrm "100av-rose" \
  "https://arweave.net/Ea1KXujzJatQgCFSMzGOzp_UtHqB1pyia--U3AtkMAY"

download_vrm "100av-robert" \
  "https://arweave.net/gwG7w4bY-A5c3R6A6GOz3xBCgbPvkFQmqPIDtvnNsYI"

download_vrm "100av-olivia" \
  "https://arweave.net/MgsNlTetzAoVEC6E-lswj65vp7StkOZXXd5OjjqzYZI"

download_vrm "100av-mikel" \
  "https://arweave.net/-eJyDjujQRvakRImdvulg-1dKQkPwMeQv-55IbKqLh4"

download_vrm "100av-amazonas" \
  "https://arweave.net/fqZDwToo41u1a7VnHhZX1BTK5lktXpK_H6H20MVbPqQ"

download_vrm "100av-dinokid" \
  "https://arweave.net/T1gkB95XKXAZl_VmU1ozg5Txm--o9nY0Nge3s8zNoBs"

# ============================================================================
# Summary
# ============================================================================

TOTAL=$((DOWNLOADED + EXISTING))
if [ "$DOWNLOADED" -gt 0 ]; then
  echo -e "${GREEN}Avatar models: ${DOWNLOADED} downloaded, ${EXISTING} already existed (${TOTAL}/14 total)${NC}"
elif [ "$EXISTING" -eq 14 ]; then
  echo -e "${GREEN}All 14 avatar models already exist${NC}"
else
  echo -e "${YELLOW}Avatar models: ${TOTAL}/14 present${NC}"
fi
