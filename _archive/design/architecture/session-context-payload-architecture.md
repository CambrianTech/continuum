# Session + Context Payload Architecture

## 🎯 **Universal Session & Context Propagation**

**Core Insight:** Every payload automatically carries both routing context (for environment delegation) and session identity (for cross-system accountability and sandboxing).

## 🏗️ **Architecture Overview**

### **JTAGPayload Enhancement:**
```typescript
export abstract class JTAGPayload {
  readonly context: JTAGContext;    // Environment routing identity  
  readonly sessionId: string;       // Cross-environment session identity
  
  constructor(context?: JTAGContext, sessionId?: string) {
    super();
    this.context = context || getCurrentContext();
    this.sessionId = sessionId || getCurrentSessionId();
  }
  
  // Existing encode/decode methods handle new fields automatically
}
```

### **Automatic Inheritance Chain:**
```
JTAGPayload (context + sessionId)
  ├─ CommandParams (all command inputs get context + session)
  ├─ CommandResult (all command outputs get context + session)  
  ├─ Event Payloads (all events get context + session)
  └─ Error Payloads (all errors get context + session)
```

## 🌐 **Distributed Session Architecture**

### **Local Session Flow:**
```
1. SessionDaemon creates session → generates sessionId
2. JTAGContext created with sessionId as UUID
3. All payloads inherit context + sessionId automatically
4. ArtifactoryDaemon uses sessionId for workspace routing
```

### **Remote Session Flow:**
```
1. Browser command executes with local context + sessionId
2. remoteExecute() sends payload with both fields to server/remote
3. Remote system receives full accountability chain:
   - payload.context: { uuid: "jtag_browser_123", environment: "browser" }
   - payload.sessionId: "sess-abc123"
4. Remote creates sandboxed workspace: .continuum/sessions/remote/jtag_browser_123/sess-abc123/
```

### **Cross-System Accountability:**
```typescript
// Every operation has complete traceability:
const screenshotResult = await remoteScreenshot(params);
// Result contains:
// {
//   context: { uuid: "jtag_server_456", environment: "server" },  // Where it executed
//   sessionId: "sess-abc123",                                      // Which session
//   success: true,
//   filepath: ".continuum/sessions/remote/jtag_browser_123/sess-abc123/screenshots/shot.png"
// }
```

## 🗂️ **Session Directory Structure**

### **Local Sessions:**
```
.continuum/sessions/
├── user/
│   └── joel-a1b2c3d4/
│       ├── logs/
│       │   ├── browser-sess-abc123.log
│       │   └── server-sess-abc123.log
│       ├── screenshots/
│       ├── chat/
│       └── uploads/
```

### **Remote Sessions (Sandboxed):**
```
.continuum/sessions/
├── remote/
│   ├── jtag_browser_123/          # Requesting system context UUID
│   │   └── sess-abc123/           # Session ID
│   │       ├── screenshots/
│   │       ├── logs/
│   │       └── artifacts/
│   └── jtag_laptop_789/           # Different requesting system
│       └── sess-def456/           # Different session
```

## 🔄 **Event System Integration**

### **Session Workflow Events:**
```typescript
export const SYSTEM_EVENTS = {
  // ... existing events ...
  SESSION_ESTABLISHING: 'system.session.establishing',
  SESSION_ESTABLISHED: 'system.session.established',
  // ... rest of events ...
} as const;

export class SessionEstablishedPayload extends JTAGPayload {
  entityRoot: string;
  workspacePaths: Record<string, string>;
  
  constructor(data: Partial<SessionEstablishedPayload>) {
    super(); // Automatically gets context + sessionId
    Object.assign(this, data);
  }
}
```

### **Daemon Initialization Dependencies:**
```
1. SessionDaemon + ArtifactoryDaemon start first
2. SessionDaemon creates session → emits SESSION_ESTABLISHED
3. Session-dependent daemons wait for SESSION_ESTABLISHED:
   - ConsoleDaemon (needs session for log routing)  
   - CommandDaemon (needs session for file operations)
   - HealthDaemon (needs session for health logs)
4. System emits READY when all daemons initialized
```

## 🛠️ **Implementation Benefits**

### **Zero Configuration Commands:**
```typescript
export class ScreenshotCommand extends CommandBase {
  async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
    // params automatically has context + sessionId
    // result automatically gets context + sessionId
    
    // Just focus on business logic:
    const imageData = await captureScreenshot(params.selector);
    
    return new ScreenshotResult({
      success: true,
      filepath: savedPath,
      // context + sessionId added automatically
    });
  }
}
```

### **Automatic File Operations:**
```typescript
// ArtifactoryDaemon routes files based on payload session
export class ArtifactoryDaemon extends DaemonBase {
  async handleSave(params: { relativePath: string, data: any }) {
    // params.sessionId and params.context available automatically
    const workspace = this.getSessionWorkspace(params.sessionId);
    const fullPath = path.join(workspace.root, params.relativePath);
    
    await fs.writeFile(fullPath, params.data);
    return { fullPath, sessionId: params.sessionId };
  }
}
```

### **Universal Error Traceability:**
```typescript
// All errors carry complete routing information
try {
  return await this.processCommand(params);
} catch (error) {
  return new ErrorResult({
    success: false,
    error: error.message,
    // context + sessionId automatically included for tracing
  });
}
```

## 🌟 **Architectural Elegance**

### **Single Point of Enhancement:**
- **One change** to `JTAGPayload` base class
- **All commands** get session + context automatically
- **All events** get session + context automatically  
- **All errors** get session + context automatically

### **Universal Distributed Computing:**
- **Local operations** use session for workspace routing
- **Remote operations** use session for sandboxed accountability
- **Error tracing** works across any number of system hops
- **Session persistence** works regardless of where operations execute

### **Developer Experience:**
- **Commands stay focused** on business logic
- **No manual session propagation** needed anywhere
- **Automatic distributed tracing** for debugging
- **Consistent behavior** across local/remote execution

## 🎯 **Next Steps**

1. **Enhance JTAGPayload** with context + sessionId fields
2. **Create SessionDaemon** as session coordinator  
3. **Update payload constructors** to use new base pattern
4. **Implement session workflow events** for daemon coordination
5. **Test distributed session flow** across environments

**This architecture enables seamless distributed session management with complete accountability while maintaining the elegance of automatic inheritance.**