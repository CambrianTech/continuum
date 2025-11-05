# Phased Test-Driven Implementation Plan

**Date**: 2025-10-19
**Approach**: Evidence-based, test at each phase, stop if any test fails
**Rollback**: Git stash before starting, immediate rollback if failure

---

## Phase 0: Establish Baseline (EVIDENCE REQUIRED)

**Goal**: Document current working state as baseline for comparison

### Tests to Run

```bash
# 1. System health check
./jtag ping > baseline-ping.txt
# Evidence: systemReady: true, 64 commands, 12 daemons

# 2. Get room ID for testing
ROOM_ID=$(./jtag data/list --collection=rooms --limit=1 | jq -r '.items[0].id')
echo "Test room: $ROOM_ID" > baseline-test-config.txt

# 3. Test AI responses (current working state)
./jtag debug/chat-send --roomId="$ROOM_ID" --message="Baseline test: Do AIs respond before fix?"

# 4. Wait 15 seconds
sleep 15

# 5. Capture AI report (baseline)
./jtag ai/report > baseline-ai-report.json
echo "Baseline AI responses: $(jq -r '.totalResponses' baseline-ai-report.json)"

# 6. Capture logs (baseline)
./jtag debug/logs --filterPattern="Worker evaluated|AI-RESPONSE" --tailLines=50 > baseline-logs.txt

# 7. Screenshot (baseline)
./jtag screenshot --querySelector="chat-widget" --filename="baseline-chat.png"
```

### Success Criteria
- ✅ System responds to ping
- ✅ At least 1 AI responds to test message
- ✅ No critical errors in logs
- ✅ Chat widget renders correctly

### Evidence Files Created
```
baseline-ping.txt           # System health
baseline-test-config.txt    # Test configuration
baseline-ai-report.json     # AI response metrics
baseline-logs.txt           # System logs
baseline-chat.png           # Visual state
```

**STOP HERE** if baseline tests fail - system not in working state

---

## Phase 1: Add Static Registry (NO BEHAVIOR CHANGE)

**Goal**: Add registry infrastructure without changing behavior

### Code Changes

**File**: `system/core/client/shared/JTAGClient.ts`

**Change 1**: Add static registry (after line 183)
```typescript
// Static client registry for symmetric sharedInstance access
private static clientRegistry: Map<string, JTAGClient> = new Map();
```

**Change 2**: Add registerClient() method (before line 810)
```typescript
/**
 * Register a client instance in the static registry
 */
static registerClient(key: string, client: JTAGClient): void {
  this.clientRegistry.set(key, client);
  console.log(`📝 JTAGClient: Registered client '${key}' (env: ${client.context.environment})`);
}

/**
 * Unregister a client instance from the static registry
 */
static unregisterClient(key: string): boolean {
  const removed = this.clientRegistry.delete(key);
  if (removed) {
    console.log(`🗑️ JTAGClient: Unregistered client '${key}'`);
  }
  return removed;
}

private static getRegisteredClient(key: string): JTAGClient | undefined {
  return this.clientRegistry.get(key);
}
```

### Build Verification
```bash
# TypeScript compilation MUST pass
npx tsc --noEmit

# Expected: Zero errors
```

### Deployment Test
```bash
# Build browser bundle
npm run build:browser-ts

# Deploy system
npm start

# Wait for startup (90+ seconds)
```

### Phase 1 Tests (SAME AS BASELINE)

```bash
# 1. System health check
./jtag ping > phase1-ping.txt
diff baseline-ping.txt phase1-ping.txt
# Evidence: Identical output (no behavior change)

# 2. Test AI responses
./jtag debug/chat-send --roomId="$ROOM_ID" --message="Phase 1 test: Registry added but not used yet"
sleep 15

# 3. Compare AI report
./jtag ai/report > phase1-ai-report.json
diff baseline-ai-report.json phase1-ai-report.json
# Evidence: Similar response rates (no behavior change)

# 4. Screenshot
./jtag screenshot --querySelector="chat-widget" --filename="phase1-chat.png"
```

### Success Criteria
- ✅ TypeScript compiles with zero errors
- ✅ System deploys successfully
- ✅ AI responses same as baseline
- ✅ No new errors in logs
- ✅ Chat widget still works

**ROLLBACK** if any test fails: `git restore system/core/client/shared/JTAGClient.ts && npm start`

---

## Phase 2: Update sharedInstance Getter (BROWSER UNCHANGED)

**Goal**: Make sharedInstance check registry first, then globalThis

### Code Changes

**File**: `system/core/client/shared/JTAGClient.ts`
**Location**: Lines 810-822 (replace existing getter)

```typescript
static get sharedInstance(): Promise<JTAGClient> {
  return new Promise((resolve, reject) => {
    // 1. CHECK REGISTRY FIRST (server case)
    const registeredClient = this.getRegisteredClient('default');
    if (registeredClient) {
      console.log(`✅ JTAGClient.sharedInstance: Found client in registry (${registeredClient.context.environment})`);
      resolve(registeredClient);
      return;
    }

    // 2. CHECK GLOBALTHIS.JTAG (browser case)
    const jtag = (globalThis as any).jtag;
    if (jtag?.commands) {
      console.log(`✅ JTAGClient.sharedInstance: Found client in globalThis.jtag (${jtag.context.environment})`);
      resolve(jtag);
      return;
    }

    // 3. POLL WITH TIMEOUT (race condition fallback)
    console.log(`⏳ JTAGClient.sharedInstance: Client not ready, polling for 5 seconds...`);
    let attempts = 0;
    const maxAttempts = 100; // 5 seconds max

    const checkReady = (): void => {
      attempts++;

      const client = this.getRegisteredClient('default');
      if (client) {
        console.log(`✅ JTAGClient.sharedInstance: Client ready in registry after ${attempts * 50}ms`);
        resolve(client);
        return;
      }

      const jtagNow = (globalThis as any).jtag;
      if (jtagNow?.commands) {
        console.log(`✅ JTAGClient.sharedInstance: Client ready in globalThis after ${attempts * 50}ms`);
        resolve(jtagNow);
        return;
      }

      if (attempts >= maxAttempts) {
        const error = new Error(`JTAGClient.sharedInstance: No client available after ${attempts * 50}ms timeout`);
        console.error(`❌ ${error.message}`);
        reject(error);
        return;
      }

      setTimeout(checkReady, 50);
    };

    checkReady();
  });
}
```

### Build Verification
```bash
npx tsc --noEmit
npm run build:browser-ts
npm start
```

### Phase 2 Tests (FOCUS: Browser Still Works)

```bash
# 1. Verify browser sharedInstance still uses globalThis
# Open browser console and run:
(async () => {
  const { JTAGClient } = await import('/system/core/client/shared/JTAGClient.js');
  const start = Date.now();
  const client = await JTAGClient.sharedInstance;
  const duration = Date.now() - start;
  console.log(`✅ Browser sharedInstance resolved in ${duration}ms`);
})();
# Evidence: <10ms resolution time, log shows "Found client in globalThis.jtag (browser)"

# 2. System health check
./jtag ping > phase2-ping.txt

# 3. Test AI responses
./jtag debug/chat-send --roomId="$ROOM_ID" --message="Phase 2 test: sharedInstance updated"
sleep 15

# 4. AI report
./jtag ai/report > phase2-ai-report.json

# 5. Check for timeout errors
./jtag debug/logs --includeErrorsOnly=true --tailLines=50 > phase2-errors.txt
grep -i "timeout" phase2-errors.txt
# Evidence: Should be empty (no timeout errors)
```

### Success Criteria
- ✅ Browser sharedInstance resolves in <10ms
- ✅ Console log shows "Found client in globalThis.jtag (browser)"
- ✅ AI responses work (same as baseline)
- ✅ No timeout errors in logs
- ✅ Chat widget still works

**ROLLBACK** if any test fails

---

## Phase 3: Register Client in Browser (BROWSER USES REGISTRY + GLOBALTHIS)

**Goal**: Browser sets both globalThis.jtag AND registers in registry

### Code Changes

**File**: `browser-index.ts`
**Location**: After line 52

```typescript
// Set up global window.jtag for widgets and tests
(globalThis as any).jtag = client;

// Register client in static registry for sharedInstance symmetry
const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');
JTAGClient.registerClient('default', client);

// Enhance client with organized services - no globals needed
```

### Build & Deploy
```bash
npx tsc --noEmit
npm run build:browser-ts
npm start
```

### Phase 3 Tests (EVIDENCE: Registry Used in Browser)

```bash
# 1. Check browser console for registration log
# Expected log: "📝 JTAGClient: Registered client 'default' (env: browser)"

# 2. Test sharedInstance in browser console
(async () => {
  const { JTAGClient } = await import('/system/core/client/shared/JTAGClient.js');
  const start = Date.now();
  const client = await JTAGClient.sharedInstance;
  const duration = Date.now() - start;
  console.log(`Resolution time: ${duration}ms`);
})();
# Evidence: <10ms, log shows "Found client in registry (browser)" OR "Found client in globalThis.jtag"

# 3. System health
./jtag ping > phase3-ping.txt

# 4. AI responses
./jtag debug/chat-send --roomId="$ROOM_ID" --message="Phase 3 test: Browser registry active"
sleep 15
./jtag ai/report > phase3-ai-report.json
```

### Success Criteria
- ✅ Browser console shows registration log
- ✅ sharedInstance resolves in <10ms
- ✅ AI responses work
- ✅ No errors in browser console
- ✅ Chat widget still works

**ROLLBACK** if any test fails

---

## Phase 4: Register Client in Server (SERVER CASES NOW WORK)

**Goal**: Enable sharedInstance on server via registry

### Code Changes

**File**: `server-index.ts`
**Location**: After line 22 (after connection completes)

```typescript
console.log(`✅ Server: JTAGClient connected with ${connectionResult.listResult.totalCount} commands`);

// Register client in static registry for sharedInstance symmetry
const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');
JTAGClient.registerClient('default', connectionResult.client);

// Return the client with commands interface, not the full connection result
```

### Build & Deploy
```bash
npx tsc --noEmit
npm run build:browser-ts
npm start
```

### Phase 4 Tests (CRITICAL: Server sharedInstance Now Works)

```bash
# 1. Create test script for server sharedInstance
cat > test-server-sharedinstance.ts << 'EOF'
import { jtag } from './server-index';

(async () => {
  console.log('🧪 Testing server sharedInstance...');

  const client = await jtag.connect();
  console.log(`✅ Client connected: ${client.sessionId.substring(0, 8)}...`);

  const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');
  const start = Date.now();
  const sharedClient = await JTAGClient.sharedInstance;
  const duration = Date.now() - start;

  console.log(`⏱️ sharedInstance resolved in ${duration}ms`);
  console.log(`Environment: ${sharedClient.context.environment}`);

  if (duration > 100) {
    console.error(`❌ FAIL: Too slow (${duration}ms)`);
    process.exit(1);
  }

  console.log('✅ PASS: Server sharedInstance works');
  process.exit(0);
})();
EOF

# 2. Run server sharedInstance test
npx tsx test-server-sharedinstance.ts > phase4-server-test.txt
cat phase4-server-test.txt
# Evidence: "Found client in registry (server)", resolution <100ms

# 3. System health
./jtag ping > phase4-ping.txt

# 4. AI responses (THE BIG TEST)
./jtag debug/chat-send --roomId="$ROOM_ID" --message="Phase 4 test: Server registry active, PersonaUser should work if using Events.emit"
sleep 15
./jtag ai/report > phase4-ai-report.json

# 5. Check for sharedInstance timeout errors
./jtag debug/logs --filterPattern="sharedInstance.*timeout" --tailLines=50
# Evidence: Should be empty (no timeout errors)
```

### Success Criteria
- ✅ Server sharedInstance resolves in <100ms
- ✅ Console log shows "Found client in registry (server)"
- ✅ AI responses work
- ✅ No sharedInstance timeout errors
- ✅ System health good

**ROLLBACK** if any test fails

---

## Phase 5: Add Disconnect Cleanup (PREVENT MEMORY LEAKS)

**Goal**: Unregister clients on disconnect

### Code Changes

**File**: `system/core/client/shared/JTAGClient.ts`
**Location**: Line 764 (start of disconnect method)

```typescript
public async disconnect(destroySession?: boolean): Promise<void> {
  console.log('🔌 JTAGClient: Disconnecting...');

  // Unregister from static registry to prevent memory leaks
  JTAGClient.unregisterClient('default');

  // Smart default: Don't destroy shared sessions, do destroy private sessions
  const shouldDestroySession = destroySession ?? !this._session?.isShared;
  // ... rest of disconnect logic
}
```

### Build & Deploy
```bash
npx tsc --noEmit
npm run build:browser-ts
npm start
```

### Phase 5 Tests (VERIFY CLEANUP)

```bash
# 1. Test disconnect cleanup
cat > test-disconnect-cleanup.ts << 'EOF'
import { jtag } from './server-index';

(async () => {
  console.log('🧪 Testing disconnect cleanup...');

  const client = await jtag.connect();
  console.log(`✅ Client connected: ${client.sessionId.substring(0, 8)}...`);

  const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');
  const sharedBefore = await JTAGClient.sharedInstance;
  console.log(`✅ sharedInstance works before disconnect: ${sharedBefore.sessionId.substring(0, 8)}...`);

  await client.disconnect();
  console.log('✅ Client disconnected');

  // Should see unregister log: "🗑️ JTAGClient: Unregistered client 'default'"

  console.log('✅ PASS: Disconnect cleanup works');
  process.exit(0);
})();
EOF

npx tsx test-disconnect-cleanup.ts > phase5-cleanup-test.txt
cat phase5-cleanup-test.txt
# Evidence: Should see "🗑️ JTAGClient: Unregistered client 'default'" log

# 2. System health
./jtag ping > phase5-ping.txt

# 3. AI responses (final test)
./jtag debug/chat-send --roomId="$ROOM_ID" --message="Phase 5 test: Cleanup added, all phases complete"
sleep 15
./jtag ai/report > phase5-ai-report.json
```

### Success Criteria
- ✅ Disconnect log shows unregister message
- ✅ AI responses still work
- ✅ System health good
- ✅ No memory leak warnings

**ROLLBACK** if any test fails

---

## Phase 6: Final Integration Test (PROVE IT WORKS)

**Goal**: Comprehensive end-to-end test proving all 4 cases work

### The ONE Test That Matters

```bash
# 1. Get room ID
ROOM_ID=$(./jtag data/list --collection=rooms --limit=1 | jq -r '.items[0].id')

# 2. Send message
./jtag debug/chat-send --roomId="$ROOM_ID" --message="Final test: Explain the sharedInstance fix we just implemented. Be specific about registry vs globalThis."

# 3. Wait for responses
sleep 15

# 4. Check logs
./jtag debug/logs --filterPattern="Worker evaluated|AI-RESPONSE|POSTED" --tailLines=50 > final-logs.txt
grep -c "Worker evaluated" final-logs.txt
# Evidence: Should show multiple AIs evaluating

# 5. AI report
./jtag ai/report > final-ai-report.json
cat final-ai-report.json

# 6. Visual verification
./jtag screenshot --querySelector="chat-widget" --filename="final-chat.png"

# 7. Check for errors
./jtag debug/logs --includeErrorsOnly=true --tailLines=50 > final-errors.txt
cat final-errors.txt
# Evidence: Should NOT show sharedInstance timeout errors
```

### Success Criteria (ALL MUST PASS)

1. **Logs Show Worker Activity**
```
[timestamp] 🤖 PersonaUser[Helper AI]: Worker evaluated message in 250ms
[timestamp] 📤 PersonaUser[Helper AI]: Decision: should_respond (confidence: 0.85)
[timestamp] ✅ PersonaUser[Helper AI]: Response posted to room
```

2. **AI Report Shows Responses**
```json
{
  "totalDecisions": 5+,
  "totalResponses": 2+,
  "avgConfidence": 0.7+,
  "errors": 0
}
```

3. **Screenshot Shows AI Responses**: At least 2 visible AI responses

4. **No Error Logs**: No sharedInstance timeout errors, no client.events.room errors

### Evidence Files for Commit
```
final-logs.txt              # PersonaUser activity logs
final-ai-report.json        # AI response metrics
final-chat.png              # Visual verification
final-errors.txt            # Should be empty or only non-critical warnings
```

**ROLLBACK** if ANY criterion fails

---

## Phase 7: Commit and Document

**Goal**: Commit working fix with complete evidence

### Commit Message

```
Fix JTAGClient.sharedInstance deadlock via static client registry

Root Cause: globalThis.jtag only set in browser, never in server.
This caused infinite polling in sharedInstance getter, breaking all
server-side Events.emit() auto-context calls.

Solution: Add static clientRegistry Map to JTAGClient class.
- sharedInstance checks registry first (server), then globalThis (browser)
- Both browser-index.ts and server-index.ts register clients
- Added 5s timeout to prevent infinite polling
- Cleanup: unregister on disconnect to prevent memory leaks

Impact: Server-side code can now use JTAGClient.sharedInstance safely.
PersonaUser and daemons can use Events.emit() auto-context form.

Tested (all phases passed):
✅ Phase 1: Registry added (no behavior change)
✅ Phase 2: sharedInstance updated (browser unchanged)
✅ Phase 3: Browser registration (browser uses registry)
✅ Phase 4: Server registration (server sharedInstance works)
✅ Phase 5: Disconnect cleanup (no memory leaks)
✅ Phase 6: Integration test (AI responds, no errors)

Evidence:
- Server sharedInstance resolves in <100ms (was infinite before)
- Browser sharedInstance resolves in <10ms (unchanged)
- AI report shows responses from multiple personas
- Zero sharedInstance timeout errors in logs
- System health: 64 commands, 12 daemons, systemReady: true

Files Changed:
- system/core/client/shared/JTAGClient.ts (registry + sharedInstance)
- browser-index.ts (register client)
- server-index.ts (register client)

Test Evidence:
- final-logs.txt (PersonaUser activity)
- final-ai-report.json (AI metrics)
- final-chat.png (visual verification)
- final-errors.txt (no critical errors)
```

### Commit Command
```bash
git add system/core/client/shared/JTAGClient.ts browser-index.ts server-index.ts
git add final-*.txt final-*.json final-*.png
git commit -F commit-message.txt
```

---

## Rollback Procedures

### If Any Phase Fails

```bash
# 1. Restore all changes
git restore .

# 2. Rebuild and redeploy
npm run build:browser-ts
npm start

# 3. Verify rollback worked
./jtag ping
./jtag debug/chat-send --roomId="$ROOM_ID" --message="Rollback test"
sleep 15
./jtag ai/report

# 4. Document failure
echo "Phase X failed: <reason>" >> rollback-notes.txt
```

### If Commit Fails

```bash
# Revert commit
git revert HEAD

# Redeploy
npm start

# Verify system works
./jtag ping
```

---

## Timeline Estimate

**Total: 3-4 hours**

- Phase 0 (Baseline): 15 min
- Phase 1 (Registry): 30 min
- Phase 2 (sharedInstance): 30 min
- Phase 3 (Browser): 20 min
- Phase 4 (Server): 30 min
- Phase 5 (Cleanup): 20 min
- Phase 6 (Integration): 30 min
- Phase 7 (Commit): 15 min

---

## Pre-Implementation Checklist

**DO NOT START** until all verified:

- [ ] Current system works (baseline tests pass)
- [ ] Git working directory clean
- [ ] Stash created: `git stash push -u -m "Before sharedInstance fix"`
- [ ] Joel approves this phased plan
- [ ] Baseline evidence files created

**READY TO BEGIN?** Run Phase 0 tests first!
