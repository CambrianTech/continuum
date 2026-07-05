#!/bin/bash
# ArtifactsAPI Test Runner
# Tests filesystem abstraction without browser dependency

set -e

echo "🧪 Running ArtifactsAPI Tests"
echo "════════════════════════════════════════════════════════════"

# Ensure we're in the right directory
cd "$(dirname "$0")/../.."

# Run TypeScript tests via tsx
npx tsx tests/artifacts-api/test-artifacts-api.ts

echo ""
echo "✅ All ArtifactsAPI tests completed"
