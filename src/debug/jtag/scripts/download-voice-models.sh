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

# Load config.env if it exists to get WHISPER_MODEL preference
CONFIG_FILE="$HOME/.continuum/config.env"
if [ -f "$CONFIG_FILE" ]; then
  # Source the config to load WHISPER_MODEL
  set -a  # Export all variables
  source "$CONFIG_FILE"
  set +a
fi

# Default to large-v3-turbo if not set (best balance of speed + accuracy)
WHISPER_MODEL_NAME="${WHISPER_MODEL:-large-v3-turbo}"

# Map model name to filename and URL
case "$WHISPER_MODEL_NAME" in
  "base")
    WHISPER_FILE="ggml-base.en.bin"
    WHISPER_SIZE="~74MB"
    WHISPER_DESC="fastest, ~60-70% accuracy"
    ;;
  "small")
    WHISPER_FILE="ggml-small.en.bin"
    WHISPER_SIZE="~244MB"
    WHISPER_DESC="fast, ~75-80% accuracy"
    ;;
  "medium")
    WHISPER_FILE="ggml-medium.en.bin"
    WHISPER_SIZE="~1.5GB"
    WHISPER_DESC="balanced, ~75-85% accuracy"
    ;;
  "large-v3")
    WHISPER_FILE="ggml-large-v3.bin"
    WHISPER_SIZE="~3GB"
    WHISPER_DESC="best accuracy ~90-95%, slower"
    ;;
  "large-v3-turbo")
    WHISPER_FILE="ggml-large-v3-turbo.bin"
    WHISPER_SIZE="~1.5GB"
    WHISPER_DESC="best balance ~90-95% accuracy, 6x faster"
    ;;
  *)
    echo -e "${YELLOW}Warning: Unknown WHISPER_MODEL='$WHISPER_MODEL_NAME', defaulting to large-v3-turbo${NC}"
    WHISPER_FILE="ggml-large-v3-turbo.bin"
    WHISPER_SIZE="~1.5GB"
    WHISPER_DESC="best balance ~90-95% accuracy"
    ;;
esac

WHISPER_MODEL_PATH="$WHISPER_DIR/$WHISPER_FILE"
WHISPER_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$WHISPER_FILE"

if [ ! -f "$WHISPER_MODEL_PATH" ]; then
  echo -e "${YELLOW}Downloading Whisper $WHISPER_MODEL_NAME model ($WHISPER_SIZE) - $WHISPER_DESC${NC}"
  echo -e "${YELLOW}This may take a few minutes on first install${NC}"
  if command -v curl &> /dev/null; then
    curl -L --progress-bar -o "$WHISPER_MODEL_PATH" "$WHISPER_URL"
  elif command -v wget &> /dev/null; then
    wget -q --show-progress -O "$WHISPER_MODEL_PATH" "$WHISPER_URL"
  else
    echo -e "${RED}Error: curl or wget required to download models${NC}"
    exit 1
  fi
  echo -e "${GREEN}Whisper $WHISPER_MODEL_NAME model downloaded${NC}"
else
  echo -e "${GREEN}Whisper $WHISPER_MODEL_NAME model already exists${NC}"
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
