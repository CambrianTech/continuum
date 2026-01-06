#!/bin/bash
# Inference Benchmark & Hardware Classification
# Works on: Apple Silicon (Metal), NVIDIA (CUDA), CPU

SOCKET="${1:-/tmp/jtag-inference.sock}"

echo "=== INFERENCE HARDWARE BENCHMARK ==="
echo ""

# Detect hardware
detect_hardware() {
    if system_profiler SPDisplaysDataType 2>/dev/null | grep -q "Metal"; then
        echo "Metal"
    elif nvidia-smi >/dev/null 2>&1; then
        echo "CUDA"
    else
        echo "CPU"
    fi
}

HARDWARE=$(detect_hardware)
echo "Hardware: $HARDWARE"

case "$HARDWARE" in
    "Metal")
        CHIP=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "Unknown")
        MEM=$(sysctl -n hw.memsize 2>/dev/null | awk '{print $1/1024/1024/1024" GB"}')
        echo "Chip: $CHIP"
        echo "Memory: $MEM"
        # M1 Pro = ~200GB/s, M3 Max = ~400GB/s, M4 Max = ~540GB/s
        ;;
    "CUDA")
        nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null
        ;;
    "CPU")
        CPU=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || cat /proc/cpuinfo 2>/dev/null | grep "model name" | head -1 | cut -d: -f2)
        echo "CPU: $CPU"
        ;;
esac

echo ""

# Check socket
if [ ! -S "$SOCKET" ]; then
    echo "Error: Socket $SOCKET not found"
    echo "Start the inference worker first"
    exit 1
fi

echo "Socket: $SOCKET"
echo ""

# Ping test
echo "=== PING TEST ==="
PING_RESULT=$(echo '{"command":"ping","request_id":"ping"}' | nc -U $SOCKET 2>&1)
echo "$PING_RESULT" | jq -r '.result.version // "Error"' 2>/dev/null || echo "Failed: $PING_RESULT"
echo ""

# List models
echo "=== LOADED MODELS ==="
MODELS=$(echo '{"command":"model/list","request_id":"list"}' | nc -U $SOCKET 2>&1)
echo "$MODELS" | jq -r '.result.models[]? | "\(.model_id) (\(.architecture))"' 2>/dev/null || echo "None"
echo ""

# Load test model if not loaded
MODEL_ID="Qwen/Qwen2-0.5B-Instruct"
if ! echo "$MODELS" | jq -e ".result.models[] | select(.model_id == \"$MODEL_ID\")" >/dev/null 2>&1; then
    echo "Loading $MODEL_ID..."
    LOAD_RESULT=$(echo "{\"command\":\"model/load\",\"request_id\":\"load\",\"model_id\":\"$MODEL_ID\"}" | nc -U $SOCKET 2>&1)
    echo "$LOAD_RESULT" | jq -r '.result.status // .error // "Unknown"' 2>/dev/null
    echo ""
fi

# Benchmark function
benchmark() {
    local TOKENS=$1
    local PROMPT="Count from 1 to 100. 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15"

    START=$(date +%s.%N)
    RESULT=$( (echo "{\"command\":\"generate\",\"request_id\":\"bench$TOKENS\",\"model_id\":\"$MODEL_ID\",\"prompt\":\"$PROMPT\",\"max_tokens\":$TOKENS}"; sleep 120) | nc -U $SOCKET 2>&1 | head -1)
    END=$(date +%s.%N)

    DURATION=$(echo "$END - $START" | bc)
    GEN_TOKENS=$(echo "$RESULT" | grep -o '"generated_tokens":[0-9]*' | grep -o '[0-9]*' || echo "0")

    if [ "$GEN_TOKENS" != "0" ]; then
        TPS=$(echo "scale=1; $GEN_TOKENS / $DURATION" | bc)
        echo "  $TOKENS tokens → $GEN_TOKENS generated in ${DURATION}s = $TPS tok/s"
    else
        echo "  $TOKENS tokens → Failed: $(echo "$RESULT" | head -c 100)"
    fi
}

echo "=== GENERATION BENCHMARK ==="
echo "Model: $MODEL_ID"
echo ""

# Warmup
echo "Warmup..."
benchmark 10

echo ""
echo "Benchmark runs:"
benchmark 50
benchmark 100
benchmark 200

echo ""
echo "=== EXPECTED PERFORMANCE ==="
case "$HARDWARE" in
    "Metal")
        echo "Apple Silicon (Metal):"
        echo "  M1 Pro:   15-25 tok/s (BF16)"
        echo "  M3 Max:   30-50 tok/s (BF16)"
        echo "  M4 Max:   50-80 tok/s (BF16)"
        ;;
    "CUDA")
        echo "NVIDIA CUDA:"
        echo "  RTX 3090: 80-120 tok/s (FP16)"
        echo "  RTX 4090: 150-200 tok/s (FP16)"
        echo "  RTX 5090: 200-300+ tok/s (FP16)"
        ;;
    "CPU")
        echo "CPU (slow, for testing only):"
        echo "  Any:      2-5 tok/s (F32)"
        ;;
esac

echo ""
echo "=== RECOMMENDATIONS ==="
case "$HARDWARE" in
    "Metal")
        echo "- Use BF16 dtype for Metal (implemented)"
        echo "- Consider GGUF Q4 quantization for 2-4x speedup"
        echo "- Keep context window under 4K for optimal speed"
        ;;
    "CUDA")
        echo "- Use FP16 dtype for CUDA (best balance)"
        echo "- Batch requests when possible"
        echo "- Enable flash attention if model supports it"
        ;;
    "CPU")
        echo "- Use GGUF Q4 quantization (required for usability)"
        echo "- Keep max_tokens low (<100)"
        echo "- Consider Ollama for better CPU optimization"
        ;;
esac
