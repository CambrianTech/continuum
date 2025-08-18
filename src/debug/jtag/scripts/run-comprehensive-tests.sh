#!/bin/bash

# Comprehensive Test Suite Runner
# Organized test execution with proper error handling and logging

set -e

echo "🚀 AUTONOMOUS TEST SUITE - Full End-to-End"

# Compiler checks first
echo "🔍 Running compiler error detection..."
npx tsx tests/compiler-error-detection.test.ts

# Core system tests
echo "📋 Running bootstrap tests..."
npx tsx tests/bootstrap-comprehensive.test.ts

echo "🌐 Running browser integration tests..."
npx tsx tests/integration/browser-automated-tests.test.ts

echo "🔄 Running router coordination tests..."
npx tsx tests/integration/router-coordination-simple.test.ts

# Screenshot and visual tests
echo "📸 Running screenshot tests..."
npx tsx tests/server-screenshot.test.ts
npx tsx tests/screenshot-verification.test.ts
npx tsx tests/screenshot-integration-advanced.test.ts

# Signal system tests
echo "📡 Running signal system tests..."
npx tsx tests/signal-system.test.ts

# Chat integration tests
echo "💬 Running chat daemon integration..."
npx tsx tests/chat-daemon-integration.test.ts

# Unit tests
echo "🧪 Running unit tests..."
npx tsx tests/unit/router-broadcast.test.ts
npx tsx tests/unit/room-scoped-event-routing.test.ts
npx tsx tests/unit/events-daemon-unit.test.ts

# Cross-environment event tests
echo "🌉 Running cross-environment event tests..."
npx tsx tests/integration/server-browser-event-flow.test.ts
npx tsx tests/integration/browser-server-event-flow.test.ts
npx tsx tests/integration/chat-widget-room-events.test.ts
npx tsx tests/integration/cross-environment-events-working.test.ts

# Multi-user chat tests
echo "👥 Running multi-user chat tests..."
npx tsx tests/integration/simple-multiuser-chat.test.ts
npx tsx tests/integration/server-to-browser-chat-proof.test.ts

echo "✅ All comprehensive tests completed successfully!"