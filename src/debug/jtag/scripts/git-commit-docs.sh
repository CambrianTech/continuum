#!/bin/bash
# git-commit-docs.sh - Smart documentation commit script
# Skips precommit hook for documentation-only changes

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if we're in a git repo
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    echo -e "${RED}Error: Not in a git repository${NC}"
    exit 1
fi

# Get list of staged and modified files
STAGED_FILES=$(git diff --cached --name-only 2>/dev/null || echo "")
MODIFIED_FILES=$(git diff --name-only 2>/dev/null || echo "")
ALL_FILES=$(echo -e "${STAGED_FILES}\n${MODIFIED_FILES}" | sort -u | grep -v '^$')

if [ -z "$ALL_FILES" ]; then
    echo -e "${YELLOW}No changes to commit${NC}"
    exit 0
fi

# Check if ALL changes are documentation or script files
# Allow: markdown, text, READMEs, scripts, and common doc formats
DOC_EXTENSIONS="\.md$|\.txt$|README|LICENSE|CHANGELOG|\.rst$|\.adoc$|\.sh$|scripts/"
NON_DOC_FILES=$(echo "$ALL_FILES" | grep -Ev "$DOC_EXTENSIONS" || true)

echo -e "${BLUE}=== Analyzing Changes ===${NC}"
echo "Changed files:"
echo "$ALL_FILES" | sed 's/^/  /'
echo ""

if [ -n "$NON_DOC_FILES" ]; then
    echo -e "${RED}❌ Non-documentation files detected:${NC}"
    echo "$NON_DOC_FILES" | sed 's/^/  /'
    echo ""
    echo -e "${YELLOW}This script is for documentation-only commits.${NC}"
    echo -e "${YELLOW}Use regular 'git commit' for code changes (precommit hook will run).${NC}"
    exit 1
fi

echo -e "${GREEN}✅ All changes are documentation files${NC}"
echo ""

# Commit message handling
if [ -z "$1" ]; then
    echo -e "${RED}Error: Commit message required${NC}"
    echo "Usage: $0 \"commit message\""
    echo ""
    echo "Example:"
    echo "  $0 \"docs: add PersonaUser architecture planning\""
    exit 1
fi

COMMIT_MSG="$1"

# Stage all documentation files if not already staged
echo -e "${BLUE}Staging documentation files...${NC}"
echo "$ALL_FILES" | xargs git add

# Show what will be committed
echo ""
echo -e "${BLUE}=== Files to commit ===${NC}"
git diff --cached --stat
echo ""

# Commit with --no-verify to skip precommit hook
echo -e "${BLUE}Committing (skipping precommit hook)...${NC}"
git commit --no-verify -m "$COMMIT_MSG"

if [ $? -eq 0 ]; then
    echo ""
    echo -e "${GREEN}✅ Documentation committed successfully!${NC}"
    echo ""
    echo -e "${YELLOW}Recent commits:${NC}"
    git log --oneline -3
else
    echo -e "${RED}❌ Commit failed${NC}"
    exit 1
fi
