# JTAGClient.sharedInstance Fix - Implementation and Testing Plan

**Date**: 2025-10-19
**Status**: READY FOR REVIEW - DO NOT IMPLEMENT UNTIL APPROVED
**Risk Level**: HIGH - Touches core connection architecture
**Rollback Strategy**: Git stash before starting, revert immediately if any test fails

---

## Overview

Fix `JTAGClient.sharedInstance` to work symmetrically in all four connection cases by adding a static client registry that works alongside the existing `globalThis.jtag` mechanism.

**Current State**: Server cases (3 & 4) infinite poll because `globalThis.jtag` is never set
**Target State**: All four cases resolve sharedInstance immediately via registry or globalThis

---

## Code Changes

### CHANGE 1: Add Static Client Registry to JTAGClient

**File**: `system/core/client/shared/JTAGClient.ts`
**Location**: After line 183 (inside JTAGClient class)

**BEFORE**:
```typescript
export abstract class JTAGClient extends JTAGBase implements ITransportHandler {
  protected systemTransport?: JTAGTransport;
  protected connection?: JTAGConnection;
  // TODO: Remove discoveredCommands - redundant with CommandsInterface (ISSUE 2)
  protected discoveredCommands: Map<string, CommandSignature> = new Map();
  protected systemInstance?: JTAGSystem;
  protected responseCorrelator: ResponseCorrelator = new ResponseCorrelator(30000);
```

**AFTER**:
```typescript
export abstract class JTAGClient extends JTAGBase implements ITransportHandler {
  // Static client registry for symmetric sharedInstance access
  private static clientRegistry: Map<string, JTAGClient> = new Map();

  protected systemTransport?: JTAGTransport;
  protected connection?: JTAGConnection;
  // TODO: Remove discoveredCommands - redundant with CommandsInterface (ISSUE 2)
  protected discoveredCommands: Map<string, CommandSignature> = new Map();
  protected systemInstance?: JTAGSystem;
  protected responseCorrelator: ResponseCorrelator = new ResponseCorrelator(30000);
```

**Expected Behavior**: No behavior change yet - just adds empty registry

**Validation**: `npx tsc --noEmit` must pass with zero errors

---

### CHANGE 2: Add registerClient() Static Method

**File**: `system/core/client/shared/JTAGClient.ts`
**Location**: Before line 810 (before existing sharedInstance getter)

**ADD NEW METHOD**:
```typescript
  /**
   * Register a client instance in the static registry
   * Enables sharedInstance to work in server environments where globalThis.jtag is not set
   *
   * @param key - Registry key (typically 'default' for main client, or context.uuid for multi-client)
   * @param client - Client instance to register
   */
  static registerClient(key: string, client: JTAGClient): void {
    this.clientRegistry.set(key, client);
    console.log(`📝 JTAGClient: Registered client '${key}' (environment: ${client.context.environment}, sessionId: ${client.sessionId.substring(0, 8)}...)`);
  }

  /**
   * Unregister a client instance from the static registry
   * Called during client.disconnect() to prevent memory leaks
   *
   * @param key - Registry key to remove
   */
  static unregisterClient(key: string): boolean {
    const removed = this.clientRegistry.delete(key);
    if (removed) {
      console.log(`🗑️ JTAGClient: Unregistered client '${key}'`);
    }
    return removed;
  }

  /**
   * Get registered client by key
   * Used internally by sharedInstance
   */
  private static getRegisteredClient(key: string): JTAGClient | undefined {
    return this.clientRegistry.get(key);
  }
```

**Expected Behavior**: Methods exist but do nothing until called

**Validation**: `npx tsc --noEmit` must pass with zero errors

---

### CHANGE 3: Update sharedInstance Getter with Registry Support

**File**: `system/core/client/shared/JTAGClient.ts`
**Location**: Lines 810-822 (replace existing sharedInstance getter)

**BEFORE**:
```typescript
  /**
   * Get shared instance from global context - works in browser and server
   * Browser: (window as WindowWithJTAG).jtag
   * Server: (globalThis as any).jtag
   */
  static get sharedInstance(): Promise<JTAGClient> {
    return new Promise((resolve) => {
      const checkReady = (): void => {
        const jtag = (globalThis as any).jtag;
        if (jtag?.commands) {
          resolve(jtag);
        } else {
          setTimeout(checkReady, 50);
        }
      };
      checkReady();
    });
  }
```

**AFTER**:
```typescript
  /**
   * Get shared instance - symmetric access for browser and server
   *
   * Resolution order:
   * 1. Check static registry (works for server LocalConnection/RemoteConnection)
   * 2. Check globalThis.jtag (works for browser after initialization)
   * 3. Poll with timeout (fallback for race conditions, max 5 seconds)
   *
   * Browser: Uses globalThis.jtag set by browser-index.ts
   * Server: Uses static registry set by server-index.ts
   */
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
      const maxAttempts = 100; // 5 seconds max (50ms * 100)

      const checkReady = (): void => {
        attempts++;

        // Check registry again
        const client = this.getRegisteredClient('default');
        if (client) {
          console.log(`✅ JTAGClient.sharedInstance: Client ready in registry after ${attempts * 50}ms`);
          resolve(client);
          return;
        }

        // Check globalThis again
        const jtagNow = (globalThis as any).jtag;
        if (jtagNow?.commands) {
          console.log(`✅ JTAGClient.sharedInstance: Client ready in globalThis after ${attempts * 50}ms`);
          resolve(jtagNow);
          return;
        }

        // Timeout after max attempts
        if (attempts >= maxAttempts) {
          const error = new Error(`JTAGClient.sharedInstance: No client available after ${attempts * 50}ms timeout. Registry empty, globalThis.jtag not set.`);
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

**Expected Behavior**:
- Browser: Still uses globalThis.jtag (no change in behavior)
- Server: Now uses registry (fixes infinite polling)
- Both: Timeout after 5s instead of infinite polling

**Validation**: `npx tsc --noEmit` must pass with zero errors

---

### CHANGE 4: Register Client in Browser Entry Point

**File**: `browser-index.ts`
**Location**: Line 52 (after existing globalThis.jtag assignment)

**BEFORE**:
```typescript
    // Set up global window.jtag for widgets and tests
    (globalThis as any).jtag = client;


    // Enhance client with organized services - no globals needed
    const services = createJTAGClientServices(client);
```

**AFTER**:
```typescript
    // Set up global window.jtag for widgets and tests
    (globalThis as any).jtag = client;

    // Register client in static registry for sharedInstance symmetry
    const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');
    JTAGClient.registerClient('default', client);

    // Enhance client with organized services - no globals needed
    const services = createJTAGClientServices(client);
```

**Expected Behavior**: Browser sets BOTH globalThis.jtag AND registry

**Validation**: Browser connects successfully, widgets load

---

### CHANGE 5: Register Client in Server Entry Point

**File**: `server-index.ts`
**Location**: Line 23 (after client is created, before return)

**BEFORE**:
```typescript
    console.log(`✅ Server: JTAGClient connected with ${connectionResult.listResult.totalCount} commands`);

    // Return the client with commands interface, not the full connection result
    return connectionResult.client;
  },
```

**AFTER**:
```typescript
    console.log(`✅ Server: JTAGClient connected with ${connectionResult.listResult.totalCount} commands`);

    // Register client in static registry for sharedInstance symmetry
    const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');
    JTAGClient.registerClient('default', connectionResult.client);

    // Return the client with commands interface, not the full connection result
    return connectionResult.client;
  },
```

**Expected Behavior**: Server now registers client in registry (enables sharedInstance on server)

**Validation**: Server connects successfully, commands execute

---

### CHANGE 6: Unregister Client on Disconnect

**File**: `system/core/client/shared/JTAGClient.ts`
**Location**: Line 764 (inside disconnect() method, at the start)

**BEFORE**:
```typescript
  public async disconnect(destroySession?: boolean): Promise<void> {
    console.log('🔌 JTAGClient: Disconnecting...');

    // Smart default: Don't destroy shared sessions, do destroy private sessions
    const shouldDestroySession = destroySession ?? !this._session?.isShared;
```

**AFTER**:
```typescript
  public async disconnect(destroySession?: boolean): Promise<void> {
    console.log('🔌 JTAGClient: Disconnecting...');

    // Unregister from static registry to prevent memory leaks
    JTAGClient.unregisterClient('default');

    // Smart default: Don't destroy shared sessions, do destroy private sessions
    const shouldDestroySession = destroySession ?? !this._session?.isShared;
```

**Expected Behavior**: Client is removed from registry on disconnect (prevents memory leaks)

**Validation**: Disconnect succeeds, registry is empty after disconnect

---

## Build Verification

After ALL changes are made, run these commands IN ORDER:

```bash
# 1. TypeScript compilation - MUST PASS
npx tsc --noEmit

# 2. If compilation passes, build browser bundle
npm run build:browser-ts

# 3. If browser build passes, deploy system
npm start
```

**STOP IMMEDIATELY** if any command fails. Do NOT proceed to testing if build fails.

---

## Integration Testing Strategy

### Test Execution Order

**CRITICAL**: Tests MUST be run in this exact order. If ANY test fails, STOP and rollback immediately.

```bash
# 1. Verify system is running
./jtag ping

# 2. Test CASE 2 first (simplest - browser LocalConnection)
# 3. Test CASE 1 next (browser RemoteConnection)
# 4. Test CASE 4 (server LocalConnection)
# 5. Test CASE 3 (server RemoteConnection - most complex)
# 6. Test PersonaUser (real-world usage)
```

---

## TEST CASE 1: Browser RemoteConnection

**Scenario**: Browser client connects to remote server via WebSocket

**Test Type**: Manual browser test

**Test Steps**:
```bash
# 1. Deploy system
npm start

# 2. Wait for browser to open
# Browser should automatically open to http://localhost:9003

# 3. Open browser console
# Check for these log messages in order:
```

**Expected Console Logs**:
```
🔌 Browser: Connecting via JTAGClientBrowser (local connection)
✅ JTAGClient: Connection established
🔄 JTAGClient: Discovering available commands...
✅ JTAGClient: Bootstrap complete - 64+ commands
📝 JTAGClient: Registered client 'default' (environment: browser, sessionId: ...)
✅ Browser connected successfully
```

**Success Criteria**:
- ✅ Client connects successfully
- ✅ `📝 JTAGClient: Registered client 'default'` log appears
- ✅ Widgets load and render
- ✅ Chat widget is visible
- ✅ No console errors

**Test sharedInstance Access**:
```javascript
// Run in browser console
(async () => {
  const { JTAGClient } = await import('/system/core/client/shared/JTAGClient.js');
  const start = Date.now();
  const client = await JTAGClient.sharedInstance;
  const duration = Date.now() - start;
  console.log(`✅ sharedInstance resolved in ${duration}ms`);
  console.log(`Client environment: ${client.context.environment}`);
  console.log(`Client sessionId: ${client.sessionId}`);
  console.log(`Client has commands: ${!!client.commands}`);
})();
```

**Expected Output**:
```
✅ JTAGClient.sharedInstance: Found client in globalThis.jtag (browser)
✅ sharedInstance resolved in <10ms
Client environment: browser
Client sessionId: <valid-uuid>
Client has commands: true
```

**FAILURE CRITERIA** (rollback if ANY occur):
- ❌ Client does not connect
- ❌ Registration log does not appear
- ❌ sharedInstance takes >100ms to resolve
- ❌ sharedInstance throws timeout error
- ❌ Console shows TypeScript errors
- ❌ Widgets do not load

---

## TEST CASE 2: Browser LocalConnection

**Scenario**: Browser client connects to local browser system (isLocal=true)

**Test Type**: Same as Test Case 1 (browser-index.ts uses JTAGClientBrowser.connectLocal())

**Expected Behavior**: IDENTICAL to Test Case 1
- Browser always uses LocalConnection when JTAGSystemBrowser is available
- `browser-index.ts:48` calls `JTAGClientBrowser.connectLocal()`
- `JTAGClient.initialize()` finds local system and creates LocalConnection

**Success Criteria**: Same as Test Case 1

**Connection Type Verification**:
```javascript
// Run in browser console
const client = window.jtag;
console.log(`Connection type: ${client.isLocal ? 'LocalConnection' : 'RemoteConnection'}`);
console.log(`Expected: LocalConnection`);
```

**Expected Output**:
```
Connection type: LocalConnection
Expected: LocalConnection
```

**FAILURE CRITERIA**: Same as Test Case 1, plus:
- ❌ Connection type is not LocalConnection

---

## TEST CASE 3: Server RemoteConnection

**Scenario**: Server CLI command connects to remote server via transport

**Test Type**: CLI command execution

**Test Command**:
```bash
# Simple command that doesn't require complex setup
./jtag system/ping
```

**Expected Output**:
```
📝 JTAGClient: Registered client 'default' (environment: server, sessionId: ...)
{
  "success": true,
  "timestamp": "...",
  "systemReady": true,
  "daemonCount": 12,
  "commandCount": 64,
  "version": "1.0.3411"
}
```

**Success Criteria**:
- ✅ Command executes successfully
- ✅ Registration log appears
- ✅ Response is valid JSON
- ✅ No timeout errors
- ✅ Command completes in <5 seconds

**Test sharedInstance Access** (via test script):
```bash
# Create test file: test-sharedinstance-server.ts
npx tsx test-sharedinstance-server.ts
```

**Test File Content**:
```typescript
// test-sharedinstance-server.ts
import { jtag } from './server-index';

(async () => {
  console.log('🧪 Testing JTAGClient.sharedInstance on server...');

  // Connect client
  const client = await jtag.connect();
  console.log(`✅ Client connected: ${client.sessionId.substring(0, 8)}...`);

  // Test sharedInstance
  const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');
  const start = Date.now();
  const sharedClient = await JTAGClient.sharedInstance;
  const duration = Date.now() - start;

  console.log(`✅ sharedInstance resolved in ${duration}ms`);
  console.log(`Environment: ${sharedClient.context.environment}`);
  console.log(`SessionId: ${sharedClient.sessionId.substring(0, 8)}...`);
  console.log(`Has commands: ${!!sharedClient.commands}`);

  if (duration > 100) {
    console.error(`❌ FAILED: sharedInstance took ${duration}ms (expected <100ms)`);
    process.exit(1);
  }

  if (!sharedClient.commands) {
    console.error(`❌ FAILED: sharedInstance has no commands`);
    process.exit(1);
  }

  console.log('✅ TEST PASSED');
  process.exit(0);
})();
```

**Expected Output**:
```
🧪 Testing JTAGClient.sharedInstance on server...
📝 JTAGClient: Registered client 'default' (environment: server, sessionId: ...)
✅ Client connected: 12345678...
✅ JTAGClient.sharedInstance: Found client in registry (server)
✅ sharedInstance resolved in 5ms
Environment: server
SessionId: 12345678...
Has commands: true
✅ TEST PASSED
```

**FAILURE CRITERIA** (rollback if ANY occur):
- ❌ Command times out
- ❌ sharedInstance takes >100ms to resolve
- ❌ sharedInstance throws timeout error
- ❌ Registration log does not appear
- ❌ Test script exits with code 1

---

## TEST CASE 4: Server LocalConnection

**Scenario**: Server code running in same process as system (PersonaUser, daemons)

**Test Type**: Same as Test Case 3, but verify LocalConnection is used

**Connection Type Verification**:
```typescript
// Add to test-sharedinstance-server.ts
const client = await jtag.connect();
console.log(`Connection type: ${client.isLocal ? 'LocalConnection' : 'RemoteConnection'}`);
console.log(`Expected: LocalConnection`);
```

**Expected Output**:
```
Connection type: LocalConnection
Expected: LocalConnection
```

**Success Criteria**: Same as Test Case 3, plus:
- ✅ Connection type is LocalConnection

**FAILURE CRITERIA**: Same as Test Case 3, plus:
- ❌ Connection type is not LocalConnection

---

## TEST CASE 5: PersonaUser Event Emission

**Scenario**: PersonaUser in Worker Thread emits AI decision events

**Test Type**: Send message in chat and verify AI responds

**Prerequisites**:
- System must be running
- At least one AI persona must be active
- Room must exist with AI as member

**Test Steps**:
```bash
# 1. Get room ID
./jtag data/list --collection=rooms --limit=1

# 2. Send test message
./jtag debug/chat-send --roomId="<ROOM-ID>" --message="Test: verify PersonaUser events work after sharedInstance fix"

# 3. Wait 10 seconds

# 4. Check AI response logs
./jtag debug/logs --filterPattern="AI-RESPONSE|POSTED|Worker evaluated" --tailLines=30

# 5. Get AI report
./jtag ai/report

# 6. Visual verification
./jtag screenshot --querySelector="chat-widget" --filename="test-persona-events.png"
```

**Expected Log Output**:
```
[timestamp] 🤖 PersonaUser[Helper AI]: Worker evaluated message in 250ms
[timestamp] 📤 PersonaUser[Helper AI]: Decision: should_respond (confidence: 0.85)
[timestamp] ✅ PersonaUser[Helper AI]: Response posted to room
[timestamp] AI-RESPONSE: Helper AI responded in thread
```

**Expected AI Report Output**:
```json
{
  "totalDecisions": 5,
  "totalResponses": 2,
  "avgConfidence": 0.82,
  "avgLatency": 280,
  "errors": 0
}
```

**Success Criteria**:
- ✅ AI receives message and evaluates it
- ✅ AI decision event is emitted (shows in logs)
- ✅ AI generates response
- ✅ AI response appears in chat widget
- ✅ No "Worker Thread" errors in logs
- ✅ No "sharedInstance timeout" errors
- ✅ AI report shows at least 1 response
- ✅ Screenshot shows AI response in chat

**FAILURE CRITERIA** (rollback if ANY occur):
- ❌ AI does not respond after 30 seconds
- ❌ Logs show "Worker Thread" errors
- ❌ Logs show "sharedInstance timeout"
- ❌ AI report shows 0 responses
- ❌ Screenshot shows no AI response
- ❌ Console shows PersonaUser errors

---

## TEST CASE 6: Events.emit() Auto-Context Form

**Scenario**: Verify auto-context Events.emit() works in all environments

**Test Type**: Programmatic test in both browser and server

**Browser Test** (run in browser console):
```javascript
(async () => {
  const { Events } = await import('/system/core/shared/Events.js');

  console.log('🧪 Testing Events.emit() auto-context in browser...');

  const start = Date.now();
  const result = await Events.emit('test:browser:event', { test: true });
  const duration = Date.now() - start;

  console.log(`✅ Events.emit() completed in ${duration}ms`);
  console.log(`Result: ${JSON.stringify(result)}`);

  if (!result.success) {
    console.error('❌ FAILED: Events.emit() returned success=false');
  } else if (duration > 1000) {
    console.error(`❌ FAILED: Events.emit() took ${duration}ms (expected <1000ms)`);
  } else {
    console.log('✅ TEST PASSED');
  }
})();
```

**Expected Browser Output**:
```
🧪 Testing Events.emit() auto-context in browser...
✅ JTAGClient.sharedInstance: Found client in globalThis.jtag (browser)
✅ Events: Emitted test:browser:event from browser/<uuid>
✅ Events.emit() completed in 15ms
Result: {"success":true}
✅ TEST PASSED
```

**Server Test** (create test file):
```typescript
// test-events-autocontext-server.ts
import { jtag } from './server-index';

(async () => {
  console.log('🧪 Testing Events.emit() auto-context on server...');

  // Connect client first
  const client = await jtag.connect();
  console.log(`✅ Client connected: ${client.sessionId.substring(0, 8)}...`);

  // Test auto-context Events.emit()
  const { Events } = await import('./system/core/shared/Events');

  const start = Date.now();
  const result = await Events.emit('test:server:event', { test: true });
  const duration = Date.now() - start;

  console.log(`✅ Events.emit() completed in ${duration}ms`);
  console.log(`Result: ${JSON.stringify(result)}`);

  if (!result.success) {
    console.error('❌ FAILED: Events.emit() returned success=false');
    process.exit(1);
  } else if (duration > 1000) {
    console.error(`❌ FAILED: Events.emit() took ${duration}ms (expected <1000ms)`);
    process.exit(1);
  } else {
    console.log('✅ TEST PASSED');
    process.exit(0);
  }
})();
```

**Run Server Test**:
```bash
npx tsx test-events-autocontext-server.ts
```

**Expected Server Output**:
```
🧪 Testing Events.emit() auto-context on server...
📝 JTAGClient: Registered client 'default' (environment: server, sessionId: ...)
✅ Client connected: 12345678...
✅ JTAGClient.sharedInstance: Found client in registry (server)
✅ Events: Emitted test:server:event from server/<uuid>
✅ Events.emit() completed in 18ms
Result: {"success":true}
✅ TEST PASSED
```

**Success Criteria**:
- ✅ Browser test passes (<1000ms, success=true)
- ✅ Server test passes (<1000ms, success=true)
- ✅ Both tests resolve sharedInstance quickly
- ✅ No timeout errors
- ✅ Events are emitted successfully

**FAILURE CRITERIA** (rollback if ANY occur):
- ❌ Browser test takes >1000ms
- ❌ Server test takes >1000ms
- ❌ Either test returns success=false
- ❌ Either test throws timeout error
- ❌ Test process hangs indefinitely

---

## Proof of Success Matrix

All cells MUST be ✅ before fix is considered complete:

| Case | Connection Type     | sharedInstance Works | Events.emit() Works | PersonaUser Works | Integration Test Passes |
|------|---------------------|----------------------|---------------------|-------------------|-------------------------|
| 1    | Browser Remote      | ✅ (must verify)     | ✅ (must verify)    | ✅ (must verify)  | ✅ (must verify)        |
| 2    | Browser Local       | ✅ (must verify)     | ✅ (must verify)    | ✅ (must verify)  | ✅ (must verify)        |
| 3    | Server Remote       | ❌ (currently broken)| ❌ (currently broken)| ❌ (currently broken)| ❌ (currently broken) |
| 4    | Server Local        | ❌ (currently broken)| ❌ (currently broken)| ❌ (currently broken)| ❌ (currently broken) |

**After Fix, ALL cells must be ✅**

---

## Rollback Plan

If ANY test fails:

```bash
# 1. STOP IMMEDIATELY - Do not continue testing

# 2. Check git status
git status

# 3. Restore all changes
git restore .

# 4. Verify rollback
npx tsc --noEmit
npm start
./jtag ping

# 5. Re-run ONLY the failing test to confirm it works with old code
# If old code also fails, this is a different bug - investigate separately

# 6. Document the failure in design/JTAGCLIENT-SHAREDINSTANCE-ROLLBACK-NOTES.md
```

---

## Timeline Estimate

**Total Time**: 3-4 hours

1. **Implementation**: 30 minutes
   - Add registry to JTAGClient: 5 min
   - Add registerClient/unregisterClient methods: 5 min
   - Update sharedInstance getter: 10 min
   - Update browser-index.ts: 2 min
   - Update server-index.ts: 2 min
   - Update disconnect(): 2 min
   - Build and deploy: 4 min

2. **Testing**: 2-3 hours
   - Test Case 1 (Browser Remote): 20 min
   - Test Case 2 (Browser Local): 15 min
   - Test Case 3 (Server Remote): 30 min
   - Test Case 4 (Server Local): 20 min
   - Test Case 5 (PersonaUser): 45 min
   - Test Case 6 (Events auto-context): 30 min

3. **Documentation**: 30 minutes
   - Update JTAGCLIENT-SHAREDINSTANCE-DEADLOCK-ANALYSIS.md with results
   - Document any issues encountered
   - Update CLAUDE.md if needed

---

## Risk Assessment

**HIGH RISK AREAS**:
1. ❗ **JTAGClient.sharedInstance** - Core client access mechanism used throughout system
2. ❗ **Events.emit() auto-context** - Used by PersonaUser, widgets, and daemons
3. ❗ **Worker Threads** - PersonaUser runs in separate thread, may have different behavior

**MEDIUM RISK AREAS**:
1. ⚠️ **Browser-index.ts** - Main browser entry point, affects all browser clients
2. ⚠️ **Server-index.ts** - Main server entry point, affects all CLI commands
3. ⚠️ **Client disconnect** - Memory leak prevention, affects cleanup

**LOW RISK AREAS**:
1. ℹ️ **Static registry** - Isolated data structure, doesn't affect existing code
2. ℹ️ **Console logging** - Debugging output, no functional impact

---

## Success Definition

The fix is considered **COMPLETE AND SUCCESSFUL** when:

1. ✅ All 6 integration tests pass without errors
2. ✅ All 4 connection cases resolve sharedInstance in <100ms
3. ✅ PersonaUser responds to messages (proves Events work)
4. ✅ No timeout errors in any logs
5. ✅ No TypeScript compilation errors
6. ✅ System deploys successfully (`npm start` completes)
7. ✅ All 64+ commands are registered and executable
8. ✅ Browser widgets load and render correctly
9. ✅ Chat functionality works end-to-end
10. ✅ AI report shows responses from multiple personas

**ONLY THEN** can we commit the changes with message:
```
Fix JTAGClient.sharedInstance deadlock on server via static registry

- Add static clientRegistry to JTAGClient for symmetric access
- Update sharedInstance getter to check registry before globalThis
- Register clients in both browser-index.ts and server-index.ts
- Add timeout (5s) to prevent infinite polling
- Unregister clients on disconnect to prevent memory leaks

Fixes: SOTA AI models crashing with "this.client.events.room is not a function"
Tested: All 4 connection cases + PersonaUser + Events.emit() auto-context

✅ Browser RemoteConnection: sharedInstance resolves via globalThis
✅ Browser LocalConnection: sharedInstance resolves via globalThis
✅ Server RemoteConnection: sharedInstance resolves via registry
✅ Server LocalConnection: sharedInstance resolves via registry
✅ PersonaUser: AI responses work in all cases
✅ Events.emit(): Auto-context form works in all cases
```

---

## Pre-Implementation Checklist

Before starting implementation, verify:

- [ ] `npm start` currently works (baseline working state)
- [ ] `./jtag ping` returns successful response
- [ ] Browser loads and shows widgets
- [ ] Chat widget is visible and functional
- [ ] Can send test message: `./jtag debug/chat-send --roomId=<ID> --message="Test"`
- [ ] Git working directory is clean (`git status` shows no uncommitted changes)
- [ ] Created git stash point: `git stash push -u -m "Before sharedInstance fix"`
- [ ] This document is reviewed and approved by Joel

**DO NOT PROCEED WITH IMPLEMENTATION UNTIL ALL BOXES ARE CHECKED**
