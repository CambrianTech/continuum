# Command Reorganization Plan

**Status**: Ready to execute (system tested and working after revert)

**Lesson Learned**: Moving everything at once = disaster. Do ONE category at a time, test between each.

## Goal: 8-Category Structure

Organize ~50 top-level command namespaces into functional categories:

1. **utilities** (4) - hello, docs, pipe, lease
2. **workspace** (4) - git, recipe, task, tree
3. **interface** (9) - click, get-text, navigate, proxy-navigate, screenshot, scroll, type, wait-for-element, web
4. **collaboration** (5) - chat, decision, activity, wall, content
5. **storage** (4) - data, file, media, logs
6. **development** (7) - debug, code, compile-typescript, generate, schema, exec, shell
7. **platform** (11) - system, help, list, ping, indicator, process-registry, security, session, state, theme, user
8. **intelligence** (6+) - ai, genome, persona, rag, training, continuum

## Category Shortcuts (Path Aliases)

Add to `tsconfig.json` after each category is moved:

```json
{
  "paths": {
    "@commands/*": ["commands/*"],
    "@daemons/*": ["daemons/*"],
    "@system/*": ["system/*"],
    "@widgets/*": ["widgets/*"],
    "@shared/*": ["shared/*"],
    "@types/*": ["types/*"],
    "@browser/*": ["browser/*"],
    "@server/*": ["server/*"],
    "@generator/*": ["generator/*"],

    // Category shortcuts (add incrementally)
    "@commands-utilities/*": ["commands/utilities/*"],
    "@commands-workspace/*": ["commands/workspace/*"],
    "@commands-interface/*": ["commands/interface/*"],
    "@commands-collaboration/*": ["commands/collaboration/*"],
    "@commands-storage/*": ["commands/storage/*"],
    "@commands-development/*": ["commands/development/*"],
    "@commands-platform/*": ["commands/platform/*"],
    "@commands-intelligence/*": ["commands/intelligence/*"]
  }
}
```

## Incremental Migration Strategy

### Do ONE category at a time:

1. ✅ **Physical Move** - Create directory, move command folders
2. ✅ **Update Generated Files** - Fix browser/generated.ts and server/generated.ts
3. ✅ **Update Command Constructors** - Change `super('hello', ...)` to `super('utilities/hello', ...)`
4. ✅ **Update Imports** - Use wildcards to fix cross-command imports
5. ✅ **Update Constants** - Fix any command name constants
6. ✅ **Build & Test** - `npm run build:ts` then `npm start`
7. ✅ **Commit** - One category per commit
8. ✅ **Add Path Alias** - Update tsconfig.json with category shortcut

### Wildcard Patterns for Automation

For each category, these patterns need updating:

#### 1. Generated Files (`browser/generated.ts`, `server/generated.ts`)
```bash
# Example for utilities category:
sed -i '' 's|commands/hello/|commands/utilities/hello/|g' browser/generated.ts server/generated.ts
sed -i '' 's|commands/docs/|commands/utilities/docs/|g' browser/generated.ts server/generated.ts
sed -i '' 's|commands/pipe/|commands/utilities/pipe/|g' browser/generated.ts server/generated.ts
sed -i '' 's|commands/lease/|commands/utilities/lease/|g' browser/generated.ts server/generated.ts
```

#### 2. Cross-Command Imports (Relative Paths)
```bash
# Fix imports FROM the moved category
find commands/utilities -name "*.ts" -exec sed -i '' \
  -e 's|'"'"'../../../system/|'"'"'@system/|g' \
  -e 's|'"'"'../../../daemons/|'"'"'@daemons/|g' \
  -e 's|'"'"'../../../shared/|'"'"'@shared/|g' \
  {} \;

# Fix imports TO the moved category (from other commands)
find commands -name "*.ts" -exec sed -i '' \
  -e 's|'"'"'../../hello/|'"'"'@commands/utilities/hello/|g' \
  -e 's|'"'"'../../docs/|'"'"'@commands/utilities/docs/|g' \
  -e 's|'"'"'../../pipe/|'"'"'@commands/utilities/pipe/|g' \
  -e 's|'"'"'../../lease/|'"'"'@commands/utilities/lease/|g' \
  {} \;
```

#### 3. Command Constructors
```bash
# Update super() calls to include category prefix
find commands/utilities -name "*Command.ts" -exec sed -i '' \
  -e "s|super('hello'|super('utilities/hello'|g" \
  -e "s|super('docs'|super('utilities/docs'|g" \
  -e "s|super('pipe'|super('utilities/pipe'|g" \
  -e "s|super('lease'|super('utilities/lease'|g" \
  {} \;
```

#### 4. System Files (daemons, system, widgets, services)
```bash
# Fix any hardcoded command names in system code
find daemons system widgets services -name "*.ts" -exec sed -i '' \
  -e "s|executeCommand('hello'|executeCommand('utilities/hello'|g" \
  -e "s|executeCommand('docs'|executeCommand('utilities/docs'|g" \
  {} \;
```

#### 5. Test Files
```bash
# Fix test files that reference moved commands
find tests -name "*.ts" -exec sed -i '' \
  -e 's|@commands/hello/|@commands/utilities/hello/|g' \
  -e 's|@commands/docs/|@commands/utilities/docs/|g' \
  {} \;
```

#### 6. Documentation
```bash
# Update markdown files
find . -name "*.md" -exec sed -i '' \
  -e 's|commands/hello/|commands/utilities/hello/|g' \
  -e 's|commands/docs/|commands/utilities/docs/|g' \
  {} \;
```

## Execution Checklist (Per Category)

### Phase 1: Physical Move
- [ ] Create `commands/<category>/` directory
- [ ] Move command directories: `git mv commands/<cmd> commands/<category>/`
- [ ] Verify: `ls commands/<category>/` shows all expected commands

### Phase 2: Update Generated Files
- [ ] Update `browser/generated.ts` imports
- [ ] Update `server/generated.ts` imports
- [ ] Update command registry `name:` fields to include category prefix
- [ ] Verify: `grep "commands/<category>/" browser/generated.ts | wc -l`

### Phase 3: Update Command Constructors
- [ ] Find all Command classes: `find commands/<category> -name "*Command.ts"`
- [ ] Update `super()` calls to include category in command name
- [ ] Verify: `grep "super('<category>/" commands/<category>/**/*Command.ts`

### Phase 4: Update Imports (Wildcards)
- [ ] Run wildcard sed commands for:
  - [ ] Cross-command imports
  - [ ] System file imports
  - [ ] Test file imports
  - [ ] Documentation
- [ ] Verify: `npm run build:ts 2>&1 | grep "Cannot find module" | wc -l` (should be 0)

### Phase 5: Update Constants
- [ ] Check `commands/shared/SystemCommandConstants.ts`
- [ ] Check `commands/shared/CommandConstants.ts`
- [ ] Update any command name constants to include category prefix
- [ ] Verify: Test imports work

### Phase 6: Build & Test
- [ ] `npm run build:ts` - Must succeed with 0 errors
- [ ] `npm start` - Wait ~130 seconds for deployment
- [ ] `./jtag ping` - Verify server connection
- [ ] `./jtag <category>/<command>` - Test a command from the category
- [ ] Browser test: Open localhost and verify widgets work
- [ ] Check logs: `tail -f .continuum/sessions/*/logs/server.log`

### Phase 7: Add Path Alias
- [ ] Add `"@commands-<category>/*": ["commands/<category>/*"]` to tsconfig.json
- [ ] Verify: `npm run build:ts` still works
- [ ] Optional: Update some imports to use new alias

### Phase 8: Commit
- [ ] `git add .`
- [ ] `git commit -m "refactor: organize <category> commands into commands/<category>/"`
- [ ] Document in this file which category is complete

## Migration Order (Easiest → Hardest)

### ✅ Round 1: Simple Categories (Low Risk)
1. **utilities** (4 commands) - Least dependencies, good test case
2. **workspace** (4 commands) - Mostly self-contained
3. **interface** (9 commands) - Browser automation, minimal cross-deps

### ✅ Round 2: Medium Categories
4. **collaboration** (5 commands) - Chat, decision system
5. **development** (7 commands) - Dev tools

### ✅ Round 3: Complex Categories (High Risk)
6. **storage** (4 commands) - Used EVERYWHERE, high impact
7. **platform** (11 commands) - Core system commands
8. **intelligence** (6+ commands) - AI/ML commands

## Progress Tracker

| Category | Commands | Status | Commit | Notes |
|----------|----------|--------|--------|-------|
| utilities | 4 | ⏳ Ready | - | Start here! |
| workspace | 4 | 📋 Planned | - | |
| interface | 9 | 📋 Planned | - | |
| collaboration | 5 | 📋 Planned | - | |
| development | 7 | 📋 Planned | - | |
| storage | 4 | 📋 Planned | - | High impact |
| platform | 11 | 📋 Planned | - | Core system |
| intelligence | 6+ | 📋 Planned | - | AI/ML |

## Rollback Plan

If something breaks:
```bash
# Revert the category
git reset --hard HEAD~1

# Or stash and investigate
git stash push -m "broken: <category> migration"

# Check logs
tail -50 .continuum/sessions/*/logs/server.log
```

## Success Criteria

- ✅ All commands moved to category directories
- ✅ `npm run build:ts` succeeds with 0 errors
- ✅ `npm start` deploys successfully
- ✅ `./jtag ping` works
- ✅ Browser widgets load without errors
- ✅ All 8 category shortcuts added to tsconfig.json
- ✅ System tested end-to-end

## Notes

- **NEVER move multiple categories at once**
- **ALWAYS test after each category**
- **ALWAYS commit after each category**
- **Use wildcards for automation, but verify results**
- **Check git diff before committing to catch mistakes**
