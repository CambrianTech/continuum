#!/bin/bash
#
# Test script for genome training environment
#
# Purpose: Bulletproof end-to-end test with minimal dataset
# Philosophy: Test the shit out of it before going live
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}ℹ️  $1${NC}"; }
log_success() { echo -e "${GREEN}✅ $1${NC}"; }
log_error() { echo -e "${RED}❌ $1${NC}"; }

echo "🧬 Genome Training Environment Test"
echo "===================================="
echo ""

# Step 1: Check if environment exists
log_info "Checking if environment is bootstrapped..."

if [[ ! -f "$SCRIPT_DIR/train-wrapper.sh" ]]; then
    log_error "Environment not bootstrapped"
    log_info "Run: bash $SCRIPT_DIR/bootstrap.sh"
    exit 1
fi

log_success "Environment found"

# Step 2: Create minimal test dataset
log_info "Creating minimal test dataset..."

TEST_DIR="$SCRIPT_DIR/test-run-$(date +%s)"
mkdir -p "$TEST_DIR"

cat > "$TEST_DIR/test-dataset.jsonl" << 'EOF'
{"messages": [{"role": "user", "content": "What is 2+2?"}, {"role": "assistant", "content": "The answer is 4."}]}
{"messages": [{"role": "user", "content": "What is the capital of France?"}, {"role": "assistant", "content": "The capital of France is Paris."}]}
{"messages": [{"role": "user", "content": "What is the largest planet?"}, {"role": "assistant", "content": "The largest planet is Jupiter."}]}
EOF

log_success "Test dataset created (3 examples)"

# Step 3: Create training config
log_info "Creating training config..."

cat > "$TEST_DIR/config.json" << EOF
{
  "baseModel": "unsloth/Qwen3-7b",
  "datasetPath": "$TEST_DIR/test-dataset.jsonl",
  "rank": 8,
  "alpha": 16,
  "epochs": 1,
  "learningRate": 0.0001,
  "batchSize": 1,
  "outputDir": "$TEST_DIR/output"
}
EOF

log_success "Config created"

# Step 4: Run training (THIS IS THE REAL TEST)
log_info "Running training (this may take 2-5 minutes)..."
echo ""

mkdir -p "$TEST_DIR/output"

if "$SCRIPT_DIR/train-wrapper.sh" \
    "$(dirname "$SCRIPT_DIR")/../../src/system/genome/fine-tuning/server/adapters/scripts/unsloth-train.py" \
    --config "$TEST_DIR/config.json" \
    --output "$TEST_DIR/output"; then

    log_success "Training completed successfully"
else
    log_error "Training failed"
    log_info "Logs saved in: $TEST_DIR"
    exit 1
fi

# Step 5: Verify output files
log_info "Verifying output files..."

EXPECTED_FILES=(
    "adapter_config.json"
    "adapter_model.safetensors"
)

MISSING_FILES=()
for file in "${EXPECTED_FILES[@]}"; do
    if [[ ! -f "$TEST_DIR/output/$file" ]]; then
        MISSING_FILES+=("$file")
    fi
done

if [[ ${#MISSING_FILES[@]} -gt 0 ]]; then
    log_error "Missing output files: ${MISSING_FILES[*]}"
    exit 1
fi

log_success "All output files present"

# Step 6: Check file sizes
log_info "Checking file sizes..."

for file in "${EXPECTED_FILES[@]}"; do
    SIZE=$(stat -f%z "$TEST_DIR/output/$file" 2>/dev/null || stat -c%s "$TEST_DIR/output/$file" 2>/dev/null)
    if [[ $SIZE -lt 100 ]]; then
        log_error "$file is suspiciously small ($SIZE bytes)"
        exit 1
    fi
    log_success "$file: $(numfmt --to=iec-i --suffix=B $SIZE || echo $SIZE bytes)"
done

# Step 7: Summary
echo ""
echo "===================================="
log_success "🎉 All tests passed!"
echo ""
log_info "Test results:"
log_info "  Dataset: $TEST_DIR/test-dataset.jsonl"
log_info "  Config: $TEST_DIR/config.json"
log_info "  Output: $TEST_DIR/output"
log_info "  Adapter: $TEST_DIR/output/adapter_model.safetensors"
echo ""
log_info "Cleaning up test directory in 60 seconds (Ctrl+C to keep)..."
sleep 60 && rm -rf "$TEST_DIR" &
