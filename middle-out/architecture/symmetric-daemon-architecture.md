# Symmetric Daemon Architecture

## 🎯 Vision: Unified Client/Server Daemon Pattern

The universal module structure creates **perfect symmetry** between client and server daemons, enabling a unified mental model and consistent development patterns across all execution contexts.

## 🏗️ Current State vs Future Vision

### **Current Architecture (Asymmetric)**
```
src/
├── daemons/              # Server-only daemons
│   ├── logger/
│   ├── session-manager/
│   └── browser-manager/
├── ui/                   # Client-only components
│   ├── daemons/          # Browser-specific daemons
│   └── components/
└── integrations/         # Mixed implementations
```

### **Future Architecture (Symmetric)**
```
src/
├── daemons/
│   ├── logger/
│   │   ├── shared/       # Universal logging types
│   │   ├── server/       # Node.js logger daemon
│   │   ├── client/       # Browser logger daemon
│   │   └── tests/        # Unified test suite
│   ├── session-manager/
│   │   ├── shared/       # Session protocols
│   │   ├── server/       # Server session daemon
│   │   ├── client/       # Browser session daemon
│   │   └── tests/        # Cross-context tests
│   └── browser-manager/
│       ├── shared/       # Browser control protocols
│       ├── server/       # Server browser controller
│       ├── client/       # In-browser automation
│       └── tests/        # Browser integration tests
```

## 🧠 Mental Model Benefits

### **Unified Daemon Concept**
Every daemon follows the same pattern regardless of execution context:
- **Same interfaces** - LoggerInterface works in browser and server
- **Same message types** - DaemonMessage<T> used everywhere
- **Same lifecycle** - start(), stop(), processMessage() pattern
- **Same testing** - Unified test patterns across contexts

### **Cognitive Simplification**
```typescript
// Server daemon
class ServerLoggerDaemon extends ProcessBasedDaemon<LoggerMessage> {
  async processMessage(message: LoggerMessage): Promise<DaemonResponse> {
    // Server-specific file I/O
  }
}

// Browser daemon (future)
class ClientLoggerDaemon extends ProcessBasedDaemon<LoggerMessage> {
  async processMessage(message: LoggerMessage): Promise<DaemonResponse> {
    // Browser-specific localStorage/indexedDB
  }
}
```

**Same shape, different implementation details.**

## 🔄 Cross-Context Communication

### **Symmetric Message Passing**
```typescript
// Server to Browser
serverLoggerDaemon.send(browserLoggerDaemon, message);

// Browser to Server  
browserLoggerDaemon.send(serverLoggerDaemon, message);

// Both use identical DaemonMessage<T> protocol
```

### **Unified Development Experience**
- **Same debugging patterns** - Message tracing works identically
- **Same error handling** - Consistent error propagation
- **Same performance monitoring** - Queue metrics across contexts
- **Same testing strategies** - Mock patterns work everywhere

## 🎨 Implementation Elegance

### **Shared Base Classes**
```typescript
// Universal daemon foundation
abstract class ProcessBasedDaemon<T> {
  // Works in Node.js with child_process
  // Works in Browser with Web Workers
  // Works in Remote with distributed queues
}

// Context-specific implementations
class ServerDaemon extends ProcessBasedDaemon<T> {
  // Node.js specific: fs, child_process, etc.
}

class ClientDaemon extends ProcessBasedDaemon<T> {
  // Browser specific: DOM, Web Workers, etc.
}
```

### **Transparent Context Switching**
```typescript
// Same API, different execution context
const logger = createLogger(context.environment);
await logger.info(context, "Message works everywhere");

// Routes to:
// - ServerLoggerDaemon in Node.js
// - ClientLoggerDaemon in browser
// - RemoteLoggerDaemon in distributed mode
```

## 🚀 Migration Strategy

### **Phase 1: Server Foundation (Current)**
- Implement ProcessBasedDaemon pattern
- Create server-side async queue architecture
- Establish message type patterns

### **Phase 2: Symmetric Structure**
- Refactor existing browser daemons to match server pattern
- Move browser daemons into universal module structure
- Create shared protocol definitions

### **Phase 3: Unified Development**
- Single codebase for daemon logic
- Context-specific adapters for execution differences
- Unified testing and debugging tools

## 🧪 Testing Symmetry

### **Unified Test Patterns**
```typescript
// Same test structure for all contexts
describe('LoggerDaemon', () => {
  describe('Server Context', () => {
    // Server-specific tests
  });
  
  describe('Client Context', () => {
    // Browser-specific tests
  });
  
  describe('Cross-Context Integration', () => {
    // Communication tests
  });
});
```

### **Consistent Mocking**
- Same mock patterns for daemon messages
- Same test utilities across contexts
- Same performance benchmarks

## 🎯 Developer Experience Benefits

### **Reduced Cognitive Load**
- **One pattern to learn** - Works everywhere
- **Consistent APIs** - Same methods across contexts
- **Unified debugging** - Same tools, same patterns
- **Predictable behavior** - Same lifecycle everywhere

### **Faster Development**
- **Code reuse** - Shared protocols and types
- **Pattern familiarity** - Know one, know all
- **Consistent tooling** - Same dev experience
- **Easier refactoring** - Move logic between contexts

### **Better Architecture**
- **Clean separation** - Context vs business logic
- **Testable design** - Mock any context
- **Scalable patterns** - Add contexts easily
- **Maintainable code** - Single source of truth

## 🔮 Future Possibilities

### **Context-Agnostic Daemons**
```typescript
// Same daemon code runs in any context
class UniversalDaemon extends ProcessBasedDaemon<T> {
  constructor(context: ExecutionContext) {
    super(context);
    this.adapter = createAdapter(context.environment);
  }
}
```

### **Seamless Migration**
- Move daemons between contexts without code changes
- Dynamic context switching at runtime
- Load balancing across execution environments

### **Distributed Coordination**
- Server and browser daemons coordinate seamlessly
- P2P daemon communication
- Fault tolerance through context redundancy

## **🎯 CORE PRINCIPLE: DUMB ROUTER PATTERN**

Each daemon is **completely isolated** and handles **one specific concern**. Daemons register themselves with routing patterns. The router is **dumb** - it just routes messages based on registered patterns, with zero business logic.

### **Architecture Tests (Test-Driven Design)**

```typescript
// Test 1: Dumb Router - No Business Logic
describe('CommandRouter', () => {
  it('should route messages based on registered patterns only', async () => {
    const router = new CommandRouter();
    
    // Router should have NO knowledge of HttpApiHandler, WebSocketHandler, etc.
    // It only knows about registered patterns
    router.registerHandler('http_request', 'http-api-handler');
    router.registerHandler('websocket_message', 'websocket-handler');
    router.registerHandler('execute_command', 'command-executor');
    
    const message = { type: 'http_request', data: {...} };
    const result = await router.route(message);
    
    expect(result.routedTo).toBe('http-api-handler');
    expect(router.hasBusinessLogic()).toBe(false); // Router is dumb!
  });
});

// Test 2: HttpApiHandler Self-Registration
describe('HttpApiHandler', () => {
  it('should register itself and handle only HTTP concerns', async () => {
    const handler = new HttpApiHandler();
    const mockRouter = new MockRouter();
    
    // Handler registers itself - no external knowledge needed
    await handler.registerWithRouter(mockRouter);
    
    expect(mockRouter.getRegisteredPatterns()).toContain('handle_api');
    expect(handler.getConcern()).toBe('http-api-parsing'); // Single concern
    expect(handler.knowsAbout('websocket')).toBe(false); // No cross-daemon knowledge
  });
  
  it('should parse HTTP requests and forward to execution', async () => {
    const handler = new HttpApiHandler();
    const message = { type: 'handle_api', data: { path: '/api/commands/screenshot' } };
    
    const result = await handler.handleMessage(message);
    
    // Should extract command and forward, nothing more
    expect(result.forwardTo).toBe('command-executor');
    expect(result.extractedCommand).toBe('screenshot');
    expect(result.concern).toBe('http-parsing-only');
  });
});

// Test 3: WebSocketHandler Self-Registration  
describe('WebSocketHandler', () => {
  it('should register itself and handle only WebSocket concerns', async () => {
    const handler = new WebSocketHandler();
    const mockRouter = new MockRouter();
    
    await handler.registerWithRouter(mockRouter);
    
    expect(mockRouter.getRegisteredPatterns()).toContain('websocket_message');
    expect(mockRouter.getRegisteredPatterns()).toContain('execute_command');
    expect(handler.getConcern()).toBe('websocket-message-parsing');
    expect(handler.knowsAbout('http')).toBe(false);
  });
});

// Test 4: CommandExecutor Self-Registration
describe('CommandExecutor', () => {
  it('should register itself and handle only execution concerns', async () => {
    const executor = new CommandExecutor();
    const mockRouter = new MockRouter();
    
    await executor.registerWithRouter(mockRouter);
    
    expect(mockRouter.getRegisteredPatterns()).toContain('command.execute');
    expect(executor.getConcern()).toBe('command-execution-only');
    expect(executor.knowsAbout('http')).toBe(false);
    expect(executor.knowsAbout('websocket')).toBe(false);
  });
  
  it('should execute commands through registry without routing knowledge', async () => {
    const executor = new CommandExecutor();
    const message = { type: 'command.execute', data: { command: 'screenshot', parameters: {} } };
    
    const result = await executor.handleMessage(message);
    
    expect(result.executedThrough).toBe('UniversalCommandRegistry');
    expect(result.concern).toBe('execution-only');
    expect(executor.hasRoutingLogic()).toBe(false); // No routing knowledge
  });
});
```

### **🔧 DUMB ROUTER IMPLEMENTATION PRINCIPLES**

1. **Router has ZERO business logic** - just pattern matching and delegation
2. **Each daemon registers itself** - no external configuration
3. **Single concern per daemon** - HTTP parsing, WebSocket parsing, command execution
4. **No cross-daemon knowledge** - HttpApiHandler doesn't know WebSocketHandler exists
5. **Forwards messages only** - no transformation, just routing

### **Current Architecture Problems (Identified by Tests)**

❌ **Router is too smart** - CommandRouter has business logic for command extraction
❌ **Handlers know too much** - HttpApiHandler knows about CommandRouter
❌ **No self-registration** - Manual wiring instead of daemon self-registration
❌ **Mixed concerns** - Single daemons handling multiple concerns

### **Fixed Architecture Pattern**

```typescript
// Dumb Router - Just routes based on registered patterns
class CommandRouter {
  private handlers = new Map<string, string>();
  
  registerHandler(pattern: string, handlerName: string) {
    this.handlers.set(pattern, handlerName);
  }
  
  async route(message: DaemonMessage): Promise<DaemonResponse> {
    const handler = this.handlers.get(message.type);
    if (!handler) {
      return { success: false, error: `No handler for ${message.type}` };
    }
    
    // Just forward - zero business logic
    return await this.forwardToHandler(handler, message);
  }
}

// Self-Registering Daemon Pattern
class HttpApiHandler {
  async registerWithRouter(router: CommandRouter) {
    router.registerHandler('handle_api', this.name);
  }
  
  async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    // Single concern: Parse HTTP request and forward
    const command = this.extractCommandFromPath(message.data.path);
    
    return {
      success: true,
      forwardTo: 'command-executor',
      forwardMessage: {
        type: 'command.execute',
        data: { command, parameters: message.data.body }
      }
    };
  }
}
```

### **🎯 SYMMETRIC DAEMON BENEFITS**

- **Testable**: Each daemon has single responsibility
- **Modular**: Add new daemons without changing existing ones  
- **Debuggable**: Clear concern boundaries
- **Scalable**: Daemons can run in separate processes
- **Maintainable**: No complex interdependencies

### **Next Steps**

1. Write failing tests for dumb router pattern
2. Refactor CommandRouter to be dumb (just routing)
3. Implement self-registration in each daemon
4. Remove business logic from router
5. Verify all tests pass with new architecture

## 💡 Key Insight

**The symmetric structure isn't just about code organization - it's about creating a unified mental model that makes the entire system more predictable, maintainable, and extensible.**

When every daemon follows the same pattern, developers can:
- **Reason about any daemon** using the same mental model
- **Debug any issue** using the same tools and patterns
- **Extend any functionality** using familiar patterns
- **Test any component** using consistent strategies

This symmetry transforms a complex distributed system into a coherent, understandable architecture where the same principles apply everywhere.