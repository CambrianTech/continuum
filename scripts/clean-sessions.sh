#!/bin/bash

# Clean old session directories

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🧹 Continuum Session Cleaner${NC}"
echo -e "${BLUE}=============================${NC}"

# Parse command line arguments
KEEP_HOURS=1
DELETE_ALL=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --keep-hours)
            KEEP_HOURS="$2"
            shift 2
            ;;
        --all)
            DELETE_ALL=true
            shift
            ;;
        *)
            echo "Usage: $0 [--keep-hours N] [--all]"
            echo "  --keep-hours N  Keep sessions from last N hours (default: 1)"
            echo "  --all          Delete all sessions"
            exit 1
            ;;
    esac
done

# Clean session directories
if [ -d ".continuum/sessions" ]; then
    # Count total sessions before cleaning
    TOTAL_BEFORE=$(find .continuum/sessions -type d -name "*-*" 2>/dev/null | wc -l | tr -d ' ')
    
    if [ "$DELETE_ALL" = true ]; then
        echo -e "${YELLOW}🗑️  Deleting all sessions (except validation)...${NC}"
        find .continuum/sessions -mindepth 1 -maxdepth 1 -type d -not -name "validation" -exec rm -rf {} \; 2>/dev/null || true
        echo -e "${GREEN}✅ All sessions deleted (validation preserved)${NC}"
    else
        echo -e "${YELLOW}🕐 Keeping sessions from last ${KEEP_HOURS} hour(s)...${NC}"
        
        # Calculate minutes
        KEEP_MINUTES=$((KEEP_HOURS * 60))
        
        # Find and delete old sessions (but never delete validation directory)
        find .continuum/sessions -type d -name "*-*" -not -path "*/validation/*" -mmin +${KEEP_MINUTES} -exec rm -rf {} \; 2>/dev/null || true
        
        # Count remaining sessions
        TOTAL_AFTER=$(find .continuum/sessions -type d -name "*-*" 2>/dev/null | wc -l | tr -d ' ')
        DELETED=$((TOTAL_BEFORE - TOTAL_AFTER))
        
        echo -e "${GREEN}✅ Cleaned ${DELETED} old sessions (kept ${TOTAL_AFTER} recent)${NC}"
    fi
    
    # Show disk space saved
    if command -v du >/dev/null 2>&1; then
        SPACE_USED=$(du -sh .continuum/sessions 2>/dev/null | cut -f1 || echo "0")
        echo -e "${BLUE}💾 Sessions now using: ${SPACE_USED}${NC}"
    fi
else
    echo -e "${GREEN}✅ No sessions directory found${NC}"
fi

# Also clean any orphaned log files
echo -e "${YELLOW}🧹 Cleaning orphaned logs...${NC}"
find .continuum -name "*.log" -type f -mmin +$((KEEP_HOURS * 60)) -not -path "*sessions*" -delete 2>/dev/null || true
echo -e "${GREEN}✅ Orphaned logs cleaned${NC}"

# Clean dist directory and build artifacts
echo -e "${YELLOW}🧹 Cleaning build artifacts...${NC}"
rm -rf dist/ 2>/dev/null || true
rm -f .tsbuildinfo 2>/dev/null || true
rm -f src/ui/continuum-browser.js* 2>/dev/null || true
echo -e "${GREEN}✅ Build artifacts cleaned${NC}"

# If we deleted all sessions, kill ALL continuum processes to ensure clean state
if [ "$DELETE_ALL" = true ]; then
    echo -e "${YELLOW}🔄 Stopping all Continuum processes for clean state...${NC}"
    # Kill ALL continuum-related processes
    pkill -f "tsx.*main\.ts|node.*main\.ts|continuum|esbuild.*service" 2>/dev/null || true
    # Also kill any orphaned node processes on port 9000
    lsof -ti:9000 | xargs kill -9 2>/dev/null || true
    sleep 2
    echo -e "${GREEN}✅ All processes stopped - clean state achieved${NC}"
fi

echo -e "${BLUE}=============================${NC}"
echo -e "${GREEN}🎉 Session cleanup complete!${NC}"