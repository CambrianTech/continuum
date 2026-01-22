#!/bin/bash
# Download voice models for streaming-core (STT/TTS)
# Called automatically by npm start if models don't exist

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Models go in jtag/models (relative to where binary runs)
MODELS_DIR="models"
WHISPER_DIR="$MODELS_DIR/whisper"
KOKORO_DIR="$MODELS_DIR/kokoro"

# Create directories
mkdir -p "$WHISPER_DIR"
mkdir -p "$KOKORO_DIR"

# Whisper model (for STT)
WHISPER_MODEL="$WHISPER_DIR/ggml-base.en.bin"
WHISPER_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"

if [ ! -f "$WHISPER_MODEL" ]; then
  echo -e "${YELLOW}Downloading Whisper STT model (~140MB)...${NC}"
  if command -v curl &> /dev/null; then
    curl -L --progress-bar -o "$WHISPER_MODEL" "$WHISPER_URL"
  elif command -v wget &> /dev/null; then
    wget -q --show-progress -O "$WHISPER_MODEL" "$WHISPER_URL"
  else
    echo -e "${RED}Error: curl or wget required to download models${NC}"
    exit 1
  fi
  echo -e "${GREEN}Whisper model downloaded${NC}"
else
  echo -e "${GREEN}Whisper model already exists${NC}"
fi

# Kokoro TTS model - check if ONNX exports are available
# Note: Kokoro official ONNX exports may vary - this is a placeholder
KOKORO_MODEL="$KOKORO_DIR/kokoro-v0_19.onnx"

if [ ! -f "$KOKORO_MODEL" ]; then
  echo -e "${YELLOW}Kokoro TTS model not found${NC}"
  echo -e "${YELLOW}Manual download required from: https://huggingface.co/hexgrad/Kokoro-82M${NC}"
  echo -e "${YELLOW}Place ONNX file in: $KOKORO_MODEL${NC}"
  echo -e "${YELLOW}TTS will use silence fallback until model is available${NC}"
  # Don't fail - TTS has a graceful fallback
else
  echo -e "${GREEN}Kokoro model already exists${NC}"
fi

echo -e "${GREEN}Voice model check complete${NC}"
