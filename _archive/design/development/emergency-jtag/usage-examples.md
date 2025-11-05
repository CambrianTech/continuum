# Universal Emergency JTAG - Usage Examples

## 🌐 **Browser Setup**
```typescript
import { UniversalEmergencyJTAG as JTAG } from './UniversalEmergencyJTAG';

// Initialize in browser context
JTAG.initialize('browser', {
  enableRemoteLogging: true,  // Send logs to server via port 9001
  jtagPort: 9001
});

// Browser debugging
JTAG.critical('FileSaveClient', 'SAVE_FAILED', { 
  command: 'file_save', 
  error: 'registry empty!' 
});

JTAG.trace('WebSocketManager', 'sendMessage', 'ENTER', { 
  type: 'execute_command',
  command: 'file_save' 
});
```

## 🚨 **Server Setup**  
```typescript
import { UniversalEmergencyJTAG as JTAG } from './UniversalEmergencyJTAG';

// Initialize in server context - starts JTAG server on port 9001
JTAG.initialize('server', {
  logDirectory: '.continuum/logs',
  jtagPort: 9001
});

// Server debugging
JTAG.critical('CommandRouter', 'FORWARDING_TO_HANDLER', { 
  type: 'command.execute',
  handlerName: 'command-executor' 
});

JTAG.probe('UniversalCommandRegistry', 'command_count', { 
  count: registry.size,
  commands: Array.from(registry.keys())
});
```

## 🔄 **Cross-Context Debugging Session**

### 1. **Start Emergency JTAG Server**
```typescript
// In your server startup
JTAG.initialize('server');
// Creates HTTP server on port 9001 to receive browser logs
```

### 2. **Initialize Browser JTAG**  
```typescript
// In your browser bundle
JTAG.initialize('browser');
// Sends logs to server via fetch() to port 9001
```

### 3. **Debug the Flow**
```typescript
// Browser side
JTAG.critical('BROWSER', 'SCREENSHOT_COMMAND_START', { filename: 'test.png' });
// ... screenshot logic ...
JTAG.critical('BROWSER', 'FILE_SAVE_ATTEMPT', { command: 'file_save' });

// Server side  
JTAG.critical('SERVER', 'WEBSOCKET_MESSAGE_RECEIVED', { type: 'command.execute' });
JTAG.critical('SERVER', 'ROUTING_TO_EXECUTOR', { handlerName: 'command-executor' });
```

### 4. **Check Unified Logs**
```bash
# All contexts combined
cat .continuum/logs/universal-emergency-jtag.log

# Critical events only
cat .continuum/logs/critical.emergency.log
```

## 🎯 **Real Debugging Scenario**

**Problem**: Browser file_save hitting "registry empty!"

**Emergency JTAG Investigation**:

```typescript
// 1. Browser FileSaveClient
JTAG.trace('FileSaveClient', 'saveFile', 'ENTER', { filename: 'test.png' });
JTAG.critical('FileSaveClient', 'CALLING_CONTINUUM_EXECUTE', { command: 'file_save' });

// 2. Browser WebSocket  
JTAG.trace('WebSocketManager', 'sendMessage', 'ENTER', { type: 'execute_command' });

// 3. Server WebSocket Handler
JTAG.trace('WebSocketDaemon', 'routeCommandToProcessor', 'ENTER', { command: 'file_save' });

// 4. Command Processor Wrapper
JTAG.critical('Wrapper', 'ROUTING_TO_NEW_ARCHITECTURE', { type: 'command.execute' });

// 5. Command Router
JTAG.critical('Router', 'FORWARDING_TO_HANDLER', { 
  type: 'command.execute', 
  handlerName: 'command-executor'  // Should be command-executor, not websocket-handler!
});

// 6. Command Executor  
JTAG.probe('CommandExecutor', 'REGISTRY_CHECK', { 
  registrySize: registry.size,
  hasFileSave: registry.has('file_save')
});
```

**Expected Log Flow**:
```
[BROWSER] FileSaveClient: CALLING_CONTINUUM_EXECUTE | {"command":"file_save"}
[BROWSER] WebSocketManager: sendMessage ENTER | {"type":"execute_command"}  
[SERVER] WebSocketDaemon: routeCommandToProcessor ENTER | {"command":"file_save"}
[SERVER] Wrapper: ROUTING_TO_NEW_ARCHITECTURE | {"type":"command.execute"}
[SERVER] Router: FORWARDING_TO_HANDLER | {"handlerName":"command-executor"}
[SERVER] CommandExecutor: REGISTRY_CHECK | {"registrySize":38,"hasFileSave":true}
```

## 🚀 **Advanced Features**

### **Log Correlation**
```typescript
// Generate correlation ID for cross-context tracing
const correlationId = `trace-${Date.now()}-${Math.random().toString(36)}`;

// Browser
JTAG.log('BROWSER', 'REQUEST_START', { correlationId, command: 'file_save' });

// Server (same correlationId)
JTAG.log('SERVER', 'REQUEST_PROCESSING', { correlationId, handler: 'command-executor' });
```

### **Performance Timing**
```typescript
const startTime = Date.now();
JTAG.trace('Component', 'expensiveOperation', 'ENTER');
// ... expensive operation ...
JTAG.trace('Component', 'expensiveOperation', 'EXIT', { 
  duration: Date.now() - startTime 
});
```

### **Error Aggregation**
```typescript
try {
  // risky operation
} catch (error) {
  JTAG.critical('Component', 'OPERATION_FAILED', {
    error: error.message,
    stack: error.stack,
    context: 'user-action'
  });
  throw error;
}
```

Universal Emergency JTAG gives you **one debugging API** that works identically across **browser and server**, with automatic log aggregation and correlation!