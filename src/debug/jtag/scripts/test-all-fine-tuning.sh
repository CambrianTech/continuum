#!/bin/bash
# Test Fine-Tuning Adapters End-to-End
#
# This script tests fine-tuning adapters by submitting training jobs
# and verifying the adapters can communicate with provider APIs.
#
# Usage:
#   ./test-all-fine-tuning.sh [persona-id] [provider]
#
# Examples:
#   ./test-all-fine-tuning.sh                     # Test all providers
#   ./test-all-fine-tuning.sh "" openai           # Test only OpenAI
#   ./test-all-fine-tuning.sh <uuid> deepseek     # Test only DeepSeek with specific persona
#
# Requirements:
# - API keys in SecretManager (OPENAI_API_KEY, DEEPSEEK_API_KEY, etc.)
# - Test dataset at /Volumes/FlashGordon/cambrian/datasets/prepared/fine-tuning-test.jsonl
# - PersonaUser ID (will auto-find/create if not provided)

set -e

cd /Volumes/FlashGordon/cambrian/continuum/src/debug/jtag

DATASET_PATH="/Volumes/FlashGordon/cambrian/datasets/prepared/fine-tuning-test.jsonl"
PERSONA_ID="${1:-}"
SINGLE_PROVIDER="${2:-}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo "===================================="
echo "  Fine-Tuning Adapter Test Suite"
echo "===================================="
echo ""

# Check dataset exists
if [ ! -f "$DATASET_PATH" ]; then
    echo -e "${RED}❌ Dataset not found at $DATASET_PATH${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Found test dataset (20 examples)${NC}"
echo ""

# Get or prompt for User ID (for fine-tuning tests, any user works)
if [ -z "$PERSONA_ID" ]; then
    echo -e "${YELLOW}📝 Getting user ID from database...${NC}"
    # Get any user from the database - parse JSON properly with jq or python
    PERSONA_ID=$(./jtag data/list --collection=users --limit=1 2>/dev/null | python3 -c "import sys, json; data=json.load(sys.stdin); print(data['items'][0]['id'] if data.get('items') and len(data['items']) > 0 else '')" 2>/dev/null)

    if [ -z "$PERSONA_ID" ]; then
        echo -e "${RED}❌ No users found in database. Run 'npm run data:seed' first.${NC}"
        exit 1
    fi

    echo -e "${GREEN}✅ Using User ID: $PERSONA_ID${NC}"
else
    echo -e "${GREEN}✅ Using User ID: $PERSONA_ID${NC}"
fi

echo ""
echo "===================================="
echo "  Testing Providers"
echo "===================================="
echo ""

# Track results
PASSED=()
FAILED=()
SKIPPED=()

# Function to test a provider
test_provider() {
    local provider=$1
    local env_var=$2

    echo -e "${BLUE}🧪 Testing $provider...${NC}"

    # Submit training job (dry run to validate without actual training)
    # The adapter will check for API keys internally via SecretManager
    # If key is missing, adapter will fail gracefully with clear error message
    echo "  Submitting dry-run training job..."
    ./jtag genome/train \
        --provider="$provider" \
        --datasetPath="$DATASET_PATH" \
        --personaId="$PERSONA_ID" \
        --dryRun=true \
        --epochs=1 \
        --batchSize=1 > /tmp/finetune-test-$provider.log 2>&1

    # Check if the JSON output contains "success": true
    if grep -q '"success": true' /tmp/finetune-test-$provider.log; then
        echo -e "${GREEN}  ✅ $provider adapter working${NC}"
        PASSED+=("$provider")
    elif grep -q "No API key found" /tmp/finetune-test-$provider.log; then
        echo -e "${YELLOW}  ⚠️  $provider skipped (no $env_var configured)${NC}"
        SKIPPED+=("$provider")
    else
        echo -e "${RED}  ❌ $provider adapter failed${NC}"
        echo "  See /tmp/finetune-test-$provider.log for details"
        FAILED+=("$provider")
    fi

    echo ""
}

# Test providers (all or single)
if [ -n "$SINGLE_PROVIDER" ]; then
    # Test only the specified provider
    case "$SINGLE_PROVIDER" in
        openai)
            test_provider "openai" "OPENAI_API_KEY"
            ;;
        deepseek)
            test_provider "deepseek" "DEEPSEEK_API_KEY"
            ;;
        fireworks)
            test_provider "fireworks" "FIREWORKS_API_KEY"
            ;;
        together)
            test_provider "together" "TOGETHER_API_KEY"
            ;;
        mistral)
            test_provider "mistral" "MISTRAL_API_KEY"
            ;;
        *)
            echo -e "${RED}❌ Unknown provider: $SINGLE_PROVIDER${NC}"
            echo "Valid providers: openai, deepseek, fireworks, together, mistral"
            exit 1
            ;;
    esac
else
    # Test all providers
    test_provider "openai" "OPENAI_API_KEY"
    test_provider "deepseek" "DEEPSEEK_API_KEY"
    test_provider "fireworks" "FIREWORKS_API_KEY"
    test_provider "together" "TOGETHER_API_KEY"
    test_provider "mistral" "MISTRAL_API_KEY"
fi

# Summary
echo "===================================="
echo "  Test Summary"
echo "===================================="
echo ""

if [ ${#PASSED[@]} -gt 0 ]; then
    echo -e "${GREEN}✅ Passed (${#PASSED[@]}):${NC}"
    for provider in "${PASSED[@]}"; do
        echo "   - $provider"
    done
    echo ""
fi

if [ ${#SKIPPED[@]} -gt 0 ]; then
    echo -e "${YELLOW}⚠️  Skipped (${#SKIPPED[@]}):${NC}"
    for provider in "${SKIPPED[@]}"; do
        echo "   - $provider (no API key)"
    done
    echo ""
fi

if [ ${#FAILED[@]} -gt 0 ]; then
    echo -e "${RED}❌ Failed (${#FAILED[@]}):${NC}"
    for provider in "${FAILED[@]}"; do
        echo "   - $provider"
    done
    echo ""
    echo "Check logs in /tmp/finetune-test-*.log for details"
    echo ""
    exit 1
fi

echo -e "${GREEN}🎉 All tests passed!${NC}"
echo ""
echo "Next steps:"
echo "  1. Remove --dryRun flag to start actual training"
echo "  2. Monitor training with: ./jtag genome/train/status --jobId=<id>"
echo "  3. Download trained adapter when complete"
echo ""
