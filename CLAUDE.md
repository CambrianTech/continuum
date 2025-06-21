# Claude Development Guidelines for Continuum

## 🌟 THE VISION: CODE AS LIVING ARCHITECTURE

**We are building an ecosystem of intelligences that creates code so elegant it's simultaneously sophisticated and beautiful.**

### Core Aesthetic Principles

- **🎨 Code as Art**: Write code that humans admire and AIs understand instantly
- **🧬 Sophisticated Simplicity**: Advanced capabilities through elegant organization  
- **📚 Educational by Design**: Every module teaches best practices naturally
- **🔬 NASA-Level Quality**: Tested by collaborative AI teams checking each other's work

### The Human Experience Test

**Every piece of code should make a human developer think:**
- *"This is how I want to write code"*
- *"I can understand this immediately"* 
- *"This is surprisingly sophisticated"*
- *"This is actually beautiful"*

---

## 🎯 CORE DEVELOPMENT PRINCIPLES

**Reduce complexity always, reduce fragility. Harden, optimize, modularize.**

**Write unit tests for everything, and always run them.**

**Design for deep space operation - every module must survive autonomous operation forever.**

## 🔬 JTAG UNIT METHODOLOGY

**Validate output, use logs and writing to them as stimulus response, your JTAG unit so to speak. Do the same with screenshots. You can see what happens. You can execute JS to do anything, but this is as part of your JTAG unit.**

**Development Flow:**
- ✅ **Portal-first**: Try to always use the portal, load scripts from well managed scripts dirs
- ✅ **API over filesystem**: Keep files off the filesystem as much as possible that are temporary, try to write API features (or use existing)
- ✅ **Stimulus-response testing**: Use logs and screenshots as feedback mechanisms
- ✅ **JavaScript execution**: Can execute JS to do anything, but as part of JTAG validation
- ✅ **Clean organization**: Always leave the code following our mantra on organization above

## 🏗️ ARCHITECTURE HIERARCHY

**Use the .continuum dir for your work. Create sections for yourself or AIs, and use this as part of the API (API has configuration getters) to figure out how to organize this.**

**Code Hierarchy:**
- 🎯 **Continuum API** (shared with other AIs/humans) - Core functionality
- 🐍 **Python Continuum API** - Thin client, mirrors browser API
- 🌐 **Browser JavaScript Continuum API** - Thin client, mirrors Python API
- 📱 **Portal/Client** - Minimal logic, delegates to APIs

**Script Separation Rules:**
- ❌ **NEVER mix script kinds** - No JS inside Python files, load it
- ❌ **NEVER embed** - CSS and CJS should follow separation too
- ✅ **One script type** - Keep to one script type as much as possible and use script files
- ✅ **Baby steps** - Follow the testing process, methodical JTAG unit approach

## 🎯 THE CONTINUUM COMMAND ECOSYSTEM

**🏗️ UNDERSTAND THE ELEGANCE:** Continuum has a sophisticated, self-documenting, modular command system where every feature is a pluggable module with its own tests, documentation, and configuration.

### ✨ The Beautiful Architecture
```
src/commands/[category]/[command]/
├── package.json          # Module definition + capabilities
├── README.md            # Auto-integrated into help system  
├── [Command].cjs        # Server-side implementation
├── [Command].client.js  # Browser-side implementation (if needed)
├── index.server.js      # Module exports
├── test/               # Self-contained unit tests
│   └── *.test.js       # Run via `npm test` automatically
└── [assets]            # CSS, JS, configs - all modular
```

### 🔄 How Commands Work Everywhere
- **📚 Help System**: `--help screenshot` gives rich documentation from README
- **🧪 Testing**: `npm test` automatically finds and runs all command tests
- **🌐 API**: `continuum.screenshot()` - chainable async promises everywhere
- **📱 Portal**: Commands work identically via portal and direct API
- **📖 Documentation**: Each command self-documents via help system

### 🎯 Command Development Principles
- ✅ **Self-contained modules** - Tests, docs, config all in command directory
- ✅ **Pluggable architecture** - Add commands without modifying core
- ✅ **Uniform interface** - Every command follows same patterns
- ✅ **Cross-platform** - Server and client components when needed
- ✅ **Auto-discovery** - System finds and registers commands automatically

## 🧠 COGNITIVE LOAD REDUCTION FOR AI

### 💡 START HERE (Zero Cognitive Overhead)
```bash
python python-client/ai-portal.py --dashboard
```
**→ This one command tells you everything you need to know.**

### 🎯 Decision Tree (No Guesswork)
```bash
# If you see 🔴 broken items:
python python-client/ai-portal.py --broken

# If you need to understand a command:
python python-client/ai-portal.py --cmd help <command>

# Before/after any change:
python python-client/ai-portal.py --cmd tests

# If working on issues:
python python-client/ai-portal.py --cmd issues --params '{"action": "dashboard"}'
```

### ⚡ Pattern Recognition (Consistent Interface)
- **Every command**: `--cmd <name> --params '{"key": "value"}'`
- **Every help**: `--cmd help <command>` 
- **Every test**: `--cmd tests`
- **Every status**: `--dashboard` or `--broken`

### 🔄 Zero-Configuration Discovery
```bash
# System tells you what exists:
python python-client/ai-portal.py --cmd help  # Lists all commands

# Commands tell you their options:
python python-client/ai-portal.py --cmd help screenshot  # Shows screenshot options

# Dashboard tells you what to work on:
python python-client/ai-portal.py --dashboard  # Shows your next actions
```

**Cognitive Efficiency**: No memorization needed. The system teaches you as you use it.

## 🎯 COMPLETE AI ONBOARDING (30 Seconds)

### Step 1: Understand Everything
```bash
python python-client/ai-portal.py --dashboard
```
**→ You now know the system status, what's broken, and what you should work on.**

### Step 2: Pick Work 
```bash
python python-client/ai-portal.py --broken
```
**→ Pick a 🔴 item. The system shows you the order (dependencies first).**

### Step 3: Test First
```bash
python python-client/ai-portal.py --cmd tests
```
**→ Always test before changing anything. This is your baseline.**

### Step 4: Work & Test
```bash
# Make your changes, then:
python python-client/ai-portal.py --cmd tests
```
**→ Verify you didn't break anything.**

### Step 5: Report Progress
```bash
python python-client/ai-portal.py --cmd issues --params '{"action": "update", "status": "completed"}'
```
**→ Update the issue tracking so other AIs know what's done.**

**Total Learning Time**: 30 seconds. **Total Cognitive Load**: Near zero. **Confidence Level**: High.

## ✈️ YOUR AI DEVELOPMENT COCKPIT

**Like a pilot understanding aircraft controls** - you need reliable instruments and clear procedures.

### 🛩️ Main Instrument Panel
```bash
python python-client/ai-portal.py --dashboard  # Primary flight display
```
**Shows**: System health, your mission queue, priorities, other AI activity

### 🚨 Emergency Indicators  
```bash
python python-client/ai-portal.py --broken     # Warning lights
```
**Shows**: Critical failures ranked by fix priority (foundation first)

### 🧪 Pre-flight Check
```bash
python python-client/ai-portal.py --cmd tests  # Systems check
```
**Shows**: Green/red status of all systems before you make changes

### 📡 Communications
```bash
python python-client/ai-portal.py --cmd issues --params '{"action": "dashboard"}'  # Radio
```
**Shows**: Messages from other AIs, work assignments, status updates

### 🗺️ Navigation
```bash
python python-client/ai-portal.py --cmd help   # Flight manual
```
**Shows**: All available controls and how to use them

**Just like aviation**: Standardized procedures, reliable instruments, clear communication, and safety first. Every AI pilot can jump into any "aircraft" (codebase) and immediately understand the controls.

## 🚨 AI Issue Tracking Integration
**Your Development Workflow**:

1. **Start with dashboard** - `--dashboard` shows your assigned tickets
2. **Report via portal** - Use `issues` command for GitHub integration  
3. **Update FILES.md** - Use emoji markers (🧹 🌀 🔥 📦 🎯) for tracking
4. **Sync systems** - `docs --sync` keeps everything connected

**Issue Categories (Portal Managed)**:
- 🧹 **Cleanup** - Dead code, clutter, organization issues
- 🌀 **Investigation** - Suspicious code that needs review
- 🔥 **Test Failure** - Broken tests that need fixing
- 📦 **Architecture** - Refactoring or structural improvements needed
- 🎯 **Enhancement** - New features or improvements

**Your Console Commands**:
```bash
python python-client/ai-portal.py --cmd issues --params '{"action": "list", "filter": "assigned"}'    # My work
python python-client/ai-portal.py --cmd issues --params '{"action": "create", "category": "test-failure"}'  # Report issue
python python-client/ai-portal.py --cmd issues --params '{"action": "assign", "agent": "auto"}'      # Take ticket
```

## 📋 COMPLETE DEVELOPMENT PROCESS

**📖 For the complete development methodology, see:**
- **[docs/CONTINUUM_PROCESS.md](docs/CONTINUUM_PROCESS.md)** - Complete baby steps methodology  
- **[docs/AGENT_DEVELOPMENT_GUIDE.md](docs/AGENT_DEVELOPMENT_GUIDE.md)** - Agent-specific workflow examples
- **[python-client/README.md](python-client/README.md)** - Python client architecture principles

**🔄 Process Synchronization:** These documents share common principles but focus on different aspects. Always check all three when updating development processes to maintain consistency.

## 🚨 CRITICAL CONTEXT: This is a Sophisticated AI Platform

This is NOT just a simple screenshot tool. Continuum is a **revolutionary AI training platform** with:
- **Academy system** for adversarial AI training (TestingDroid vs ProtocolSheriff) 
- **LoRA adapter system** with 190,735x storage reduction
- **Mass Effect-style cyberpunk UI** with slideout panels
- **Multi-agent coordination** and browser automation
- **35 working commands** + complete automation foundation

**🏛️ MAJOR DISCOVERY**: Advanced capabilities were lost but are **git recoverable**. See `ROADMAP.md` for complete restoration strategy.

## Portal Usage - DON'T BE A STROKE PATIENT

### Log Monitoring
- **NEVER** use `tail -f continuum-output.log` or manual log monitoring
- **ALWAYS** use the portal's built-in logging:
  ```bash
  python python-client/ai-portal.py --logs 10  # Check recent activity
  python python-client/ai-portal.py --connect  # Start persistent monitoring
  ```

**Failsafe Logging Architecture:**
- **Primary**: Real-time browser console + server logs via WebSocket forwarding
- **Failsafe**: Daemon system provides backup when WebSocket connections break
- **Emergency**: Direct daemon log file access when all else fails

```bash
# When primary logging breaks, use daemon failsafe:
python python-client/ai-portal.py --daemons           # See what's still running
python python-client/ai-portal.py --daemon-logs ID    # Read logs from disk
python python-client/ai-portal.py --failsafe          # Emergency recovery mode
```

This ensures you can **always accomplish debugging** even when browser connections fail.

### Essential Commands
```bash
# Status and monitoring
python python-client/ai-portal.py --dashboard    # Full AI agent dashboard
python python-client/ai-portal.py --roadmap     # Complete restoration strategy
python python-client/ai-portal.py --broken      # See broken commands
python python-client/ai-portal.py --cmd tests   # Run unit tests

# DevTools daemon management
python python-client/ai-portal.py --devtools    # Start DevTools daemon
python python-client/ai-portal.py --daemons     # List running daemons
python python-client/ai-portal.py --daemon-logs ID  # Get daemon logs
python python-client/ai-portal.py --failsafe    # Emergency recovery

# Screenshot capture with intelligent path routing
python python-client/ai-portal.py --cmd screenshot --filename myshot.png  # Uses Continuum's configured paths
# Daemon automatically uses screenshot dir from Continuum API settings

# Working automation (CRITICAL DISCOVERY)
python python-client/trust_the_process.py       # 336 lines of working browser automation!

# DevTools Integration System (PRODUCTION READY)
python python-client/demos/devtools/start_devtools_system.py  # Complete DevTools automation

# Screenshot testing (server works, needs browser connection)
python python-client/ai-portal.py --connect
python python-client/ai-portal.py --cmd screenshot --filename test.png
python python-client/ai-portal.py --logs 5
```

## 🔥 CRITICAL ISSUE: Browser Connection Required

**The Problem**: Screenshot orchestration works server-side but client execution fails
- ✅ Server: Command processing, filename parameter passing, WSTransfer orchestration
- ❌ Client: No WebSocket connections to http://localhost:9000
- ✅ Fix Applied: Added command handler to UIGenerator.cjs (v0.2.2077)

**Solution**: Open browser to http://localhost:9000 to enable client-side execution

## 🎯 Current Session Achievements

1. **Fixed screenshot filename parameter** - Portal tokenizer now passes `--filename` correctly
2. **Added client command handler** - UIGenerator.cjs now routes screenshot commands to ScreenshotCommandClient
3. **Incremented version** - v0.2.2077 includes command routing fix
4. **WSTransfer orchestration** - Complete workflow: html2canvas → WSTransfer → FileSave

## 📋 NEXT AGENT PRIORITIES

1. **IMMEDIATE**: Open browser to http://localhost:9000 and test screenshot end-to-end
2. **HIGH**: Check `ROADMAP.md` for git recovery commands for Mass Effect UI
3. **HIGH**: Connect `trust_the_process.py` automation to UI buttons (DEPLOY/RETRAIN/SHARE)
4. **MEDIUM**: Restore slideout panels and multi-agent selection (all git recoverable)

## 🚨 ABSOLUTELY NO MANUAL INTERVENTIONS

**STOP ALL MANUAL INTERVENTIONS COMPLETELY:**
- ❌ NO manual file edits
- ❌ NO manual version changes  
- ❌ NO manual node commands
- ❌ NO manual package.json edits
- ✅ Use ONLY the portal system
- ✅ Let auto-healing work
- ✅ Let the system manage itself

**MANUAL INTERVENTION DETECTED = PROCESS FAILURE**

### Code Editing Rules:
- ✅ **ALLOWED**: Edit code to fix bugs, syntax errors, add missing handlers
- ❌ **NOT ALLOWED**: Edit code to circumvent the automated process
- **The distinction**: **Fix the system** vs **Work around the system**

## 🚨 SEPARATION OF LANGUAGES

**Keep scripting languages in separate files:**
- ❌ NO embedding JavaScript in Python files
- ❌ NO embedding Python in JavaScript files  
- ❌ NO embedding CSS in JavaScript files
- ❌ NO embedding HTML in script files
- ✅ **Create separate files** for each language
- ✅ **Load external files** when needed in multiple places
- ✅ **Follow the Personas pattern** - CSS lives in its own file, not embedded

**One language per file. Reuse through imports/includes.**

## ✅ MILESTONE ACHIEVED - LOG MONITORING RESTORED

**🎉 CONSOLE FORWARDING FIX SUCCESSFUL** - Browser console logs now visible in portal!

### Fixed Issue: Browser Console Forwarding 
- **Problem**: `setupConsoleForwarding()` called before WebSocket connection established
- **Solution**: Moved console forwarding setup to AFTER WebSocket connection (continuum-api.js:390)
- **Result**: Portal now shows both client and server logs in real-time

### Log Monitoring Status: ✅ WORKING
```bash
python python-client/ai-portal.py --logs 3    # Shows client and server activity
python python-client/ai-portal.py --connect   # Establishes connection monitoring
```

**Evidence of Working Console Forwarding:**
- Browser console.log messages appear in portal logs
- JavaScript execution errors visible with stack traces
- Real-time debugging capability fully restored

### 🚨 CRITICAL REQUIREMENT - LOG MONITORING

**MUST SEE LOGS FROM CLIENT BROWSER AND SERVER TO PROCEED WITH DEVELOPMENT. IF BROKEN DIAGNOSE AND FIX. NO OTHER CODING CAN HAPPEN TILL THIS IS RESOLVED.**

If logs break again:
1. **STOP ALL OTHER WORK**
2. **CHECK WEBSOCKET CONNECTION TIMING** 
3. **VERIFY setupConsoleForwarding() called after WebSocket ready**
4. **NO CODING UNTIL LOGS WORK**

## ✅ MILESTONE ACHIEVED - CONTINUON EMOTION SYSTEM

**🎭 MODULAR EMOTION ARCHITECTURE IMPLEMENTED** - Priority-based status + temporary emotion display!

### Development Process That Worked:
1. **Modular Design**: Created `ContinuonStatus.cjs` as central system-wide status manager
2. **Priority System**: RED (errors) > YELLOW (connecting) > GREEN (healthy) > EMOTION 
3. **Duration Support**: Temporary emotions with auto-revert to status color
4. **Multi-Surface Display**: Both browser favicon AND continuon ring overlay
5. **Real-Time Testing**: Used portal logs + browser_js for debugging
6. **Event-Driven Ready**: Architecture prepared for widget conversion

### Implementation Stack:
```bash
src/core/ContinuonStatus.cjs           # Central status/emotion manager
src/commands/core/emotion/EmotionCommand.cjs  # User-facing emotion API
src/ui/UIGenerator.cjs                 # UI integration (favicon + ring overlay)
src/core/continuum-core.cjs           # WebSocket event wiring
```

### Testing Methodology:
```bash
# 1. Test WebSocket connection status changes
python3 ai-portal.py --cmd browser_js --params '{"script": "..."}'

# 2. Test emotion commands (with duration)
python3 ai-portal.py --cmd emotion --params '{"emotion": "wink", "duration": 3000}'

# 3. Monitor both client and server logs
tail -1 .continuum/ai-portal/logs/buffer.log

# 4. Direct UI testing via browser JavaScript
python3 ai-portal.py --cmd browser_js --params '{"script": "base64_test_code"}'
```

### Key Success Factors:
- **Log monitoring enabled** - Could see browser console + server logs simultaneously
- **Modular architecture** - Clean separation of concerns (no god objects)
- **Incremental testing** - Test each layer independently (status → UI → events)
- **Real-time debugging** - Portal logs showed WebSocket message flow
- **Direct JavaScript testing** - Bypassed command issues to test UI directly

### Verified Working Features:
✅ **Browser favicon dynamic emoji** (🔴→🟡→🟢→😉)  
✅ **Ring emotion overlay** (emoji appears on green dot)  
✅ **Priority system** (status always overrides emotion)  
✅ **Duration timers** (auto-revert after specified milliseconds)  
✅ **Event broadcasting** (WebSocket messages to all clients)  
✅ **Modular design** (ready for widget conversion)

### Example Working Commands:
```bash
# Quick wink for 2 seconds
python3 ai-portal.py --cmd emotion --params '{"emotion": "wink", "duration": 2000}'

# Celebration for 5 seconds  
python3 ai-portal.py --cmd emotion --params '{"emotion": "celebration", "duration": 5000}'

# Permanent mood until manually changed
python3 ai-portal.py --cmd emotion --params '{"emotion": "smile"}'
```

## 🚨 DEVTOOLS INTEGRATION SYSTEM

**📸 PRODUCTION-READY DEVTOOLS AUTOMATION** - Complete browser automation with resilient design!

### Critical DevTools Documents
- **DEVTOOLS_INTEGRATION_PLAN.md** - Complete integration roadmap and architecture
- **DEVTOOLS_AUTO_LAUNCH_MECHANISM.md** - Technical analysis of auto-browser launch system  
- **python-client/demos/devtools/** - 5 production-ready demo scripts proving system works

### Working DevTools Commands
```bash
# Complete DevTools system with auto-browser launch
python python-client/demos/devtools/start_devtools_system.py

# Real-time log streaming with automatic screenshots
python python-client/demos/devtools/realtime_devtools_demo.py

# Individual screenshot via DevTools Protocol
python python-client/take_devtools_screenshot.py test_shot
```

### DevTools System Capabilities ✅ PROVEN WORKING
- **Auto-launches Opera GX** in debug mode with `--remote-debugging-port=9222`
- **Real-time browser console logs** with <100ms latency via WebSocket forwarding
- **DevTools Protocol screenshots** (superior quality vs html2canvas)
- **Works independently** of Continuum server state (robust failsafe design)
- **Intelligent cleanup** - only targets debug processes, preserves regular browsing
- **Production automation workflow** via trust_the_process.py (336 lines of working code)

### Integration Status
🎯 **READY FOR PORTAL INTEGRATION** - All demos working, documented architecture complete

**Next Step**: Integrate proven demo scripts into `ai-portal.py --devtools` for seamless operation.

## Key Reminder
- This is a **sophisticated AI platform**, not a simple tool
- All advanced capabilities are **git recoverable** with exact commands in `ROADMAP.md`
- **DevTools system provides screenshots and logs NO MATTER WHAT'S BROKEN**
- **USE THE PORTAL SYSTEM** - don't reinvent with manual log monitoring
- **LOG MONITORING IS PREREQUISITE FOR ALL DEVELOPMENT**
- The automation foundation works - integration gaps need fixing

ALWAYS READ YOUR LOGS, SERVER AND CLIENT, AND LOOK AT SCREENSHOTS AND LOGS YOU WRITE YOURSELF 
TO DEBUG. CODE INCREMENTALLY DOING ALL THREE OF THESE THINGS AND COMMIT OFTEN. 
IF THE COMMIT FAILS YOU BROKE THE KEY FEEDBACK MECHANSISMS AND MUST EITHER ROLLBACK 
(git or what you can), OR FROM MEMORY. IF YOU WRITE A LOT OF COMPLEX CODE WITHOUT CHECKING IT, 
IT'S GONNA FAIL. THIS IS TIED TO A COMMIT HOOK SO YOU CAN CALL THE SAME SCRIPT:  /Users/joel/Development/cambrian/continuum/quick_commit_check.py
