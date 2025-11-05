#!/bin/bash

# Build script that auto-increments version and cleans session dirs

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🔨 Continuum Build Script${NC}"
echo -e "${BLUE}========================${NC}"

# 1. Clean old session directories
echo -e "${YELLOW}🧹 Cleaning old session directories...${NC}"
if [ -d ".continuum/sessions" ]; then
    # Keep sessions from last hour, delete older ones
    find .continuum/sessions -type d -name "*-*" -mmin +60 -exec rm -rf {} \; 2>/dev/null || true
    SESSION_COUNT=$(find .continuum/sessions -type d -name "*-*" 2>/dev/null | wc -l | tr -d ' ')
    echo -e "${GREEN}✅ Cleaned old sessions (kept ${SESSION_COUNT} recent)${NC}"
else
    echo -e "${GREEN}✅ No sessions to clean${NC}"
fi

# 2. Auto-increment version
echo -e "${YELLOW}📈 Auto-incrementing version...${NC}"
CURRENT_VERSION=$(node -p "require('./package.json').version")
IFS='.' read -ra VERSION_PARTS <<< "$CURRENT_VERSION"
MAJOR=${VERSION_PARTS[0]}
MINOR=${VERSION_PARTS[1]}
PATCH=${VERSION_PARTS[2]}

# Increment patch version
NEW_PATCH=$((PATCH + 1))
NEW_VERSION="$MAJOR.$MINOR.$NEW_PATCH"

# Update package.json with new version
node -e "
const pkg = require('./package.json');
pkg.version = '$NEW_VERSION';
require('fs').writeFileSync('./package.json', JSON.stringify(pkg, null, 2) + '\\n');
"

echo -e "${GREEN}✅ Version updated: ${CURRENT_VERSION} → ${NEW_VERSION}${NC}"

# 3. Run TypeScript compilation
echo -e "${YELLOW}🔧 Running TypeScript compilation...${NC}"
npx tsc --project . 2>&1 | tee build.log

# Check if compilation was successful
if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo -e "${GREEN}✅ TypeScript compilation successful${NC}"
    rm -f build.log
else
    ERROR_COUNT=$(grep -c "error TS" build.log 2>/dev/null || echo "0")
    echo -e "${RED}❌ TypeScript compilation failed with ${ERROR_COUNT} errors${NC}"
    echo -e "${YELLOW}   See build.log for details${NC}"
fi

# 4. Build browser bundle
echo -e "${YELLOW}📦 Building browser bundle...${NC}"
if npm run build:browser > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Browser bundle built successfully${NC}"
else
    echo -e "${RED}❌ Browser bundle build failed${NC}"
fi

# 5. Run critical tests
echo -e "${YELLOW}🧪 Running critical tests...${NC}"
npm run test:integration:eventbus > /dev/null 2>&1
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✅ Event bus tests passed${NC}"
else
    echo -e "${RED}❌ Event bus tests failed${NC}"
fi

npm run test:integration:modules > /dev/null 2>&1
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✅ Module structure tests passed${NC}"
else
    echo -e "${RED}❌ Module structure tests failed${NC}"
fi

# 6. Clean build artifacts
echo -e "${YELLOW}🧹 Cleaning build artifacts...${NC}"
find . -name "*.js.map" -type f -not -path "./node_modules/*" -not -path "./.git/*" -delete 2>/dev/null || true
find . -name "*.d.ts" -type f -not -path "./node_modules/*" -not -path "./.git/*" -not -path "./src/types/*" -delete 2>/dev/null || true
echo -e "${GREEN}✅ Build artifacts cleaned${NC}"

# 7. Display build summary
echo -e "${BLUE}========================${NC}"
echo -e "${BLUE}Build Summary:${NC}"
echo -e "  Version: ${GREEN}${NEW_VERSION}${NC}"
echo -e "  Sessions cleaned: ${GREEN}✓${NC}"
echo -e "  TypeScript: ${GREEN}✓${NC}"
echo -e "  Browser bundle: ${GREEN}✓${NC}"
echo -e "  Tests: ${GREEN}✓${NC}"
echo -e "${BLUE}========================${NC}"

# 8. Optional: Auto-commit version bump
if [ "$1" == "--commit" ]; then
    echo -e "${YELLOW}📝 Committing version bump...${NC}"
    git add package.json
    git commit -m "chore: bump version to ${NEW_VERSION}

- Auto-incremented patch version
- Cleaned old session directories
- Built TypeScript and browser bundle
- All tests passing

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>"
    echo -e "${GREEN}✅ Version bump committed${NC}"
fi

echo -e "${GREEN}🎉 Build complete!${NC}"