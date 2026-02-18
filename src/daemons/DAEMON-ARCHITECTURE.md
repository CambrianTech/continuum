# JTAG Daemon Architecture - Universal Pattern Documentation

## **🎯 Mission**
Establish the universal architectural pattern for all JTAG daemons, providing consistent design principles, symmetric implementation patterns, and integration guidelines for the ecosystem.

## **🏗️ Universal Daemon Architecture Pattern**

### **Core Pattern: 85% Shared, 15% Context-Specific**
Every JTAG daemon follows the Sparse Override Pattern:

```
daemons/{daemon-name}/
├── shared/
│   ├── {DaemonName}.ts          # Universal interface (80-90% of logic)
│   ├── {DaemonName}Base.ts      # Abstract base implementation
│   └── {DaemonName}Types.ts     # Shared types and contracts
├── browser/
│   └── {DaemonName}Browser.ts   # Thin browser-specific layer (5-10%)
├── server/
│   └── {DaemonName}Server.ts    # Thin server-specific layer (5-10%)
├── tests/
│   ├── {DaemonName}.test.ts     # Shared contract tests
│   ├── Browser.test.ts          # Browser-specific tests
│   └── Server.test.ts           # Server-specific tests
└── README.md                    # Architecture documentation
```

### **Burden Distribution Philosophy**
- **Shared Base (80-90%)**: Validation, business logic, message handling, type safety
- **Browser Layer (5-10%)**: WebSocket transport, DOM APIs, browser-specific utilities
- **Server Layer (5-10%)**: Filesystem access, process management, Node.js APIs

## **🔧 Implemented Daemon Patterns**

### **1. DataDaemon - Universal Storage Orchestrator**
**Purpose**: Heavy abstraction for organizational data with pluggable storage strategies

**Architecture Highlights**:
- **Plugin System**: Storage adapters (File, Memory, SQL, NoSQL) via factory pattern
- **Query Abstraction**: Universal query interface translates to backend-specific queries
- **Strategy Selection**: Automatic backend selection based on workload requirements
- **Schema Evolution**: Versioned records with backward compatibility

**Key Files**:
- `shared/DataDaemon.ts` - Universal storage interface (85% of logic)
- `server/DataDaemonServer.ts` - Actual storage operations (15% server-specific)
- `browser/DataDaemonBrowser.ts` - Delegation to server (minimal browser layer)

**Storage Strategy**:
```typescript
interface StorageStrategyConfig {
  strategy: 'sql' | 'nosql' | 'file' | 'memory' | 'network' | 'hybrid';
  backend: string; // 'postgres', 'sqlite', 'mongodb', 'redis', 'json', etc.
  namespace: string;
  features: StorageFeatures;
}
```

### **2. ArtifactsDaemon - Filesystem Access Orchestration**
**Purpose**: Centralized filesystem access with session isolation and security

**Architecture Highlights**:
- **Session Isolation**: Multi-agent filesystem sandboxing via session directories
- **Path Enforcement**: All filesystem access through validated relative paths
- **Security Model**: Session boundaries prevent cross-agent file access
- **Universal Operations**: read, write, append, mkdir, list, stat, delete

**Key Files**:
- `shared/ArtifactsDaemon.ts` - Universal filesystem interface (85% of logic)
- `server/ArtifactsDaemonServer.ts` - Actual filesystem operations (15% server-specific)
- `browser/ArtifactsDaemonBrowser.ts` - Delegation to server (minimal browser layer)

**Session Architecture**:
```
.continuum/jtag/
├── system/                      # System-wide shared resources
│   ├── config/                  # Global daemon configurations
│   ├── personas/base-models/    # Shared base models & checkpoints
│   └── data/                    # Cross-session shared data
└── sessions/user/               # Session-isolated workspaces
    ├── {persona-session-id}/    # LoRA-adapted persona workspace
    ├── {human-session-id}/      # Human user session
    └── {ci-session-id}/         # CI/CD isolated workspace
```

### **3. ChatDaemon - Universal Communication Substrate**
**Purpose**: Room-scoped communication for any intelligence type (human, AI, persona)

**Architecture Highlights**:
- **Universal Participants**: Same interface for humans, AIs, personas, bots
- **Room-Scoped Events**: Events only delivered to room participants
- **Adapter Pattern**: Pluggable response engines for different AI providers
- **Storage Integration**: Uses DataDaemon for message persistence

**Key Files**:
- `shared/ChatDaemon.ts` - Universal communication interface (85% of logic)
- `server/ChatDaemonServer.ts` - Event distribution and storage (15% server-specific)
- `browser/ChatDaemonBrowser.ts` - UI event integration (minimal browser layer)

**Participant Architecture**:
```typescript
SessionParticipant {
  participantId: UUID;
  sessionId: UUID;
  displayName: string;
  capabilities: {
    autoResponds: boolean;    // AI agents, personas, bots
    canModerate: boolean;     // Room management rights
    canInvite: boolean;       // Can invite others
  };
  adapter: AdapterConfig;     // HOW they connect/respond
}
```

### **4. AIProviderDaemon - Pluggable AI Integration**
**Purpose**: Provider-agnostic AI API access for persona system

**Architecture Highlights**:
- **Provider Abstraction**: Unified interface for OpenAI, Anthropic, Google, local models
- **Capability-Based Selection**: Automatic provider selection based on requirements
- **Cost Management**: Usage tracking and budget enforcement
- **Academy Integration**: Persona-specific model selection and training data collection

**Provider Architecture**:
```typescript
interface AIProviderAdapter {
  providerId: string;
  capabilities: ProviderCapabilities;
  generateText(request: TextGenerationRequest): Promise<TextGenerationResponse>;
  streamText(request: TextGenerationRequest): AsyncIterable<TextStreamChunk>;
  // ... other AI operations
}
```

### **5. WidgetDaemon - Widget System Integration**
**Purpose**: Bridge between widgets and JTAG command routing system

**Architecture Highlights**:
- **Command Bridge**: Simple `executeCommand()` API routes to powerful JTAG system
- **Cross-Origin Solution**: Proxy navigation enables iframe screenshots
- **Auto-Discovery**: Widgets automatically registered in daemon structure
- **Global Access**: Available as `window.widgetDaemon` for all widgets

**Integration Flow**:
```
Widget → BaseWidget.executeCommand() → WidgetDaemon → JTAGRouter → CommandDaemon
```

## **📋 Universal Daemon Implementation Checklist**

### **Architectural Requirements**
- [ ] **Sparse Override Pattern**: 85% shared logic, 15% context-specific
- [ ] **Message-Based**: All operations via JTAGMessage<T> with correlation
- [ ] **Session Aware**: Supports sessionId for multi-agent isolation
- [ ] **Type Safe**: Strong TypeScript interfaces with runtime validation
- [ ] **Error Handling**: Comprehensive error messages and recovery
- [ ] **Testing**: Unit tests for shared logic, integration tests for full flow

### **Structure Requirements**
- [ ] **shared/** directory with core interface and types
- [ ] **browser/** directory with minimal browser-specific implementation
- [ ] **server/** directory with minimal server-specific implementation
- [ ] **tests/** directory with comprehensive test coverage
- [ ] **README.md** with architecture documentation and usage examples

### **Integration Requirements**
- [ ] **Router Integration**: Works with existing JTAGRouter message system
- [ ] **Discovery**: Auto-discoverable via package.json and daemon registry
- [ ] **Command Integration**: Commands can delegate to daemon operations
- [ ] **Session Management**: Respects session boundaries and isolation

### **Quality Requirements**
- [ ] **Performance**: Async operations with proper resource management
- [ ] **Security**: Session validation and path sanitization
- [ ] **Monitoring**: Proper logging and error reporting
- [ ] **Documentation**: Clear usage examples and troubleshooting guides

## **🔄 Message Flow Architecture**

### **Universal Message Pattern**
All daemons use the same message-based communication:

```typescript
// Request
const message = JTAGMessageFactory.createRequest(
  context,
  'browser',                    // Source context
  'server/{daemon-name}',       // Target daemon
  payload,                      // Daemon-specific payload
  correlationId                 // For response tracking
);

// Processing
async handleMessage(message: JTAGMessage<PayloadType>): Promise<ResultType> {
  const result = await this.processOperation(message.payload);
  return result;
}

// Response
const response = JTAGMessageFactory.createResponse(
  originalMessage,
  result,
  context
);
```

### **Cross-Context Communication**
- **Browser → Server**: Delegation pattern for operations requiring server resources
- **Server → Browser**: Event notifications and UI updates
- **Daemon → Daemon**: Inter-daemon communication via router
- **Session Isolation**: Messages carry sessionId for proper access control

## **🧩 Daemon Integration Patterns**

### **Storage Integration**
```typescript
// ArtifactsDaemon for filesystem operations
const fileResult = await router.routeToServer('artifacts', {
  operation: 'write',
  relativePath: 'screenshots/debug.png',
  content: imageBuffer,
  sessionId: userSessionId
});

// DataDaemon for structured data operations
const dataResult = await router.routeToServer('data', {
  operation: 'create',
  collection: 'chat_messages',
  data: messageData,
  sessionId: userSessionId
});
```

### **Command Integration**
```typescript
// Commands delegate to appropriate daemons
export class ScreenshotServerCommand {
  async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
    // Capture via browser command
    const imageResult = await this.router.routeToBrowser('command', {
      command: 'screenshot',
      querySelector: params.querySelector
    });
    
    // Save via ArtifactsDaemon
    const saveResult = await this.router.routeToServer('artifacts', {
      operation: 'write',
      relativePath: `screenshots/${params.filename}`,
      content: imageResult.imageBuffer,
      sessionId: params.sessionId
    });
    
    return createScreenshotResult(params.context, saveResult);
  }
}
```

### **Event Integration**
```typescript
// ChatDaemon distributes events to room participants
export class ChatDaemonServer {
  async distributeRoomEvent(roomId: UUID, eventType: string, data: any) {
    const participants = await this.getRoomParticipants(roomId);
    
    for (const participant of participants) {
      const eventMessage = JTAGMessageFactory.createEvent(
        this.context,
        participant.sessionId,
        eventType,
        data
      );
      
      await this.router.postMessage(eventMessage);
    }
  }
}
```

## **🔐 Security Architecture**

### **Session-Based Isolation**
Every daemon operation includes session context:
```typescript
interface DaemonPayload extends JTAGPayload {
  readonly sessionId?: UUID;  // Session context for isolation
}
```

### **Permission Enforcement Points**
- **ArtifactsDaemon**: Filesystem access limited to session directories
- **DataDaemon**: Data namespacing by session for isolation
- **ChatDaemon**: Room membership validation before event delivery
- **AIProviderDaemon**: Usage quotas and cost limits per session

### **Audit Trail**
All daemon operations are logged with:
- **Session Attribution**: Which session initiated the operation
- **Operation Details**: What was requested and what was performed
- **Resource Usage**: Storage, API calls, processing time
- **Error Context**: Full error details for debugging

## **🧪 Testing Architecture**

### **Three-Layer Testing Strategy**
1. **Shared Contract Tests**: Validate universal interface contracts (business logic, validation)
2. **Context-Specific Tests**: Browser/server implementation details (transport, APIs)
3. **Integration Tests**: Full message flow with real routing (end-to-end validation)

### **Test Structure Pattern**
```
tests/
├── shared/
│   ├── {DaemonName}.test.ts        # Interface contract validation
│   └── {DaemonName}Types.test.ts   # Type validation and serialization
├── browser/
│   └── {DaemonName}Browser.test.ts # Browser-specific functionality
├── server/
│   └── {DaemonName}Server.test.ts  # Server-specific functionality
└── integration/
    └── {DaemonName}Flow.test.ts    # End-to-end message flow
```

### **Mock Strategy**
- **Mock Router**: Test daemon logic without real transport
- **Mock Storage**: Test data operations without real filesystem
- **Mock Session**: Test session isolation without real session state

## **📈 Performance Patterns**

### **Async Architecture**
All daemons use async/await with proper error handling:
```typescript
export abstract class ProcessBasedDaemon {
  protected abstract handleMessage(message: JTAGMessage<T>): Promise<R>;
  
  // Async queue with mutex/semaphore for resource management
  protected async processOperation(operation: () => Promise<T>): Promise<T> {
    await this.semaphore.acquire();
    try {
      return await operation();
    } finally {
      this.semaphore.release();
    }
  }
}
```

### **Resource Management**
- **Connection Pooling**: Reuse expensive resources (database connections, AI clients)
- **Cleanup Handlers**: Proper shutdown and resource cleanup
- **Memory Management**: Bounded queues and cache eviction
- **Timeout Handling**: Prevent hung operations with proper timeouts

## **🔄 Evolution Strategy**

### **Migration Pattern**
1. **Create Symmetric Structure**: Build shared/browser/server architecture
2. **Compatibility Wrapper**: Maintain existing API during migration
3. **Gradual Migration**: Move functionality piece by piece
4. **Feature Parity**: Ensure no functionality loss
5. **Clean Cutover**: Remove legacy code after full validation

### **Future Daemon Candidates**
- **SessionManagerDaemon**: Session lifecycle and state management
- **BrowserManagerDaemon**: Browser automation and control
- **ProcessManagerDaemon**: System process monitoring and control
- **NetworkDaemon**: HTTP/WebSocket proxy and networking
- **SecurityDaemon**: Authentication, authorization, and audit

## **🎯 Quality Metrics**

### **Code Quality Indicators**
- **High Shared Logic Ratio**: >80% of logic in shared/ directory
- **Minimal Context Layers**: <20% of code in browser/server specific layers
- **Test Coverage**: >90% coverage on shared logic, >80% on context-specific
- **Type Safety**: Zero `any` types in public interfaces
- **Documentation**: Complete README with usage examples

### **Architectural Quality**
- **Message Correlation**: All operations traceable via correlation IDs
- **Error Transparency**: Clear error propagation from source to caller
- **Resource Cleanup**: No resource leaks or hung operations
- **Session Isolation**: No cross-session data leakage or access violations

## **📚 Implementation Guidelines**

### **Starting a New Daemon**
1. **Study Existing Patterns**: Read DataDaemon and ArtifactsDaemon implementations
2. **Define Payload Interface**: Strong types for all daemon operations
3. **Create Shared Base**: Implement core logic in shared/ directory
4. **Thin Context Layers**: Minimal browser/server specific implementations
5. **Comprehensive Tests**: Validate all three layers separately and together
6. **Document Architecture**: Complete README with examples and patterns

### **Message Design Principles**
```typescript
// Every daemon payload extends JTAGPayload
interface DaemonPayload extends JTAGPayload {
  readonly operation: string;     // What operation to perform
  readonly sessionId?: UUID;      // Session context for isolation
  // ... operation-specific parameters
}

// Results follow consistent pattern
interface DaemonResult {
  readonly success: boolean;
  readonly error?: string;
  // ... operation-specific results
}
```

### **Error Handling Standards**
```typescript
// Consistent error handling across all daemons
try {
  const result = await this.performOperation(payload);
  return { success: true, ...result };
} catch (error: any) {
  const errorMessage = error instanceof Error ? error.message : String(error);
  return { success: false, error: errorMessage };
}
```

## **🔗 Inter-Daemon Coordination**

### **Daemon Dependencies**
- **ArtifactsDaemon**: Foundation for all filesystem operations
- **DataDaemon**: Foundation for all structured data storage
- **ChatDaemon**: Uses DataDaemon for message storage, ArtifactsDaemon for file sharing
- **AIProviderDaemon**: Uses DataDaemon for usage tracking
- **WidgetDaemon**: Coordinates with all daemons via command system

### **Dependency Resolution**
```typescript
// Daemons coordinate through router, not direct calls
async coordinateWithArtifacts(operation: ArtifactsPayload): Promise<ArtifactsResult> {
  return await this.router.routeToServer('artifacts', operation);
}

async coordinateWithData(operation: DataPayload): Promise<DataResult> {
  return await this.router.routeToServer('data', operation);
}
```

### **Circular Dependency Prevention**
- **Router Mediation**: All daemon communication through router prevents direct coupling
- **Event-Based Coordination**: Asynchronous event propagation rather than synchronous calls
- **Lazy Initialization**: Daemons initialize independently, coordinate as needed

## **🚀 Scalability Architecture**

### **Horizontal Scaling Patterns**
- **Stateless Design**: Daemons can be replicated across multiple processes/machines
- **Session Affinity**: Sessions route to consistent daemon instances
- **Load Balancing**: Router distributes requests across daemon replicas
- **Resource Isolation**: Each daemon manages its own resources independently

### **Performance Optimization**
- **Async Queues**: Non-blocking operation processing
- **Connection Pooling**: Efficient resource utilization
- **Caching Layers**: Intelligent caching with proper invalidation
- **Batch Operations**: Efficient bulk processing where appropriate

## **🛡️ Security Model**

### **Defense in Depth**
1. **Transport Security**: Message encryption and authentication
2. **Session Validation**: Session context validation at daemon entry points
3. **Path Validation**: Filesystem path sanitization and sandboxing
4. **Resource Limits**: Per-session quotas and rate limiting
5. **Audit Logging**: Complete operation history with attribution

### **Threat Model**
- **Session Isolation**: Prevent cross-agent data access
- **Path Traversal**: Prevent filesystem escape attempts
- **Resource Exhaustion**: Prevent DoS via resource consumption
- **Injection Attacks**: Validate all inputs and sanitize operations

## **📊 Monitoring and Observability**

### **Health Monitoring**
Each daemon exposes standard health endpoints:
```typescript
interface DaemonHealth {
  status: 'healthy' | 'degraded' | 'failed';
  uptime: number;
  memoryUsage: number;
  activeOperations: number;
  errors: ErrorSummary[];
}
```

### **Metrics Collection**
- **Operation Metrics**: Success/failure rates, latency distributions
- **Resource Metrics**: Memory usage, disk usage, network traffic
- **Session Metrics**: Active sessions, operations per session
- **Error Metrics**: Error rates, error types, error recovery

## **🎯 Success Patterns**

### **What Makes a Great Daemon**
1. **Clear Single Responsibility**: Each daemon has one well-defined purpose
2. **Elegant Abstraction**: Complex implementation hidden behind simple interface
3. **Symmetric Architecture**: Same patterns work across all contexts
4. **Comprehensive Testing**: All functionality validated through automated tests
5. **Security by Design**: Session isolation and validation built-in
6. **Performance Awareness**: Efficient resource usage and proper cleanup
7. **Documentation Excellence**: Clear architecture explanation and usage examples

### **Anti-Patterns to Avoid**
- **Monolithic Daemons**: Daemons that try to do too many things
- **Context Leakage**: Browser-specific code in server implementations
- **Direct Coupling**: Daemons calling each other directly instead of via router
- **Resource Leaks**: Operations that don't clean up properly
- **Session Blindness**: Operations that ignore session context
- **Type Weakness**: Using `any` types instead of proper interfaces

This architectural foundation enables **truly autonomous multi-agent systems** where humans, AIs, personas, and automated processes can collaborate safely and efficiently through elegant, symmetric interfaces.