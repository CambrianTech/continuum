# Chat Drain Worker

**Status**: Architecture Complete, Implementation Stub
**Type**: Rust Worker (Reference Implementation)
**Purpose**: Process chat messages off the Node.js main thread

---

## Overview

The **Chat Drain Worker** is the first complete implementation of the Universal Worker Protocol. It demonstrates the power plant architecture where critical operations run in isolated processes that can be monitored, maintained, and healed independently.

**What it solves**: During high AI activity (6+ personas responding simultaneously), the Node.js main thread becomes overloaded with:
- RAG context building (database queries)
- AI API calls (external HTTP requests)
- Tool execution coordination
- Message persistence

This worker moves all that heavy processing off the main thread into a separate Rust process.

---

## Architecture

### Queue-Based Processing
```
Node.js Main Thread (fast)
    ↓ Unix Socket
Chat Drain Worker (Rust process)
    ↓ Connection Handler (non-blocking)
    ↓ mpsc::channel (queue)
    ↓ Background Processor Thread
    ↓ Heavy Operations:
       - RAG context building
       - AI API calls
       - Tool execution
       - Response persistence
```

### Universal Protocol Implementation

This worker is the **reference implementation** of the Universal Worker Protocol, implementing all three required interfaces:

#### 1. Health Check (`ping`)
```json
Request:  { "type": "ping", "id": "uuid" }
Response: {
  "success": true,
  "data": {
    "uptime_ms": 123456,
    "queue_depth": 42,
    "processed_total": 1500,
    "errors_total": 3,
    "memory_mb": 12.5,
    "status": "healthy"
  }
}
```

#### 2. Graceful Shutdown (`shutdown`)
```json
Request:  { "type": "shutdown", "id": "uuid", "data": { "timeout_ms": 10000, "force": false } }
Response: {
  "success": true,
  "data": {
    "queue_drained": 25,
    "shutdown_time_ms": 3450
  }
}
```

#### 3. Status/Diagnostics (`status`)
```json
Request:  { "type": "status", "id": "uuid", "data": { "verbose": true } }
Response: {
  "success": true,
  "data": {
    "worker_type": "chat-drain",
    "version": "1.0.0",
    "pid": 12345,
    "uptime_ms": 3600000,
    "status": "healthy",
    "metrics": { "queue_depth": 5, "processed_total": 15000, ... },
    "resources": { "memory_mb": 18.5, "threads": 4, ... }
  }
}
```

---

## File Structure

```
chat-drain/
├── src/
│   ├── main.rs               # Socket listener, queue setup, thread spawning
│   ├── connection_handler.rs # Message parsing and routing (universal + chat)
│   ├── processor.rs          # Background chat processing (stub)
│   ├── health.rs             # Universal protocol implementation (COMPLETE)
│   └── messages.rs           # Chat-specific types with ts-rs
├── bindings/                 # Generated TypeScript types
│   ├── PingResult.ts
│   ├── ShutdownResult.ts
│   ├── StatusResult.ts
│   ├── ChatMessagePayload.ts
│   └── ...
├── Cargo.toml               # Dependencies
└── README.md                # This file
```

---

## Implementation Status

### ✅ Complete (Reference Implementation)
- **Universal Protocol**: ping, shutdown, status fully implemented
- **Queue Architecture**: Non-blocking fast path + background processing
- **Health Tracking**: Stats for uptime, throughput, errors, queue depth
- **TypeScript Bindings**: All types exported with ts-rs
- **Graceful Shutdown**: Drains queue before exit
- **Multi-threaded**: Concurrent connection handling

### 🚧 Stub (To Be Implemented)
- **RAG Context Building**: Database queries, embeddings (stub: 10ms sleep)
- **AI API Calls**: OpenAI, Anthropic, etc. (stub: 50ms sleep)
- **Tool Execution**: Coordination with PersonaToolExecutor (stub: 20ms sleep)
- **Response Persistence**: Save to database, emit events (stub: 5ms sleep)

**Why stub?** The architecture and protocol are proven. The domain-specific logic can be filled in incrementally without changing the structure.

---

## Usage

### Building
```bash
cd workers/chat-drain
cargo build --release
```

### Running
```bash
cargo run --release -- /tmp/chat-drain-worker.sock
```

### Testing Protocol
```bash
# Health check
echo '{"type":"ping","id":"test-1"}' | nc -U /tmp/chat-drain-worker.sock

# Status (verbose)
echo '{"type":"status","id":"test-2","data":{"verbose":true}}' | nc -U /tmp/chat-drain-worker.sock

# Graceful shutdown
echo '{"type":"shutdown","id":"test-3","data":{"timeout_ms":5000,"force":false}}' | nc -U /tmp/chat-drain-worker.sock
```

### Processing Chat
```bash
echo '{"type":"process-chat","id":"test-4","data":{"room_id":"general","sender_id":"user-1","sender_name":"Alice","content":"Hello world"}}' | nc -U /tmp/chat-drain-worker.sock
```

---

## Integration with Node.js

### TypeScript Client (To Be Created)
```typescript
// system/workers/chat-drain/ChatDrainClient.ts
export class ChatDrainClient {
  private socket: net.Socket;

  async ping(): Promise<PingResult> {
    return this.sendMessage({ type: 'ping', id: uuid() });
  }

  async processChatMessage(payload: ChatMessagePayload): Promise<ChatProcessResult> {
    return this.sendMessage({
      type: 'process-chat',
      id: uuid(),
      data: payload
    });
  }

  async shutdown(timeout = 10000): Promise<ShutdownResult> {
    return this.sendMessage({
      type: 'shutdown',
      id: uuid(),
      data: { timeout_ms: timeout, force: false }
    });
  }

  async status(verbose = false): Promise<StatusResult> {
    return this.sendMessage({
      type: 'status',
      id: uuid(),
      data: { verbose }
    });
  }
}
```

### Integration Points
- **Chat message creation**: Instead of processing in Node.js, send to chat-drain worker
- **Persona response generation**: Offload to worker
- **RAG context queries**: Worker handles database access
- **AI API calls**: Worker makes external HTTP requests

---

## Performance Characteristics

### Fast Path (Non-Blocking)
- **Connection handling**: < 1ms (spawns thread)
- **Message queuing**: < 1ms (mpsc::send)
- **Response time**: < 5ms total (queue + ack)

### Slow Path (Background)
- **Chat processing**: 85ms (stub simulation)
  - RAG context: 10ms
  - AI API calls: 50ms
  - Tool execution: 20ms
  - Persistence: 5ms
- **Real implementation**: Variable (AI API latency dominates)

### Throughput
- **Queue depth**: Unbounded (monitor for backpressure)
- **Concurrent connections**: Unlimited (thread-per-connection)
- **Processing rate**: ~10 messages/sec (with stubs), real rate TBD

---

## Monitoring & Maintenance

### AI Persona Integration

AI personas can act as system mechanics:

```bash
# Check worker health
./jtag worker/health --worker=chat-drain

# View detailed status
./jtag worker/status --worker=chat-drain --verbose

# Restart if unhealthy
./jtag worker/restart --worker=chat-drain
```

**Via Tools**:
```xml
<tool_name="worker/health">
  <param name="worker">chat-drain</param>
</tool_name>
```

### Status Interpretation

**Healthy** (`status: "healthy"`):
- Queue depth < 800
- Error rate < 5%
- Processing normally

**Degraded** (`status: "degraded"`):
- Queue depth > 800 (backpressure building)
- Still processing but falling behind
- Consider scaling (multiple workers)

**Failing** (`status: "failing"`):
- Error rate > 5%
- Repeated failures
- Requires restart or investigation

---

## Future Enhancements

### Phase 1: Complete Stub Implementation
- Implement RAG context building
- Integrate AI API clients
- Add tool execution coordination
- Persist responses to database

### Phase 2: Advanced Features
- **Worker pool**: Multiple chat-drain instances for load balancing
- **Priority queue**: Urgent messages processed first
- **Circuit breaker**: Automatic backoff on AI API failures
- **Metrics export**: Prometheus endpoint for monitoring

### Phase 3: Worker-to-Worker Communication
- **Chat → AI Provider**: Delegate AI calls to dedicated worker
- **Chat → Logger**: Send logs directly
- **Chat → Data**: Persist responses via dedicated worker

---

## Reference for Future Workers

This worker serves as the template for:
- **AI Provider Worker**: Isolated AI API calls
- **Genome Training Worker**: LoRA fine-tuning without crashing system
- **Data Persistence Worker**: Database operations off main thread

**Copy this structure**, replace `processor.rs` with domain logic, keep the universal protocol implementation intact.

---

## Philosophy

> "Every worker is a unit in a power plant. Each has gauges (status), controls (shutdown), and safety mechanisms (health checks) that operators (AI personas) can inspect and manipulate."

The Chat Drain Worker embodies this philosophy:
- **Isolated**: Crashes don't affect Node.js or other workers
- **Observable**: Full visibility into health and metrics
- **Controllable**: Can be shut down gracefully
- **Self-healing**: Can be restarted automatically
- **AI-maintainable**: AI personas can monitor and fix issues

This is the foundation for an autonomous, self-healing system.
