#!/usr/bin/env bash
#
# Compact From Capture — Local compaction pipeline
#
# Takes a gate capture directory (from runpod-gate-capture.sh) and produces
# a compacted safetensors model with pruned heads and mixed precision topology.
#
# This script:
#   1. Reads gate_gradients.json from the capture directory
#   2. Downloads the base model from HuggingFace (if not already cached)
#   3. Runs plasticity analysis (shows what will be pruned/quantized)
#   4. Compacts the model (physically removes dead head tensors)
#   5. Saves compacted model + topology to the capture directory
#
# Usage:
#   ./compact-from-capture.sh <capture-dir> [--model-cache=<dir>]
#
# Examples:
#   ./compact-from-capture.sh ~/.continuum/gate-captures/Qwen-Qwen2.5-Coder-32B-Instruct-1773650329
#   ./compact-from-capture.sh ~/.continuum/gate-captures/Qwen-Qwen2.5-Coder-32B-Instruct-1773650329 --model-cache=/data/models

set -euo pipefail

# ── Parse args ────────────────────────────────────────────────────────
CAPTURE_DIR="${1:-}"
MODEL_CACHE="${HF_HOME:-$HOME/.cache/huggingface/hub}"

for arg in "$@"; do
    case "$arg" in
        --model-cache=*) MODEL_CACHE="${arg#--model-cache=}" ;;
    esac
done

if [[ -z "$CAPTURE_DIR" ]]; then
    echo "Usage: $0 <capture-dir>"
    echo ""
    echo "Example:"
    echo "  $0 ~/.continuum/gate-captures/Qwen-Qwen2.5-Coder-32B-Instruct-1773650329"
    exit 1
fi

# ── Load config ───────────────────────────────────────────────────────
if [[ -f "$HOME/.continuum/config.env" ]]; then
    source "$HOME/.continuum/config.env"
fi

# ── Find gate_gradients.json ──────────────────────────────────────────
GRADIENTS=""
if [[ -f "$CAPTURE_DIR/results/gate_gradients.json" ]]; then
    GRADIENTS="$CAPTURE_DIR/results/gate_gradients.json"
elif [[ -f "$CAPTURE_DIR/gate_gradients.json" ]]; then
    GRADIENTS="$CAPTURE_DIR/gate_gradients.json"
else
    echo "ERROR: gate_gradients.json not found in $CAPTURE_DIR or $CAPTURE_DIR/results/"
    exit 1
fi

echo "================================================================"
echo "  Compact From Capture"
echo "================================================================"
echo "  Capture: $CAPTURE_DIR"
echo "  Gradients: $GRADIENTS"
echo ""

# ── Read model name from gradients ────────────────────────────────────
MODEL=$(python3 -c "import json; print(json.load(open('$GRADIENTS'))['model_name'])")
echo "  Model: $MODEL"

# ── Step 1: Analysis ──────────────────────────────────────────────────
echo ""
echo "── Step 1: Analyzing gate gradients ──────────────────────────────"
python3 -c "
import json

with open('$GRADIENTS') as f:
    data = json.load(f)

layers = data['layer_scores']
num_heads = data['num_heads']
num_kv_heads = data['num_kv_heads']

all_scores = [s for layer in layers for s in layer]
tier_counts = {'removed': 0, 'ternary': 0, 'q2': 0, 'q4': 0, 'q8': 0, 'bf16': 0}
for s in all_scores:
    if s < 0.1: tier_counts['removed'] += 1
    elif s < 0.2: tier_counts['ternary'] += 1
    elif s < 0.3: tier_counts['q2'] += 1
    elif s < 0.5: tier_counts['q4'] += 1
    elif s < 0.7: tier_counts['q8'] += 1
    else: tier_counts['bf16'] += 1

total = len(all_scores)
print(f'  Layers: {len(layers)}, Heads/layer: {num_heads}, KV heads: {num_kv_heads}')
print(f'  Total heads: {total}')
print()
for tier, count in tier_counts.items():
    pct = count / total * 100
    bar = '█' * int(pct / 2)
    print(f'    {tier:>8s}: {count:4d} ({pct:5.1f}%) {bar}')
print()
print(f'  Score range: {min(all_scores):.4f} - {max(all_scores):.4f}')
print(f'  Mean: {sum(all_scores)/len(all_scores):.4f}')
"

# ── Step 2: Download/locate base model ────────────────────────────────
echo ""
echo "── Step 2: Locating base model ───────────────────────────────────"

# Convert HF model name to cache path format: models--Org--Name
CACHE_NAME="models--$(echo "$MODEL" | tr '/' '--')"
MODEL_CACHE_DIR="$MODEL_CACHE/$CACHE_NAME"

# Check if model is already cached (full download)
MODEL_DIR=""
if [[ -d "$MODEL_CACHE_DIR" ]]; then
    # Find the snapshot directory (usually a hash)
    SNAPSHOT_DIR=$(find "$MODEL_CACHE_DIR/snapshots" -maxdepth 1 -mindepth 1 -type d 2>/dev/null | head -1)
    if [[ -n "$SNAPSHOT_DIR" && -f "$SNAPSHOT_DIR/config.json" ]]; then
        MODEL_DIR="$SNAPSHOT_DIR"
        echo "  Found cached model: $MODEL_DIR"
    fi
fi

if [[ -z "$MODEL_DIR" ]]; then
    echo "  Model not cached. Downloading from HuggingFace..."
    echo "  (This is a one-time download — the 32B model is ~65GB)"
    echo ""

    # Use huggingface-cli to download (handles auth, resume, etc.)
    if command -v huggingface-cli &>/dev/null; then
        HF_ARGS=""
        if [[ -n "${HF_TOKEN:-}" ]]; then
            HF_ARGS="--token $HF_TOKEN"
        fi
        huggingface-cli download "$MODEL" --include "*.safetensors" --include "config.json" --include "*.json" $HF_ARGS
    else
        echo "  ERROR: huggingface-cli not found. Install with: pip install huggingface_hub"
        echo "  Or download the model manually to: $MODEL_CACHE_DIR"
        exit 1
    fi

    # Re-locate after download
    SNAPSHOT_DIR=$(find "$MODEL_CACHE_DIR/snapshots" -maxdepth 1 -mindepth 1 -type d 2>/dev/null | head -1)
    if [[ -n "$SNAPSHOT_DIR" && -f "$SNAPSHOT_DIR/config.json" ]]; then
        MODEL_DIR="$SNAPSHOT_DIR"
    else
        echo "  ERROR: Model download completed but snapshot directory not found at $MODEL_CACHE_DIR"
        exit 1
    fi
fi

# Count safetensors shards
SHARD_COUNT=$(find "$MODEL_DIR" -name "model*.safetensors" | wc -l | tr -d ' ')
TOTAL_SIZE=$(du -sh "$MODEL_DIR" 2>/dev/null | cut -f1)
echo "  Location: $MODEL_DIR"
echo "  Shards: $SHARD_COUNT, Size: ${TOTAL_SIZE:-unknown}"

# ── Step 3: Read model config for validation ──────────────────────────
echo ""
echo "── Step 3: Validating model config ───────────────────────────────"
python3 -c "
import json

with open('$MODEL_DIR/config.json') as f:
    config = json.load(f)

# Qwen2 uses different keys than Llama
num_heads = config.get('num_attention_heads', config.get('n_head', 0))
num_kv_heads = config.get('num_key_value_heads', config.get('n_head_kv', num_heads))
hidden_size = config.get('hidden_size', 0)
num_layers = config.get('num_hidden_layers', 0)
head_dim = hidden_size // num_heads if num_heads > 0 else 0

print(f'  Architecture: {config.get(\"model_type\", \"unknown\")}')
print(f'  Hidden size: {hidden_size}')
print(f'  Num layers: {num_layers}')
print(f'  Num heads: {num_heads} (KV: {num_kv_heads})')
print(f'  Head dim: {head_dim}')
print(f'  GQA ratio: {num_heads // num_kv_heads if num_kv_heads > 0 else 1}:1')

# Cross-check with gate_gradients.json
with open('$GRADIENTS') as f:
    grad = json.load(f)

assert len(grad['layer_scores']) == num_layers, \
    f'Layer count mismatch: gradients={len(grad[\"layer_scores\"])}, config={num_layers}'
assert grad['num_heads'] == num_heads, \
    f'Head count mismatch: gradients={grad[\"num_heads\"]}, config={num_heads}'
assert grad['num_kv_heads'] == num_kv_heads, \
    f'KV head count mismatch: gradients={grad[\"num_kv_heads\"]}, config={num_kv_heads}'

print('  Config matches gate gradients ✓')
"

# ── Step 4: Compact the model ─────────────────────────────────────────
echo ""
echo "── Step 4: Compacting model ──────────────────────────────────────"
echo "  This is the main operation: reading all safetensor shards,"
echo "  pruning dead attention heads, and writing compacted output."
echo ""

OUTPUT_DIR="$CAPTURE_DIR/compacted"
mkdir -p "$OUTPUT_DIR"

# The Rust compaction runs through IPC if the server is running,
# otherwise we use the Python fallback compactor.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../../../../.." && pwd)"
JTAG="$PROJECT_ROOT/jtag"

if [[ -x "$JTAG" ]] && "$JTAG" ping 2>/dev/null; then
    echo "  Using Rust compactor via IPC..."
    "$JTAG" plasticity/pipeline \
        --capturePath="$CAPTURE_DIR" \
        --modelPath="$MODEL_DIR" \
        --outputPath="$OUTPUT_DIR"
else
    echo "  Server not running. Using Python compactor..."
    python3 "$SCRIPT_DIR/compact-safetensors.py" \
        --gradients "$GRADIENTS" \
        --model-dir "$MODEL_DIR" \
        --output-dir "$OUTPUT_DIR"
fi

# ── Step 5: Summary ──────────────────────────────────────────────────
echo ""
echo "================================================================"
echo "  Compaction Complete!"
echo "================================================================"

if [[ -f "$OUTPUT_DIR/compacted_model.safetensors" ]]; then
    COMPACT_SIZE=$(du -sh "$OUTPUT_DIR/compacted_model.safetensors" 2>/dev/null | cut -f1)
    echo "  Compacted model: $OUTPUT_DIR/compacted_model.safetensors ($COMPACT_SIZE)"
fi

if [[ -f "$OUTPUT_DIR/compacted_model.topology.json" ]]; then
    echo "  Topology: $OUTPUT_DIR/compacted_model.topology.json"
fi

if [[ -f "$OUTPUT_DIR/analysis.json" ]]; then
    echo "  Analysis: $OUTPUT_DIR/analysis.json"
fi

echo ""
echo "  Original: ${TOTAL_SIZE:-unknown}"
echo "  Compacted: ${COMPACT_SIZE:-calculating...}"
echo ""
echo "  To load in Candle (when CompactLlama is ready):"
echo "    ./jtag ai/generate --prompt 'Hello' \\"
echo "      --model=$OUTPUT_DIR/compacted_model.safetensors"
echo "================================================================"
