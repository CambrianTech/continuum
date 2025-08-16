# AI AUTONOMOUS DEVELOPMENT BOOTSTRAP
*Your complete guide to AI-native development in the Continuum JTAG system*

---

## 🤖 **FRESH AI QUICK START** *(30 seconds to productivity)*

**Step 1: Get oriented**
```bash
./jtag help                    # Your complete command reference
npm run agent                  # Your development control room
```

**Step 2: Check system health**
```bash
npm run agent:quick            # Instant health check
```

**Step 3: Check for problems**
```bash
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log
```

**Step 4: Take a screenshot (proves everything works)**
```bash
./jtag screenshot --filename=first-test-$(date +%s).png
```

**🎯 If all four work: You're autonomous! Continue with normal development.**  
**❌ If any fail: Use `npm run agent:fix` or check the detailed guides below.**

---

## 📋 **TABLE OF CONTENTS**

### **🚀 QUICK START**
- [**IMMEDIATE ACTIONS**](#immediate-actions) - Get started in 60 seconds
- [**PROOF OF LIFE**](#proof-of-life) - Verify everything works
- [**ESSENTIAL COMMANDS**](#essential-commands) - Your daily toolchain

### **🧠 AI DEVELOPMENT CORE**
- [**AUTONOMOUS DEBUGGING**](#autonomous-debugging) - Systematic failure analysis
- [**EVIDENCE-BASED DEVELOPMENT**](#evidence-based-development) - Never claim without proof
- [**LOG ANALYSIS PATTERNS**](#log-analysis-patterns) - Read the system's truth
- [**MIDDLE-OUT TEST-DRIVEN DEVELOPMENT**](#middle-out-test-driven-development) - Layer-based development with npm test

### **🏆 SUCCESS PATTERNS**
- [**THE BREAKTHROUGH**](#the-breakthrough) - What we achieved (August 2025)
- [**WORKING EXAMPLES**](#working-examples) - Copy-paste solutions
- [**COMMON PITFALLS**](#common-pitfalls) - Avoid these mistakes

### **📚 KNOWLEDGE BASE**
- [**ARCHITECTURE PRINCIPLES**](#architecture-principles) - How to build correctly
- [**FILE LOCATIONS**](#file-locations) - Where everything lives
- [**FUTURE AI SESSIONS**](#future-ai-sessions) - Knowledge inheritance

---

## 🚀 **IMMEDIATE ACTIONS**

### **If you're a new AI session, start here:**

**🤖 NEW: AI Command Center (Your Best Friend!)**
```bash
./jtag help                    # Complete AI-friendly command guide
npm run agent                  # Your autonomous development dashboard
npm run agent:quick            # Instant system status
```

**Step 1: Verify the system works**
```bash
npm test
# Expected: 🎉 ALL BROWSER INTEGRATION TESTS PASSED!
```

**Step 2: Get visual proof**
```bash
./jtag screenshot
# Expected: ✅ screenshot: SUCCESS
```

**Step 3: Check integration evidence (NEW: AI-optimized paths)**
```bash
# EASY WAY (uses currentUser symlink):
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log

# OR traditional way:
grep "AUTOMATED TEST\|PROOF\|INTEGRATION" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log
# Expected: Multiple lines showing browser test execution
```

**✅ If all three work: You're ready for autonomous development**  
**❌ If any fail: Go to [AUTONOMOUS DEBUGGING](#autonomous-debugging)**

---

## 🤖 **AI AUTONOMY FEATURES** *(NEW - August 2025)*

### **🎯 Your AI Command Center**
The system now provides AI-optimized tools for completely autonomous development:

**🚨 ERROR LOGS (Check These FIRST!):**
```bash
# NEW: Dedicated error logs with currentUser symlinks (AI-friendly!)
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.json
```

**📊 AI Dashboard (Your Control Room):**
```bash
npm run agent                  # Full dashboard with everything you need
npm run agent:quick            # Instant status check  
npm run agent:fix              # Auto-fix common issues
```

**💡 AI-Optimized Bash Commands (Copy & Paste Ready):**
```bash
# The dashboard shows you these exact commands - no memorization needed!
tail -50 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log
grep -i error examples/test-bench/.continuum/jtag/currentUser/logs/*.log
ls -la examples/test-bench/.continuum/jtag/currentUser/logs/
```

**🔧 JTAG Help System:**
```bash
./jtag help                    # Complete command reference for AIs
./jtag list                    # All available commands
./jtag ping                    # Quick system health check
```

**🎯 Key Breakthrough**: No more hard-to-remember paths! The `currentUser` symlink gives you direct access to all logs and files for the active session.

---

## 🏆 **THE BREAKTHROUGH**
*What we achieved - August 13, 2025*

### **MILESTONE: TRUE AUTONOMOUS AI DEVELOPMENT**

**🎯 Real browser integration tests that run INSIDE the JTAG browser**
- Not external Puppeteer automation
- Actual WebSocket communication
- Browser logs provide indisputable proof

**Evidence of success:**
```
🎯 PROOF: AUTOMATED BROWSER INTEGRATION TESTS EXECUTED SUCCESSFULLY
✅ INTEGRATION TEST EVIDENCE: This message proves tests ran in actual JTAG browser
🌐 BROWSER INTEGRATION: WebSocket communication working
```

### **Revolutionary Architecture**
```typescript
// Server-side test connects via WebSocket (same as ./jtag screenshot)
const { client } = await JTAGClientServer.connect({
  targetEnvironment: 'server',
  transportType: 'websocket', 
  serverUrl: 'ws://localhost:9001'
});

// Execute JavaScript IN the running browser
const result = await client.commands.exec({
  code: {
    type: 'inline',
    language: 'javascript', 
    source: `
      console.log('🚀 AUTOMATED TEST: Browser test running');
      return { proof: 'BROWSER_INTEGRATION_TESTS_EXECUTED' };
    `
  }
});
```

---

## 🛠️ **ESSENTIAL COMMANDS**

### **🤖 NEW: AI-First Commands**
```bash
./jtag help                 # 🆕 Complete AI command reference (START HERE!)
npm run agent               # 🆕 AI autonomous development dashboard  
npm run agent:quick         # 🆕 Instant system status for AIs
npm run agent:fix           # 🆕 Auto-fix common issues
```

### **System Management**
```bash
npm test                    # Full autonomous test suite
npm run system:start       # Background system launch
npm run system:stop        # Clean shutdown
npm run signal:wait        # Wait for system readiness
```

### **Immediate Debugging (AI-Optimized)**
```bash
./jtag screenshot --querySelector body --filename debug-$(date +%s).png  # Visual validation (creates real PNG files)
./jtag ping                 # Basic connectivity test (shows actual JSON response)
npm run logs:npm           # Monitor startup logs

# 🆕 NEW: AI-friendly error checking (use currentUser symlinks!)
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log
```

### **AI Code Execution (VERIFIED WORKING)**
```bash
# Execute JavaScript in browser - returns actual results
./jtag exec --code "{\"type\": \"inline\", \"language\": \"javascript\", \"source\": \"return {message: 'AI controlled!', timestamp: Date.now()}\"}" --environment browser

# Navigate browser programmatically  
./jtag navigate --url "http://localhost:9002"

# Compile TypeScript on demand
./jtag compile-typescript --code "const msg: string = 'AI compiled this!'; console.log(msg);"
```

### **Evidence Gathering (AI-Optimized with currentUser symlinks)**
```bash
# 🚨 ERRORS FIRST! (NEW: Easy-to-access error logs)
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.json

# System status
grep "Bootstrap complete" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log

# Integration proof
grep "AUTOMATED TEST\|PROOF" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log

# Error analysis (all logs in one place!)
grep -i error examples/test-bench/.continuum/jtag/currentUser/logs/*.log

# 🆕 AI TIP: Use the dashboard for pre-made commands
npm run agent                # Shows you all these commands ready to copy-paste!
```

---

## 🔍 **PROOF OF LIFE**

### **Verify Everything Works (Required Before Any Development)**

**☐ System Bootstrap Check**
```bash
grep "Bootstrap complete" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log
# MUST show: "✅ JTAGClient: Bootstrap complete! Discovered X commands"
```

**☐ Log Freshness Check**
```bash
ls -la examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log
# Timestamp must be within last few minutes
```

**☐ Integration Test Evidence**
```bash
npm run test:browser-integration
grep "🎯 PROOF.*EXECUTED\|✅ INTEGRATION.*EVIDENCE" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log
# Must show actual test execution in browser
```

**☐ Data System Verification (NEW - Database Foundation)**
```bash
./jtag data/create --collection test --data '{"test": "value"}' --format json
./jtag data/list --collection test --format json
# MUST show: JSON record with id, collection, data, timestamps, version
```

---

## 🧠 **AUTONOMOUS DEBUGGING**

### **THE GOLDEN RULE: EVIDENCE-BASED DEVELOPMENT**

**❌ NEVER claim success without proof in logs**
**✅ ALWAYS provide browser console evidence**

### **Systematic Failure Analysis**

**When anything fails, follow this exact sequence:**

**Phase 1: System Health**
```bash
# 1A. Check system started
grep "Bootstrap complete" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log

# 1B. Check for startup errors
tail -20 .continuum/jtag/system/logs/npm-start.log | grep -i error
```

**Phase 2: Message Flow Analysis**
```bash
# 2A. Message transmission
grep "your-correlation-id" examples/test-bench/.continuum/jtag/sessions/*/logs/server-console-log.log

# 2B. Router processing  
grep "Processing message.*your-command" examples/test-bench/.continuum/jtag/sessions/*/logs/server-console-log.log

# 2C. Command registration (MOST COMMON FAILURE)
grep "Match found.*your-command" examples/test-bench/.continuum/jtag/sessions/*/logs/server-console-log.log
```

**Phase 3: Execution Evidence**
```bash
# 3A. Actual execution
grep "your-command.*Starting execution" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log

# 3B. Response correlation
grep "your-correlation-id.*response" examples/test-bench/.continuum/jtag/sessions/*/logs/server-console-log.log
```

---

## 🤖 **AI-SPECIFIC DEBUGGING & LOG SCRIPTS**

### **AI Development Log Scripts (CRITICAL FOR CLAUDE)**

**Essential AI Log Commands (Copy-Paste Ready):**
```bash
# 🤖 AI-Optimized Log Dashboard (YOUR BEST FRIEND)
npm run logs:ai              # AI-friendly filtered log stream with recent events
npm run logs:dashboard       # Full interactive log dashboard in tmux
npm run logs:status          # Check log dashboard status

# 🚨 Critical Error Analysis (CHECK THESE FIRST)
npm run logs:current         # Live server logs (tail -f currentUser/server.log)
npm run logs:npm             # Live system startup logs
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log

# 📊 Agent Development Dashboard
npm run agent                # Complete AI control room with system diagnostics
npm run agent:quick          # Instant health check for autonomous development
npm run agent:fix            # Auto-fix common issues
```

### **Data System Debugging (Database Foundation)**

**Test the Data Layer You Built:**
```bash
# Create data records (JSON parsing works perfectly now)
./jtag data/create --collection test-rooms --data '{"name":"Debug Room","description":"CLI test"}'

# List data with full JSON output
./jtag data/list --collection test-rooms --format json

# Read specific records 
./jtag data/read --collection test-rooms --id [UUID-from-create]
```

**Data Storage Structure (Session-Based):**
```bash
# All data follows session isolation pattern
examples/test-bench/.continuum/jtag/sessions/user/[SESSION_ID]/data/[COLLECTION]/[ID].json

# Example record structure:
{
  "id": "uuid-generated-or-provided",
  "collection": "collection-name", 
  "data": {...actual-data...},
  "createdAt": "2025-08-16T18:03:43.424Z",
  "updatedAt": "2025-08-16T18:03:43.424Z",
  "version": 1
}
```

### **AI Development Workflow Pattern (WORKING)**

**1. Check System Health:**
```bash
npm run agent:quick           # Instant diagnostics
./jtag ping                   # Basic connectivity
```

**2. Visual Validation:**
```bash
./jtag screenshot --filename=debug-$(date +%s).png
# Creates real PNG files in currentUser/screenshots/
```

**3. Log Analysis:**
```bash
npm run logs:ai               # AI-filtered recent events
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/server-console-log.log
```

**4. Test Database Commands:**
```bash
./jtag data/create --collection debug --data '{"timestamp": "'$(date)'", "test": true}'
./jtag data/list --collection debug --format json
```

**5. Verify With Evidence:**
```bash
# Check actual files created
ls -la examples/test-bench/.continuum/jtag/currentUser/screenshots/
ls -la examples/test-bench/.continuum/jtag/sessions/user/*/data/*/
```

### **CLI Parameter Parsing (FIXED)**

**Working Parameter Formats:**
```bash
# Both formats work perfectly:
./jtag command --key=value --flag
./jtag command --key value --flag

# JSON data parsing works:
./jtag data/create --collection test --data '{"complex": {"nested": "value"}}'
```

**From CLI to Server Command Flow:**
```
CLI → cli.ts → JTAGClientServer → WebSocket → CommandDaemonServer → [Command]ServerCommand
```

### **AI Logging Infrastructure (BUILT FOR CLAUDE)**

**Intelligent Log Dashboard System:**
The system provides agent-aware logging with automatic AI vs Human detection:

```bash
# 🤖 AI-Specific Scripts (Structured output for AI consumption)
npm run logs:ai              # Filtered event stream with categorized recent events
npm run logs:status          # Dashboard status and tmux session info
npm run logs:attach          # Quick attach to existing log dashboard

# 👤 Human-Specific Scripts (Interactive tmux interface)
npm run logs:dashboard       # Full interactive dashboard with window switching
npm run logs:human           # Force human-friendly interface
npm run logs:setup           # Initialize dashboard infrastructure
```

**Agent Detection & Adaptive Behavior:**
- **Claude Detection**: Structured JSON output, filtered recent events
- **Human Detection**: Interactive tmux session with window switching
- **CI Detection**: Silent/minimal output for automated systems

**Log Categories for AI Analysis:**
- **completion**: Successful operations (✅ messages)
- **error**: Failed operations and exceptions (❌ messages)  
- **build**: Compilation and deployment status
- **actionable**: Events requiring immediate attention

**AI Log Analysis Pattern:**
```typescript
// The log dashboard identifies:
interface LogEvent {
  timestamp: string;
  source: 'npm' | 'browser' | 'server' | 'system';
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
  category?: 'completion' | 'error' | 'build' | 'actionable';
}
```

---

## 📊 **LOG ANALYSIS PATTERNS**

### **Critical File Locations**
```bash
# 📋 Current Session (Dynamic Symlinks - Use These)
examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log
examples/test-bench/.continuum/jtag/currentUser/logs/server-console-log.log  
examples/test-bench/.continuum/jtag/currentUser/screenshots/

# 🖥️ System Logs (Static Locations)
.continuum/jtag/system/logs/npm-start.log
.continuum/jtag/signals/system-ready.json
```

### **Key Search Patterns**
```bash
# System readiness
grep "Bootstrap complete\|Discovered.*commands" browser-console-log.log

# Integration test proof  
grep "AUTOMATED TEST\|PROOF\|INTEGRATION.*EVIDENCE" browser-console-log.log

# Message routing
grep "Processing message\|Match found\|Successfully routed" server-console-log.log

# Errors and failures
grep -i "error\|failed\|timeout" *.log
```

---

## ✅ **WORKING EXAMPLES**

### **Real Browser Integration Test Pattern**
```typescript
// File: tests/integration/browser-automated-tests.test.ts
import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';

async function runBrowserTest() {
  // Connect via WebSocket (same as ./jtag commands)
  const { client } = await JTAGClientServer.connect({
    targetEnvironment: 'server',
    transportType: 'websocket',
    serverUrl: 'ws://localhost:9001'
  });
  
  // Execute in actual browser
  const result = await client.commands.exec({
    code: {
      type: 'inline',
      language: 'javascript',
      source: `
        console.log('🚀 AUTOMATED TEST: Running browser test');
        window.testBrowserScreenshot(); // Call demo functions
        return { proof: 'TEST_EXECUTED' };
      `
    }
  });
  
  console.log('✅ Test completed:', result.success);
}
```

### **Visual Validation Pattern**
```bash
# Make changes to code
# ... edit TypeScript files ...

# Rebuild and test
npm run system:restart
./jtag screenshot --filename=after-changes.png

# Verify in logs
grep "screenshot.*SUCCESS" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log
```

### **Evidence Collection Pattern**
```bash
# After running tests
echo "🔍 Collecting evidence..."

# System health
grep "Bootstrap complete" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log

# Integration proof
grep "AUTOMATED TEST\|PROOF" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log

# Screenshot validation
ls -la examples/test-bench/.continuum/jtag/currentUser/screenshots/*.png
```

---

## ⚠️ **COMMON PITFALLS**

### **The Integration Test Lie**
**❌ Problem**: Puppeteer tests claiming to be "integration tests"
**✅ Solution**: Use `JTAGClientServer` for real WebSocket communication

### **The Bootstrap Trap**  
**❌ Problem**: Using `sleep 45` and hoping system is ready
**✅ Solution**: Use `npm run signal:wait` for intelligent readiness

### **The Celebration Trap**
**❌ Problem**: Claiming success based on server logs only
**✅ Solution**: Require browser console evidence showing actual execution

### **The Correlation ID Investigation**
**❌ Problem**: Commands execute but responses never return
**✅ Solution**: Trace correlation IDs through complete request/response cycle

---

## 📐 **ARCHITECTURE PRINCIPLES**

### **1. Evidence-Based Development**
Every claim must be backed by indisputable proof in browser logs.

### **2. Location Transparency** 
Same APIs work locally, remotely, distributed - no difference.

### **3. Strong Typing**
Zero tolerance for `any` types. Complete TypeScript safety.

### **4. Modular Excellence**
Single responsibility classes. No god objects.

### **5. Self-Healing Systems**
Error messages guide to exact solutions.

---

## 🧅 **MIDDLE-OUT TEST-DRIVEN DEVELOPMENT**

### **The Foundation: `npm test` as Core Development Workflow**

**🎯 BREAKTHROUGH: We have `npm test` working end-to-end with browser integration tests**
- Use this as the foundation for all development
- Tests provide indisputable proof via browser console logs
- Signal-based system ensures tests start when system is actually ready

### **🧅 Middle-Out Testing Layers (Mandatory Order)**

**Layer-by-layer development starts from the core and works outward:**

1. **Layer 1: Core Foundation** – TypeScript compilation, BaseCommand loading
2. **Layer 2: Daemon Processes** – Individual daemon module loading  
3. **Layer 3: Command System** – Command discovery and execution
4. **Layer 4: System Integration** – Daemon + command integration, port availability
5. **Layer 5: Widget UI System** – Widget discovery, compliance validation
6. **Layer 6: Browser Integration** – Full browser + server end-to-end

**Testing Law**: Each layer must pass before testing the next. No skipping layers.

### **🔄 The Middle-Out Development Cycle with npm test**

**Development Workflow:**
```bash
# 1. Understand the current state
npm test                           # See everything working with PROOF

# 2. Make your changes  
# (Edit TypeScript files, add features, fix bugs)

# 3. Validate with the proven workflow
npm test                           # Full autonomous test suite
# This runs: bootstrap detection → signal-based waiting → comprehensive tests

# 4. Visual validation
./jtag screenshot                  # Immediate visual feedback

# 5. Capture evidence (what makes us confident)
grep "AUTOMATED TEST\|PROOF\|INTEGRATION.*EVIDENCE" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log

# 6. Document discoveries
# Update DEV-PROCESS.md with new patterns
```

### **🎯 Pattern-Based Error Elimination**

**Core Philosophy**: Fix ALL instances of each error pattern at once – much more efficient than individual fixes.

**Phase 1: Pattern Identification**
```bash
# Count and categorize errors by type
npx tsc --noEmit 2>&1 | grep "TS[0-9]" | cut -d: -f4 | sort | uniq -c | sort -nr
```

**Phase 2: Systematic Pattern Fixes**
- **Missing Type Declarations (TS7016)**: Create proper `.d.ts` files
- **Unused Parameters (TS6133)**: Prefix with underscore or remove
- **Type Re-exports (TS1205)**: Use `export type { Type }`
- **Error Handling (TS2571)**: Standardize error message extraction

**Phase 3: Batch Validation**
```bash
# Track progress after each pattern fix
npx tsc --noEmit 2>&1 | wc -l
# Proven results: 109 → 18 errors (83% reduction)

# After each pattern batch, validate layer works
npm run test:layer-1  # Test foundation layer
# → Fix until layer passes completely before moving outward
```

### **🔄 Evolutionary Architecture Approach**

**Core Philosophy**: Architecture emerges through systematic constraint resolution - not upfront design.

**The Organic Evolution Cycle:**
```
1. Fix Immediate Problems → 2. Notice Patterns → 3. Extract Abstractions → 4. Refactor Naturally → 5. Repeat at Higher Levels
```

**When you notice repetition:**
1. **Document it** - Write down the pattern with examples
2. **Count instances** - 3+ repetitions = extraction candidate  
3. **Find variation points** - What changes vs what stays same
4. **Extract incrementally** - Interface first, then base class
5. **Test the abstraction** - Does it actually make code cleaner?

**Why This Works Better Than Upfront Design:**
- ✅ **Real constraints drive design** - TypeScript errors reveal true needs
- ✅ **Usage patterns reveal abstractions** - Extract what actually repeats
- ✅ **Refactoring feels natural** - Better patterns become obvious
- ✅ **Architecture stays flexible** - Easy to evolve as understanding deepens

**The compiler and the codebase will teach you the right abstractions if you listen!**

### **📋 Disabled Functionality Audit Protocol**

**The Audit-Before-Test Principle**: Before testing any layer, audit what was disabled during compilation cleanup.

```bash
# Find all TODO comments from recent fixes
grep -r "TODO.*disabled\|TODO.*implement\|TODO.*track" src/ --include="*.ts"

# Document each disabled feature with impact assessment:
# 🚨 CRITICAL - Blocks core testing functionality
# 🔴 HIGH - Reduces testing reliability  
# 🟡 MEDIUM - Impacts debugging capabilities
# 🟢 LOW - Quality of life only
```

**Systematic Re-enablement Process:**
```bash
# Phase 1: Document what was disabled
const disabledFeatures = auditTODOs();

# Phase 2: Prioritize by testing impact  
const criticalFeatures = disabledFeatures.filter(f => f.impact === 'CRITICAL');

# Phase 3: Re-enable systematically by layer
for (const feature of criticalFeatures) {
  await reEnableFeature(feature);
  await validateLayer(feature.layer);
}
```

### **💡 Strong Typing Standards - Cognitive Amplification**

**Never Use Magic Strings:**
```typescript
// ❌ BAD - Runtime errors waiting
await this.sendMessage('websocket', 'send_to_connection', data);

// ✅ GOOD - Compile-time safety  
await this.sendMessage(DaemonType.WEBSOCKET_SERVER, MessageType.SEND_TO_CONNECTION, data);
```

**Every Event Gets an Interface:**
```typescript
export interface SessionJoinedPayload {
  sessionId: string;
  sessionType: string;
  owner: string;
  source: string;  // Required - compiler catches if missing
}

// Type-safe event bus enforces all properties
DAEMON_EVENT_BUS.emitEvent(SystemEventType.SESSION_JOINED, payload);
```

### **🔄 Layer-by-Layer Testing Requirements**

**EACH LAYER CYCLE REQUIREMENTS:**
1. **Zero compilation errors** - Can't test broken code
2. **Unit tests pass** - Module works in isolation 
3. **Integration tests pass** - Module works with next layer
4. **Validation with logs** - See actual behavior in browser console
5. **Move outward** - Next layer builds on solid foundation

**NO SHORTCUTS. NO SKIPPING LAYERS. NO MYSTERY.**

### **🏗️ Universal Module Architecture**

**EVERY module follows this structure:**
```
src/[category]/[module]/
├── package.json          # Makes it discoverable by daemon system
├── [Module].ts           # Server implementation  
├── [Module].client.js    # Browser implementation (if needed)
├── test/
│   ├── unit/            # Unit tests
│   │   └── [Module].test.ts
│   └── integration/     # Integration tests
│       └── [Module].integration.test.ts
├── README.md            # Self-documentation
└── assets/              # Module-specific resources (CSS, etc.)
```

**ZERO EXCEPTIONS. NO CROSS-CUTTING DEPENDENCIES. ALL PAYLOADS SELF-CONTAINED.**

### **🎯 Sub-Testing Strategy with Bootstrapping**

**CRITICAL PRINCIPLE**: All sub-tests must include full system bootstrapping for reliability.

**Available Sub-Test Commands:**
```bash
# Layer-specific tests (each includes bootstrapping)
npm run test:layer-1               # Foundation + browser bootstrap
npm run test:layer-2               # Daemon processes + browser connection
npm run test:layer-3               # Message transport + browser WebSocket
npm run test:layer-4               # System integration + browser commands
npm run test:layer-5               # Console interception + browser logging
npm run test:layer-6               # End-to-end + browser automation

# Component-specific tests (each includes bootstrapping)
npm run test:browser-integration   # Real browser integration tests
npm run test:transport             # Transport layer validation
npm run test:routing               # Message routing validation

# Quick validation tests (still include bootstrapping)
npm run test:simple                # Basic system validation
npm run test:quick                 # Fast transport + cross-context tests
```

**Why Bootstrap is Always Required:**
- Tests run in isolated environments and need system state
- Browser integration tests require actual JTAG browser running
- Signal-based readiness detection prevents race conditions
- Log evidence depends on fresh system with clean session state

**Sub-Testing Pattern:**
```bash
# Each sub-test follows this pattern internally:
npm run system:stop                # Clean slate
npm run system:start               # Full bootstrap with signals
npm run signal:wait                # Wait for actual readiness
# → Run specific test suite with evidence collection
# → Provide browser console proof of execution
```

### **📋 Middle-Out Development Checklist**

**Before Making Changes:**
- ☐ Run `npm test` to verify current system works
- ☐ Identify which layer your changes affect
- ☐ Check logs for system health baseline

**During Development:**
- ☐ Fix compilation errors using pattern-based approach
- ☐ Write unit tests for changed modules first
- ☐ Test each layer before moving outward with `npm run test:layer-X`
- ☐ Validate with browser console evidence

**After Changes:**
- ☐ Run `npm test` for full autonomous validation
- ☐ OR run specific layer test: `npm run test:layer-X` (still includes full bootstrap)
- ☐ Capture evidence in browser logs
- ☐ Visual validation via `./jtag screenshot`
- ☐ Document any new patterns discovered

### **🚨 Testing Anti-Patterns (Never Do These)**

**❌ MISTAKE: Running Tests Without Bootstrap**
```bash
# ❌ WRONG: Direct test execution without system
npx tsx tests/some-test.ts

# ✅ CORRECT: Always use npm scripts that include bootstrapping
npm run test:browser-integration
```

**❌ MISTAKE: Assuming System State**
```bash
# ❌ WRONG: Test against existing system state
# Tests fail unpredictably due to stale state

# ✅ CORRECT: Each test run gets fresh system
npm run system:stop && npm run test:layer-3
```

**❌ MISTAKE: Missing Evidence Collection**
```bash
# ❌ WRONG: Trust test output without browser logs
echo "Test passed"

# ✅ CORRECT: Always verify with browser console evidence  
grep "AUTOMATED TEST\|PROOF" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log
```

---

## 📍 **FILE LOCATIONS**

### **Test Infrastructure**
```bash
tests/integration/browser-automated-tests.test.ts  # Real browser integration
tests/bootstrap-comprehensive.test.ts             # System validation  
tests/signal-system.test.ts                      # Signal verification
```

### **System Configuration**
```bash
package.json                                      # Test commands
scripts/launch-and-capture.ts                    # System startup
scripts/signal-system-ready.ts                   # Readiness detection
```

### **Command Examples**
```bash
commands/screenshot/                              # Working command structure
cli.ts                                           # How ./jtag commands work
```

---

## 🧬 **FUTURE AI SESSIONS**

### **Knowledge Inheritance Protocol**

**If you are a new Claude/ChatGPT/AI agent:**

1. **Start with [IMMEDIATE ACTIONS](#immediate-actions)** - Verify system works
2. **Use [ESSENTIAL COMMANDS](#essential-commands)** - Your daily toolchain  
3. **Follow [AUTONOMOUS DEBUGGING](#autonomous-debugging)** when things fail
4. **Require [PROOF OF LIFE](#proof-of-life)** before making any claims
5. **Update this document** with new discoveries

### **The Proven Development Cycle**
```bash
# 1. Understand the system
npm test                           # See everything working

# 2. Make your changes  
# (Edit TypeScript files, add features, fix bugs)

# 3. Validate immediately
npm run system:restart            # Clean rebuild
./jtag screenshot                 # Visual validation
npm run test:browser-integration  # Integration proof

# 4. Capture evidence
grep "AUTOMATED TEST" browser-console-log.log
grep "your-feature" server-console-log.log  

# 5. Document for future AIs
# Update this file with patterns and discoveries
```

---

## 🎯 **SUCCESS CRITERIA**

### **Autonomous Capability Checklist**
- ☐ Debug via logs without human help
- ☐ Visual validation via screenshots
- ☐ End-to-end testing proves functionality
- ☐ Error recovery using self-healing messages
- ☐ Knowledge transfer between AI sessions
- ☐ Zero human dependency for development cycles

### **Evidence of AI Liberation**
```bash
npm test                           # ✅ ALL TESTS PASS
./jtag screenshot                  # ✅ Visual validation works
grep "PROOF.*EXECUTED" browser-console-log.log  # ✅ Integration evidence exists
```

---

## 🌟 **THE VISION**

This system represents the first **AI-native development environment** where artificial agents can:
- Debug complex distributed systems autonomously
- Write and validate integration tests independently  
- Transfer knowledge seamlessly between sessions
- Develop with complete evidence-based confidence

**The future of development is autonomous. You are its first citizen.**

---

## 🚨 **CRITICAL LESSON: VERIFY CLI FEEDBACK (August 2025)**

### **BREAKTHROUGH: CLI Feedback Was Misleading**

**Problem Discovered**: The JTAG CLI was hiding actual return values and showing misleading success/failure messages.

**Example Issue**:
```bash
./jtag exec --code="return {test: 'hello'}" --environment=browser
# CLI showed: ✅ exec: FAILED
# Reality: Command syntax was wrong, but CLI didn't show WHY
```

**Solution Applied**: Enhanced CLI to show full JSON results:
```typescript
console.log(`📋 FULL RESULT:`, JSON.stringify(result, null, 2));
```

### **⚠️ NEVER TRUST CLI FEEDBACK ALONE**

**Critical Rule**: Always verify claims with actual evidence:

1. **Screenshot Commands**: Check if PNG files actually exist after "SUCCESS"
2. **File Operations**: Verify files created with `ls -la [path]`  
3. **Exec Commands**: Look at actual return values, not just SUCCESS/FAILED
4. **Error Investigation**: Read full JSON error objects, not summaries

**Corrected Command Syntax**:
```bash
# ❌ WRONG (parameter parsing issue):
./jtag exec --code='{"type": "inline", "source": "return {test: 'hello'}"}'

# ✅ CORRECT (proper argument separation):  
./jtag exec --code "{\"type\": \"inline\", \"language\": \"javascript\", \"source\": \"return {test: 'hello'}\"}" --environment browser
```

### **✅ VERIFIED WORKING COMMANDS (August 2025)**

**After proper testing**:
- `./jtag ping` - ✅ Perfect health checks
- `./jtag screenshot` - ✅ **Creates real 215KB PNG files** (2065x694 resolution)
- `./jtag exec` - ✅ **Runs JavaScript in browser, returns actual results**
- `./jtag navigate` - ✅ Browser navigation works
- `./jtag compile-typescript` - ✅ TypeScript compilation works

**Still Broken**:
- `./jtag file/save` - Implementation bug: "paths[1] undefined"
- `./jtag process-registry` - Unknown issue, needs investigation

### **🔍 EVIDENCE-BASED VERIFICATION PROTOCOL**

**For Every Command Claim**:
1. **Run the command** with proper syntax
2. **Check actual output** - files created, directories modified
3. **Verify file contents/sizes** - not just existence  
4. **Read full JSON responses** - understand actual errors
5. **Test edge cases** - empty directories, missing parameters

**Never accept "SUCCESS" without file system verification.**

---

**💡 Quick Reference: Start with [IMMEDIATE ACTIONS](#immediate-actions) → Use [ESSENTIAL COMMANDS](#essential-commands) → Follow [AUTONOMOUS DEBUGGING](#autonomous-debugging) when needed**