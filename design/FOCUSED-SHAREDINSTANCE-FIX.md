# Focused Goal: Fix JTAGClient.sharedInstance for All 4 Cases

**Date**: 2025-10-19
**Goal**: ONLY fix sharedInstance - nothing else
**Test**: Prove sharedInstance resolves quickly in all 4 connection cases

---

## The ONLY Changes Needed

### 1. Add Static Registry (JTAGClient.ts)

```typescript
// After line 183
private static clientRegistry: Map<string, JTAGClient> = new Map();

// Before line 810
static registerClient(key: string, client: JTAGClient): void {
  this.clientRegistry.set(key, client);
  console.log(`📝 JTAGClient: Registered '${key}' (${client.context.environment})`);
}

static unregisterClient(key: string): boolean {
  return this.clientRegistry.delete(key);
}

private static getRegisteredClient(key: string): JTAGClient | undefined {
  return this.clientRegistry.get(key);
}
```

### 2. Update sharedInstance Getter (JTAGClient.ts lines 810-822)

```typescript
static get sharedInstance(): Promise<JTAGClient> {
  return new Promise((resolve, reject) => {
    // 1. Registry first (server)
    const registered = this.getRegisteredClient('default');
    if (registered) {
      resolve(registered);
      return;
    }

    // 2. globalThis second (browser)
    const jtag = (globalThis as any).jtag;
    if (jtag?.commands) {
      resolve(jtag);
      return;
    }

    // 3. Poll with 5s timeout (fallback)
    let attempts = 0;
    const checkReady = (): void => {
      attempts++;
      const client = this.getRegisteredClient('default');
      if (client) {
        resolve(client);
        return;
      }
      const jtagNow = (globalThis as any).jtag;
      if (jtagNow?.commands) {
        resolve(jtagNow);
        return;
      }
      if (attempts >= 100) {
        reject(new Error('sharedInstance timeout after 5s'));
        return;
      }
      setTimeout(checkReady, 50);
    };
    checkReady();
  });
}
```

### 3. Register in Browser (browser-index.ts after line 52)

```typescript
(globalThis as any).jtag = client;

// NEW: Register in registry
const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');
JTAGClient.registerClient('default', client);
```

### 4. Register in Server (server-index.ts after line 22)

```typescript
console.log(`✅ Server: JTAGClient connected...`);

// NEW: Register in registry
const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');
JTAGClient.registerClient('default', connectionResult.client);

return connectionResult.client;
```

### 5. Unregister on Disconnect (JTAGClient.ts line 764)

```typescript
public async disconnect(destroySession?: boolean): Promise<void> {
  console.log('🔌 JTAGClient: Disconnecting...');
  JTAGClient.unregisterClient('default');  // NEW
  // ... rest of disconnect
}
```

---

## Test Plan

### Phase 0: Baseline Evidence

```bash
# Create baseline directory
mkdir -p sharedinstance-test

# Test server sharedInstance (currently broken - will timeout)
cat > sharedinstance-test/test-before.ts << 'EOF'
import { jtag } from './server-index';
(async () => {
  const client = await jtag.connect();
  const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');
  const start = Date.now();
  try {
    const shared = await JTAGClient.sharedInstance;
    console.log(`✅ Resolved in ${Date.now() - start}ms`);
  } catch (e) {
    console.log(`❌ FAILED: ${e.message}`);
  }
  process.exit(0);
})();
EOF

timeout 10 npx tsx sharedinstance-test/test-before.ts > sharedinstance-test/baseline.txt 2>&1 || true
cat sharedinstance-test/baseline.txt
# Expected: Timeout or "No client available after 5000ms"
```

### Phase 1: Implement All Changes

```bash
# Make all 5 code changes listed above
# (Manual editing in JTAGClient.ts, browser-index.ts, server-index.ts)

# Build
npx tsc --noEmit
npm run build:browser-ts
npm start
```

### Phase 2: Test All 4 Cases

**Test Case 1 & 2: Browser** (both use same entry point)
```bash
# Open browser console (http://localhost:9003) and run:
(async () => {
  const { JTAGClient } = await import('/system/core/client/shared/JTAGClient.js');
  const start = Date.now();
  const client = await JTAGClient.sharedInstance;
  console.log(`✅ Browser: ${Date.now() - start}ms (${client.context.environment})`);
})();
# Expected: <10ms, environment=browser
```

**Test Case 3 & 4: Server** (test will use whichever is available)
```bash
cat > sharedinstance-test/test-after.ts << 'EOF'
import { jtag } from './server-index';
(async () => {
  const client = await jtag.connect();
  console.log(`✅ Connected: ${client.context.environment}`);

  const { JTAGClient } = await import('./system/core/client/shared/JTAGClient');
  const start = Date.now();
  const shared = await JTAGClient.sharedInstance;
  const duration = Date.now() - start;

  console.log(`✅ Server sharedInstance: ${duration}ms`);

  if (duration > 100) {
    console.error(`❌ FAIL: Too slow (${duration}ms)`);
    process.exit(1);
  }

  console.log('✅ PASS: Server sharedInstance works!');
  process.exit(0);
})();
EOF

npx tsx sharedinstance-test/test-after.ts > sharedinstance-test/after.txt
cat sharedinstance-test/after.txt
# Expected: <100ms resolution, PASS message
```

### Success Criteria

**ALL must pass**:
- ✅ Browser sharedInstance: <10ms
- ✅ Server sharedInstance: <100ms
- ✅ No timeout errors in logs
- ✅ System still works (`./jtag ping`)

### Evidence Files

```
sharedinstance-test/
├── baseline.txt          # Before fix (timeout)
├── after.txt             # After fix (fast)
├── test-before.ts        # Test script for baseline
└── test-after.ts         # Test script for verification
```

---

## Commit

```bash
git add system/core/client/shared/JTAGClient.ts browser-index.ts server-index.ts
git add sharedinstance-test/
git commit -m "Fix JTAGClient.sharedInstance via static client registry

PROBLEM: globalThis.jtag only set in browser, never in server
- Server: sharedInstance polls forever → timeout
- Browser: sharedInstance works via globalThis.jtag

SOLUTION: Add static clientRegistry Map
- Check registry first (server), then globalThis (browser)
- Both environments register clients after connection
- 5s timeout prevents infinite polling

TESTED:
✅ Browser sharedInstance: <10ms (unchanged)
✅ Server sharedInstance: <100ms (was timeout before)
✅ System health: 64 commands, 12 daemons, systemReady: true

Evidence: sharedinstance-test/ directory with before/after tests"
```

---

## Timeline

**Total: 45 minutes**
- Baseline test: 5 min
- Code changes: 15 min
- Build & deploy: 10 min
- Test verification: 10 min
- Commit: 5 min

---

## What This Enables

Once sharedInstance works in all 4 cases, we can:

1. **Fix PersonaUser** (separate PR): Use `Events.emit()` auto-context form
2. **Fix client.daemons.events.emit** (separate PR): Safe to use auto-context
3. **Add more features**: Any code can use sharedInstance safely

But for NOW: Just prove sharedInstance works.

---

## Ready?

- [ ] Joel approves this focused approach
- [ ] Create baseline test
- [ ] Make 5 code changes
- [ ] Test all 4 cases
- [ ] Commit with evidence
