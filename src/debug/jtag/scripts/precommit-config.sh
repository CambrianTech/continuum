#!/bin/bash
# Precommit Hook Configuration
# Modify this file to change validation behavior without touching the main hook

# ==============================================================================
# VALIDATION PHASES (enable/disable with true/false)
# ==============================================================================
export ENABLE_TYPESCRIPT_CHECK=true
export ENABLE_SYSTEM_RESTART=true
export ENABLE_BROWSER_TEST=true
export ENABLE_ARTIFACTS_COLLECTION=false  # Disabled - adds unnecessary files to git

# ==============================================================================
# RESTART BEHAVIOR
# ==============================================================================
# When should we restart the system?
# - always: Always restart on every commit
# - on_code_change: Restart if .ts/.js/.css/.html files changed
# - on_ping_fail: Only restart if ping fails
# - never: Never restart (fastest, but risky)
export RESTART_STRATEGY="on_code_change"

# ==============================================================================
# TEST CONFIGURATION
# ==============================================================================
# Which tests to run (space-separated list of test files)
export PRECOMMIT_TESTS="tests/precommit/browser-ping.test.ts"

# Add more tests here as needed:
# export PRECOMMIT_TESTS="$PRECOMMIT_TESTS tests/precommit/crud-smoke-test.ts"
# export PRECOMMIT_TESTS="$PRECOMMIT_TESTS tests/precommit/ai-response-test.ts"

# Test timeout (seconds)
export TEST_TIMEOUT=30

# ==============================================================================
# DEPLOYMENT CONFIGURATION
# ==============================================================================
# Startup timeout (seconds)
export STARTUP_TIMEOUT=90

# Settle time after startup (seconds)
export SETTLE_TIME=3

# ==============================================================================
# FILE PATTERNS
# ==============================================================================
# Which file extensions trigger code change detection
export CODE_FILE_PATTERNS='\.(ts|tsx|js|jsx|css|html)$'

# Which file extensions are documentation-only (no restart needed)
export DOC_FILE_PATTERNS='\.(md|txt|json)$'

# ==============================================================================
# LOGGING
# ==============================================================================
export PRECOMMIT_LOG_DIR=".continuum/sessions/validation"
export ENABLE_VERBOSE_LOGGING=false
