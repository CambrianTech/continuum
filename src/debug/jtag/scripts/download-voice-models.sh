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
PIPER_DIR="$MODELS_DIR/piper"

# Create directories
mkdir -p "$WHISPER_DIR"
mkdir -p "$PIPER_DIR"

# Whisper model (for STT) - UPGRADED to medium for better accuracy
WHISPER_MODEL="$WHISPER_DIR/ggml-medium.en.bin"
WHISPER_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin"

if [ ! -f "$WHISPER_MODEL" ]; then
  echo -e "${YELLOW}Downloading Whisper medium.en model (~1.5GB) for better accuracy...${NC}"
  echo -e "${YELLOW}This may take a few minutes on first install${NC}"
  if command -v curl &> /dev/null; then
    curl -L --progress-bar -o "$WHISPER_MODEL" "$WHISPER_URL"
  elif command -v wget &> /dev/null; then
    wget -q --show-progress -O "$WHISPER_MODEL" "$WHISPER_URL"
  else
    echo -e "${RED}Error: curl or wget required to download models${NC}"
    exit 1
  fi
  echo -e "${GREEN}Whisper medium model downloaded (better transcription accuracy)${NC}"
else
  echo -e "${GREEN}Whisper model already exists${NC}"
fi

# Piper TTS model (high quality, native ONNX - no Python conversion needed!)
PIPER_MODEL="$PIPER_DIR/en_US-libritts_r-medium.onnx"
PIPER_CONFIG="$PIPER_DIR/en_US-libritts_r-medium.onnx.json"
PIPER_MODEL_URL="https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/libritts_r/medium/en_US-libritts_r-medium.onnx"
PIPER_CONFIG_URL="https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/libritts_r/medium/en_US-libritts_r-medium.onnx.json"

if [ ! -f "$PIPER_MODEL" ] || [ ! -f "$PIPER_CONFIG" ]; then
  echo -e "${YELLOW}Downloading Piper TTS model (high quality, ~60MB)...${NC}"

  if command -v curl &> /dev/null; then
    curl -L --progress-bar -o "$PIPER_MODEL" "$PIPER_MODEL_URL"
    curl -L --progress-bar -o "$PIPER_CONFIG" "$PIPER_CONFIG_URL"
  elif command -v wget &> /dev/null; then
    wget -q --show-progress -O "$PIPER_MODEL" "$PIPER_MODEL_URL"
    wget -q --show-progress -O "$PIPER_CONFIG" "$PIPER_CONFIG_URL"
  else
    echo -e "${RED}Error: curl or wget required to download models${NC}"
    exit 1
  fi
  echo -e "${GREEN}Piper TTS model downloaded (high-quality voice synthesis)${NC}"
else
  echo -e "${GREEN}Piper TTS model already exists${NC}"
fi

echo -e "${GREEN}Voice model check complete${NC}"
