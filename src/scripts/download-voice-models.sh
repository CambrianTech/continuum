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

# Kokoro TTS model (fast, high quality, local ONNX)
KOKORO_DIR="$MODELS_DIR/kokoro"
KOKORO_VOICES_DIR="$KOKORO_DIR/voices"
KOKORO_MODEL="$KOKORO_DIR/model_quantized.onnx"
KOKORO_TOKENIZER="$KOKORO_DIR/tokenizer.json"
KOKORO_VOICE="$KOKORO_VOICES_DIR/af.bin"

KOKORO_BASE_URL="https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main"
KOKORO_MODEL_URL="$KOKORO_BASE_URL/onnx/model_quantized.onnx"
KOKORO_TOKENIZER_URL="$KOKORO_BASE_URL/tokenizer.json"
KOKORO_VOICE_URL="$KOKORO_BASE_URL/voices/af.bin"

mkdir -p "$KOKORO_VOICES_DIR"

# Clean up stale vocab.json (wrong URL in earlier versions, produces 15-byte error file)
if [ -f "$KOKORO_DIR/vocab.json" ] && [ "$(wc -c < "$KOKORO_DIR/vocab.json")" -lt 100 ]; then
  rm -f "$KOKORO_DIR/vocab.json"
fi

if [ ! -f "$KOKORO_MODEL" ] || [ ! -f "$KOKORO_TOKENIZER" ] || [ ! -f "$KOKORO_VOICE" ]; then
  echo -e "${YELLOW}Downloading Kokoro TTS model (82M, high quality, ~40-80MB)...${NC}"

  if command -v curl &> /dev/null; then
    [ ! -f "$KOKORO_MODEL" ] && curl -L --progress-bar -o "$KOKORO_MODEL" "$KOKORO_MODEL_URL"
    [ ! -f "$KOKORO_TOKENIZER" ] && curl -L --progress-bar -o "$KOKORO_TOKENIZER" "$KOKORO_TOKENIZER_URL"
    [ ! -f "$KOKORO_VOICE" ] && curl -L --progress-bar -o "$KOKORO_VOICE" "$KOKORO_VOICE_URL"
  elif command -v wget &> /dev/null; then
    [ ! -f "$KOKORO_MODEL" ] && wget -q --show-progress -O "$KOKORO_MODEL" "$KOKORO_MODEL_URL"
    [ ! -f "$KOKORO_TOKENIZER" ] && wget -q --show-progress -O "$KOKORO_TOKENIZER" "$KOKORO_TOKENIZER_URL"
    [ ! -f "$KOKORO_VOICE" ] && wget -q --show-progress -O "$KOKORO_VOICE" "$KOKORO_VOICE_URL"
  fi

  if [ -f "$KOKORO_MODEL" ] && [ -f "$KOKORO_TOKENIZER" ] && [ -f "$KOKORO_VOICE" ]; then
    echo -e "${GREEN}Kokoro TTS model downloaded${NC}"
  else
    echo -e "${YELLOW}Kokoro TTS download incomplete${NC}"
  fi
else
  echo -e "${GREEN}Kokoro TTS model already exists${NC}"
fi

# Pocket-TTS voice embeddings (voice cloning presets)
# Model weights auto-download via HF hub on first use (~236MB, gated — requires HF_TOKEN)
# Voice embeddings are pre-computed audio prompts for 8 preset voices
POCKET_DIR="$MODELS_DIR/pocket-tts/voices"
POCKET_BASE_URL="https://huggingface.co/kyutai/pocket-tts/resolve/main/embeddings"
POCKET_VOICES="alba cosette eponine fantine javert jean marius azelma"

mkdir -p "$POCKET_DIR"

# Check if any voices are missing
POCKET_MISSING=false
for voice in $POCKET_VOICES; do
  if [ ! -f "$POCKET_DIR/${voice}.safetensors" ]; then
    POCKET_MISSING=true
    break
  fi
done

if [ "$POCKET_MISSING" = true ]; then
  if [ -n "$HF_TOKEN" ]; then
    echo -e "${YELLOW}Downloading Pocket-TTS voice embeddings (8 voices, ~4MB total)...${NC}"
    for voice in $POCKET_VOICES; do
      if [ ! -f "$POCKET_DIR/${voice}.safetensors" ]; then
        echo -e "  Downloading ${voice}..."
        if command -v curl &> /dev/null; then
          curl -sL -H "Authorization: Bearer $HF_TOKEN" -o "$POCKET_DIR/${voice}.safetensors" "$POCKET_BASE_URL/${voice}.safetensors"
        elif command -v wget &> /dev/null; then
          wget -q --header="Authorization: Bearer $HF_TOKEN" -O "$POCKET_DIR/${voice}.safetensors" "$POCKET_BASE_URL/${voice}.safetensors"
        fi
      fi
    done
    echo -e "${GREEN}Pocket-TTS voice embeddings downloaded${NC}"
  else
    echo -e "${YELLOW}Pocket-TTS: Skipping voice embeddings (no HF_TOKEN in config.env)${NC}"
    echo -e "${YELLOW}  To enable: 1) Accept terms at https://huggingface.co/kyutai/pocket-tts${NC}"
    echo -e "${YELLOW}  2) Set HF_TOKEN in ~/.continuum/config.env${NC}"
  fi
else
  echo -e "${GREEN}Pocket-TTS voice embeddings already exist${NC}"
fi

# Silero VAD model (voice activity detection)
SILERO_DIR="$MODELS_DIR/vad"
SILERO_MODEL="$SILERO_DIR/silero_vad.onnx"
SILERO_URL="https://huggingface.co/onnx-community/silero-vad/resolve/main/onnx/model.onnx"

mkdir -p "$SILERO_DIR"

if [ ! -f "$SILERO_MODEL" ]; then
  echo -e "${YELLOW}Downloading Silero VAD model (~2MB)...${NC}"
  if command -v curl &> /dev/null; then
    curl -L --progress-bar -o "$SILERO_MODEL" "$SILERO_URL"
  elif command -v wget &> /dev/null; then
    wget -q --show-progress -O "$SILERO_MODEL" "$SILERO_URL"
  fi
  echo -e "${GREEN}Silero VAD model downloaded${NC}"
else
  echo -e "${GREEN}Silero VAD model already exists${NC}"
fi

# Orpheus TTS (3B, LoRA-trainable — auto-download Q4_K_M quantized)
# Model: Llama-3B fine-tuned for expressive speech with emotion tags
# GGUF via Candle for inference, Unsloth for LoRA training
# 3 files needed: GGUF model (~2.4GB), tokenizer, SNAC audio codec decoder
ORPHEUS_DIR="$MODELS_DIR/orpheus"
ORPHEUS_GGUF="$ORPHEUS_DIR/orpheus-3b-0.1-ft-q4_k_m.gguf"
ORPHEUS_TOKENIZER="$ORPHEUS_DIR/tokenizer.json"
ORPHEUS_SNAC="$ORPHEUS_DIR/snac_decoder.onnx"

ORPHEUS_GGUF_URL="https://huggingface.co/isaiahbjork/orpheus-3b-0.1-ft-Q4_K_M-GGUF/resolve/main/orpheus-3b-0.1-ft-q4_k_m.gguf"
ORPHEUS_TOKENIZER_URL="https://huggingface.co/canopylabs/orpheus-3b-0.1-ft/resolve/main/tokenizer.json"
ORPHEUS_SNAC_URL="https://huggingface.co/laion/SNAC-24khz-decoder-onnx/resolve/main/snac24_int2wav_static.onnx"

mkdir -p "$ORPHEUS_DIR"

# Clean up stale tokenizer (earlier versions downloaded error page instead of actual file)
if [ -f "$ORPHEUS_TOKENIZER" ] && [ "$(wc -c < "$ORPHEUS_TOKENIZER")" -lt 10000 ]; then
  echo -e "${YELLOW}Orpheus tokenizer is stale (error file), removing...${NC}"
  rm -f "$ORPHEUS_TOKENIZER"
fi

if [ ! -f "$ORPHEUS_GGUF" ] || [ ! -f "$ORPHEUS_TOKENIZER" ] || [ ! -f "$ORPHEUS_SNAC" ]; then
  echo -e "${YELLOW}Downloading Orpheus TTS (3B, LoRA-trainable, ~2.5GB total)...${NC}"
  echo -e "${YELLOW}  GGUF model (~2.4GB) + tokenizer (~22MB, requires HF_TOKEN) + SNAC decoder (~53MB)${NC}"

  if command -v curl &> /dev/null; then
    # Tokenizer is on a gated repo — requires HF_TOKEN authentication
    if [ ! -f "$ORPHEUS_TOKENIZER" ]; then
      if [ -n "$HF_TOKEN" ]; then
        echo "  Downloading tokenizer (gated, using HF_TOKEN)..."
        curl -sL -H "Authorization: Bearer $HF_TOKEN" --progress-bar -o "$ORPHEUS_TOKENIZER" "$ORPHEUS_TOKENIZER_URL"
        # Verify it's actually a tokenizer and not an error page
        if [ -f "$ORPHEUS_TOKENIZER" ] && [ "$(wc -c < "$ORPHEUS_TOKENIZER")" -lt 10000 ]; then
          echo -e "${RED}  Orpheus tokenizer download failed (got error page). Check HF_TOKEN has access to canopylabs/orpheus-3b-0.1-ft${NC}"
          rm -f "$ORPHEUS_TOKENIZER"
        fi
      else
        echo -e "${YELLOW}  Skipping tokenizer (requires HF_TOKEN in config.env with access to canopylabs/orpheus-3b-0.1-ft)${NC}"
      fi
    fi
    [ ! -f "$ORPHEUS_SNAC" ] && echo "  Downloading SNAC audio codec (~53MB)..." && \
      curl -L --progress-bar -o "$ORPHEUS_SNAC" "$ORPHEUS_SNAC_URL"
    [ ! -f "$ORPHEUS_GGUF" ] && echo "  Downloading GGUF model (~2.4GB, Q4_K_M)..." && \
      curl -L --progress-bar -o "$ORPHEUS_GGUF" "$ORPHEUS_GGUF_URL"
  elif command -v wget &> /dev/null; then
    if [ ! -f "$ORPHEUS_TOKENIZER" ] && [ -n "$HF_TOKEN" ]; then
      wget -q --show-progress --header="Authorization: Bearer $HF_TOKEN" -O "$ORPHEUS_TOKENIZER" "$ORPHEUS_TOKENIZER_URL"
    fi
    [ ! -f "$ORPHEUS_SNAC" ] && wget -q --show-progress -O "$ORPHEUS_SNAC" "$ORPHEUS_SNAC_URL"
    [ ! -f "$ORPHEUS_GGUF" ] && wget -q --show-progress -O "$ORPHEUS_GGUF" "$ORPHEUS_GGUF_URL"
  fi

  if [ -f "$ORPHEUS_GGUF" ] && [ -f "$ORPHEUS_TOKENIZER" ] && [ -f "$ORPHEUS_SNAC" ]; then
    echo -e "${GREEN}Orpheus TTS downloaded (3B, LoRA-trainable, 8 voices + emotion tags)${NC}"
  else
    echo -e "${YELLOW}Orpheus TTS download incomplete (some files missing)${NC}"
  fi
else
  echo -e "${GREEN}Orpheus TTS (3B, LoRA-trainable) already exists${NC}"
fi

echo -e "${GREEN}Voice model check complete${NC}"
