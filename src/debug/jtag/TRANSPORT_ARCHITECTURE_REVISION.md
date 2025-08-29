# Transport Architecture Revision Plan

## 🎯 **OBJECTIVE**
Create **dumb pipe** transport architecture that is resilient to radical reconfiguration. Enable versatile module design for widget development, AI persona integration, and dynamic deployment scenarios.

## 🚨 **ROOT CAUSE ANALYSIS**

### **Fundamental Problem** 
**Hardcoded assumptions block radical reconfiguration**. Current system cannot handle:
- Dynamic port assignment (multiple widget instances)
- Different deployment sites (beyond localhost)  
- AI-directed content delivery (HTML/CSS through router)
- Modular widget development with persona integration

### **Core Issue: Transports Not "Dumb Pipes"**
Transports currently know too much:
- Configuration structure details (`context.config.instance.ports.*`)
- Port assignment logic  
- Message formatting (JTAG-specific)
- Connection business logic (reconnection, health checks)

### **Current Violations Inventory**

**Files with Transport Violations:**
- `TransportTypes.ts` - Bloated config interface with JTAG concepts
- `TransportBase.ts` - Base class mixing transport and business logic
- `WebSocketTransportServer.ts` - Transport managing sessions and routing
- `WebSocketTransportClient.ts` - Transport with reconnection and ping logic
- `HTTPTransport.ts` - Transport hardcoding endpoint structure
- `TransportFactoryBrowser.ts` - Factory reading deep config structures
- `TransportFactoryServer.ts` - Factory with same config violations
- `TransportConfig.ts` - Helper with hardcoded port selection logic

**Violation Patterns:**
- Transports read `context.config.instance.ports.*`
- Transport constructors take complex config objects
- Transports manage JTAG sessions, events, handlers
- Hardcoded knowledge of JTAG message routing
- Business logic mixed with transport protocol logic

## 🏗️ **DUMB PIPES ARCHITECTURE VISION**

### **Three-Layer Separation of Concerns**
```
┌─────────────────────┐    ┌─────────────────────┐    ┌─────────────────────┐
│   Orchestrator      │    │      Factory        │    │    Transport        │
│  (Business Logic)   │    │  (Config Resolver)  │    │   (Dumb Pipe)       │
├─────────────────────┤    ├─────────────────────┤    ├─────────────────────┤
│ • Message format    │    │ • Read any config   │    │ • connect(url)      │
│ • Reconnection      │────│ • Resolve destinations│────│ • send(data)        │
│ • Health monitoring │    │ • Handle env diffs  │    │ • onData(handler)   │
│ • Request/response  │    │ • Create transports │    │ • close()           │
│ • Event integration │    │ • Port allocation   │    │ • isConnected()     │
└─────────────────────┘    └─────────────────────┘    └─────────────────────┘
```

### **Radical Reconfiguration Examples**
**Dumb pipes handle ANY destination without code changes:**

```typescript
// Dynamic ports - no hardcoding
const transport = new WebSocketTransport("ws://localhost:37593");

// Different sites - no localhost assumptions  
const transport = new WebSocketTransport("wss://widgets.continuum.ai:443");

// Multiple widget instances - factory resolves
const widget1 = factory.create('websocket', { host: 'site1.com', port: 9001 });
const widget2 = factory.create('websocket', { host: 'site2.com', port: 9002 });

// Protocol changes - same interface
const transport = new HTTPTransport("https://api.continuum.ai/messages");
const transport = new UDPTransport("udp://mesh.continuum.ai:37472");
```

### **Pure Pipe Contract**
**Transports implement ONLY protocol-specific data movement:**
- ✅ **Know**: How to connect to destination string  
- ✅ **Know**: Protocol-specific data transmission
- ❌ **Don't know**: Configuration structures
- ❌ **Don't know**: Port assignment logic
- ❌ **Don't know**: Message formats (JSON, HTML, etc.)
- ❌ **Don't know**: Business logic (retry, health, correlation)

## 📋 **SYSTEMATIC REVISION PLAN**

### **Phase 1: Define Dumb Pipe Interface**

**Pure Transport Contract:**
```typescript
interface PureTransport {
  readonly protocol: string;                    // 'websocket', 'http', 'udp'
  
  // Connection lifecycle - protocol specific only
  connect(destination: string): Promise<void>;  // "ws://host:port", "https://api", "udp://host:port"  
  close(): Promise<void>;
  isConnected(): boolean;
  
  // Raw data movement - zero message interpretation
  send(data: string | Buffer): Promise<void>;
  onData(handler: (data: string | Buffer) => void): void;
  
  // Connection events - for orchestrator monitoring
  onConnect(handler: () => void): void;
  onDisconnect(handler: (reason?: string) => void): void;
  onError(handler: (error: Error) => void): void;
}
```

**Configuration Resolver Interface:**
```typescript  
interface TransportFactory {
  // Creates dumb pipe with resolved destination
  createTransport(protocol: string, request: TransportRequest): Promise<PureTransport>;
  
  // Resolves any config to destination string
  resolveDestination(protocol: string, request: TransportRequest): string;
  
  // Environment-aware protocol support
  getSupportedProtocols(): string[];
}

interface TransportRequest {
  protocol: string;                      // 'websocket', 'http', 'udp'
  role: 'client' | 'server';            // Connection behavior
  
  // Optional overrides - factory falls back to config
  host?: string;                         // Override default
  port?: number;                         // Override default
  path?: string;                         // For HTTP: /api/messages
  secure?: boolean;                      // Use TLS/SSL
  
  // Context for resolution
  environment: 'browser' | 'server';     // Factory uses appropriate config
  configSource: any;                     // Raw config (JTAGContext, etc.)
}
```

**Business Logic Orchestrator Interface:**
```typescript
interface TransportOrchestrator {
  // High-level connection management  
  connect(request: TransportRequest): Promise<void>;
  disconnect(): Promise<void>;
  reconnect(): Promise<void>;
  
  // Message operations (handles serialization)
  sendMessage(message: any): Promise<void>;
  onMessage(handler: (message: any) => void): void;
  
  // Health and monitoring
  getHealth(): TransportHealth;
  onHealthChange(handler: (health: TransportHealth) => void): void;
}
```

**Benefits Validation Tests:**
- [ ] Transport works with ANY valid destination string
- [ ] Same transport handles localhost and remote sites  
- [ ] Protocol changes require zero transport code changes
- [ ] Factory resolves complex config to simple destination
- [ ] Orchestrator handles all business logic independently
- [ ] No hardcoded assumptions anywhere in transport layer

### **Phase 2: Widget Development Enablement Strategy**

**Engineering Priority: Enable radical reconfiguration for widget/persona development**

**Phase 2A: Factory Configuration Abstraction**
1. **Create TransportFactory abstraction** that reads ANY config source
2. **Abstract destination resolution** from complex config structures  
3. **Enable dynamic port assignment** for multiple widget instances
4. **Support different deployment sites** beyond localhost

**Phase 2B: Pure WebSocket Transport Implementation**  
1. **Create WebSocketPureTransport** - dumb pipe that only knows WebSocket protocol
2. **Remove all JTAG/config coupling** - transport takes destination string only
3. **Test radical reconfiguration** - same transport on different hosts/ports
4. **Validate zero hardcoding** - transport works with any valid WebSocket URL

**Phase 2C: Content-Aware Routing Layer**
1. **Extend router for HTML/CSS delivery** - support content beyond JSON messages
2. **Enable AI persona content generation** - route persona-generated HTML/CSS
3. **Support widget development workflow** - dynamic template delivery

**Pure Transport Type System:**
- ✅ `TransportProtocolContracts.ts` - Strongly typed cross-environment API contracts
- ✅ `PureTransportTypes.ts` - Clean interfaces for dumb transport pipes  
- ✅ `TransportAdapterBase.ts` - Generic adapter foundation with separated concerns
- ✅ `TransportOrchestrator.ts` - Bridges pure transports with JTAG business logic

**Pure WebSocket Transport:**
- ✅ `PureWebSocketTransport.ts` - Client transport (WebSocket connection management only)
- 🔄 `PureWebSocketServerTransport.ts` - Server transport (WebSocket server management only)
- Remove: JTAG concepts, message interpretation, session handling
- Keep: Raw WebSocket protocol operations, connection lifecycle, binary/string data

**Pure HTTP Transport:**
- 🔄 `PureHTTPTransport.ts` - HTTP request/response operations only
- Remove: hardcoded endpoints, JTAG message knowledge, routing logic
- Keep: HTTP methods, headers, request/response handling

**Unit Tests for Phase 2 (Revised):**
- ✅ Pure transport types enforce protocol contracts with TypeScript
- ✅ Each pure transport implements clean interface correctly
- ✅ No JTAG imports in pure transport files
- ✅ Transports work with any valid protocol parameters  
- ✅ Protocol-specific features work (WebSocket events, HTTP methods)
- ✅ Generic adapter base provides separated concerns

### **Phase 3: Complete Pure Transport Implementation (REVISED)**

**Transport Orchestrator Integration:**
- 🔄 Complete `TransportOrchestrator.ts` - JTAG business logic integration
- 🔄 Message serialization/deserialization (JSON ↔ JTAGMessage)
- 🔄 Event system integration (TRANSPORT_EVENTS)
- 🔄 Response correlation handling
- 🔄 Session management bridge

**Pure HTTP Transport Completion:**
- 🔄 `adapters/PureHTTPTransportAdapter.ts` - HTTP-specific adapter
- 🔄 GET/POST/PUT/DELETE method implementations
- 🔄 Header management and request/response handling
- 🔄 Error handling for HTTP status codes

**Comprehensive Testing:**
- ✅ `test/TransportArchitectureValidation.test.ts` - End-to-end architecture validation
- ✅ TypeScript protocol contract enforcement validation
- ✅ Generic adapter base validation with separation of concerns
- ✅ Cross-environment compatibility validation

**Unit Tests for Phase 3 (Revised):**
- ✅ Protocol contracts enforce TypeScript safety at compile time
- ✅ Generic adapter base handles all transport protocols
- ✅ Separation of concerns validated (base vs adapter responsibilities)
- ✅ Cross-environment compatibility (browser/server) validated
- ✅ Transport lifecycle management (connect/send/disconnect) validated
- ✅ Error handling and callback systems validated

### **Phase 4: Fix Factory Pattern**

**Clean Factory Implementation:**
```typescript
class TransportFactory {
  createWebSocket(context: JTAGContext): Transport {
    const url = this.buildWebSocketURL(context); // Factory reads config
    const transport = new WebSocketTransport();  // Transport gets clean params
    await transport.connect(url);
    return transport;
  }
  
  private buildWebSocketURL(context: JTAGContext): string {
    const port = context.config.instance.ports.websocket_server;
    return `ws://localhost:${port}`;
  }
}
```

**Unit Tests for Phase 4:**
- [ ] Factory correctly extracts URLs from context
- [ ] Factory creates transports with proper parameters
- [ ] Factory handles missing config gracefully
- [ ] Factory supports all transport types (WebSocket, HTTP, UDP)
- [ ] Factory-created transports work correctly

**Integration Tests for Phase 4:**
- [ ] Factory + Transport + Adapter chain works end-to-end
- [ ] Can swap transport types without changing factory interface
- [ ] Configuration changes reflected in created transports

### **Phase 5: Update Router Integration**

**Router Changes:**
- Remove transport config building from router
- Use factories to create transports
- Use adapters instead of raw transports
- Router focuses purely on message routing logic

**Unit Tests for Phase 5:**
- [ ] Router delegates transport creation to factories
- [ ] Router uses adapters for JTAG message handling
- [ ] Router doesn't contain transport configuration logic
- [ ] Router routing logic works with any adapter

**Integration Tests for Phase 5:**
- [ ] Full message routing works with new architecture
- [ ] Cross-environment routing (server↔browser) works
- [ ] Event bridging works with revised transports
- [ ] Response correlation works with new transport layer

### **Phase 6: Cross-Boundary Validation**

**System Integration Tests:**
- [ ] Server-to-browser message delivery
- [ ] Browser-to-server message delivery  
- [ ] Bidirectional event broadcasting
- [ ] Request-response correlation across boundaries
- [ ] Multiple concurrent connections
- [ ] Transport failover scenarios
- [ ] Network interruption recovery

**Performance Tests:**
- [ ] Message throughput comparable to current system
- [ ] Connection establishment time reasonable
- [ ] Memory usage within acceptable bounds
- [ ] No message loss under normal conditions

## 🧪 **TESTING STRATEGY**

### **Unit Test Architecture**
```
Transport Tests (Pure Protocol)
├── WebSocket Transport
│   ├── Connection Tests
│   ├── Send/Receive Tests
│   └── Error Handling Tests
├── HTTP Transport
│   ├── Request/Response Tests
│   └── Error Handling Tests
└── UDP Transport
    ├── Packet Send/Receive Tests
    └── Error Handling Tests

Adapter Tests (JTAG Layer)
├── Message Serialization Tests
├── Message Deserialization Tests
├── Error Handling Tests
└── Correlation Tests

Factory Tests (Configuration)
├── Config Reading Tests
├── Transport Creation Tests
├── Parameter Extraction Tests
└── Error Handling Tests
```

### **Integration Test Architecture**
```
Cross-Boundary Tests
├── Server ↔ Browser Communication
├── Event Broadcasting
├── Request-Response Patterns
└── Connection Management

System Tests  
├── Full Message Flow
├── Multiple Transport Types
├── Failover Scenarios
└── Performance Validation
```

### **Mock Strategy**
- **Unit Tests**: Mock at adapter boundaries, test real transports
- **Integration Tests**: Use real transports with test servers
- **System Tests**: Full system with all real components

## 🔄 **MIGRATION SAFETY PLAN**

### **Backward Compatibility Approach**
1. **Keep existing interfaces temporarily** for backward compatibility
2. **Implement new pattern alongside old**
3. **Switch consumers one by one**
4. **Remove old pattern when all consumers migrated**
5. **Validate `npm test` passes at each step**

### **Migration Checkpoints**
- [ ] Phase 1 complete: Pure interface defined, basic tests pass
- [ ] Phase 2 complete: All transports revised, unit tests pass
- [ ] Phase 3 complete: Adapter layer working, integration tests pass
- [ ] Phase 4 complete: Factories revised, factory tests pass
- [ ] Phase 5 complete: Router updated, system tests pass
- [ ] Phase 6 complete: Full validation passed, old code removed

## ✅ **SUCCESS CRITERIA FOR WIDGET/PERSONA DEVELOPMENT**

### **Radical Reconfiguration Resilience**
1. **Multiple widget instances**: Deploy widgets on different ports simultaneously
2. **Dynamic site assignment**: Same code works on localhost, staging, production domains
3. **AI persona integration**: Personas can direct HTML/CSS generation through router
4. **Protocol flexibility**: Swap WebSocket/HTTP without breaking widget development
5. **Zero hardcoded assumptions**: No localhost, port, or configuration coupling

### **Dumb Pipe Architecture Validation**  
1. **Pure transport interface**: `new WebSocketTransport("ws://any.domain:port")`
2. **Configuration abstraction**: Factory resolves ANY config to destination strings  
3. **Business logic separation**: All retry/health/correlation logic in orchestrator
4. **Content delivery support**: Router handles JSON messages AND HTML/CSS content
5. **Modular testing**: Each layer testable independently

### **Widget Development Workflow Enabled**
1. **Dynamic widget deployment**: Create widget instances on demand with different configs
2. **AI persona content routing**: Personas generate HTML/CSS routed through transport layer  
3. **Multi-site widget development**: Same widget code works across development environments
4. **Content-type flexibility**: Transport layer handles any content type (JSON, HTML, CSS, JS)
5. **Hot reconfiguration**: Change deployment parameters without code changes

## 📁 **IMPLEMENTATION FILES**

### **Files to Modify**
- `system/transports/shared/TransportTypes.ts` - Clean interface definition
- `system/transports/websocket-transport/` - All WebSocket transport files
- `system/transports/http-transport/` - All HTTP transport files
- `system/transports/udp-multicast-transport/` - All UDP transport files
- `system/transports/shared/TransportBase.ts` - Base class simplification
- `system/transports/browser/TransportFactoryBrowser.ts` - Factory revision
- `system/transports/server/TransportFactoryServer.ts` - Factory revision

### **Files to Create**
- `system/transports/shared/JTAGTransportAdapter.ts` - Business logic layer
- `tests/unit/transports/` - Comprehensive unit test suite
- `tests/integration/transports/` - Cross-boundary integration tests

### **Files to Remove** (after migration)
- `system/transports/shared/TransportConfig.ts` - Replaced by factory logic
- Any other obsolete transport configuration files

---

## 🤔 **CRITICAL ANALYSIS OF EACH PHASE**

### **Phase 1: Define Pure Transport Contract**

**Expectations:**
- Create a simple, universal interface that works for all transport types
- WebSocket, HTTP, and UDP can all implement the same interface

**Reality Check:**
- **WebSocket** is bidirectional, persistent connection
- **HTTP** is request-response, stateless
- **UDP** is packet-based, unreliable
- These are fundamentally different communication patterns

**Things to Look Out For:**
- ⚠️ **Interface Mismatch**: HTTP doesn't have "onMessage" - it's request-response only
- ⚠️ **Protocol Requirements**: WebSocket needs connection lifecycle, HTTP doesn't
- ⚠️ **Reliability Assumptions**: UDP might need packet ordering/reliability at transport level
- ⚠️ **Performance**: Generic interface might lose protocol-specific optimizations

**Revised Approach Needed:**
- Consider protocol-specific interfaces instead of one-size-fits-all
- Or create base interface with optional protocol-specific extensions

### **Phase 2: Strip Each Transport to Essentials**

**Expectations:**
- Clear separation between "transport protocol" and "business logic"
- Easy to identify what stays vs what goes

**Reality Check:**
- **WebSocket reconnection** - Is this protocol requirement or business logic?
- **HTTP connection pooling** - Transport optimization or business logic?
- **UDP packet ordering** - Protocol requirement or application concern?

**Things to Look Out For:**
- ⚠️ **False Business Logic**: Some "business logic" might be protocol requirements
- ⚠️ **Reliability Dependencies**: Current system might depend on transport-level reliability
- ⚠️ **Performance Degradation**: Removing optimizations might hurt performance
- ⚠️ **Hidden Dependencies**: Other parts of system might expect current transport behavior

**Validation Required:**
- Map all current transport features and classify: protocol vs business logic
- Check what external code depends on current transport interfaces
- Performance test stripped-down transports

### **Phase 3: Create Business Logic Layer**

**Expectations:**
- Clean adapter pattern with minimal overhead
- Easy JSON serialization/deserialization

**Reality Check:**
- **Performance Overhead**: Extra layer means extra function calls, memory allocations
- **Error Handling**: Adapter needs to handle both transport errors AND serialization errors
- **Type Safety**: JSON parsing loses compile-time type safety

**Things to Look Out For:**
- ⚠️ **Double Serialization**: Message might get serialized multiple times through layers
- ⚠️ **Error Context Loss**: Adapter errors might lose transport-specific context
- ⚠️ **Memory Overhead**: Adapter holds references to both transport and handlers
- ⚠️ **Correlation Breakage**: Message correlation IDs might get lost in JSON parsing

**Testing Critical:**
- Performance benchmarks vs current system
- Error propagation testing
- Memory leak testing with long-running connections

### **Phase 4: Fix Factory Pattern**

**Expectations:**
- Factory cleanly extracts config and creates simple transports
- Config complexity hidden from transports

**Reality Check:**
- **Context Structure**: `context.config.instance.ports.websocket_server` is complex
- **Environment Differences**: Browser vs server configs are different
- **Dynamic Config**: Ports might be dynamically assigned, not static

**Things to Look Out For:**
- ⚠️ **Factory Complexity**: Factory might become as complex as original transports
- ⚠️ **Config Coupling**: Factory still tightly coupled to context structure
- ⚠️ **Environment Assumptions**: Factory assumptions might not work across environments
- ⚠️ **Circular Dependencies**: Factory needs context, context might need factory

**Design Questions:**
- Should factory be environment-specific? (BrowserTransportFactory vs ServerTransportFactory)
- How to handle dynamic port allocation?
- What happens when config is missing or invalid?

### **Phase 5: Update Router Integration**

**Expectations:**
- Router becomes simpler by delegating to factories
- Clean separation between routing logic and transport management

**Reality Check:**
- **Current Router Complexity**: Router has 900+ lines, handles many edge cases
- **Transport Lifecycle**: Router currently manages transport connection/disconnection
- **Message Correlation**: Router tracks request/response correlation across transports

**Things to Look Out For:**
- ⚠️ **Router Performance**: Adapter layer might slow down message routing
- ⚠️ **Correlation Breakage**: Existing correlation tracking might break
- ⚠️ **Connection Management**: Who manages transport connections lifecycle?
- ⚠️ **Error Handling**: Current router error handling might not work with adapters

**High Risk Areas:**
- Cross-environment message routing (server ↔ browser)
- Response correlation system (the one we just fixed!)
- Connection health monitoring
- Message deduplication

### **Phase 6: Cross-Boundary Validation**

**Expectations:**
- Everything works the same as before, just cleaner
- Cross-boundary communication is unaffected

**Reality Check:**
- **Cross-boundary is most fragile**: Server↔browser communication fails often
- **Event System Dependencies**: Event bridging has complex routing logic
- **Timing Dependencies**: Current system might have timing assumptions

**Things to Look Out For:**
- ⚠️ **Event System Breakage**: Event bridging is already complex, might break completely
- ⚠️ **WebSocket Connection Timing**: Browser WebSocket connection timing is finicky
- ⚠️ **Message Ordering**: Current system might guarantee message ordering
- ⚠️ **Connection Recovery**: Current reconnection logic might be essential

**Critical Test Scenarios:**
- Browser refreshes during active connections
- Network interruptions and recovery
- Multiple concurrent browser connections
- Server restart scenarios

## 🚨 **MAJOR RISKS IDENTIFIED**

### **1. Over-Engineering Risk**
**Problem**: Creating a "perfect" architecture that's actually worse than current system
**Mitigation**: Start with minimal changes, measure impact at each step

### **2. Performance Regression Risk**  
**Problem**: Adapter layers and generic interfaces might slow down the system
**Mitigation**: Performance benchmarks at each phase, rollback if regression detected

### **3. Cross-Boundary Communication Breakage**
**Problem**: Server↔browser communication is fragile, might break completely
**Mitigation**: Phase 6 integration tests MUST pass before proceeding

### **4. Event System Dependency Risk**
**Problem**: Event bridging has complex dependencies on current transport behavior
**Mitigation**: Map all event system dependencies before making transport changes

### **5. Configuration Complexity Risk**
**Problem**: Current config system is complex, factory might not simplify it
**Mitigation**: Consider config simplification as separate effort, don't over-scope

## 🔄 **REVISED IMPLEMENTATION APPROACH**

### **Start Small:**
1. Pick ONE transport type (HTTP - simplest)
2. Create pure version alongside existing version
3. Test side-by-side, measure performance
4. Only proceed if improvement is measurable

### **Validate Continuously:**
- Run `npm test` after every single change
- Performance benchmarks at every phase
- Cross-boundary tests before any router changes

### **Rollback Plan:**
- Keep old interfaces until new system 100% proven
- Feature flags to switch between old/new systems
- Immediate rollback if any tests fail

---

## 🔍 **SPECIFIC DIAGNOSTIC COMMANDS & VALIDATION STEPS**

### **Phase 1: Interface Mismatch Detection**

**🚨 Issue**: HTTP doesn't have "onMessage" - forcing different protocols into same interface

**Diagnostic Commands:**
```bash
# Check current HTTP transport usage patterns
grep -r "onMessage\|setMessageHandler" system/transports/http-transport/
grep -r "connect.*http" system/core/router/

# Check WebSocket vs HTTP differences
diff system/transports/websocket-transport/shared/WebSocketTransportClient.ts \
     system/transports/http-transport/shared/HTTPTransport.ts

# Find all transport interface consumers
grep -r "JTAGTransport\|TransportBase" system/core/router/
```

**What to Look For:**
- ❌ HTTP transport with `onMessage` method (doesn't make sense)
- ❌ Router expecting persistent connections from HTTP
- ❌ Same interface used for fundamentally different protocols

**Validation Before Proceeding:**
```bash
# Test each transport's natural usage pattern
node -e "
const http = require('./system/transports/http-transport/shared/HTTPTransport.ts');
console.log('HTTP methods:', Object.getOwnPropertyNames(http.HTTPTransport.prototype));
"

# Check if forced interface breaks protocol semantics
grep -A5 -B5 "send.*HTTP" system/transports/http-transport/
```

### **Phase 2: Protocol vs Business Logic Classification**

**🚨 Issue**: WebSocket reconnection - protocol requirement or business logic?

**Diagnostic Commands:**
```bash
# Find all "business logic" that might be protocol requirements
grep -r "reconnect\|retry\|timeout\|ping\|pong" system/transports/
grep -r "session\|correlation\|handler" system/transports/

# Check what external code depends on these features
grep -r "reconnect" system/core/router/ system/core/client/
grep -r "setMessageHandler" system/core/router/ system/core/client/

# Look for reliability assumptions
grep -r "connected\|disconnect\|error.*transport" system/core/
```

**What to Look For:**
- 🔍 **WebSocket reconnection**: Used by router? Required for reliability?
- 🔍 **Connection state management**: Does router assume persistent connections?
- 🔍 **Message handlers**: Are these transport features or business logic?

**Classification Test:**
```bash
# Create minimal WebSocket without "business logic" and see what breaks
cat > test-minimal-websocket.js << 'EOF'
class MinimalWebSocket {
  constructor(url) { this.ws = new WebSocket(url); }
  send(data) { this.ws.send(data); }
  close() { this.ws.close(); }
}
// Test if router can work with this minimal version
EOF

# Check dependency chains
grep -r "reconnectAttempts\|pingInterval" system/core/
```

### **Phase 3: Performance Overhead Detection**

**🚨 Issue**: Adapter layer causing performance regression

**Diagnostic Commands:**
```bash
# Benchmark current message throughput
cd examples/test-bench
node -e "
const start = Date.now();
for(let i = 0; i < 1000; i++) {
  // Test current transport performance
}
console.log('Current time:', Date.now() - start);
"

# Count current serialization passes
grep -r "JSON.stringify\|JSON.parse" system/transports/
grep -r "serialize\|deserialize" system/core/router/

# Find current memory allocation patterns
grep -r "new.*Transport\|new.*Message" system/core/router/
```

**What to Look For:**
- 📊 **Baseline performance**: How fast is current system?
- 🔄 **Serialization count**: How many times is data serialized currently?
- 💾 **Memory patterns**: How much memory do current transports use?

**Performance Testing Setup:**
```bash
# Create performance test for current vs new
cat > transport-performance-test.js << 'EOF'
function testCurrentTransport() {
  const start = Date.now();
  // ... existing transport code
  return Date.now() - start;
}

function testNewAdapter() {
  const start = Date.now();
  // ... new adapter code  
  return Date.now() - start;
}

console.log('Current:', testCurrentTransport());
console.log('New:', testNewAdapter());
console.log('Regression:', (testNewAdapter() - testCurrentTransport()));
EOF
```

### **Phase 4: Factory Complexity Assessment**

**🚨 Issue**: Factory might become as complex as original transports

**Diagnostic Commands:**
```bash
# Measure current config complexity
find system/transports/ -name "*Config*" -exec wc -l {} \;
grep -r "context\.config\." system/transports/ | wc -l

# Check environment-specific config paths  
grep -r "instance\.ports\." system/transports/
grep -r "browser.*config\|server.*config" system/transports/

# Find circular dependency risks
grep -r "TransportFactory" system/core/types/ system/core/shared/
```

**What to Look For:**
- 📏 **Config complexity**: How many lines of config code?
- 🌍 **Environment differences**: Browser vs server config paths different?
- 🔄 **Circular dependencies**: Does context import factory types?

**Factory Complexity Test:**
```bash
# Count lines of factory logic after extraction
cat > test-factory-complexity.sh << 'EOF'
echo "Current transport config logic:"
grep -r "config\." system/transports/ | wc -l

echo "Proposed factory config logic:"
# Count lines needed to extract all config reading
EOF
```

### **Phase 5: Router Integration Risks**

**🚨 Issue**: Breaking response correlation and cross-environment routing

**Diagnostic Commands:**
```bash
# Check current router-transport integration points
grep -n "transport\." system/core/router/shared/JTAGRouter.ts
grep -n "correlationId" system/core/router/shared/JTAGRouter.ts

# Find all cross-environment routing paths
grep -r "browser.*server\|server.*browser" system/core/router/
grep -r "extractEnvironment\|routeRemotely" system/core/router/

# Check message correlation dependencies
grep -r "ResponseCorrelator" system/core/router/
grep -A10 -B10 "resolveRequest" system/core/router/shared/JTAGRouter.ts
```

**What to Look For:**
- 🔗 **Correlation tracking**: How does router track request/response pairs?
- 🌉 **Cross-boundary routing**: Where does server→browser routing happen?
- ⚡ **Message flow**: What's the exact path of a cross-environment message?

**Router Integration Validation:**
```bash
# Test current correlation flow
grep -A20 "handleIncomingResponse" system/core/router/shared/JTAGRouter.ts

# Map the exact message flow
echo "Current message flow:"
echo "1. Router.postMessage() ->" 
grep -A5 "postMessage" system/core/router/shared/JTAGRouter.ts | head -5
echo "2. Transport layer ->"
echo "3. Response correlation ->"
grep -A5 "resolveRequest" system/core/shared/ResponseCorrelator.ts | head -5
```

### **Phase 6: Cross-Boundary Communication Validation**

**🚨 Issue**: Event bridging and server↔browser communication breakage

**Diagnostic Commands:**
```bash
# Check current event bridging dependencies
grep -r "EventsBridge\|routeToOtherEnvironments" daemons/events-daemon/
grep -r "cross.*environment\|bridge.*event" system/events/

# Test current cross-boundary communication
cd examples/test-bench
npm start &
sleep 10
curl -X POST http://localhost:9002/api/jtag/message \
  -H "Content-Type: application/json" \
  -d '{"test": "cross-boundary"}'

# Check WebSocket connection stability
node -e "
const ws = new WebSocket('ws://localhost:9001');
ws.onopen = () => console.log('Connected');
ws.onerror = (err) => console.log('Error:', err);
setTimeout(() => ws.close(), 1000);
"
```

**What to Look For:**
- 🌉 **Event bridge paths**: How do events cross server↔browser boundary?
- 🔌 **WebSocket reliability**: How stable are current connections?
- 📨 **Message ordering**: Are messages guaranteed to arrive in order?

**Cross-Boundary Test Setup:**
```bash
# Create comprehensive cross-boundary test
cat > test-cross-boundary.js << 'EOF'
async function testServerToBrowser() {
  // Test server→browser message delivery
  console.log('Testing server→browser...');
  // ... test implementation
}

async function testBrowserToServer() {
  // Test browser→server message delivery  
  console.log('Testing browser→server...');
  // ... test implementation
}

async function testEventBridging() {
  // Test event system cross-boundary
  console.log('Testing event bridging...');
  // ... test implementation
}

// Run all tests and report failures
Promise.all([
  testServerToBrowser(),
  testBrowserToServer(), 
  testEventBridging()
]).then(results => {
  console.log('Cross-boundary test results:', results);
});
EOF
```

### **Major Risk Detection Commands**

**Over-Engineering Risk:**
```bash
# Count current vs proposed lines of code
find system/transports/ -name "*.ts" -exec wc -l {} \; | awk '{sum+=$1} END {print "Current lines:", sum}'

# Measure actual complexity reduction
grep -r "new.*Transport" system/core/ | wc -l
echo "vs proposed adapter calls"
```

**Performance Regression Risk:**
```bash
# Create before/after performance benchmark
npm test -- --grep "performance" 2>&1 | grep -E "ms|time|duration"

# Memory usage monitoring
node --expose-gc -e "
global.gc();
console.log('Memory before:', process.memoryUsage());
// ... transport operations
global.gc();
console.log('Memory after:', process.memoryUsage());
"
```

**Event System Dependency Mapping:**
```bash
# Map all event system transport dependencies
grep -r "transport.*event\|event.*transport" system/events/
grep -r "EventsDaemon.*router\|router.*EventsDaemon" daemons/events-daemon/

# Check event routing assumptions
grep -A10 -B10 "routeToOtherEnvironments" daemons/events-daemon/shared/EventsDaemon.ts
```

### **Red Flag Detection**

**Immediate Stop Conditions:**
```bash
# If any of these commands show problems, STOP immediately:

# 1. npm test fails
npm test || echo "🛑 STOP: Tests failing"

# 2. Cross-boundary communication fails
timeout 10s node test-cross-boundary.js || echo "🛑 STOP: Cross-boundary broken"

# 3. Performance regression > 20%
# (compare current vs new performance numbers)

# 4. Memory usage increases significantly
# (compare memory before/after numbers)

# 5. Event system stops working
grep "EventsDaemon.*error" .continuum/jtag/system/logs/*.log && echo "🛑 STOP: Events broken"
```

---

**Implementation Rule**: Run ALL diagnostic commands for a phase BEFORE making any changes. If any red flags detected, revise approach before proceeding.