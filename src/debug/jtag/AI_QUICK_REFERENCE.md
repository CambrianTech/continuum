# AI QUICK REFERENCE - Instant Problem Solutions

## 🚨 **EMERGENCY COMMANDS** (Run these immediately when lost)

```bash
npm run agent:quick                    # Instant system status + next steps
npx tsx test-event-system-final.ts    # Verify events work (1 DOM event = success)
./jtag screenshot --filename=debug-$(date +%s).png  # Visual state capture
tail -20 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log
```

## 🎯 **PROBLEM → COMMAND MAPPING**

| Problem | Exact Command | Expected Result |
|---------|---------------|-----------------|
| Events not working | `npx tsx test-event-system-final.ts` | "✅ FINAL EVENT SYSTEM TEST PASSED" |
| Tests hanging | Add `process.exit(0)` in finally block | Tests exit cleanly |
| npm test fails | `npm run system:stop && npm test` | "🎉 ALL BROWSER INTEGRATION TESTS PASSED!" |
| System broken | `npm run system:stop && npm run system:start` | System starts in 45 seconds |
| TypeScript errors | `npx tsc --noEmit --project .` | Silent success (no output) |
| Need visual proof | `./jtag screenshot --filename=proof-$(date +%s).png` | PNG file created |
| Lost/confused | `npm run agent:quick` | Clear status + copy-paste commands |

## 🔧 **DEBUGGING DECISION TREE**

### **Start Here: Is the system even running?**
```bash
npm run agent:quick
# If system down → npm run system:start && sleep 45
# If system up → continue to specific problem
```

### **Events Problem?**
```bash
# Quick test: Are DOM events working?
npx tsx test-event-system-final.ts
# If 1 DOM event received → events work, fix your test API
# If 0 events → check system logs with grep "EventsDaemon.*Router result"
```

### **Test Problem?**
```bash
# Which tests are failing?
npm test 2>&1 | grep -A5 "FAILED\|timeout"
# Fix specific failing test with clean exit pattern
```

### **Visual Problem?**
```bash
# Can you see the current state?
./jtag screenshot --filename=debug-$(date +%s).png
# Check file size: ls -la examples/test-bench/.continuum/jtag/currentUser/screenshots/
```

## 📊 **SUCCESS INDICATORS** (What to look for)

### **✅ Events Working:**
- `npx tsx test-event-system-final.ts` shows "1 DOM event received"
- `grep "DOMEventBridge.*Emitted" browser-console-log.log` shows DOM events

### **✅ System Healthy:**
- `npm run agent:quick` shows green status
- `./jtag ping` returns JSON with success: true
- `grep "Bootstrap complete" browser-console-log.log` shows system ready

### **✅ Tests Passing:**
- `npm test` shows "🎉 ALL BROWSER INTEGRATION TESTS PASSED!"
- No hanging - exits in under 5 minutes

### **✅ Visual Validation:**
- `./jtag screenshot` creates PNG files >100KB
- Screenshots show actual interface state

## 🚀 **AUTONOMOUS AI WORKFLOW** (Copy-paste ready)

### **New AI Session Startup:**
```bash
# 1. Quick health check
npm run agent:quick

# 2. Verify core functionality
./jtag ping && npx tsx test-event-system-final.ts

# 3. Visual validation
./jtag screenshot --filename=startup-$(date +%s).png

# 4. If all pass: You're ready for development
# 5. If any fail: Use specific problem commands above
```

### **Before Making Changes:**
```bash
# Capture baseline
./jtag screenshot --filename=before-$(date +%s).png
npm test  # Ensure starting from working state
```

### **After Making Changes:**
```bash
# Quick validation
npx tsc --noEmit --project .  # Compilation check
./jtag screenshot --filename=after-$(date +%s).png  # Visual check
npx tsx test-event-system-final.ts  # Event system check
```

### **Before Committing:**
```bash
npm test  # Full validation
# Must show: "🎉 ALL BROWSER INTEGRATION TESTS PASSED!"
```

## 🎯 **MODULAR PATTERNS** (Use these instead of reinventing)

### **Event Test Pattern:**
```typescript
// Use this exact pattern for any event test:
import { DOMEventListenerPatterns, ValidationChecks } from './system/events/shared/EventValidationPatterns';

let client: any = null;
try {
  client = await jtag.connect({ targetEnvironment: 'server' });
  
  // Setup DOM listener (not JTAG listener)
  await client.commands.exec({
    code: { type: 'inline', language: 'javascript',
      source: DOMEventListenerPatterns.CHAT_MESSAGE_LISTENER('testName') }
  });
  
  // Send message
  await client.commands['chat/send-message']({
    roomId: 'test', message: 'test', metadata: { test: true }
  });
  
  // Wait and validate
  await new Promise(resolve => setTimeout(resolve, 1000));
  const result = await client.commands.exec({
    code: { type: 'inline', language: 'javascript',
      source: ValidationChecks.BASIC_SUCCESS('testName') }
  });
  
  console.log('Events received:', result.commandResult?.result?.domEventsReceived);
  
} finally {
  if (client?.disconnect) await client.disconnect();
  process.exit(0);
}
```

### **Required Field Pattern:**
```typescript
// Always make required fields required (never optional)
interface MyType {
  id: string;           // Required - not id?: string
  sessionId: string;    // Required - not sessionId?: string  
  metadata: object;     // Required - not metadata?: object
}
```

### **Clean Exit Pattern:**
```typescript
// Use this pattern in ANY test that might hang:
async function myTest() {
  let client: any = null;
  try {
    client = await jtag.connect({ targetEnvironment: 'server' });
    // ... test logic ...
    console.log('✅ Test passed');
  } catch (error) {
    console.error('❌ Test failed:', error);
    throw error;
  } finally {
    if (client?.disconnect) await client.disconnect();
    process.exit(0);
  }
}
```

## 💡 **AI EFFICIENCY TIPS**

1. **Start with working examples** - Don't reinvent patterns
2. **Use modular utilities** - EventTestUtils, ValidationChecks, etc.
3. **Follow exact command sequences** - Don't modify working commands
4. **Verify with evidence** - Screenshots + logs, never assume
5. **Exit cleanly** - Always use process.exit(0) in test finally blocks

**🎯 RESULT: AI can solve problems in minutes instead of getting lost for hours.**