# JTAG Middle-Out Testing Methodology

## 🎯 **Testing Philosophy: Real Cross-Environment Integration**

The JTAG system is inherently **multi-environment** (server ↔ browser), so testing with mocks misses the actual integration points. Following middle-out methodology, we test systematically across domain boundaries using **real environments**.

## 🏗️ **Architecture: npm start → Test-Bench → Cross-Environment Tests**

### **Core Pattern:**
```bash
npm start                           # 1. Clean build + launch test-bench
  ↓ 
examples/test-bench                 # 2. Real server (port 9002) + browser bundle
  ↓
tests/middle-out/                   # 3. Test real server ↔ browser communication
```

### **Why This Works:**
- ✅ **Real WebSocket connections** (not mocked)
- ✅ **Actual browser environment** (not simulated)
- ✅ **Cross-context routing** (server → browser → server)
- ✅ **File system validation** (logs, screenshots created)
- ✅ **Build-time manifests** (auto-discovery tested)

## 📋 **Middle-Out Test Domains**

### **Domain 1: Environment Detection & Auto-Discovery**
**Test**: Does `JTAGSystem.connect()` correctly auto-discover in both environments?

```typescript
// Server-side test
const serverJtag = await JTAGSystem.connect();
assert(serverJtag.getSystemInfo().context.environment === 'server');
assert(serverJtag.getSystemInfo().daemons.includes('CommandDaemon'));

// Browser-side test (via WebSocket to test-bench)
const browserResult = await sendToBrowser(`
  const browserJtag = await JTAGSystem.connect();
  return {
    environment: browserJtag.getSystemInfo().context.environment,
    daemons: browserJtag.getSystemInfo().daemons
  };
`);
assert(browserResult.environment === 'browser');
```

### **Domain 2: Cross-Context Command Routing**
**Test**: Server command → browser execution → server response

```typescript
// This should route: Server → WebSocket → Browser → html2canvas → WebSocket → Server
const screenshot = await serverJtag.commands.screenshot({
  filename: 'cross-context-test.png',
  selector: 'body'
});

assert(screenshot.success === true);
assert(screenshot.context === 'server'); // Response processed by server
assert(fs.existsSync(screenshot.filepath)); // File actually created
```

### **Domain 3: Console Interception & Transport**
**Test**: Console calls in browser appear in server log files

```typescript
// Execute in browser via test-bench
await sendToBrowser(`console.log('Browser test message', {testId: '${testId}'})`);

// Validate server received and logged it
await waitForLogEntry(testId);
const serverLogs = fs.readFileSync('.continuum/jtag/logs/browser.log.txt', 'utf8');
assert(serverLogs.includes('Browser test message'));
```

### **Domain 4: Build-Time Auto-Discovery Integration**
**Test**: Manifests work correctly in both environments

```typescript
// Verify daemon auto-discovery via manifests
const serverDaemons = await serverJtag.getDaemons();
assert(serverDaemons.has('CommandDaemon'));
assert(serverDaemons.has('ConsoleDaemon'));

// Verify command auto-discovery via manifests  
const commandDaemon = serverDaemons.get('CommandDaemon');
const availableCommands = commandDaemon.getAvailableCommands();
assert(availableCommands.includes('screenshot'));
```

### **Domain 5: Transport Resilience & Fallback**
**Test**: System handles transport failures gracefully

```typescript
// Test with WebSocket disconnection
await disconnectWebSocket();
await serverJtag.commands.screenshot({filename: 'offline-test.png'});
// Should queue command and retry when connection restored

await reconnectWebSocket();  
await waitForConnection();
// Command should execute after reconnection
```

## 🚀 **Implementation Strategy**

### **Phase 1: Real Test-Bench Integration**
1. **Update `npm test`** to use `npm start` (test-bench driven)
2. **Create middle-out test runner** that coordinates server + browser tests
3. **Replace mock-based tests** with real cross-environment tests

### **Phase 2: Domain-Crossing Test Suite**
1. **Environment boundary tests** (server ↔ browser discovery)
2. **Command routing tests** (cross-context execution)  
3. **Transport layer tests** (real WebSocket communication)
4. **File system tests** (actual log/screenshot creation)

### **Phase 3: Systematic Validation**
1. **Auto-discovery verification** (manifests work in both environments)
2. **Constructor injection validation** (clean dependency flow)
3. **Error handling tests** (transport failures, recovery)
4. **Performance tests** (real WebSocket latency, throughput)

## 📁 **New Test Structure**

```
tests/middle-out/
├── 00-test-bench-integration.test.ts    # Ensure test-bench is ready
├── 01-environment-detection.test.ts     # Domain 1: Auto-discovery
├── 02-cross-context-routing.test.ts     # Domain 2: Command routing  
├── 03-console-transport.test.ts         # Domain 3: Console interception
├── 04-manifest-discovery.test.ts        # Domain 4: Build-time manifests
├── 05-transport-resilience.test.ts      # Domain 5: Failure handling
├── shared/
│   ├── TestBenchClient.ts               # Coordinates with test-bench
│   ├── BrowserExecutor.ts               # Executes code in browser
│   └── CrossEnvironmentValidator.ts     # Validates cross-domain results
└── README.md                            # This file
```

## 🔄 **Test Execution Flow**

### **1. Setup Phase**
```bash
npm start                               # Launches test-bench (server + browser)
```
- Clean build (manifests regenerated)
- Start server on port 9002
- Bundle and serve browser assets
- Server runs with JTAG system active

### **2. Test Phase**
```bash
npm test                                # Runs middle-out test suite
```
- Connect to running test-bench
- Execute cross-environment tests
- Validate real server ↔ browser flows
- Check actual file system results

### **3. Validation Phase**
- ✅ **Log files created** in `.continuum/jtag/logs/`
- ✅ **Screenshots captured** in `.continuum/jtag/screenshots/`
- ✅ **WebSocket traffic** visible in browser dev tools
- ✅ **Console interception** working across environments
- ✅ **Auto-discovery** functioning via manifests

## 🎯 **Success Criteria**

### **No More Mock Testing for Cross-Environment Features**
- ❌ `MockSuccessTransport` for testing WebSocket communication
- ❌ Simulated browser environments for console interception
- ❌ Fake cross-context routing for command execution

### **Real Integration Validation**
- ✅ Actual server ↔ browser WebSocket communication
- ✅ Real console interception in browser → server transport
- ✅ Genuine cross-context command routing and execution
- ✅ Authentic file system operations (logs, screenshots)
- ✅ True auto-discovery via build-time manifests

### **Middle-Out Domain Coverage**
- ✅ **Environment Detection** (server vs browser)
- ✅ **Cross-Context Routing** (command delegation)
- ✅ **Transport Layer** (WebSocket communication)
- ✅ **File System Integration** (log/screenshot creation)
- ✅ **Auto-Discovery System** (manifest-based initialization)

---

**Result**: Tests that validate the actual system behavior across real environment boundaries, eliminating the gap between mocked tests and production reality.