# CLAUDE - MIDDLE-OUT ARCHITECTURE

## **🚨🚨🚨 CRITICAL: ALWAYS RUN `npm start` BEFORE ANY COMMANDS 🚨🚨🚨**
## **🚨🚨🚨 CRITICAL: ALWAYS RUN `npm start` BEFORE ANY COMMANDS 🚨🚨🚨**
## **🚨🚨🚨 CRITICAL: ALWAYS RUN `npm start` BEFORE ANY COMMANDS 🚨🚨🚨**

## **🔄 BEFORE ANYTHING: npm start**
## **🔄 AFTER CHANGES: npm start**
## **🔄 WHEN IN DOUBT: npm start**
## **🔄 BROKEN SOMETHING: npm start**
## **🔄 TESTING ANYTHING: npm start**
## **🔄 DEBUGGING ISSUE: npm start**

## **📋 DEBUGGING RULE #1: CHECK LOGS IMMEDIATELY**

**BEFORE THEORIZING OR SPINNING:**
1. **Check session logs first**: `.continuum/sessions/user/shared/[SESSION_ID]/logs/server.log` 
2. **Look for actual execution paths** - What's actually being called?
3. **Trace the call stack** - Where are messages really going?
4. **Don't assume routing works** - Verify messages reach intended handlers
5. **IF LOGS DON'T WORK, FIX THEM FIRST** - No debugging without proper logs

**NEVER spin on theories without checking logs first. The logs always tell the truth.**

## **📚 FURTHER READING BY ROLE:**

**🧪 If you're testing:** `middle-out/development/testing-workflow.md`
**🏗️ If you're architecting:** `middle-out/architecture/universal-module-structure.md`
**🐛 If you're debugging:** `middle-out/jtag/README.md`
**🔧 If you're migrating modules:** `middle-out/architecture-patterns/incremental-migration.md`
**📖 For everything else:** `middle-out/README.md`

## **🎯 CURRENT WORK: NEW PARSER MODULE**
Building reference implementation following `middle-out/architecture/universal-module-structure.md`:
- ✅ **ParserBase.ts** - Abstract foundation for all parsers
- ✅ **CLI output formatting** - Screenshot command now user-friendly
- 🚧 **Integration tests** - Testing the new architecture
- 🚧 **CLI integration** - Universal output formatting for all commands

## **🔧 HOW TO TEST AND STUFF:**

### **Immediate Testing (Right Now):**
```bash
npm start                                        # Start system (ALWAYS FIRST)
npm start                                        # YES, RUN IT AGAIN IF UNSURE

# CRITICAL: Check if we broke logging (check session logs directory)
ls -la .continuum/sessions/user/shared/*/logs/
# MUST HAVE: browser.log, browser.log.json, browser.info.json, browser.error.json, browser.warn.json, browser.probe.json
# MUST HAVE: server.log, server.log.json, server.info.json, server.error.json, server.warn.json, server.debug.json
# ALL BROWSER & SERVER LOG FILES ARE CRITICAL FOR AI FEEDBACK AND DEVELOPMENT
# IF ANY MISSING: WE TOTALLY BROKE LOGGING - STASH CHANGES IMMEDIATELY

./continuum screenshot                           # Test basic output
./continuum screenshot --querySelector=body     # Test querySelector
npm test -- src/parsers/                        # Test parser module
```

### **See Your Changes:**
```bash
# FIRST: Make sure system is running
npm start

# Take a screenshot to see what you built
./continuum screenshot --filename=test-changes.png

# View your screenshots
open .continuum/sessions/user/shared/*/screenshots/

# Watch logs in real-time
tail -f .continuum/sessions/user/shared/*/logs/server.log
```

## 📸 **CLAUDE VISUAL DEVELOPMENT FEEDBACK**

**BREAKTHROUGH**: Claude can now get immediate visual feedback on development changes!

### **🎯 Screenshot-Driven Development**
```bash
# Get visual feedback on UI changes
./continuum screenshot --querySelector="chat-widget" --filename="claude-debug-chat.png"
./continuum screenshot --querySelector="continuum-sidebar" --filename="claude-debug-sidebar.png"
./continuum screenshot --querySelector="body" --filename="claude-debug-full.png"
```

### **📁 Screenshot Storage Location**
All screenshots are automatically saved to:
```
.continuum/sessions/user/shared/{SESSION_ID}/screenshots/
```

### **🔄 Visual Development Cycle**
1. **Make changes** - Edit widget or UI code
2. **Restart system** - `npm start` (ALWAYS!)
3. **Capture state** - Screenshot relevant components
4. **Analyze visually** - Check if changes worked
5. **Iterate** - Repeat until satisfied

### **🎨 Verified UI Selectors**
- **`chat-widget`** - Chat interface component
- **`continuum-sidebar`** - Main sidebar navigation  
- **`body`** - Full page capture
- **`div`** - Generic container elements
- **`.app-container`** - Main application container

**Claude can now develop with confidence using visual validation!**

### **Full Validation (Before Commit):**
```bash
npm start                                        # ALWAYS START HERE
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
6. **⚠️ LAUNCHES BROWSER TAB** - `npm start` automatically opens browser interface

## 🏗️ **ARCHITECTURE BREAKTHROUGH: MODULAR CLIENT PATTERN**

### **🎯 REVOLUTIONARY SHARED/CLIENT/SERVER ARCHITECTURE**

**Universal Module Pattern** - Every component follows the same structure:
```
src/api/continuum/              src/commands/browser/screenshot/
├── shared/                     ├── shared/
│   ├── ContinuumClient.ts      │   ├── ScreenshotTypes.ts
│   └── ContinuumTypes.ts       │   └── ScreenshotValidator.ts
├── client/                     ├── client/
│   └── ContinuumBrowserClient.ts │   └── ScreenshotClient.ts
├── server/                     ├── server/
│   └── ContinuumServerClient.ts  │   └── ScreenshotCommand.ts
└── README.md                   └── README.md
```

### **🚀 CODE COMPACTION THROUGH ELEGANT ABSTRACTION**

**Before** (scattered, duplicated):
- `ContinuumBrowserClient.ts`: 386 lines
- `ContinuumServerClient.ts`: ~300 lines
- Duplicate validation, types, error handling

**After** (shared abstractions):
- `shared/ContinuumClient.ts`: ~50 lines (interface)
- `client/ContinuumBrowserClient.ts`: ~100 lines (browser-specific)
- `server/ContinuumServerClient.ts`: ~80 lines (server-specific)

**Code compression ratio**: ~40% reduction through smart abstraction layers

### **✅ BENEFITS ACHIEVED:**
- 🔄 **Eliminated Duplication**: Validation, types, error handling shared
- 📦 **Modular**: Each piece has single responsibility
- 🎯 **Testable**: Shared tests for interface, specific tests for implementations
- 🚀 **Scalable**: Add new client types by extending shared base
- 💡 **Maintainable**: Change shared behavior once, propagates everywhere

### **🎯 SPARSE OVERRIDE PATTERN - CENTRALIZATION OF BURDEN**

**Burden Distribution:**
- **Shared Base**: 80-90% of complexity (validation, processing, formatting)
- **Client Override**: 5-10% (WebSocket, DOM specifics) 
- **Server Override**: 5-10% (file system, daemon routing)

**Three-Layer Testing:**
- **Shared Tests**: Core contract & business logic (integration-agnostic)
- **Client Tests**: Browser APIs, WebSocket, DOM edge cases
- **Server Tests**: File system, process management, daemon communication

**Result**: Most client/server files are thin transport adapters. All heavy cognitive load centralized in shared base.

### **Core Principle: All Commands Accessible via `continuum.commandName()`**
- Browser client is THIN - no business logic, just routing
- Commands route through daemon system, not direct calls
- `continuum.fileSave()`, `continuum.screenshot()` - clean, simple API
- No command-to-command calls, no brittle dependencies

### **Architecture Pattern:**
```
Browser Client (THIN) → Daemon Router → Command → Response
```

### **What NOT to Do:**
- ❌ Fat browser clients with file operations
- ❌ Commands calling other commands directly  
- ❌ Verbose, brittle code fighting the architecture
- ❌ CS 101 overthinking simple problems

### **Follow the Existing Pattern:**
- ✅ Modular daemon routing structure
- ✅ Clean separation of concerns
- ✅ Thin clients, smart commands
- ✅ Elegant simplicity over complexity

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

**See `middle-out/architecture/` for complete documentation**

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
# FIRST: Restart the system
npm start

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
# FIRST: Restart the system
npm start

# Run full validation (what git hook does)
npm run jtag

# Check specific tests
npm test -- src/parsers/

# Test CLI output formatting
./continuum screenshot
./continuum help
```

### **Safety References:**
- **Migration strategy**: `middle-out/architecture/incremental-migration.md`
- **Testing methodology**: `middle-out/development/testing-workflow.md`
- **JTAG debugging**: `middle-out/jtag/README.md`

**NEXT STEPS**: Complete CLI parser integration, then use as template for migrating other modules.