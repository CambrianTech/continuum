#!/bin/bash
# Fix common test name and type issues across all test files

set -e

echo "🔧 FIXING TEST NAMES AND TYPES"
echo "Fixing common patterns: Joel→ServerUser1, GeneralAI→ServerUser2, Claude→DevAssistant, any→proper types"

TEST_DIR="/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/tests"

# Find all TypeScript test files
find "$TEST_DIR" -name "*.test.ts" -o -name "*.ts" | while read file; do
  echo "Processing: $file"
  
  # Create backup
  cp "$file" "$file.backup"
  
  # Fix name patterns (case sensitive)
  sed -i '' \
    -e 's/Joel/ServerUser1/g' \
    -e 's/joel/serverUser1/g' \
    -e 's/GeneralAI/ServerUser2/g' \
    -e 's/generalAI/serverUser2/g' \
    -e 's/Claude Code/DevAssistant/g' \
    -e 's/Claude-Code/DevAssistant/g' \
    -e 's/claudeCode/devAssistant/g' \
    -e 's/claude-browser-user/dev-assistant-browser/g' \
    -e 's/joel-server-user/server-user-1/g' \
    -e 's/generalai-server-user/server-user-2/g' \
    "$file"
  
  # Fix common type issues
  sed -i '' \
    -e 's/let joelServer: any/let serverUser1: JTAGClient | null/g' \
    -e 's/let generalAIServer: any/let serverUser2: JTAGClient | null/g' \
    -e 's/let browserUser: any/let controlClient: JTAGClient | null/g' \
    -e 's/let joel: any/let serverUser1: JTAGClient | null/g' \
    -e 's/let generalAI: any/let serverUser2: JTAGClient | null/g' \
    -e 's/let claudeCode: any/let devAssistant: JTAGClient | null/g' \
    "$file"
  
  # Check if file actually changed
  if ! diff -q "$file" "$file.backup" > /dev/null 2>&1; then
    echo "  ✅ Updated: $file"
  else
    echo "  ⚪ No changes: $file"
    # Remove backup if no changes
    rm "$file.backup"
  fi
done

echo "✅ Name and type fixing completed"
echo "📋 Backup files created for changed files (.backup extension)"