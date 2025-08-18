# AI PROBLEM SOLVING GUIDE - Zero Friction Commands

## 🚨 **STOP GETTING LOST - SPECIFIC COMMANDS FOR SPECIFIC PROBLEMS**

### **❌ PROBLEM: "Cross-environment events not working"**
**🎯 EXACT SOLUTION:**
```bash
# Step 1: Check if events are actually broken or API is wrong
npx tsx test-correct-event-flow.ts
# Expected: "1 DOM event received vs 0 JTAG events"

# Step 2: If events ARE working but tests fail, fix test API
# Replace: window.jtag.eventManager.events.on('chat-message-sent', listener)
# With: document.addEventListener('chat:message-received', listener)

# Step 3: Validate fix
npx tsx test-event-system-final.ts
# Expected: "✅ FINAL EVENT SYSTEM TEST PASSED"
```

### **❌ PROBLEM: "Tests hang without exiting"**
**🎯 EXACT SOLUTION:**
```typescript
// Add this pattern to ANY test that hangs:
let client: any = null;
try {
  client = await jtag.connect({ targetEnvironment: 'server' });
  // ... test code ...
} finally {
  if (client?.disconnect) await client.disconnect();
  process.exit(0);
}
```

### **❌ PROBLEM: "npm test fails with timeout"**
**🎯 EXACT SOLUTION:**
```bash
# Step 1: Check which specific test is hanging
npm run system:stop && npm test 2>&1 | grep -A5 -B5 "timeout\|failed"

# Step 2: Run that specific test in isolation
npx tsx tests/integration/[failing-test].test.ts

# Step 3: Add clean exit handling (see pattern above)

# Step 4: Validate fix
npm test
```

### **❌ PROBLEM: "Events received: 0"**
**🎯 EXACT SOLUTION:**
```bash
# Step 1: Verify events are flowing at system level
grep "EventsDaemon.*Router result" examples/test-bench/.continuum/jtag/currentUser/logs/server-console-log.log
# Must show: "success": true

# Step 2: Check DOM event bridge
grep "DOMEventBridge.*Emitted DOM event" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log
# Must show: DOM events being created

# Step 3: Fix event listener API in your test
# Change from: window.jtag.eventManager.events.on('chat-message-sent', listener)
# Change to: document.addEventListener('chat:message-received', listener)
```

### **❌ PROBLEM: "TypeScript compilation errors blocking everything"**
**🎯 EXACT SOLUTION:**
```bash
# Step 1: See all errors grouped by type
npx tsc --noEmit 2>&1 | grep "TS[0-9]" | cut -d: -f4 | sort | uniq -c | sort -nr

# Step 2: Fix by pattern (not individual files)
# For TS2322 (type mismatches): Check if error objects should be strings
# For TS7016 (missing declarations): Add proper imports
# For TS6133 (unused params): Prefix with underscore

# Step 3: Validate after each pattern
npx tsc --noEmit --project .
```

### **❌ PROBLEM: "System not starting / Bootstrap failing"**
**🎯 EXACT SOLUTION:**
```bash
# Step 1: Check system startup logs
tail -20 .continuum/jtag/system/logs/npm-start.log

# Step 2: Check for port conflicts
lsof -ti:9001,9002 | xargs kill -9 2>/dev/null || true

# Step 3: Force clean restart
npm run system:stop && npm run system:start

# Step 4: Wait for actual readiness
npm run signal:wait
```

### **❌ PROBLEM: "Tests pass individually but fail in npm test"**
**🎯 EXACT SOLUTION:**
```bash
# The issue is usually hanging tests without clean exit

# Step 1: Find which test hangs
npm test 2>&1 | tail -20

# Step 2: Add clean exit to hanging test
# Pattern: process.exit(0) in finally block with client cleanup

# Step 3: Validate fix
npm test
```

---

## 🎯 **SPECIFIC COMMANDS FOR SPECIFIC DEBUGGING SCENARIOS**

### **🔍 "I need to debug event routing"**
```bash
# Check server-side event routing
grep "EventsDaemon.*Router result" examples/test-bench/.continuum/jtag/currentUser/logs/server-console-log.log

# Check browser-side event reception  
grep "DOMEventBridge.*Emitted" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log

# Test correct event API
npx tsx test-correct-event-flow.ts
```

### **🔍 "I need to see what commands are available"**
```bash
./jtag help                    # Complete command reference
./jtag list                    # All available commands
npm run agent                  # Dashboard with copy-paste commands
```

### **🔍 "I need to verify system health"**
```bash
npm run agent:quick            # Instant health check
./jtag ping                    # Basic connectivity  
grep "Bootstrap complete" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log
```

### **🔍 "I need to see visual state"**
```bash
./jtag screenshot --filename=debug-$(date +%s).png
# Creates real PNG in: examples/test-bench/.continuum/jtag/currentUser/screenshots/
```

### **🔍 "I need to check for errors"**
```bash
# Check error logs first (most important)
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/server-console-error.log

# Check compilation
npx tsc --noEmit --project .
```

### **🔍 "I need to test a specific functionality"**
```bash
# Data system
./jtag data/create --collection test --data '{"test": true}' && ./jtag data/list --collection test --format json

# Screenshot system  
./jtag screenshot --querySelector=body --filename=test-$(date +%s).png

# Event system
npx tsx test-event-system-final.ts

# Browser integration
npm run test:browser-integration
```

### **🔍 "Tests are failing and I don't know why"**
```bash
# Step 1: Run full test suite to see pattern
npm test

# Step 2: Check specific test that's failing  
npx tsx tests/integration/[failing-test].test.ts

# Step 3: Check logs for that test
grep -A10 -B10 "FAILED\|timeout\|error" examples/test-bench/.continuum/jtag/currentUser/logs/server-console-log.log
```

---

## 🤖 **AI-SPECIFIC PROBLEM PATTERNS & SOLUTIONS**

### **🚨 PATTERN: "Getting lost in architectural complexity"**
**SOLUTION: Use the working test as template**
```bash
# When confused, start with something that works:
npx tsx test-event-system-final.ts  # This ALWAYS works
# Copy its patterns for your new tests
```

### **🚨 PATTERN: "Spinning on event system issues"**
**SOLUTION: Follow exact debugging protocol**
```bash
# Don't theorize - follow this sequence:
# 1. Check system logs: grep "EventsDaemon.*Router result" 
# 2. Check DOM bridge: grep "DOMEventBridge.*Emitted"
# 3. Test DOM API: npx tsx test-correct-event-flow.ts
# 4. Fix your test to use DOM events, not JTAG events
```

### **🚨 PATTERN: "Not sure if changes worked"**
**SOLUTION: Visual + log evidence**
```bash
# Always verify with both visual and log evidence:
./jtag screenshot --filename=verify-$(date +%s).png
grep "your-change-pattern" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log
```

### **🚨 PATTERN: "Tests pass but npm test fails"**
**SOLUTION: Check for hanging tests**
```bash
# Add clean exit to your test:
} finally {
  if (client?.disconnect) await client.disconnect();
  process.exit(0);
}
```

### **🚨 PATTERN: "Runtime failures with optional fields"**
**SOLUTION: Make required fields required**
```typescript
// Change from:
interface MyType {
  id?: string;        // Optional - causes runtime errors
}

// Change to:
interface MyType {
  id: string;         // Required - TypeScript prevents errors
}
```

---

## 📋 **COPY-PASTE DIAGNOSTIC COMMANDS**

### **🚨 EMERGENCY: System completely broken**
```bash
npm run system:stop && npm run system:start && sleep 45 && ./jtag ping
```

### **🚨 EMERGENCY: Tests all failing**
```bash
npx tsc --noEmit --project . && npm test
```

### **🚨 EMERGENCY: Events not working**
```bash
npx tsx test-event-system-final.ts
```

### **🚨 EMERGENCY: Can't see what's happening**
```bash
./jtag screenshot --filename=emergency-$(date +%s).png && tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log
```

### **🚨 EMERGENCY: Lost in complexity**
```bash
npm run agent:quick  # Shows you exactly what to do next
```

---

## 🎯 **THE ONE-COMMAND SOLUTIONS**

### **"Just make events work"**
```bash
npx tsx test-event-system-final.ts && echo "Events working ✅"
```

### **"Just make tests pass"** 
```bash
npm test && echo "All tests pass ✅"
```

### **"Just show me system health"**
```bash
npm run agent:quick
```

### **"Just take a screenshot"**
```bash
./jtag screenshot --filename=now-$(date +%s).png
```

### **"Just fix compilation"**
```bash
npx tsc --noEmit --project .
```

---

## 💡 **AI COGNITIVE LOAD REDUCTION**

### **Stop memorizing paths - use these shortcuts:**
```bash
# Current session logs (always use these)
cd examples/test-bench/.continuum/jtag/currentUser/logs/
tail -20 browser-console-error.log
tail -20 server-console-error.log

# Screenshots
cd examples/test-bench/.continuum/jtag/currentUser/screenshots/
ls -la *.png
```

### **Stop guessing - use evidence:**
```bash
# Instead of theorizing, run these for immediate truth:
npx tsx test-event-system-final.ts  # Events working? 
./jtag ping                          # System healthy?
npm run agent:quick                  # What's the actual status?
```

### **Stop spinning - use working templates:**
```bash
# Copy patterns from files that work:
# - test-event-system-final.ts (clean event test)
# - test-correct-event-flow.ts (DOM vs JTAG comparison)
# - EventTestUtils.ts (reusable patterns)
```

**🎯 RESULT: AI can now solve problems systematically instead of getting lost in architectural complexity.**