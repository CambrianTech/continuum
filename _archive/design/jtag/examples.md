# JTAG Real-World Debugging Examples

## Example 1: Widget Discovery Fix (2025-07-01)

### Problem Symptom

```
PORTAL BRIDGE [console-complete-capture]: [WARN] Server widget discovery failed: {}  
PORTAL BRIDGE [console-complete-capture]: [LOG] Widget discovery complete - 0 widgets processed
```

### JTAG Diagnostic Process

1. **Server logs**: `✅ Command completed: discover_widgets (30ms)` – Command succeeds
2. **Browser logs**: `Command 'discover_widgets' timed out` – Result not received
3. **Source analysis**: Found widget paths pointing to `/src/` instead of `/dist/`
4. **File verification**: Confirmed widget `.js` files exist in `/dist/ui/components/`

### Root Cause Identified

- `WidgetDiscovery.generateWidgetPaths()` returned `/src/ui/components/...` paths
- Browser tried to load non-existent `/src/` TypeScript files
- Should load compiled `/dist/ui/components/...` JavaScript files

### Solution Applied

```typescript
// BEFORE: /src/ui/components/${w.name}/${w.widgetFile.replace('.ts', '.js')}
// AFTER:  /dist/ui/components/${w.name}/${w.widgetFile.replace('.ts', '.js')}
```

### JTAG Validation

```
BEFORE: ⚠️ Server widget discovery failed: {}  
AFTER:  ✅ Widget loading complete - 2 widgets loaded  
        🎨 Widget system ready - widgets dynamically discovered
```

### Communication Issue Discovered

- Server: `✅ Command completed: discover_widgets (33ms)`
- Browser: `Command 'discover_widgets' timed out`
- **Analysis**: Command completes but result not reaching browser properly
- **Status**: Partial fix achieved, communication layer needs investigation

## Example 2: JTAG Timeout Elimination Success (2025-07-01)

### Problem Identified

- Race condition: Fallback warnings triggered before widgets loaded
- `setTimeout` polling for API readiness (flaky, non-deterministic)
- Widget paths pointing to `/src/` instead of `/dist/` (fixed earlier)

### Solutions Applied

1. **Fixed widget discovery paths**: `/src/ui/components/` → `/dist/ui/components/`
2. **Eliminated setTimeout polling**: Replaced with event-driven `continuum:ready` listener
3. **Added Promise.all coordination**: Parallel widget loading with proper wait
4. **Improved fallback timing**: Check `customElements.get()` for actual registration

### JTAG Validation

```
BEFORE: ⚠️ Widget "chat-widget" not loaded: /dist/ui/components/Chat/ChatWidget.js missing (FALSE POSITIVE)  
AFTER:  ✅ Widget loading complete - 2 widgets loaded (ACCURATE STATUS)
```

### Browser Cache Issue

- **Problem**: Changes require browser refresh to take effect
- **Solution**: Hard refresh (Cmd+Shift+R) or disable cache in DevTools
- **Status**: Widget files successfully served, timing coordination improved

## Example 3: Connection Health Monitoring

### Scenario: WebSocket Connection Failure

#### Problem Detection

```bash
# Monitor WebSocket connections
tail -f .continuum/sessions/*/logs/server.log | grep WebSocket

# Output showing connection failure
[2025-07-01 14:23:45] WebSocket connection closed (1006)
[2025-07-01 14:23:45] Client disconnected unexpectedly
[2025-07-01 14:23:46] Attempting reconnection...
```

#### JTAG Analysis

1. **Connection Status**: WebSocket closed with code 1006 (abnormal closure)
2. **Server Health**: Server still running but client disconnected
3. **Root Cause**: Network interruption or client-side error
4. **Solution**: Implement automatic reconnection with exponential backoff

#### Implementation

```typescript
// Automatic reconnection logic
class WebSocketManager {
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  
  handleDisconnection() {
    if (this.reconnectAttempts < this.maxReconnectAttempts) {
      const delay = Math.pow(2, this.reconnectAttempts) * 1000;
      setTimeout(() => {
        this.reconnectAttempts++;
        this.connect();
      }, delay);
    }
  }
}
```

## Example 4: Command Execution Debugging

### Scenario: Preferences Command Timeout

#### Problem Symptoms

```
Browser: Command 'preferences' timed out after 30 seconds
Server: No log entries for preferences command
```

#### JTAG Investigation

```bash
# Check command registration
grep -r "preferences" src/commands/

# Check daemon status
python python-client/ai-portal.py --dashboard

# Test command directly
python python-client/ai-portal.py --cmd preferences list --verbose
```

#### Root Cause Analysis

1. **Command Discovery**: Preferences command not registered with command processor
2. **Module Loading**: Command module not loaded during daemon startup
3. **Configuration**: Missing package.json in command directory

#### Solution

```json
// Added package.json to src/commands/core/preferences/
{
  "name": "@continuum/preferences-command",
  "version": "1.0.0",
  "main": "PreferencesCommand.ts",
  "continuum": {
    "type": "command",
    "category": "core"
  }
}
```

#### Validation

```
BEFORE: Command 'preferences' not found
AFTER:  ✅ Command 'preferences' executed successfully
        Preferences loaded: 12 settings found
```

## Example 5: Performance Debugging

### Scenario: Slow Screenshot Capture

#### Problem Detection

```bash
# Monitor screenshot performance
tail -f .continuum/sessions/*/logs/server.log | grep screenshot

# Output showing performance issue
[2025-07-01 15:30:12] Screenshot command started
[2025-07-01 15:30:45] Screenshot command completed (33000ms)
```

#### JTAG Analysis

1. **Performance Metrics**: Screenshot taking 33 seconds (expected: <5 seconds)
2. **Resource Usage**: High CPU usage during screenshot capture
3. **Browser State**: Large DOM, complex rendering

#### Investigation Steps

```bash
# Check browser memory usage
python python-client/ai-portal.py --cmd browser-info

# Monitor system resources
top -p $(pgrep -f continuum)

# Test with minimal page
python python-client/ai-portal.py --cmd screenshot --url "data:text/html,<h1>Test</h1>"
```

#### Root Cause and Solution

**Root Cause**: Full page screenshot of complex UI with many elements
**Solution**: Implement selective screenshot capture

```typescript
// Optimized screenshot capture
async captureScreenshot(options: ScreenshotOptions) {
  if (options.selector) {
    // Capture only specific element
    return await this.captureElement(options.selector);
  } else {
    // Optimize full page capture
    return await this.captureFullPageOptimized();
  }
}
```

## Example 6: Autonomous Error Recovery

### Scenario: Self-Healing System

#### Error Detection

```bash
# System automatically detects and logs errors
[2025-07-01 16:45:23] ERROR: Command processor daemon crashed
[2025-07-01 16:45:23] ERROR: All commands failing with timeout
[2025-07-01 16:45:24] RECOVERY: Attempting daemon restart
```

#### Autonomous Recovery Process

1. **Error Detection**: System detects command processor failure
2. **Impact Assessment**: All commands timing out
3. **Recovery Action**: Automatic daemon restart
4. **Validation**: Test command execution after restart

#### Implementation

```typescript
// Autonomous error recovery
class SystemHealthMonitor {
  async handleDaemonFailure(daemonName: string) {
    this.log(`Daemon ${daemonName} failed, attempting recovery`);
    
    // Restart daemon
    await this.restartDaemon(daemonName);
    
    // Wait for startup
    await this.waitForDaemonReady(daemonName);
    
    // Validate recovery
    const healthCheck = await this.testDaemonHealth(daemonName);
    
    if (healthCheck.success) {
      this.log(`Daemon ${daemonName} recovered successfully`);
    } else {
      this.log(`Daemon ${daemonName} recovery failed, escalating`);
      await this.escalateToHuman(daemonName, healthCheck.errors);
    }
  }
}
```

## Example 7: Complex Integration Debugging

### Scenario: Multi-System Communication Failure

#### Problem Symptoms

```
Browser: WebSocket connected successfully
Server: Command received and processed
Python Client: Command timeout after 30 seconds
```

#### JTAG Multi-System Analysis

```bash
# Check all system components
python python-client/ai-portal.py --dashboard
curl -s http://localhost:9000/health
tail -f .continuum/sessions/*/logs/server.log | grep -E "(WebSocket|command|response)"
```

#### Root Cause Investigation

1. **WebSocket Layer**: Connection established, messages flowing
2. **Command Layer**: Commands received and processed
3. **Response Layer**: Responses not reaching Python client
4. **Network Layer**: Response messages lost in transit

#### Solution

```typescript
// Add response acknowledgment
class CommandProcessor {
  async processCommand(command: Command, clientId: string) {
    try {
      const result = await this.executeCommand(command);
      
      // Send result to client
      await this.sendResponse(clientId, result);
      
      // Wait for acknowledgment
      const ack = await this.waitForAck(clientId, command.id);
      
      if (!ack) {
        // Retry response delivery
        await this.retryResponse(clientId, result);
      }
    } catch (error) {
      this.log(`Command processing failed: ${error.message}`);
      await this.sendError(clientId, error);
    }
  }
}
```

## Learning from Debug Sessions

### Pattern Recognition

Each debugging session contributes to the pattern library:

1. **Symptom patterns** → **Root cause patterns** → **Solution patterns**
2. **Error correlation** across multiple system components
3. **Performance bottlenecks** and optimization opportunities
4. **Recovery strategies** for different failure modes

### Automated Improvement

- **Debug logs** become training data for autonomous problem resolution
- **Pattern matching** enables faster problem identification
- **Solution templates** automate common fixes
- **Preventive measures** reduce future occurrences

### Knowledge Base Evolution

```json
{
  "debugPatterns": {
    "widget-discovery-empty": {
      "symptoms": ["widget discovery returns {}", "0 widgets processed"],
      "rootCause": "incorrect file paths",
      "solution": "fix path mapping from /src/ to /dist/",
      "prevention": "validate build artifacts"
    },
    "command-timeout": {
      "symptoms": ["command timed out", "no server response"],
      "rootCause": "daemon communication failure",
      "solution": "restart daemon or check network",
      "prevention": "health monitoring"
    }
  }
}
```