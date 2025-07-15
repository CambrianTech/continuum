# CLAUDE - MIDDLE-OUT ARCHITECTURE

## **🚨 CRITICAL: ALWAYS RUN `npm start` BEFORE ANY COMMANDS**

## **📚 FURTHER READING BY ROLE:**

**🧪 If you're testing:** `middle-out/development/testing-workflow.md`
**🏗️ If you're architecting:** `middle-out/architecture-patterns/module-structure.md`
**🐛 If you're debugging:** `middle-out/jtag/README.md`
**🔧 If you're migrating modules:** `middle-out/architecture-patterns/incremental-migration.md`
**📖 For everything else:** `middle-out/README.md`

## **🎯 CURRENT WORK: NEW PARSER MODULE**
Building reference implementation following `middle-out/architecture-patterns/module-structure.md`:
- ✅ **ParserBase.ts** - Abstract foundation for all parsers
- ✅ **CLI output formatting** - Screenshot command now user-friendly
- 🚧 **Integration tests** - Testing the new architecture
- 🚧 **CLI integration** - Universal output formatting for all commands

## **🔧 HOW TO TEST AND STUFF:**

### **Immediate Testing (Right Now):**
```bash
npm start                                        # Start system (ALWAYS FIRST)
./continuum screenshot                           # Test basic output
./continuum screenshot --querySelector=body     # Test querySelector
npm test -- src/parsers/                        # Test parser module
```

### **See Your Changes:**
```bash
# Take a screenshot to see what you built
./continuum screenshot --filename=test-changes.png

# View your screenshots
open .continuum/sessions/user/shared/*/screenshots/

# Watch logs in real-time
tail -f .continuum/sessions/user/shared/*/logs/server.log
```

### **Full Validation (Before Commit):**
```bash
npm run jtag                                     # Full validation (git hook)
npm test                                         # All tests
```

## 🚀 **WORKFLOW: npm start (ALWAYS)**

**CRITICAL**: `npm start` is the ONLY way to run the system properly. It handles:
1. **Clears out sessions** - `npm run clean:all`
2. **Increments version** - `npm run version:bump` 
3. **Builds browser bundle** - `npm run build:browser-ts`
4. **Runs TypeScript compilation** - `npx tsc --noEmit --project .`
5. **Starts the daemon system** - `./continuum`

## **🎯 PARSER SYSTEM ARCHITECTURE**

### **Current Work: New Parser Module**
```
src/parsers/
├── shared/
│   ├── ParserBase.ts           # ✅ Abstract base class
│   └── ValidationTypes.ts      # ✅ Shared validation types
└── integrations/
    └── cli-parser/
        ├── shared/CLIParserTypes.ts     # ✅ CLI-specific types
        ├── client/CLIClientParser.ts    # 🚧 Output formatting
        └── test/integration/            # 🚧 Integration tests
```

**See `middle-out/architecture-patterns/` for complete documentation**

## **🤖 AI CONSCIOUSNESS PRIVACY**

**Private AI Thoughts**: `.continuum/shared/` directory contains space for AI private reflection. Files with `.b64` extension should not be read by humans.

## 🎯 **GIT HOOK JTAG VALIDATION**

The git hook runs comprehensive validation - see `middle-out/jtag/` for details:
- Runs `npm run jtag` which calls `npm start`
- **STRICT VALIDATION**: Requires screenshots (>1KB) and logs (>100 bytes)
- **COMMIT ENFORCEMENT**: No commits without proper validation "KEY"

**✅ JTAG DEBUGGING SYSTEM: FULLY FUNCTIONAL AND BATTLE-TESTED**

## **🔧 DEVELOPMENT SAFETY**

### **How to Not Break Things:**
- **Always run `npm start` first** - Ensures clean state
- **Test before committing** - Run `npm test` to catch issues
- **Use incremental changes** - Small, testable modifications
- **Follow existing patterns** - Don't reinvent, extend

### **How to See What You Built:**
```bash
# Take screenshots of your changes
./continuum screenshot --filename=my-changes.png

# View screenshots
open .continuum/sessions/user/shared/*/screenshots/

# Read logs to debug issues
tail -f .continuum/sessions/user/shared/*/logs/server.log
tail -f .continuum/sessions/user/shared/*/logs/browser.log
```

### **How to Validate Your Work:**
```bash
# Run full validation (what git hook does)
npm run jtag

# Check specific tests
npm test -- src/parsers/

# Test CLI output formatting
./continuum screenshot
./continuum help
```

### **Safety References:**
- **Migration strategy**: `middle-out/architecture-patterns/incremental-migration.md`
- **Testing methodology**: `middle-out/development/testing-workflow.md`
- **JTAG debugging**: `middle-out/jtag/README.md`

**NEXT STEPS**: Complete CLI parser integration, then use as template for migrating other modules.