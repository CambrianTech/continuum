# JTAGClient.sharedInstance - Pragmatic Integration Test Plan

**Date**: 2025-10-19
**Philosophy**: Test the ACTUAL use case, not implementation details
**Goal**: Prove PersonaUser can emit events and AIs respond

---

## The Real-World Use Case

**What We're Actually Testing**: PersonaUser emits AI decision events when processing chat messages.

**Why This Matters**: If PersonaUser works, it proves:
1. ✅ `sharedInstance` resolves on server (where PersonaUser runs)
2. ✅ `client.daemons.events.emit()` works (uses auto-context form)
3. ✅ Events are routed correctly through the system
4. ✅ All four connection cases work (implicitly tested)

**If This Fails**: sharedInstance is still broken, rollback immediately

---

## Pre-Implementation Verification

**Establish Known Broken State** - Document that PersonaUser currently fails:

```bash
# 1. Get room ID
./jtag data/list --collection=rooms --limit=1

# 2. Send test message
./jtag debug/chat-send --roomId="<ROOM-ID>" --message="Before fix: Do AIs respond?"

# 3. Wait 30 seconds

# 4. Check AI report (expect ZERO responses or very few)
./jtag ai/report
```

**Expected BEFORE Fix**:
```json
{
  "totalDecisions": 0,
  "totalResponses": 0,
  "errors": 5,
  "errorTypes": ["TypeError: this.client.events.room is not a function"]
}
```

**Document**: Take screenshot, save logs, record AI report output as baseline

---

## Post-Implementation Verification

**THE ONE TEST THAT MATTERS** - Prove PersonaUser works end-to-end:

### Test: AI Responds to Chat Message

**Setup**:
```bash
# 1. Deploy fixed system
npm start

# 2. Wait for full startup (90+ seconds)
# Look for: "✅ JTAGClient: Bootstrap complete - 64+ commands"

# 3. Verify system ready
./jtag ping
# Expect: systemReady: true, daemonCount: 12, commandCount: 64+
```

**Execute Test**:
```bash
# 1. Get room ID
ROOM_ID=$(./jtag data/list --collection=rooms --limit=1 | jq -r '.items[0].id')
echo "Testing with room: $ROOM_ID"

# 2. Send test message
./jtag debug/chat-send \
  --roomId="$ROOM_ID" \
  --message="After fix: Can you explain the sharedInstance fix we just implemented? Be specific about registry vs globalThis."

# 3. Wait 15 seconds (give AIs time to process)
echo "Waiting 15 seconds for AI responses..."
sleep 15

# 4. Check logs for PersonaUser activity
./jtag debug/logs --filterPattern="Worker evaluated|AI-RESPONSE|POSTED" --tailLines=50

# 5. Get AI report
./jtag ai/report

# 6. Visual verification
./jtag screenshot --querySelector="chat-widget" --filename="test-fix-verification.png"
```

**Success Criteria** (ALL must pass):

1. **Logs Show Worker Thread Activity**:
```
[timestamp] 🤖 PersonaUser[Helper AI]: Worker evaluated message in 250ms
[timestamp] 📤 PersonaUser[Helper AI]: Decision: should_respond (confidence: 0.85)
[timestamp] ✅ PersonaUser[Helper AI]: Response posted to room
```

2. **AI Report Shows Responses**:
```json
{
  "totalDecisions": 5+,
  "totalResponses": 2+,
  "avgConfidence": 0.7+,
  "avgLatency": 500,
  "errors": 0
}
```

3. **Screenshot Shows AI Responses**:
- At least 2 AI responses visible in chat
- Responses are relevant to the question
- No error messages in chat

4. **No Error Logs**:
```bash
./jtag debug/logs --includeErrorsOnly=true --tailLines=50
# Should NOT show:
# - "TypeError: this.client.events.room is not a function"
# - "JTAGClient.sharedInstance: timeout"
# - "Worker Thread" errors
```

**Failure Criteria** (ANY means rollback):
- ❌ No PersonaUser activity in logs after 30 seconds
- ❌ AI report shows 0 responses
- ❌ Errors in logs about sharedInstance timeout
- ❌ Errors in logs about client.events.room
- ❌ Screenshot shows no AI responses

---

## Secondary Tests (Optional Verification)

Only run these if the primary test passes:

### Test 2: Verify sharedInstance Performance

**Purpose**: Prove sharedInstance resolves quickly (not polling for 5 seconds)

```bash
# Run this in a server-side test
npx tsx test-sharedinstance-performance.ts
```

**test-sharedinstance-performance.ts**:
```typescript
import { jtag } from './server-index';

(async () => {
  console.log('🧪 Testing sharedInstance performance...');

  // Connect client (registers in static registry)
  const client = await jtag.connect();
  console.log(`✅ Client connected: ${client.sessionId.substring(0, 8)}...`);

  // Test sharedInstance resolution time
  const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');

  const start = Date.now();
  const sharedClient = await JTAGClient.sharedInstance;
  const duration = Date.now() - start;

  console.log(`⏱️ sharedInstance resolved in ${duration}ms`);

  if (duration > 100) {
    console.error(`❌ FAIL: Too slow (expected <100ms, got ${duration}ms)`);
    process.exit(1);
  }

  console.log('✅ PASS: sharedInstance performance acceptable');
  process.exit(0);
})();
```

**Expected Output**:
```
🧪 Testing sharedInstance performance...
📝 JTAGClient: Registered client 'default' (environment: server, sessionId: ...)
✅ Client connected: 12345678...
✅ JTAGClient.sharedInstance: Found client in registry (server)
⏱️ sharedInstance resolved in 3ms
✅ PASS: sharedInstance performance acceptable
```

**Success**: <100ms resolution time
**Failure**: >100ms or timeout error

---

### Test 3: Verify Browser Still Works

**Purpose**: Ensure browser functionality not broken by registry changes

```bash
# 1. Open browser (should happen automatically during npm start)
# URL: http://localhost:9003

# 2. Open browser console

# 3. Run this test
(async () => {
  const { JTAGClient } = await import('/system/core/client/shared/JTAGClient.js');
  const start = Date.now();
  const client = await JTAGClient.sharedInstance;
  const duration = Date.now() - start;
  console.log(`✅ Browser sharedInstance resolved in ${duration}ms`);
  console.log(`Environment: ${client.context.environment}`);
})();
```

**Expected Output**:
```
✅ JTAGClient.sharedInstance: Found client in globalThis.jtag (browser)
✅ Browser sharedInstance resolved in 2ms
Environment: browser
```

**Success**: <10ms resolution time, environment=browser
**Failure**: Timeout or error

---

## Rollback Decision Tree

```
PRIMARY TEST (AI Response):
├─ PASS → Fix is good, proceed to commit
└─ FAIL → Check error type:
    ├─ "sharedInstance timeout" → Registry not working, rollback
    ├─ "client.events.room is not a function" → Still has old bug, rollback
    ├─ Worker Thread errors → Different issue, investigate separately
    └─ No AI activity at all → Deployment issue, check npm start logs

SECONDARY TEST (Performance):
├─ PASS → Good
└─ FAIL → Not critical, but investigate why slow

TERTIARY TEST (Browser):
├─ PASS → Good
└─ FAIL → CRITICAL, rollback immediately (broke browser)
```

---

## Commit Criteria

**ONLY commit if**:
1. ✅ Primary test passes (AI responds to chat)
2. ✅ AI report shows 0 errors
3. ✅ No sharedInstance timeout errors in logs
4. ✅ Performance test shows <100ms resolution
5. ✅ Browser test shows <10ms resolution

**Commit Message** (if all tests pass):
```
Fix JTAGClient.sharedInstance deadlock via static client registry

Root Cause: globalThis.jtag only set in browser, never in server.
This caused infinite polling in sharedInstance getter, breaking all
server-side Events.emit() auto-context calls (PersonaUser, daemons).

Solution: Add static clientRegistry Map to JTAGClient class.
- sharedInstance checks registry first (server), then globalThis (browser)
- Both browser-index.ts and server-index.ts register clients
- Added 5s timeout to prevent infinite polling
- Cleanup: unregister on disconnect to prevent memory leaks

Impact: PersonaUser can now emit AI decision events properly.
SOTA AI models (Claude, GPT, Grok, DeepSeek) no longer crash.

Tested:
✅ PersonaUser responds to messages (proves server sharedInstance works)
✅ sharedInstance resolves in <100ms on server (was infinite before)
✅ sharedInstance resolves in <10ms in browser (unchanged behavior)
✅ AI report shows responses from multiple personas
✅ Zero "client.events.room is not a function" errors

Files Changed:
- system/core/client/shared/JTAGClient.ts (registry + sharedInstance)
- browser-index.ts (register client)
- server-index.ts (register client)

Fixes: #<issue-number> PersonaUser crashes with TypeError
```

---

## Timeline

**Total: 1-2 hours**

1. **Pre-Implementation** (10 min)
   - Document broken state (AI report, logs, screenshot)
   - Create git stash: `git stash push -u -m "Before sharedInstance fix"`

2. **Implementation** (30 min)
   - Make 6 code changes from fix plan
   - Build: `npx tsc --noEmit && npm run build:browser-ts`
   - Deploy: `npm start`

3. **Primary Test** (15 min)
   - Send message
   - Wait 15 seconds
   - Check logs, AI report, screenshot
   - **STOP HERE if test fails - rollback immediately**

4. **Secondary Tests** (15 min - optional)
   - Performance test (server)
   - Browser test

5. **Commit** (5 min)
   - Write commit message
   - Stage changes
   - Commit

---

## Why This Approach Is Better

**OLD PLAN**:
- 6 separate test cases
- Test all 4 connection cases explicitly
- Test implementation details (LocalConnection vs RemoteConnection)
- 2-3 hours of testing
- Complex and fragile

**NEW PLAN**:
- 1 primary test (AI responds)
- Connection cases tested implicitly
- Test behavior, not implementation
- 15 minutes of testing
- Simple and robust

**Philosophy**: If PersonaUser works, everything works. If PersonaUser fails, we know sharedInstance is broken.

---

## Final Checklist Before Starting

- [ ] System currently works (`npm start` succeeds)
- [ ] Baseline test run (document broken state)
- [ ] Git stash created
- [ ] Joel approves this simplified test plan
- [ ] Coffee ready (you'll need it if something fails)

**DO NOT START until all boxes checked**
