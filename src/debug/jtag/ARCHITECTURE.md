# JTAG Architecture - Pluggable Transport Abstraction

## 🏗️ **Core Architecture Overview**

JTAG is built around a **pluggable transport abstraction** that separates business logic from network concerns, enabling seamless integration with any messaging infrastructure while maintaining consistent debugging functionality.

### **Architecture Principles**

1. **Transport Independence**: Console routing and file creation work identically regardless of network layer
2. **Smart Fallbacks**: Automatic detection and failover ensure debugging always works
3. **Business Logic Isolation**: Core functionality testable without network dependencies
4. **Universal Integration**: Adapts to any host system (Continuum, custom infrastructure, standalone)
5. **Zero Configuration**: Auto-detects and configures optimal transport automatically

## 📡 **Transport Abstraction Layer**

### **Core Transport Interface**

```typescript
interface JTAGTransport {
  name: string;
  initialize(config: JTAGConfig): Promise<boolean>;
  send<T>(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse<T>>;
  isConnected(): boolean;
  disconnect(): Promise<void>;
  onMessage?(handler: (message: JTAGWebSocketMessage) => void): void;
  onDisconnect?(handler: () => void): void;
}
```

### **Smart Transport Manager**

The `JTAGSmartTransport` implements intelligent transport selection and failover:

```
Priority Chain:
1. Host Transport Detection → Auto-detect Continuum WebSocket daemon
2. Primary Transport       → User-specified or default WebSocket  
3. Fallback Transport      → HTTP polling or custom fallback
4. Message Queue          → Offline storage with auto-flush
```

### **Built-in Transport Implementations**

**1. `ContinuumWebSocketTransport`** - Auto-Integration
- Detects Continuum's existing WebSocket daemon infrastructure
- Routes JTAG messages through Continuum's logger daemon
- Zero configuration required when running within Continuum

**2. `DefaultWebSocketTransport`** - Standalone WebSocket
- Creates own WebSocket server on port 9001
- Direct browser ↔ server communication
- Used when no host transport detected

**3. `HTTPPollingTransport`** - Fallback HTTP
- REST API with polling for bidirectional communication
- Used when WebSocket connections fail
- Stateless, reliable fallback option

**4. `CustomTransport`** - Extensible Integration
- Interface for custom transport implementations
- Register new transport types at runtime
- Integrate with any messaging system (Redis, gRPC, Kafka, etc.)

## 🔄 **Message Flow Architecture**

### **Console Routing Flow (Example #1: Client-Side)**

```
console.log("message")
    ↓
originalConsole.log("message")    // Preserve normal console output
    ↓
jtag.log("CONSOLE", "message")    // Route to JTAG
    ↓
JTAGSmartTransport.send(message)  // Transport abstraction
    ↓
[Transport Selection Logic]
    ├─ ContinuumTransport.send()     // If Continuum detected
    ├─ WebSocketTransport.send()     // If WebSocket available
    ├─ HTTPTransport.send()          // If HTTP available
    └─ MessageQueue.enqueue()        // If all transports fail
    ↓
Server Reception & Processing
    ↓
Logger.processLogMessage()        // Business logic (transport-agnostic)
    ↓
File Creation (Steps 5-7)         // Template-based file creation
    ├─ Check: server.log.txt exists
    ├─ Create: from templates/ if needed
    └─ Append: log entry to .txt and .json files
```

### **Server-Side Direct Flow (Example #2)**

```
console.warn("warning")
    ↓
originalConsole.warn("warning")   // Preserve normal console output
    ↓
jtag.warn("CONSOLE", "warning")   // Route to JTAG
    ↓
[Skip Transport - Direct Local Processing]
    ↓
Logger.processLogMessage()        // Same business logic
    ↓
File Creation (Steps 5-7)         // Same file creation logic
```

## 🧩 **Business Logic Isolation**

### **Core Components (Transport-Agnostic)**

**1. Console Interception**
```typescript
// Preserved across all transport types
const originalConsole = {
  log: console.log.bind(console),
  warn: console.warn.bind(console),
  error: console.error.bind(console)
};

// Console override that works regardless of transport
console.log = (message: string, ...args: any[]) => {
  originalConsole.log(message, ...args);  // Preserve normal output
  jtag.log('CONSOLE', message, args);     // Route through transport abstraction
};
```

**2. Log Processing Engine**
```typescript
// Business logic completely isolated from transport concerns
class LogProcessor {
  static async processLogEntry(entry: JTAGLogEntry): Promise<void> {
    // Step 5: Check if platform.level.txt exists
    const textFile = `${entry.context}.${entry.type}.txt`;
    const jsonFile = `${entry.context}.${entry.type}.json`;
    
    // Step 6: Create from templates if needed
    if (!fs.existsSync(textFile)) {
      await this.createFromTemplate(textFile, 'txt');
    }
    if (!fs.existsSync(jsonFile)) {
      await this.createFromTemplate(jsonFile, 'json');
    }
    
    // Step 7: Append log entry
    await this.appendToFiles(entry, textFile, jsonFile);
  }
}
```

**3. File Creation System**
```typescript
// Template-driven file creation (transport-independent)
class FileManager {
  static async createFromTemplate(filename: string, type: 'txt' | 'json'): Promise<void> {
    const template = await this.loadTemplate(`log-template.${type}`);
    const content = this.substituteVariables(template, {
      platform: this.extractPlatform(filename),
      level: this.extractLevel(filename),
      timestamp: new Date().toISOString()
    });
    await fs.writeFile(filename, content);
  }
}
```

## 🎯 **Integration Patterns**

### **Automatic Host Detection**

```typescript
class JTAGTransportFactory {
  detectHostTransport(): JTAGTransport | null {
    // Browser context
    if (typeof window !== 'undefined') {
      if ((window as any).continuum?.daemonConnector) {
        return new ContinuumWebSocketTransport();
      }
    }
    
    // Node.js context
    try {
      require.resolve('../../../integrations/websocket/core/DaemonConnector');
      return new ContinuumWebSocketTransport();
    } catch {
      // Continuum not available
    }
    
    return null;
  }
}
```

### **Custom Transport Integration**

```typescript
// Example: Redis Pub/Sub Transport
class RedisPubSubTransport implements JTAGTransport {
  name = 'redis-pubsub';
  
  async initialize(config: JTAGConfig): Promise<boolean> {
    this.redis = await Redis.connect(config.redisUrl);
    return this.redis.isConnected();
  }
  
  async send(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse> {
    await this.redis.publish('jtag-logs', JSON.stringify(message));
    return { success: true, timestamp: new Date().toISOString() };
  }
}

// Register custom transport
transportFactory.registerTransport('redis-pubsub', () => new RedisPubSubTransport());
```

## 🧪 **Testing Architecture**

### **Mock Transport Strategy**

The transport abstraction enables **zero-dependency testing** through comprehensive mock implementations:

**1. Deterministic Testing**
```typescript
const mockTransport = new MockSuccessTransport();
jtag.useTransport(mockTransport);

console.log('test message');  // Predictable behavior
assert(mockTransport.getMessages().length === 1);
```

**2. Failure Scenario Testing**  
```typescript
const failingTransport = new MockFailureTransport();
jtag.useTransport(failingTransport);

console.log('test message');  // Should queue message locally
// Verify business logic continues working despite transport failure
assert(fs.existsSync('.continuum/jtag/logs/server.log.txt'));
```

**3. Network Condition Simulation**
```typescript
const networkTransport = new MockNetworkTransport(latency: 500, dropRate: 0.2);
// Test behavior under realistic network conditions without real network
```

### **Layer-Based Testing Strategy**

**Layer 1: Transport Interface Compliance**
- Mock transports validate interface compliance
- Test fallback logic without network dependencies
- Validate message queuing mechanisms

**Layer 2: Business Logic Isolation**  
- Test console routing with mock transports
- Validate file creation (steps 5-7) independently
- Ensure resilience to transport failures

**Layer 4: Transport Integration**
- Test real transport implementations
- Validate end-to-end message flow
- Integration with actual network infrastructure  

**Layer 6: Full System Validation**
- Browser automation with real WebSocket connections
- Complete console routing through actual transport layers
- Screenshot functionality with transport communication

## 📊 **Configuration System**

### **Transport Configuration**

```typescript
interface JTAGTransportConfig {
  type: 'websocket' | 'http' | 'continuum-ws' | 'custom';
  fallback?: 'http' | 'queue';
  customTransport?: JTAGTransport;
  retryAttempts?: number;
  retryDelay?: number;
}

// Package.json configuration
{
  "config": {
    "transport": {
      "type": "websocket",
      "fallback": "http",
      "retryAttempts": 3
    }
  }
}
```

### **Auto-Configuration Logic**

```typescript
class JTAGConfigManager {
  static determineOptimalTransport(): JTAGTransportConfig {
    // 1. Check for host transport (Continuum)
    const hostTransport = transportFactory.detectHostTransport();
    if (hostTransport) {
      return { type: 'continuum-ws', fallback: 'http' };
    }
    
    // 2. Use user configuration
    const userConfig = this.loadUserConfig();
    if (userConfig.transport) {
      return userConfig.transport;
    }
    
    // 3. Default configuration
    return { type: 'websocket', fallback: 'http' };
  }
}
```

## 🚀 **Performance & Reliability**

### **Message Queuing & Buffering**

```typescript
class MessageQueue {
  private queue: JTAGWebSocketMessage[] = [];
  private maxSize = 1000;
  
  enqueue(message: JTAGWebSocketMessage): void {
    if (this.queue.length >= this.maxSize) {
      this.queue.shift(); // Remove oldest message to prevent memory leaks
    }
    this.queue.push(message);
  }
  
  async flush(transport: JTAGTransport): Promise<void> {
    // Automatically flush when transport becomes available
    while (this.queue.length > 0 && transport.isConnected()) {
      const message = this.queue.shift()!;
      await transport.send(message);
    }
  }
}
```

### **Resilience Patterns**

**1. Graceful Degradation**
- Transport failure → Queue messages locally
- Network issues → Switch to HTTP fallback  
- All transports down → Continue local logging

**2. Automatic Recovery**
- Periodic retry attempts on failed transports
- Automatic queue flush when transport recovers
- Connection health monitoring and reconnection

**3. Resource Management**
- Message queue size limits prevent memory leaks
- Connection pooling for HTTP transport
- Cleanup on disconnect to prevent resource exhaustion

## 🎯 **Implementation Benefits**

### **Developer Experience**
- **Zero Configuration**: Works out of the box with optimal transport selection
- **Universal API**: Same `jtag.log()` works across all transport types
- **Failure Transparency**: Debugging continues working even when transport fails
- **Visual Validation**: Screenshot functionality across all transport types

### **System Integration**
- **Infrastructure Agnostic**: Adapts to any messaging system
- **Host System Detection**: Automatically uses existing infrastructure
- **Custom Transport Support**: Easy integration with proprietary systems
- **Backward Compatibility**: Existing JTAG code continues working unchanged

### **Testing & Reliability**
- **Mock Transport Testing**: Business logic testable without network
- **Deterministic Behavior**: Predictable test outcomes
- **Failure Scenario Validation**: Easy testing of edge cases
- **Performance Testing**: Network condition simulation without real network

### **Scalability & Maintenance**
- **Modular Architecture**: Transport layer completely replaceable
- **Clean Abstractions**: Business logic separated from transport concerns
- **Extensible Design**: New transport types added without core changes
- **Future-Proof**: Architecture supports unknown future transport requirements

---

**The transport abstraction transforms JTAG from a debugging tool into a universal debugging platform that adapts to any infrastructure while maintaining consistent, reliable functionality across all environments.**