# Session Daemon Migration Plan

## 🎯 **Migration Overview**

**Source**: `/src/daemons/session-manager/`  
**Target**: `/src/debug/jtag/daemons/session-daemon/`  
**Status**: Planning → Implementation

## 📋 **Legacy System Analysis**

### **Current Architecture**
```
src/daemons/session-manager/
├── SessionManagerDaemon.ts           # Main daemon (BaseDaemon)
├── SessionManagerCompatibilityWrapper.ts
├── client/
│   ├── ClientSessionDaemon.ts        # Client-side session management
│   ├── SessionManagerClient.ts       # Client API
│   └── WebSocketRoutingClient.ts     # Client routing
├── server/
│   ├── ModularSessionManagerDaemon.ts
│   ├── SessionManagerDaemon.ts       # Server implementation
│   └── WebSocketRoutingService.ts    # Server routing
├── shared/
│   ├── SessionManagerTypes.ts        # 310 lines - excellent types!
│   ├── SessionMessageTypes.ts        # Message definitions
│   └── WebSocketRoutingTypes.ts      # Routing types
├── handlers/
│   ├── RemoteExecutionHandler.ts     # Remote command execution
│   └── SendToSessionHandler.ts       # Session message routing
├── modules/
│   ├── ConnectionOrchestrator.ts     # Connection management
│   ├── SessionConsoleLogger.ts       # Console integration
│   └── SessionConsoleManager.ts      # Console management
└── services/
    ├── CleanupService.ts             # Session cleanup
    └── DirectoryService.ts           # Directory management
```

### **Key Capabilities**
1. **Session Isolation** - Separate contexts for different users/personas/processes
2. **Artifact Management** - Organized storage (logs, screenshots, files, recordings)
3. **Process Tracking** - Browser PIDs, DevTools tabs, cleanup management
4. **Connection Identity** - `portal`, `validation`, `user`, `persona` connection types
5. **Auto-cleanup** - Configurable session lifecycle management
6. **Session Fork/Join** - Session inheritance and sharing
7. **Remote Execution** - Safe command execution within session contexts
8. **Real-time Events** - Session created/joined/stopped/cleanup events

### **Clean Design Patterns**
- **Excellent types** in `SessionManagerTypes.ts` (310 lines of clean interfaces)
- **Event factory pattern** - `SessionEvents.create()`, `SessionEvents.created()`
- **Path utilities** - `SessionPaths.generateBasePath()`, organized artifact storage
- **Validation utilities** - `SessionValidation.validateSessionRequest()`
- **Utility functions** - `SessionUtils.needsCleanup()`, age tracking

## 🏗️ **JTAG Migration Strategy**

### **Architecture Mapping**
```typescript
// Legacy → JTAG
BaseDaemon → DaemonBase (extends JTAGModule)
DaemonMessage → JTAGMessage<SessionCommandParams>
DaemonResponse → SessionCommandResult
EventTypes → SessionEventPayload (extends JTAGPayload)
```

### **Command/Event Split**
**Commands** (request/response):
- `CreateSessionParams/Result` - Create new session
- `JoinSessionParams/Result` - Join existing session  
- `GetSessionParams/Result` - Get session info
- `StopSessionParams/Result` - Stop session
- `ForkSessionParams/Result` - Fork from existing session
- `CleanupSessionParams/Result` - Cleanup session
- `ListSessionsParams/Result` - List sessions with filters

**Events** (fire-and-forget):
- `SessionCreatedEvent` - Session created notification  
- `SessionJoinedEvent` - Someone joined session
- `SessionStoppedEvent` - Session stopped
- `SessionForkedEvent` - New session forked from existing
- `SessionCleanupEvent` - Session cleaned up

### **Multi-Context Architecture**
```typescript
abstract class SessionDaemon extends DaemonBase {
  // 80-90% shared logic
  protected sessions: Map<string, SessionInfo> = new Map();
  protected artifacts: ArtifactManager;
  protected cleanup: CleanupManager;
  
  // Abstract methods for 5-10% context-specific logic
  abstract createSessionProcess(session: SessionInfo): Promise<void>;
  abstract cleanupSessionProcess(sessionId: string): Promise<void>;
  abstract handleRemoteExecution(sessionId: string, command: string): Promise<any>;
}

class SessionDaemonBrowser extends SessionDaemon {
  // Browser-specific: Local storage, browser process management
}

class SessionDaemonServer extends SessionDaemon {
  // Server-specific: File system, server process management, centralized cleanup
}
```

## 📦 **Migration Steps**

### **Phase 1: Core Types & Payloads** ✅
- [x] Create `shared/types/SessionTypes.ts` - Migrate excellent types from legacy
- [x] Create `shared/payloads/SessionCommands.ts` - Command params/results
- [x] Create `shared/payloads/SessionEvents.ts` - Event payloads
- [x] Preserve utility functions (SessionPaths, SessionValidation, SessionUtils)

### **Phase 2: Abstract Base Daemon** ✅
- [x] Create `shared/SessionDaemon.ts` - Abstract base with shared logic
- [x] Migrate session management, artifact organization, cleanup logic
- [x] Convert to JTAG message handling pattern
- [x] Maintain session isolation and process tracking

### **Phase 3: Context Implementations** 🚧
- [ ] Create `browser/SessionDaemonBrowser.ts` - Browser-specific logic
- [ ] Create `server/SessionDaemonServer.ts` - Server-specific logic  
- [ ] Thin implementations focused on context-specific operations

### **Phase 4: Integration** 🚧
- [ ] JTAG router integration (`/session/{command}`)
- [ ] Event system integration
- [ ] Backward compatibility testing
- [ ] Migration from legacy daemon

## 🎯 **Key Architectural Improvements**

### **1. JTAG Command/Event Integration**
- Commands go through JTAG router with proper typing
- Events use JTAG event system for real-time updates
- Transport-agnostic (works with WebSocket, HTTP, UDP, P2P)

### **2. Multi-Context Awareness**
- Same session can be accessed from browser and server
- Context-specific operations (browser process vs server file management)
- Shared session state across contexts

### **3. Academy Integration Ready**
- `persona` session type ready for Academy training contexts
- Session-scoped artifact storage for training data
- Performance tracking integration hooks

### **4. P2P Mesh Ready**
- Session state can be synchronized across network nodes
- Remote execution capabilities for distributed scenarios
- Session forking across network boundaries

### **5. Enhanced Type Safety**
- Full TypeScript coverage with JTAG payload patterns
- Command validation at transport layer
- Event type safety for real-time updates

## 🔄 **Backward Compatibility**

### **Legacy Wrapper**
```typescript
// Maintain compatibility during transition
class SessionManagerCompatibilityWrapper {
  private sessionDaemon: SessionDaemon;
  
  // Translate legacy API to JTAG commands
  async createSession(request: LegacySessionRequest): Promise<LegacySessionResponse> {
    const params = new CreateSessionParams(this.translateRequest(request));
    const result = await this.sessionDaemon.createSession(params);
    return this.translateResponse(result);
  }
}
```

### **Migration Testing**
- Parallel operation of legacy and JTAG systems
- Gradual migration of dependent systems
- Comprehensive integration tests

## 🧪 **Testing Strategy**

### **Unit Tests**
- Session creation, joining, forking, cleanup
- Artifact management and organization
- Validation utilities and path generation
- Event emission and handling

### **Integration Tests**  
- Cross-context session access (browser ↔ server)
- Session lifecycle with real processes
- Cleanup automation and edge cases
- JTAG router integration

### **Performance Tests**
- Session creation/cleanup performance
- Concurrent session management
- Memory usage under load
- Artifact storage efficiency

## 🎯 **Success Criteria**

1. **Functional Parity** - All legacy session manager capabilities preserved
2. **Enhanced Architecture** - JTAG integration, better type safety, command/event split
3. **Multi-Context Support** - Browser and server implementations working seamlessly
4. **Academy Ready** - Session contexts ready for AI training scenarios
5. **P2P Ready** - Architecture supports distributed session coordination
6. **Performance** - No regression in session creation/cleanup performance
7. **Migration Path** - Smooth transition from legacy system with minimal disruption

## 📋 **Implementation Notes**

### **Preserve Excellent Design**
The legacy system has several excellent patterns to preserve:
- **Clean type definitions** in `SessionManagerTypes.ts`
- **Utility function organization** (SessionPaths, SessionValidation, SessionUtils)
- **Event factory pattern** for consistent event creation
- **Path generation logic** for organized artifact storage
- **Cleanup automation** with configurable lifecycle management

### **Enhance with JTAG**
- **Transport agnostic** - Works with any JTAG transport
- **Type-safe routing** - Commands routed through JTAG with full typing
- **Event integration** - Real-time session events through JTAG event system
- **Cross-context** - Session state shared between browser and server contexts

### **Academy Integration Hooks**
- Session contexts for persona training scenarios
- Artifact storage for training data and performance metrics
- Process isolation for safe AI experimentation
- Event hooks for Academy training lifecycle

The session daemon becomes a **universal session substrate** that chat rooms, Academy training, browser management, and P2P coordination can build upon.