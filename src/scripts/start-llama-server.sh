#!/bin/bash
# Start llama-server with the best GGUF model for this hardware.
# Resolves model from HF cache or downloads if missing.
# Called by start-workers.sh as a TCP worker on port 8090.

set -e

PORT=8090
HOST=127.0.0.1
NGL=99      # GPU layers (Metal/CUDA — offload everything)
CTX=4096    # Context window

# Model selection: same logic as quantized.rs load_default_quantized()
REPO="continuum-ai/qwen3.5-4b-code-forged-GGUF"

# Pick quant level by RAM
if [ "$(uname)" = "Darwin" ]; then
  RAM_GB=$(sysctl -n hw.memsize 2>/dev/null | awk '{printf "%d", $1/1073741824}')
else
  RAM_GB=$(grep MemTotal /proc/meminfo 2>/dev/null | awk '{printf "%d", $2/1048576}')
fi
RAM_GB=${RAM_GB:-8}

if [ "$RAM_GB" -ge 32 ]; then
  FILENAME="qwen3.5-4b-code-forged-Q8_0.gguf"
else
  FILENAME="qwen3.5-4b-code-forged-Q4_K_M.gguf"
fi

echo "System RAM: ${RAM_GB}GB → model: $FILENAME"

# Find GGUF in HF cache or common locations
HF_CACHE="${HF_HOME:-$HOME/.cache/huggingface}/hub"
REPO_DIR="$HF_CACHE/models--$(echo "$REPO" | tr '/' '--')"
MODEL_PATH=""

# Search: HF cache, .continuum/models, /tmp/gguf-test (dev)
for search_dir in "$REPO_DIR" "$HOME/.continuum/models" "/tmp/gguf-test"; do
  if [ -d "$search_dir" ]; then
    MODEL_PATH=$(find "$search_dir" -name "$FILENAME" -type f 2>/dev/null | head -1)
    [ -n "$MODEL_PATH" ] && break
  fi
done

# Fallback: try Q4_K_M if preferred quant not found
if [ -z "$MODEL_PATH" ] && [ "$FILENAME" != "qwen3.5-4b-code-forged-Q4_K_M.gguf" ]; then
  echo "Preferred $FILENAME not cached, trying Q4_K_M..."
  FILENAME="qwen3.5-4b-code-forged-Q4_K_M.gguf"
  for search_dir in "$REPO_DIR" "$HOME/.continuum/models" "/tmp/gguf-test"; do
    if [ -d "$search_dir" ]; then
      MODEL_PATH=$(find "$search_dir" -name "$FILENAME" -type f 2>/dev/null | head -1)
      [ -n "$MODEL_PATH" ] && break
    fi
  done
fi

# Download if not cached
if [ -z "$MODEL_PATH" ] || [ ! -f "$MODEL_PATH" ]; then
  echo "Downloading $REPO/$FILENAME from HuggingFace..."
  if command -v hf &>/dev/null; then
    MODEL_PATH=$(hf download "$REPO" "$FILENAME" 2>/dev/null | tail -1)
  elif command -v python3 &>/dev/null; then
    MODEL_PATH=$(python3 -c "from huggingface_hub import hf_hub_download; print(hf_hub_download('$REPO', '$FILENAME'))" 2>/dev/null)
  elif command -v curl &>/dev/null; then
    mkdir -p "$HOME/.continuum/models"
    MODEL_PATH="$HOME/.continuum/models/$FILENAME"
    curl -L "https://huggingface.co/$REPO/resolve/main/$FILENAME" -o "$MODEL_PATH"
  else
    echo "ERROR: No way to download model. Install: brew install huggingface-cli"
    exit 1
  fi
fi

if [ -z "$MODEL_PATH" ] || [ ! -f "$MODEL_PATH" ]; then
  echo "ERROR: Model not found: $FILENAME"
  exit 1
fi

echo "Model: $MODEL_PATH"
echo "Starting llama-server on $HOST:$PORT (ngl=$NGL, ctx=$CTX)"

# Find llama-server binary
LLAMA_SERVER=$(command -v llama-server 2>/dev/null || echo "")
if [ -z "$LLAMA_SERVER" ]; then
  echo "ERROR: llama-server not found. Install: brew install llama.cpp"
  exit 1
fi

exec "$LLAMA_SERVER" \
  -m "$MODEL_PATH" \
  --port "$PORT" \
  --host "$HOST" \
  -ngl "$NGL" \
  -c "$CTX"
