# JTAG Debugging Protocol

## 📋 Systematic Debugging Protocol

### **Step 1: Monitor Connection Health**

```bash
# Check system status
continuum status

# Monitor real-time server logs
tail -f .continuum/sessions/*/logs/server.log

# Monitor browser console
tail -f .continuum/sessions/*/logs/browser.log

# Check daemon processes
ps aux | grep -E "(renderer|websocket|command-processor|browser-manager)"
```

### **Step 2: Identify Problem Patterns**

```bash
# Look for timeout patterns
grep -i "timeout\|timed out" .continuum/sessions/*/logs/server.log

# Check for connection issues
grep -i "connection\|websocket" .continuum/sessions/*/logs/server.log

# Find command execution failures
grep -i "failed\|error" .continuum/sessions/*/logs/server.log
```

### **Step 3: Correlate Browser + Server Events**

```typescript
// Example correlation analysis
const serverLogs = readServerLogs();
const browserLogs = readBrowserLogs();

// Find events within 500ms of each other
const correlatedEvents = correlateLogs(serverLogs, browserLogs, 500);

// Pattern: Browser action → Server timeout
// [Browser] 91.273s: Executing command 'chat_history'
// [Server]  91.274s: Command 'chat_history' received
// [Server]  91.304s: Command 'chat_history' timed out
```

### **Step 4: Visual Validation**

```bash
# Take screenshot for current state
continuum screenshot debug-state-$(date +%s).png

# Compare with reference screenshot
continuum screenshot-compare reference.png current.png
```

### **Step 5: Automated Recovery**

```typescript
// Self-healing decision tree
if (isTimeoutError(error)) {
  await retryWithIncreasedTimeout(command);
} else if (isConnectionError(error)) {
  await reconnectWebSocket();
} else if (isDaemonUnhealthy(error)) {
  await restartUnhealthyDaemon();
} else {
  await reportUnknownError(error);
}
```

## 🎯 Pattern Recognition Framework

### **Timeout Patterns**
```bash
# Common timeout signatures
"Command 'health' timed out"
"Command 'console' timed out"
"Command 'chat_history' timed out"

# Root cause analysis
- WebSocket connection issues
- Command processor overload
- Daemon communication breakdown
```

### **Connection Patterns**
```bash
# WebSocket connection issues
"WebSocket connection failed"
"Upgrade request failed"
"Connection reset by peer"

# Resolution strategies
- Restart WebSocket daemon
- Check port availability
- Verify daemon startup order
```

### **Command Discovery Patterns**
```bash
# Command loading failures
"Command not found: 'command_name'"
"Failed to load command module"
"Command discovery timeout"

# Resolution strategies
- Verify package.json in command directories
- Check TypeScript compilation
- Validate command module structure
```

## 🔄 Self-Healing Capabilities

### **Automatic Retry Logic**
```typescript
class AutoRetryHandler {
  async executeWithRetry(command: string, maxRetries: number = 3) {
    for (let i = 0; i < maxRetries; i++) {
      try {
        return await this.executeCommand(command);
      } catch (error) {
        if (i === maxRetries - 1) throw error;
        
        // Exponential backoff
        const delay = Math.pow(2, i) * 1000;
        await new Promise(resolve => setTimeout(resolve, delay));
        
        console.log(`🔄 Retry ${i + 1}/${maxRetries} for command '${command}'`);
      }
    }
  }
}
```

### **Daemon Health Monitoring**
```typescript
class DaemonHealthMonitor {
  async monitorDaemonHealth() {
    const daemons = ['renderer', 'websocket', 'command-processor'];
    
    for (const daemon of daemons) {
      const health = await this.checkDaemonHealth(daemon);
      
      if (health.status === 'unhealthy') {
        console.warn(`🚨 Daemon ${daemon} is unhealthy`);
        await this.restartDaemon(daemon);
      }
    }
  }
  
  async restartDaemon(daemonName: string) {
    console.log(`🚑 Restarting daemon: ${daemonName}`);
    
    // Graceful shutdown
    await this.stopDaemon(daemonName);
    
    // Wait for cleanup
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Restart
    await this.startDaemon(daemonName);
    
    console.log(`✅ Daemon ${daemonName} restarted successfully`);
  }
}
```

## 🔍 Autonomous Problem-Solving Workflow

### **Phase 1: Problem Detection**
```typescript
// Continuous monitoring
const monitor = new SystemMonitor();

monitor.on('error', (error) => {
  console.log(`🚨 Error detected: ${error.message}`);
  this.triggerDiagnostics(error);
});

monitor.on('performance-degradation', (metrics) => {
  console.log(`⚠️  Performance degradation detected`);
  this.optimizeSystemResources(metrics);
});
```

### **Phase 2: Root Cause Analysis**
```typescript
async function analyzeError(error: SystemError) {
  const recentLogs = await getRecentLogs(5000); // Last 5 seconds
  const relatedEvents = correlateLogs(error.timestamp, recentLogs);
  
  // Pattern matching
  if (relatedEvents.some(e => e.includes('timeout'))) {
    return 'timeout-issue';
  } else if (relatedEvents.some(e => e.includes('connection'))) {
    return 'connection-issue';
  } else if (relatedEvents.some(e => e.includes('memory'))) {
    return 'memory-issue';
  }
  
  return 'unknown-issue';
}
```

### **Phase 3: Automated Resolution**
```typescript
async function resolveIssue(issueType: string, context: any) {
  const resolver = new IssueResolver();
  
  switch (issueType) {
    case 'timeout-issue':
      return await resolver.resolveTimeout(context);
    case 'connection-issue':
      return await resolver.resolveConnection(context);
    case 'memory-issue':
      return await resolver.resolveMemory(context);
    default:
      return await resolver.escalateToHuman(context);
  }
}
```

### **Phase 4: Verification and Learning**
```typescript
async function verifyResolution(issue: ResolvedIssue) {
  // Test the fix
  const testResult = await runSystemTests();
  
  if (testResult.success) {
    console.log(`✅ Issue resolved: ${issue.type}`);
    
    // Learn from successful resolution
    await recordSuccessfulResolution(issue);
  } else {
    console.log(`❌ Resolution failed: ${issue.type}`);
    
    // Try alternative resolution
    await tryAlternativeResolution(issue);
  }
}
```

## 📊 Debug Session Analysis

### **Session Metrics Collection**
```typescript
class DebugSessionMetrics {
  private metrics = {
    commandExecutions: 0,
    timeouts: 0,
    errors: 0,
    resolutions: 0,
    averageResponseTime: 0
  };
  
  recordCommandExecution(command: string, duration: number) {
    this.metrics.commandExecutions++;
    this.updateAverageResponseTime(duration);
  }
  
  recordTimeout(command: string) {
    this.metrics.timeouts++;
    console.log(`⏱️  Timeout recorded for: ${command}`);
  }
  
  recordError(error: Error) {
    this.metrics.errors++;
    console.log(`❌ Error recorded: ${error.message}`);
  }
  
  recordResolution(issue: string) {
    this.metrics.resolutions++;
    console.log(`✅ Resolution recorded: ${issue}`);
  }
}
```

### **Learning from Debug Sessions**
```typescript
async function analyzeDebugSession(sessionId: string) {
  const logs = await getSessionLogs(sessionId);
  const patterns = await extractPatterns(logs);
  
  // Identify common failure modes
  const failurePatterns = patterns.filter(p => p.type === 'failure');
  const resolutionPatterns = patterns.filter(p => p.type === 'resolution');
  
  // Update resolution strategies
  for (const failure of failurePatterns) {
    const resolution = findMatchingResolution(failure, resolutionPatterns);
    if (resolution) {
      await updateResolutionStrategy(failure.signature, resolution.strategy);
    }
  }
}
```

## 🧪 JTAG Validation Path (Middle-Out Testing)

### **⚠️ CRITICAL: Basic Steps That Must NEVER Be Forgotten**

**FUNDAMENTAL REBUILD & VALIDATION STEPS** (Write these down because Claude keeps forgetting):

1. **ALWAYS** run `npm run build:browser-ts` after ANY TypeScript changes
2. **ALWAYS** run `continuum reload --target=page` after rebuild  
3. **ALWAYS** check version in browser: `window.__CONTINUUM_VERSION__`
4. **ALWAYS** check browser logs are capturing: look for console messages in `.continuum/sessions/*/logs/browser.log`
5. **NEVER** assume "command completed" means JavaScript executed - CHECK THE BROWSER LOGS
6. **ALWAYS** kill and restart daemons after major changes: `kill $(ps aux | grep main.ts | awk '{print $2}') && continuum connect`

**VALIDATION CHECKLIST** (Must complete before claiming anything works):
- [ ] Built browser TypeScript: `npm run build:browser-ts` 
- [ ] Reloaded browser: `continuum reload --target=page`
- [ ] Verified version updated in browser
- [ ] Verified browser console logs are being captured
- [ ] Tested actual JavaScript execution with UUID in browser logs
- [ ] Confirmed UUIDs appear in browser.log file

### **UUID-Based Foundation Testing**

**Core Principle**: Start with simplest possible JTAG test - UUID generation and console logging - before advancing to complex features.

### **Phase 1: UUID Console Logging (Foundation)**
```bash
# Generate test UUID
TEST_UUID=$(python -c "import uuid; print(uuid.uuid4())")

# Execute JavaScript to log UUID with different levels
continuum js-execute "console.log('JTAG-UUID-TEST: $TEST_UUID'); console.warn('JTAG-UUID-WARN: $TEST_UUID'); console.error('JTAG-UUID-ERROR: $TEST_UUID');"

# Verify UUID appears in browser logs
grep "$TEST_UUID" .continuum/sessions/*/logs/browser.log

# 100% VALIDATION: All 3 log levels must show UUID in logs
```

**Success Criteria**: 
- ✅ js-execute command executes without error
- ✅ UUID appears in browser.log with console.log prefix
- ✅ UUID appears in browser.log with console.warn prefix  
- ✅ UUID appears in browser.log with console.error prefix
- ✅ All UUIDs parseable from logs

### **Phase 2: Error Generation Testing**
```bash
# Test intentional JavaScript error with UUID tracking
TEST_UUID=$(python -c "import uuid; print(uuid.uuid4())")
continuum js-execute "console.log('JTAG-ERROR-TEST: $TEST_UUID'); throw new Error('JTAG-INTENTIONAL-ERROR: $TEST_UUID');"

# Verify error UUID appears in logs
grep "JTAG-INTENTIONAL-ERROR.*$TEST_UUID" .continuum/sessions/*/logs/browser.log
```

**Success Criteria**:
- ✅ JavaScript error generated intentionally
- ✅ Error UUID traceable in logs
- ✅ Error doesn't crash browser or session

### **Phase 3: Log Correlation Testing**
```bash
# Test server-browser log correlation with UUID
TEST_UUID=$(python -c "import uuid; print(uuid.uuid4())")
continuum js-execute "console.log('JTAG-CORRELATION: $TEST_UUID'); window.location.href;"

# Verify UUID appears in both browser and server logs
grep "$TEST_UUID" .continuum/sessions/*/logs/browser.log
grep "$TEST_UUID" .continuum/sessions/*/logs/server.log
```

**Success Criteria**:
- ✅ UUID appears in browser logs
- ✅ Command execution logged in server logs with same timestamp
- ✅ Session correlation working between browser and server

### **Phase 4: Screenshot Validation** (Only after Phases 1-3 pass)
```bash
# Test screenshot with UUID marker
TEST_UUID=$(python -c "import uuid; print(uuid.uuid4())")
continuum js-execute "document.body.innerHTML += '<div id=\"jtag-marker\">JTAG-SCREENSHOT: $TEST_UUID</div>';"
continuum screenshot "jtag-test-$TEST_UUID.png"

# Verify screenshot contains UUID marker
# (Manual verification or image analysis)
```

### **Current Status** (2025-07-06):
- ❌ **Phase 1**: js-execute failing with "Script parameter is required"  
- ⏸️ **Phase 2-4**: Blocked until Phase 1 passes
- 🔍 **Investigation**: Parameter mapping issue in command execution

### **Debugging Phase 1**:
```python
# Current test - failing with parameter error
await ws.send({
    'type': 'execute_command', 
    'data': {'command': 'js-execute', 'args': {'script': js_code}, 'clientId': 'test'}
})
# Response: "Script parameter is required"
```

**Next Steps**: Fix parameter passing in js-execute command before advancing to Phase 2.

**RULE**: Do not proceed to next phase until current phase has 100% validation.

The JTAG debugging protocol provides a systematic approach to identifying, analyzing, and resolving issues through automated monitoring, pattern recognition, and self-healing capabilities.