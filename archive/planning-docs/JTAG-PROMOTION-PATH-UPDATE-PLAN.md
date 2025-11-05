# JTAG Promotion: Hardcoded Path Update Plan

**Date**: 2025-10-21
**Purpose**: Document all hardcoded `src/debug/jtag` references that must be updated before promoting JTAG to `src/`

## Critical Files Requiring Updates

### 1. package.json
**Lines**: 17, 18, 28
**Current**:
```json
"test:jtag": "cd src/debug/jtag && npm test",
"test:jtag:layer": "cd src/debug/jtag && npm run test:layer-",
"test": "echo 'WARNING_legacy system, cd into src/debug/jtag IMMEDIATELY!' && npm run compile && npm run test:unit && npm run test:integration:layer2-3",
```

**Updated**:
```json
"test:jtag": "cd src && npm test",
"test:jtag:layer": "cd src && npm run test:layer-",
"test": "npm run compile && npm run test:unit && npm run test:integration:layer2-3",
```

---

### 2. tsconfig.json
**Lines**: 44, 46, 47, 48, 49
**Current**:
```json
"src/debug/jtag/**/*",
"src/debug/jtag/shared/modules/**/*",
"src/debug/jtag/shared/transports/**/*",
"src/debug/jtag/shared/transport-examples.ts",
"src/debug/jtag/shared/JTAGTransportFactory.ts"
```

**Updated**:
```json
"src/**/*",
"src/shared/modules/**/*",
"src/shared/transports/**/*",
"src/shared/transport-examples.ts",
"src/shared/JTAGTransportFactory.ts"
```

---

### 3. .gitignore
**Lines**: 14, 129
**Current**:
```
# Generated manifest files (use generated.ts instead)
src/debug/jtag/manifests/

/src/debug/jtag/.archive
```

**Updated**:
```
# Generated manifest files (use generated.ts instead)
src/manifests/

/src/.archive
```

---

### 4. .git/hooks/pre-commit
**Line**: 6
**Current**:
```bash
exec src/debug/jtag/scripts/git-precommit.sh
```

**Updated**:
```bash
exec src/scripts/git-precommit.sh
```

---

### 5. .git/hooks/prepare-commit-msg
**Line**: 15
**Current**:
```bash
VALIDATION_DIR="/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag"
```

**Updated**:
```bash
VALIDATION_DIR="/Volumes/FlashGordon/cambrian/continuum/src"
```

---

### 6. continuum (bash script)
**Line**: 3
**Current**:
```bash
echo "THIS IS THE LEGACY SYSTEM. cd to src/debug/jtag IMMEDIATELY!";
```

**Updated**:
```bash
echo "Continuum CLI - Use ./jtag for direct JTAG system access";
```

---

### 7. scripts/run-tsx-with-paths.sh
**Line**: 16
**Current**:
```bash
npx tsx --tsconfig src/debug/jtag/tsconfig.json "$SCRIPT_PATH" "$@"
```

**Updated**:
```bash
npx tsx --tsconfig src/tsconfig.json "$SCRIPT_PATH" "$@"
```

---

### 8. CLAUDE.md
**Lines**: Multiple (11, 1249, 1270, 1334, 1451, 1751)
**Current** (examples):
```
cd src/debug/jtag
Always work from `src/debug/jtag`
**Location**: `src/debug/jtag/design/dogfood/css-debugging-visual-collaboration/`
```

**Updated**:
```
cd src
Always work from `src`
**Location**: `src/design/dogfood/css-debugging-visual-collaboration/`
```

---

### 9. system/conversation/worker/persona-worker.js
**Line**: Referenced in grep output
**Current**:
```javascript
import { OllamaAdapter } from '../../../src/debug/jtag/dist/daemons/ai-provider-daemon/shared/OllamaAdapter.js';
```

**Updated**:
```javascript
import { OllamaAdapter } from '../../../src/dist/daemons/ai-provider-daemon/shared/OllamaAdapter.js';
```

---

### 10. .claude/settings.local.json
**Lines**: Multiple hardcoded absolute paths
**Current**:
```json
"Bash(SRC=\"/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/.continuum/shared/css-session-screenshots\")",
"Bash(DST=\"/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/design/dogfood/images\")",
"Bash(./src/debug/jtag/jtag:*)",
```

**Updated**:
```json
"Bash(SRC=\"/Volumes/FlashGordon/cambrian/continuum/src/.continuum/shared/css-session-screenshots\")",
"Bash(DST=\"/Volumes/FlashGordon/cambrian/continuum/src/design/dogfood/images\")",
"Bash(./src/jtag:*)",
```

---

## Files to Leave Unchanged (Archive References)

### archive/legacy-src/**
- All references in archived files should remain unchanged
- They document the historical transition
- Examples:
  - `archive/legacy-src/README.md` - Historical documentation
  - `archive/legacy-src/ui/continuum-browser.js` - Frozen legacy code

---

## Documentation Files to Update

### ARCHITECTURAL-REVISION-PLAN.md
Multiple references to `src/debug/jtag/daemons/` paths that should be updated to `src/daemons/`

### middle-out/architecture/*.md
Multiple documentation files with `src/debug/jtag/` references that should be updated to `src/`

---

## Update Order (Critical Path)

1. **Git hooks first** (prevents commit issues)
   - `.git/hooks/pre-commit`
   - `.git/hooks/prepare-commit-msg`

2. **Build configuration**
   - `tsconfig.json`
   - `package.json`
   - `scripts/run-tsx-with-paths.sh`

3. **Git configuration**
   - `.gitignore`

4. **Scripts and settings**
   - `continuum` (bash script)
   - `.claude/settings.local.json`
   - `system/conversation/worker/persona-worker.js`

5. **Documentation**
   - `CLAUDE.md`
   - `ARCHITECTURAL-REVISION-PLAN.md`
   - `middle-out/architecture/*.md`

6. **Verification**
   - Run `npx tsc --noEmit` after each critical file update
   - Test `npm start` before final promotion
   - Verify git hooks work correctly

---

## Post-Update Verification Checklist

- [ ] TypeScript compilation passes: `npx tsc --noEmit`
- [ ] Git hooks execute without errors: `git add JTAG-PROMOTION-PATH-UPDATE-PLAN.md && git commit -m "test"`
- [ ] System starts correctly: `cd src && npm start`
- [ ] Commands register: `cd src && ./jtag ping`
- [ ] All documentation references updated
- [ ] Legacy `continuum` script gives appropriate message

---

## Next Steps After Path Updates

1. Update all paths in this document
2. Test compilation and git hooks
3. Perform the actual file move: `git mv src/debug/jtag/* src/`
4. Update remaining internal references
5. Final system test
6. Commit the promotion
