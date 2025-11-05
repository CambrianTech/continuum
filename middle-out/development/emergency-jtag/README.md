# Emergency JTAG - Bulletproof System Debugging

**Emergency JTAG** is a tricorder-like debugging system for deep system investigation when standard logging is broken.

## 🚨 **When to Use Emergency JTAG**

Emergency JTAG is your last resort debugging tool when:

- **Standard logging systems are broken**
- **WebSocket routing is failing** 
- **Daemon messaging is not working**
- **You need visibility into system internals**
- **Silent failures are masking real problems**

Think of it as a **deep-sea explorer's emergency radio** - when your main communication systems fail, you need a simple, bulletproof backup.

## 🎯 **Core Philosophy**

Emergency JTAG follows the **"worm digging deeper"** philosophy:
- **No dependencies** - writes directly to files with Node.js `fs`
- **Multiple outputs** - console.log + file logging for maximum visibility
- **Component-specific logs** - each component gets its own emergency log
- **Timestamped traces** - track execution flow through complex systems
- **Critical event capture** - flag important system state changes

## 📋 **API Reference**

### **Basic Logging**
```typescript
import { EmergencyJTAG } from './EmergencyJTAG';

// Simple component logging
EmergencyJTAG.log('WRAPPER', 'Processing message', { type: message.type });

// Critical system events
EmergencyJTAG.critical('ROUTER', 'ROUTING_FAILURE', { error: 'No handler found' });

// System state probes
EmergencyJTAG.probe('REGISTRY', 'command_count', { count: registry.size });
```

### **Function Tracing**
```typescript
// Trace function entry/exit
EmergencyJTAG.trace('DAEMON', 'handleMessage', 'ENTER', { type: message.type });
// ... function logic ...
EmergencyJTAG.trace('DAEMON', 'handleMessage', 'EXIT', { success: result.success });
```

### **Log Management**
```typescript
// Clear logs for clean debugging session
EmergencyJTAG.clearLogs();
```

## 📁 **Log File Structure**

Emergency JTAG creates multiple log files in `.continuum/logs/`:

- **`emergency-jtag.log`** - All emergency logs combined
- **`critical.emergency.log`** - Only critical system events  
- **`{component}.emergency.log`** - Component-specific logs (e.g., `wrapper.emergency.log`)

## 🔧 **Real-World Usage Example**

```typescript
// In CommandProcessorCompatibilityWrapper
protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
  EmergencyJTAG.trace('WRAPPER', 'handleMessage', 'ENTER', { 
    type: message.type, 
    migration: this.migrationEnabled,
    hasNewArch: this.hasNewArchitecture()
  });
  
  if (this.migrationEnabled && this.hasNewArchitecture()) {
    EmergencyJTAG.critical('WRAPPER', 'ROUTING_TO_NEW_ARCHITECTURE', { type: message.type });
    const result = await this.routeToNewArchitecture(message);
    EmergencyJTAG.trace('WRAPPER', 'handleMessage', 'EXIT', { success: result.success });
    return result;
  } else {
    EmergencyJTAG.critical('WRAPPER', 'FALLING_BACK_TO_LEGACY', { 
      type: message.type,
      migration: this.migrationEnabled 
    });
    return await this.routeToLegacySystem(message);
  }
}
```

## 🎮 **Debugging Game Philosophy**

Using Emergency JTAG is like playing a debugging game:

1. **Deploy your probes** - Add JTAG calls at key system points
2. **Run the system** - Execute the failing operation
3. **Read the traces** - Check `.continuum/logs/critical.emergency.log`
4. **Follow the trail** - Trace message flow through system components
5. **Find the break point** - Identify where the flow stops or diverges
6. **Deploy deeper probes** - Add more JTAG calls around the problem area
7. **Repeat until solved** - Keep digging deeper until you find the root cause

## 🚀 **Success Story**

Emergency JTAG successfully diagnosed a critical WebSocket routing issue:

**Problem**: Browser `file_save` commands hitting "registry empty!" error
**Investigation**: Added Emergency JTAG probes to trace message flow
**Discovery**: 
```
WRAPPER: ROUTING_TO_NEW_ARCHITECTURE | {"type":"command.execute"}
ROUTER: FORWARDING_TO_HANDLER | {"type":"command.execute","handlerName":"websocket-handler"}
```
**Root Cause**: WebSocketHandler was incorrectly registered for `command.execute` messages, stealing them from CommandExecutor
**Fix**: Remove duplicate registration, restore proper message routing

## 🛠️ **Extension Points**

Emergency JTAG can be extended with:
- **Performance timing** - Track execution duration
- **Memory usage tracking** - Monitor resource consumption  
- **Network request tracing** - Debug HTTP/WebSocket communications
- **Database query logging** - Track data access patterns
- **Error aggregation** - Collect and analyze failure patterns

## ⚠️ **Important Notes**

- **Remove Emergency JTAG calls** before production deployment
- **Use sparingly** - Only when standard debugging fails  
- **Clean up logs** - Call `clearLogs()` before debugging sessions
- **Performance impact** - File I/O can slow down tight loops

Emergency JTAG is your debugging Swiss Army knife - simple, reliable, and effective when everything else fails.