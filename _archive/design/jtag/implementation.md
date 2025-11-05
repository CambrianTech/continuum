# JTAG Implementation Details

## 🔌 Connection Information for Autonomy

**ConnectCommand provides real session infrastructure for autonomous debugging:**

```json
{
  "success": true,
  "data": {
    "session": {
      "sessionId": "development-system-mcr60ted-b9s3p",
      "action": "joined_existing", 
      "launched": true,
      "logPaths": {
        "browser": ".continuum/sessions/user/system/development-system-mcr60ted-b9s3p/logs/browser.log",
        "server": ".continuum/sessions/user/system/development-system-mcr60ted-b9s3p/logs/server.log"
      },
      "directories": {
        "screenshots": ".continuum/sessions/user/system/development-system-mcr60ted-b9s3p/screenshots"
      },
      "interface": "http://localhost:9000",
      "commands": {
        "info": "continuum session-info development-system-mcr60ted-b9s3p",
        "stop": "continuum session-stop development-system-mcr60ted-b9s3p"
      }
    }
  }
}
```

## 🏗️ Complete JTAG Stack Requirements

### **Layer 1: Server Daemon Logging**
- **Daemon startup/shutdown**: All process lifecycle events
- **Command execution**: Request/response logging with timing
- **Health monitoring**: Heartbeat and status reporting
- **Error tracking**: Exception logging with stack traces

### **Layer 2: Browser Console Forwarding**
- **Console capture**: All browser console.log, error, warn messages
- **Real-time forwarding**: WebSocket-based log streaming
- **Context preservation**: Stack traces and source locations
- **UI event logging**: Widget interactions and state changes

### **Layer 3: Session Management**
- **Session-based logging**: Automatic per-session log files
- **Screenshot capture**: Visual validation and debugging
- **Command correlation**: Link browser actions to server commands
- **Performance monitoring**: Timing and resource usage

### **Layer 4: DevTools Integration**
- **Network monitoring**: API call tracking and analysis
- **Performance profiling**: CPU and memory usage
- **DOM inspection**: Element state and changes
- **Console interaction**: Direct debugging access

## 🎯 Portal Bridge Architecture

### **Browser-to-Server Communication**

```typescript
// Browser console capture
console.log = (function(originalLog) {
  return function(...args) {
    // Forward to server via WebSocket
    continuum.sendLog('info', args);
    return originalLog.apply(this, args);
  };
})(console.log);

// Server log correlation
server.on('browser-log', (sessionId, level, message, timestamp) => {
  correlateWithServerLogs(sessionId, timestamp, message);
});
```

### **Server-to-Browser Commands**

```typescript
// Server can trigger browser actions
server.sendCommand(sessionId, 'screenshot', { 
  path: 'debug-widget-loading.png',
  element: '#chat-widget'
});

// Browser executes and reports back
browser.executeCommand('screenshot').then(result => {
  server.log(`Screenshot captured: ${result.path}`);
});
```

## 🔄 Session-Based Logging

### **Automatic Session Creation**
```bash
# Session directories auto-created
.continuum/sessions/user/system/development-system-mcr60ted-b9s3p/
├── logs/
│   ├── browser.log          # All browser console logs
│   ├── server.log           # All daemon activity
│   └── correlation.log      # Cross-referenced events
├── screenshots/
│   ├── startup-001.png      # Automatic visual validation
│   ├── error-002.png        # Error state captures
│   └── debug-003.png        # Manual debugging snapshots
└── session-info.json        # Session metadata
```

### **Real-Time Log Monitoring**
```bash
# Monitor server activity
tail -f .continuum/sessions/*/logs/server.log

# Monitor browser activity  
tail -f .continuum/sessions/*/logs/browser.log

# Monitor correlated events
tail -f .continuum/sessions/*/logs/correlation.log
```

## 🚀 Command Discovery and Loading

### **Dynamic Command Registration**
```typescript
// Commands auto-discovered and loaded
const commands = await discoverCommands();
console.log(`Found ${commands.length} commands`);

// Real-time command availability
server.on('command-registered', (commandName, module) => {
  console.log(`✅ Command '${commandName}' available`);
});

server.on('command-failed', (commandName, error) => {
  console.log(`❌ Command '${commandName}' failed: ${error}`);
});
```

### **Command Execution Tracing**
```typescript
// All command execution automatically logged
server.executeCommand('health').then(result => {
  // Logged: [timestamp] Command 'health' → SUCCESS (150ms)
}).catch(error => {
  // Logged: [timestamp] Command 'health' → TIMEOUT (30s)
});
```

## 🔍 Error Handling and Recovery

### **Automatic Error Detection**
```typescript
// Browser error detection
window.addEventListener('error', (event) => {
  continuum.reportError({
    message: event.message,
    source: event.filename,
    line: event.lineno,
    column: event.colno,
    stack: event.error?.stack
  });
});

// Server error correlation
server.on('browser-error', (sessionId, error) => {
  // Correlate with recent server activity
  const recentLogs = getRecentServerLogs(sessionId, 5000);
  analyzeErrorContext(error, recentLogs);
});
```

### **Self-Healing Capabilities**
```typescript
// Automatic retry on common failures
if (error.message.includes('timeout')) {
  console.log('🔄 Retrying command with increased timeout...');
  await retryWithBackoff(command, { timeout: 60000 });
}

// Automatic daemon restart on failure
if (daemonHealth.status === 'unhealthy') {
  console.log('🚑 Restarting unhealthy daemon...');
  await restartDaemon(daemonHealth.name);
}
```

## 📊 Performance Monitoring

### **Real-Time Metrics**
```typescript
// Command execution timing
const startTime = Date.now();
const result = await executeCommand(name, args);
const duration = Date.now() - startTime;

console.log(`⏱️  Command '${name}' executed in ${duration}ms`);

// System resource monitoring
const metrics = await getSystemMetrics();
console.log(`💾 Memory: ${metrics.memory.used}/${metrics.memory.total}MB`);
console.log(`⚡ CPU: ${metrics.cpu.usage}%`);
```

### **Performance Alerting**
```typescript
// Automatic performance alerts
if (duration > 5000) {
  console.warn(`🐌 Slow command detected: '${name}' took ${duration}ms`);
  await capturePerformanceSnapshot(name);
}

if (metrics.memory.usage > 0.8) {
  console.warn(`🚨 High memory usage: ${metrics.memory.usage * 100}%`);
  await triggerMemoryCleanup();
}
```

The JTAG implementation provides complete system visibility and autonomous debugging capabilities through systematic logging, correlation, and self-healing mechanisms.