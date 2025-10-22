# CONTINUUM REPOSITORY AUDIT & CLEANUP PLAN

**Date**: 2025-10-21
**Purpose**: Document current repository state and cleanup strategy for jtag promotion

**Related PR**: [#152 - Failed Migration Attempt: src/debug/jtag → Root (Lessons Learned)](https://github.com/CambrianTech/continuum/pull/152)

---

## 🎯 MISSION: Promote src/debug/jtag to Root

**Current Reality**: All working code lives in `src/debug/jtag/`
**Target State**: Clean repository with `src/debug/jtag/` promoted to root level

**Status**: ⚠️ **First Migration Attempt Failed** (2025-10-22)
- Attempted direct migration caused cascading import failures
- System recovered via `git reset --hard HEAD`
- See PR #152 for detailed 8-phase plan based on lessons learned

---

## 📊 CURRENT REPOSITORY STATE

### Active Codebase (KEEP)
```
src/debug/jtag/
├── commands/          # 66 registered commands
├── daemons/           # 12 active daemons
├── system/            # Core architecture
├── widgets/           # UI components
├── tests/             # 239 test files
├── package.json       # WILL BECOME ROOT package.json
├── tsconfig.json      # TypeScript configuration
└── scripts/           # Build and git hooks
```

### Critical Integrity Tests (MUST PASS ON COMMIT)
**Current precommit hook tests** (lines 97, 105 of `scripts/git-precommit.sh`):
1. `tests/integration/database-chat-integration.test.ts` - CRUD validation
2. `tests/integration/state-system-integration.test.ts` - State system validation

**Total test files**: 239
**Status**: Need to audit which are critical vs experimental/obsolete

### Root Directory Files (AUDIT REQUIRED)

#### Likely KEEP:
- `.git/` - Version control (obviously keep)
- `.gitignore` - Git ignore rules
- `README.md` - Main documentation (needs major update)
- `CLAUDE.md` - Development guide (critical, keep)
- `.env` - Environment config

#### Likely ARCHIVE:
- `agent-scripts/` - Old agent code
- `agents/` - Old agent code
- `examples/` - Old examples (not using jtag)
- `middle-out/` - Old architecture docs?
- `python-client/` - Python client (if obsolete)
- `scripts/` - Root scripts (git hooks moved to src/debug/jtag/scripts/)
- `system/` - Root system dir (conflicting with src/debug/jtag/system/)
- `templates/` - Old templates?
- `verification/` - Old verification code
- `verification_system/` - Old verification code
- `widgets/` - Root widgets dir (conflicting with src/debug/jtag/widgets/)
- `archived-systems/` - Already archived
- `archive/` - Already archived

#### Documentation Files (REVIEW & CONSOLIDATE):
- `ANONYMIZATION-PLAN.md`
- `ARCHITECTURAL-REVISION-PLAN.md`
- `CLEANUP_PLAN.md`
- `DOCUMENTATION-DEBT-ASSESSMENT.md`
- `JTAG-PROMOTION-PATH-UPDATE-PLAN.md`
- `MIDDLE-OUT-EVOLUTION-UPDATE.md`
- `VALIDATION_COMPLETE.md`
- `WORKING_NOTES.md`

Many of these should be archived or consolidated into design/* docs.

#### Config Files (REVIEW):
- `babel.config.cjs`
- `eslint.config.js`
- `.eslintignore`
- `.eslintrc.js`
- `jest.config.cjs`
- `jest.config.ui.js`
- `lerna.json`
- `tsconfig.*.json` files (4 files)

Need to determine which are used by jtag vs obsolete.

---

## 🗂️ TEST SUITE AUDIT (239 Files)

### Test Categories Observed:

#### Integration Tests (Most Important)
- `tests/integration/database-chat-integration.test.ts` ✅ **CRITICAL** (precommit)
- `tests/integration/state-system-integration.test.ts` ✅ **CRITICAL** (precommit)
- `tests/integration/ai-*.test.ts` - AI system validation
- `tests/integration/chat-*.test.ts` - Chat system validation
- `tests/integration/database/*.test.ts` - Database validation
- `tests/integration/widget-integration/*.test.ts` - Widget validation

#### Unit Tests
- `tests/unit/*.test.ts` - Component unit tests
- `tests/unit/router/*.test.ts` - Router unit tests

#### Archived Tests (REMOVE)
- `tests/archive/flawed-tests/*.test.ts` - Known broken tests
- `tests/archived/*.test.ts` - Already archived

#### Layer-Based Tests (REVIEW)
- `tests/layer-1-foundation/*.test.ts`
- `tests/layer-2-daemon-processes/*.test.ts`
- `tests/layer-4-system-integration/*.test.ts`
- `tests/layer-6-browser-integration/*.test.ts`

These might be obsolete or duplicative.

#### Experimental/Debug Tests (LIKELY REMOVE)
- `tests/*-debug*.test.ts` - Debug/diagnostic tests
- `tests/*-hang*.test.ts` - Hang investigation tests
- `tests/*-detection*.test.ts` - Detection/discovery tests
- `tests/*-demo*.test.ts` - Demo tests

### Recommendation: Test Cleanup Strategy

1. **KEEP (Critical for Operations)**:
   - 2 precommit tests (database-chat, state-system)
   - Core integration tests (AI, chat, database, widgets)
   - Core unit tests (router, event system)

2. **REVIEW (Potentially Useful)**:
   - AI system tests
   - Performance tests
   - E2E tests

3. **ARCHIVE (Obsolete/Experimental)**:
   - Already archived tests (tests/archived/, tests/archive/)
   - Debug/diagnostic tests
   - Demo tests
   - Layer-based tests (if replaced by integration tests)
   - Duplicate tests

**Estimated Reduction**: 239 tests → ~50-80 critical tests

---

## 🚀 PROMOTION PLAN (Step-by-Step)

### Phase 1: Repository Audit (CURRENT)
- [x] Document current state
- [ ] Identify critical vs removable tests
- [ ] List root directory files to keep/archive
- [ ] Review config files usage

### Phase 2: Archive Non-JTAG Code
```bash
mkdir -p archive/pre-jtag-promotion/
mv agent-scripts/ archive/pre-jtag-promotion/
mv agents/ archive/pre-jtag-promotion/
mv examples/ archive/pre-jtag-promotion/
mv middle-out/ archive/pre-jtag-promotion/
mv python-client/ archive/pre-jtag-promotion/
mv scripts/ archive/pre-jtag-promotion/  # git hooks already in src/debug/jtag/scripts/
mv system/ archive/pre-jtag-promotion/
mv templates/ archive/pre-jtag-promotion/
mv verification/ archive/pre-jtag-promotion/
mv verification_system/ archive/pre-jtag-promotion/
mv widgets/ archive/pre-jtag-promotion/  # conflicting with jtag widgets
```

### Phase 3: Clean Up Tests
```bash
# Remove archived tests
rm -rf src/debug/jtag/tests/archive/
rm -rf src/debug/jtag/tests/archived/

# Archive experimental tests (TBD which ones)
mkdir -p src/debug/jtag/tests/archive/experimental/
# Move identified experimental tests
```

### Phase 4: Consolidate Documentation
```bash
# Archive completed planning docs
mv *-PLAN.md archive/planning-docs/
mv VALIDATION_COMPLETE.md archive/planning-docs/
mv WORKING_NOTES.md archive/planning-docs/

# Keep critical docs
# - README.md (update it)
# - CLAUDE.md (keep as-is)
# - .gitignore
# - LICENSE (if exists)
```

### Phase 5: Update Root Package.json
```bash
# Promote jtag package.json to root
cp src/debug/jtag/package.json package.json

# Update scripts to remove src/debug/jtag prefix
# Update paths in package.json
```

### Phase 6: Update Git Hooks
```bash
# Git hooks already point to src/debug/jtag/scripts/
# After promotion, update to point to ./scripts/
```

### Phase 7: Update README
- Rewrite README to focus on jtag system
- Installation instructions (npm install, ollama setup)
- Quick start (npm start)
- Architecture overview
- Link to CLAUDE.md for development

### Phase 8: Verify Everything Works
```bash
# Run critical tests
npm test

# Run precommit validation
./scripts/git-precommit.sh

# Verify system boots
npm start
./jtag ping
```

---

## 📋 CHECKLIST: Ready for Promotion

- [ ] All 239 tests audited (keep/archive decision)
- [ ] Critical tests identified and passing
- [ ] Non-jtag code archived
- [ ] Root directory cleaned
- [ ] Documentation consolidated
- [ ] package.json promoted and updated
- [ ] Git hooks updated
- [ ] README rewritten
- [ ] design/* docs updated
- [ ] All tests passing

---

## 📚 Related Resources

### Pull Requests
- [PR #152: Failed Migration Attempt - Lessons Learned](https://github.com/CambrianTech/continuum/pull/152)
  - Documents the first migration attempt that failed (2025-10-22)
  - Contains detailed 8-phase plan for future migration based on lessons learned
  - Includes root causes, lessons learned, success criteria, and recovery protocol

### Design Documentation
- [CHAT-TESTS-ANALYSIS.md](./CHAT-TESTS-ANALYSIS.md) - Chat test suite analysis and categorization
- [OTHER-TESTS-ANALYSIS.md](./OTHER-TESTS-ANALYSIS.md) - Non-chat test suite analysis
- [MIDDLE-OUT-MINING-REPORT.md](./MIDDLE-OUT-MINING-REPORT.md) - Pattern exploitation insights

### Key Insights from Failed Migration
1. **File Interdependency Hell**: Import chains broke when relative paths changed
2. **Build System Complexity**: TypeScript, webpack, and build configurations tightly coupled
3. **Cascading Import Failures**: One broken import caused 50+ downstream failures
4. **Runtime Path Dependencies**: Commands/daemons use `process.cwd()` and relative paths

**Next Attempt**: Follow the 8-phase plan in PR #152, test each phase independently
- [ ] System boots and responds

---

## 🎯 SUCCESS CRITERIA

**After promotion, a new user should be able to**:
1. Clone repo
2. Run `npm install`
3. Install ollama (if want AI)
4. Run `npm start`
5. See system boot successfully
6. Run `./jtag ping` and see 66 commands, 12 daemons
7. Open browser to localhost and see working UI
8. Make code changes and commit (precommit tests pass)

---

## 📝 NOTES

- This is a major restructure - do incrementally
- Commit after each phase
- Keep git history clean
- Verify tests pass after each phase
- Archive, don't delete (can recover if needed)
