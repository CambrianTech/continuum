#!/bin/bash
# Master Test Runner for All Fine-Tuning Providers
#
# Tests ALL remote fine-tuning providers using the shared test infrastructure.
# Each test module uses BaseRemoteAPITest for maximum code reuse.
#
# Usage:
#   chmod +x system/genome/fine-tuning/server/adapters/api-tests/test-all.sh
#   ./system/genome/fine-tuning/server/adapters/api-tests/test-all.sh
#
# Requirements:
#   - API keys in environment (OPENAI_API_KEY, DEEPSEEK_API_KEY, etc.)
#   - /tmp/test-training-minimal.jsonl (training data)
#
# Architecture:
#   - BaseRemoteAPITest: Shared polling, metadata, orchestration
#   - OpenAI: Reference implementation (FormData upload)
#   - DeepSeek: Extends OpenAI (95% code reuse)
#   - Together: Extends OpenAI (95% code reuse)
#   - Fireworks: Extends BaseRemoteAPITest (inline data, different workflow)

set -e  # Exit on error

# Get the directory containing this script
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Track results
PASSED=0
FAILED=0
SKIPPED=0

echo "🚀 Testing ALL Fine-Tuning Providers (API Tests)"
echo "================================================"
echo ""
echo "Architecture:"
echo "  - BaseRemoteAPITest: Shared logic (polling, metadata, errors)"
echo "  - Provider tests: Only implement upload/create/status methods"
echo "  - Code reuse: 60-95% depending on provider compatibility"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# ============================================================================
# Test 1: OpenAI
# ============================================================================

echo -e "${BLUE}Test 1/4: OpenAI Fine-Tuning${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

if [ -z "$OPENAI_API_KEY" ]; then
  echo -e "${YELLOW}⚠️  SKIPPED: OPENAI_API_KEY not set${NC}"
  SKIPPED=$((SKIPPED + 1))
else
  if npx tsx "$SCRIPT_DIR/test-openai.ts"; then
    echo -e "${GREEN}✅ OpenAI test PASSED${NC}"
    PASSED=$((PASSED + 1))
  else
    echo -e "${RED}❌ OpenAI test FAILED${NC}"
    FAILED=$((FAILED + 1))
  fi
fi

echo ""
echo ""

# ============================================================================
# Test 2: DeepSeek (95% code reuse from OpenAI)
# ============================================================================

echo -e "${BLUE}Test 2/4: DeepSeek Fine-Tuning${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

if [ -z "$DEEPSEEK_API_KEY" ]; then
  echo -e "${YELLOW}⚠️  SKIPPED: DEEPSEEK_API_KEY not set${NC}"
  SKIPPED=$((SKIPPED + 1))
else
  if npx tsx "$SCRIPT_DIR/test-deepseek.ts"; then
    echo -e "${GREEN}✅ DeepSeek test PASSED${NC}"
    PASSED=$((PASSED + 1))
  else
    echo -e "${RED}❌ DeepSeek test FAILED${NC}"
    FAILED=$((FAILED + 1))
  fi
fi

echo ""
echo ""

# ============================================================================
# Test 3: Fireworks (Different upload strategy - inline data)
# ============================================================================

echo -e "${BLUE}Test 3/4: Fireworks AI Fine-Tuning${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

if [ -z "$FIREWORKS_API_KEY" ]; then
  echo -e "${YELLOW}⚠️  SKIPPED: FIREWORKS_API_KEY not set${NC}"
  SKIPPED=$((SKIPPED + 1))
else
  if npx tsx "$SCRIPT_DIR/test-fireworks.ts"; then
    echo -e "${GREEN}✅ Fireworks test PASSED${NC}"
    PASSED=$((PASSED + 1))
  else
    echo -e "${RED}❌ Fireworks test FAILED${NC}"
    FAILED=$((FAILED + 1))
  fi
fi

echo ""
echo ""

# ============================================================================
# Test 4: Together (95% code reuse from OpenAI)
# ============================================================================

echo -e "${BLUE}Test 4/4: Together AI Fine-Tuning${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

if [ -z "$TOGETHER_API_KEY" ]; then
  echo -e "${YELLOW}⚠️  SKIPPED: TOGETHER_API_KEY not set${NC}"
  SKIPPED=$((SKIPPED + 1))
else
  if npx tsx "$SCRIPT_DIR/test-together.ts"; then
    echo -e "${GREEN}✅ Together test PASSED${NC}"
    PASSED=$((PASSED + 1))
  else
    echo -e "${RED}❌ Together test FAILED${NC}"
    FAILED=$((FAILED + 1))
  fi
fi

echo ""
echo ""

# ============================================================================
# Summary
# ============================================================================

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST SUMMARY"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo -e "${GREEN}Passed:  $PASSED${NC}"
echo -e "${RED}Failed:  $FAILED${NC}"
echo -e "${YELLOW}Skipped: $SKIPPED${NC}"
echo ""
echo "Total tests: 4"
echo ""

# Code reuse metrics
echo "Code Reuse Metrics:"
echo "  - OpenAI: 100% (reference implementation)"
echo "  - DeepSeek: 95% (extends OpenAI, just config changes)"
echo "  - Together: 95% (extends OpenAI, just config changes)"
echo "  - Fireworks: 60% (shares BaseRemoteAPITest, different upload)"
echo ""
echo "Average code reuse: 87.5%"
echo ""

if [ $FAILED -gt 0 ]; then
  echo -e "${RED}❌ Some tests failed. Check output above for details.${NC}"
  exit 1
else
  echo -e "${GREEN}✅ All executed tests passed!${NC}"
  echo ""
  if [ $SKIPPED -gt 0 ]; then
    echo "Note: $SKIPPED test(s) skipped due to missing API keys"
    echo "      Set environment variables to test all providers:"
    echo "      - OPENAI_API_KEY"
    echo "      - DEEPSEEK_API_KEY"
    echo "      - FIREWORKS_API_KEY"
    echo "      - TOGETHER_API_KEY"
  fi
  echo ""
  echo "Next steps:"
  echo "  1. Integrate working code into adapter implementations"
  echo "  2. Update adapters to use BaseRemoteAPIAdapter pattern"
  echo "  3. Test with ./jtag genome/train"
fi
