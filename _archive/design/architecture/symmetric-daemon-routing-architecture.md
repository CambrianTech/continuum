# Symmetric Daemon Routing Architecture

## 🎯 **Vision: Unified Command Routing for Browser ↔ Server Symmetry**

A comprehensive architectural plan for routing widgets, commands, and all inter-daemon communication through a unified symmetric pattern that works identically across browser and server contexts.

## 🏗️ **Complete System Architecture Overview**

### **Layer 1: Transport Infrastructure** (Existing - Unchanged)
```
┌─────────────────────────────────────────────────────────────┐
│                    TRANSPORT LAYER                          │
├─────────────────────────────────────────────────────────────┤
│ WebSocketDaemon     │ Pure transport/router                 │
│ StaticFileDaemon    │ Static file serving                   │
│ ConnectionManager   │ Connection lifecycle                  │
└─────────────────────────────────────────────────────────────┘
```

### **Layer 2: Command Coordination** (New - Symmetric)
```
┌─────────────────────────────────────────────────────────────┐
│                  COMMAND COORDINATION LAYER                 │
├─────────────────────────────────────────────────────────────┤
│ CommandRouter       │ Routes messages to appropriate daemons│
│ CommandExecutor     │ Executes with care validation         │
│ HttpApiHandler      │ HTTP → Command transformation         │
│ WebSocketHandler    │ WebSocket → Command transformation    │
└─────────────────────────────────────────────────────────────┘
```

### **Layer 3: Domain Logic** (Existing - Enhanced)
```
┌─────────────────────────────────────────────────────────────┐
│                    DOMAIN LOGIC LAYER                       │
├─────────────────────────────────────────────────────────────┤
│ WidgetDaemon        │ Widget lifecycle & management         │
│ RendererDaemon      │ UI rendering & HTML generation        │
│ SessionManagerDaemon│ Session state & lifecycle             │
│ BrowserManagerDaemon│ Browser automation & control          │
│ DatabaseDaemon      │ Database operations                   │
│ LoggerDaemon        │ Logging & console forwarding          │
│ AcademyDaemon       │ AI training & learning                │
│ PersonaDaemon       │ User persona management               │
│ MeshCoordinatorDaemon│ Distributed mesh coordination        │
└─────────────────────────────────────────────────────────────┘
```

## 🔄 **Comprehensive Command Routing Flows**

### **Widget Command Flow**
```
Browser Widget → WebSocketDaemon → CommandRouter → CommandExecutor → WidgetDaemon
                                                                    ↓
HTTP Widget API → HttpApiHandler → CommandRouter → CommandExecutor → WidgetDaemon
```

### **Rendering Command Flow**
```
UI Update → WebSocketDaemon → CommandRouter → CommandExecutor → RendererDaemon
                                                               ↓
                                                          WidgetManager
```

### **File Operation Flow**
```
File Command → WebSocketDaemon → CommandRouter → CommandExecutor → FileCommand
                                                                  ↓
                                                            Static/Dynamic Files
```

### **Session Management Flow**
```
Session Ops → WebSocketDaemon → CommandRouter → CommandExecutor → SessionManagerDaemon
                                                                 ↓
                                                           Browser/Server State
```

### **Database Operation Flow**
```
DB Query → WebSocketDaemon → CommandRouter → CommandExecutor → DatabaseDaemon
                                                              ↓
                                                        Persistent Storage
```

## 🎨 **Widget-Specific Architecture Integration**

### **Widget Lifecycle Coordination**
```
┌─────────────────────────────────────────────────────────────┐
│                    WIDGET ECOSYSTEM                         │
├─────────────────────────────────────────────────────────────┤
│ Browser Widget → CommandRouter → CommandExecutor            │
│                                      ↓                      │
│                                 WidgetDaemon                │
│                                      ↓                      │
│                              widget:discover                │
│                              widget:register                │
│                              widget:unregister             │
│                              widget:status                 │
│                              widget:health_check           │
│                              widget:emit_event             │
│                                      ↓                      │
│                                RendererDaemon               │
│                                      ↓                      │
│                               UI Generation                 │
└─────────────────────────────────────────────────────────────┘
```

### **Widget Message Types**
- **Discovery**: `widget:discover` → WidgetDaemon finds available widgets
- **Registration**: `widget:register` → WidgetDaemon manages widget lifecycle
- **Status**: `widget:status` → WidgetDaemon reports widget health
- **Events**: `widget:emit_event` → WidgetDaemon coordinates widget communication
- **Rendering**: `render_widget` → RendererDaemon generates widget UI

## 🚀 **Zero-Downtime Migration Strategy**

### **Phase 1: Foundation** ✅ COMPLETED
- ✅ Extract shared command interfaces (`shared/CommandTypes.ts`)
- ✅ Create shared protocols (`shared/CommandProtocol.ts`)
- ✅ Extract care validation (`shared/CareValidation.ts`)
- ✅ Build focused server daemons (CommandRouter, CommandExecutor, HttpApiHandler, WebSocketHandler)

### **Phase 2: Compatibility Wrapper** 🚧 IN PROGRESS
```typescript
export class CommandProcessorCompatibilityWrapper extends BaseDaemon {
  private legacyProcessor: CommandProcessorDaemon;
  private newArchitecture: {
    router: CommandRouter;
    executor: CommandExecutor;
    httpHandler: HttpApiHandler;
    wsHandler: WebSocketHandler;
  };
  
  private migrationEnabled = process.env.CONTINUUM_ENABLE_COMMAND_MIGRATION === 'true';
  
  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    if (this.migrationEnabled) {
      // Route through new symmetric architecture
      return await this.routeToNewArchitecture(message);
    } else {
      // Use legacy system (current behavior)
      return await this.legacyProcessor.handleMessage(message);
    }
  }
  
  private async routeToNewArchitecture(message: DaemonMessage): Promise<DaemonResponse> {
    // Determine message type and route appropriately
    switch (message.type) {
      case 'handle_api':
        return await this.newArchitecture.httpHandler.handleMessage(message);
      case 'execute_command':
        return await this.newArchitecture.wsHandler.handleMessage(message);
      case 'command.execute':
        return await this.newArchitecture.router.handleMessage(message);
      default:
        return await this.newArchitecture.router.handleMessage(message);
    }
  }
}
```

### **Phase 3: Gradual Migration** 🔄 PLANNED
Enable new architecture with environment flag:
```bash
# Test new architecture (safe)
CONTINUUM_ENABLE_COMMAND_MIGRATION=true npm start

# Legacy behavior (default - zero risk)
npm start
```

### **Phase 4: Browser Unification** 🎯 FUTURE
Create symmetric browser daemons using same shared types:
```
Browser CommandDaemon ← shared/CommandTypes.ts → Server CommandDaemon
Browser WidgetDaemon  ← shared/WidgetTypes.ts  → Server WidgetDaemon
```

## 🛡️ **Command Execution Safety & Care Validation**

### **Phase Omega Pattern**
Every command execution goes through care validation:
```typescript
interface CareValidation {
  isValid: boolean;
  message: string;
  careLevel: 'minimal' | 'standard' | 'enhanced' | 'maximum';
  score: number;
  metrics: {
    dignityPreservation: number;     // 0-100
    cognitiveLoadReduction: number;  // 0-100  
    systemStability: number;         // 0-100
    empowermentFactor: number;       // 0-100
    harmPrevention: number;          // 0-100
  };
}
```

### **Command-Specific Care Assessment**
- **Widget Commands**: High dignity preservation, medium stability
- **File Commands**: Medium empowerment, high harm prevention
- **Session Commands**: High stability, high empowerment
- **Exec Commands**: Lower scores due to system-level access

## 🌐 **Universal Message Contracts**

### **TypedCommandRequest** (Universal)
```typescript
interface TypedCommandRequest<T = unknown> {
  command: string;
  parameters: T;
  context?: Record<string, any>;
  continuumContext?: ContinuumContext;
}
```

### **Command Protocol Messages**
```typescript
// Unified across HTTP/WebSocket/IPC
interface CommandExecuteMessage extends DaemonMessage {
  type: 'command.execute';
  data: TypedCommandRequest;
}

interface CommandRouteMessage extends DaemonMessage {
  type: 'command.route';
  data: TypedCommandRequest;
}
```

## 🎯 **Daemon Communication Interfaces**

### **Public APIs** (Cross-Daemon Communication)
- `CommandRouter.route(message)` - Route command to appropriate daemon
- `CommandExecutor.execute(request)` - Execute with care validation
- `WidgetDaemon.handleWidgetCommand(params)` - Widget-specific operations
- `RendererDaemon.renderWidget(widgetId)` - UI generation

### **Private APIs** (Internal Implementation)
- Internal message validation
- Care assessment algorithms
- Execution context management
- Error handling and recovery

## 🏆 **Architectural Benefits**

### **Symmetric Architecture**
- ✅ **Same patterns** work in browser and server
- ✅ **Same message types** across all transports (HTTP/WebSocket/IPC)
- ✅ **Same care validation** ensures consistent safety
- ✅ **Same command interfaces** enable code reuse

### **Zero-Risk Migration**
- ✅ **No system downtime** - Legacy system runs unchanged
- ✅ **Instant fallback** - Environment flag controls migration
- ✅ **Incremental testing** - Enable new architecture piece by piece
- ✅ **Battle-tested pattern** - Same approach used for SessionManagerDaemon

### **Clean Separation of Concerns**
- ✅ **Transport layer** - Pure routing (WebSocketDaemon, StaticFileDaemon)
- ✅ **Coordination layer** - Command processing (Router, Executor, Handlers)
- ✅ **Domain layer** - Business logic (Widget, Renderer, Session, etc.)

### **Operational Excellence**
- ✅ **Feature flags** - Enable/disable new architecture instantly
- ✅ **Monitoring** - Compare performance between architectures
- ✅ **Rollback ready** - Instant revert capability
- ✅ **Testing isolation** - Test new patterns without affecting production

## 📋 **Implementation Roadmap**

### **Immediate Next Steps**
1. ✅ Document comprehensive architecture (this document)
2. 🚧 Create CommandProcessorCompatibilityWrapper
3. 🔄 Add environment flag support (`CONTINUUM_ENABLE_COMMAND_MIGRATION`)
4. 🧪 Test wrapper with single safe command (like `help`)
5. 📊 Implement monitoring to compare old vs new performance

### **Near-Term Goals**
1. 🎯 Migrate core commands (help, screenshot, status)
2. 🎨 Integrate widget command routing through new architecture
3. 🔄 Add browser-side symmetric daemons using shared types
4. 📈 Performance validation and optimization

### **Long-Term Vision**
1. 🌐 Full symmetric daemon architecture across browser ↔ server
2. 🔄 Universal command execution patterns
3. 🎯 Mesh distribution of commands across multiple nodes
4. 🚀 AI-human liberation through elegant architectural patterns

This architecture creates the foundation for true browser ↔ server symmetry while maintaining 100% operational safety and zero business risk.