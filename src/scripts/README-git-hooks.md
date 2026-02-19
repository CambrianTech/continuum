# Git Hooks - Repository Validation System

## Overview

This project uses **direct git hooks** (not Husky) for comprehensive repository validation. All hook logic is in **visible scripts** (`scripts/git-*.sh`) that delegate from hidden `.git/hooks/` files.

## Quick Setup

```bash
npm run hooks:setup    # Install all hooks
npm run hooks:status   # Check hook status
npm run hooks:test     # Test all hooks
npm run hooks:remove   # Remove all hooks
```

## Hook Scripts (Visible & Editable)

| Script | Purpose | Speed |
|--------|---------|--------|
| `scripts/git-precommit.sh` | **Comprehensive validation** - CRUD + State + TypeScript + Screenshots | ~2-3min |
| `scripts/git-postcommit.sh` | **Cleanup** - Remove validation artifacts after successful commit | ~1sec |
| `scripts/git-prepush.sh` | **Lightweight checks** - Branch protection + system health | ~2sec |

## Validation Phases

### Pre-Commit (Bulletproof - Blocks Bad Commits)
1. **TypeScript Compilation** - Ensures code compiles
2. **System Deployment** - Starts JTAG system for testing
3. **CRUD Integration** - Database operations (User, Room, ChatMessage)
4. **State Integration** - State management with context injection
5. **Screenshot Proof** - Visual validation of widget state
6. **Session Artifacts** - Complete validation proof collection

### Pre-Push (Fast - Doesn't Block)
1. **Branch Protection** - Prevent direct main branch pushes (optional)
2. **System Health** - Quick connectivity check
3. **Future Enhancements** - Placeholder for remote integration tests

## Architecture

**No External Dependencies:**
- ❌ No Husky
- ❌ No lint-staged
- ❌ No other hook managers
- ✅ Pure git + bash scripts

**Delegation Pattern:**
```
.git/hooks/pre-commit  →  scripts/git-precommit.sh
.git/hooks/post-commit →  scripts/git-postcommit.sh
.git/hooks/pre-push    →  scripts/git-prepush.sh
```

**Benefits:**
- ✅ **Visible scripts** - Easy to edit and debug
- ✅ **No npm dependencies** - Just git + bash
- ✅ **Automatic setup** - `npm run hooks:setup` installs everything
- ✅ **Manageable** - Clear npm commands for all operations

## Testing Individual Hooks

```bash
# Test just the precommit validation
npm run test:precommit

# Test just the pre-push check
npm run test:prepush

# Test both hooks together
npm run hooks:test
```

## Troubleshooting

**Hook not running?**
```bash
npm run hooks:status  # Check if hooks are installed
npm run hooks:setup   # Reinstall if needed
```

**Precommit too slow?**
- The comprehensive validation is intentional (CRUD + State + TypeScript)
- Ensures bulletproof commits but takes 2-3 minutes
- Consider `git commit --no-verify` for emergency bypasses (not recommended)

**Want to bypass hooks temporarily?**
```bash
git commit --no-verify -m "emergency fix"
git push --no-verify
```