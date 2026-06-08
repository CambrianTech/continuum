# Universal Worker Protocol

**Version**: 1.0
**Status**: Specification
**Purpose**: Standard protocol that ALL Rust workers must implement for monitoring, health checks, and self-healing.

---

## Philosophy: Power Plant Architecture

Every worker is an independent process that can:
- **Crash without taking down the system**
- **Be monitored by supervisors** (including AI personas)
- **Self-heal** through automatic restarts
- **Report health** in a standardized format
- **Drain gracefully** when shutting down

Think of workers as units in a power plant - each has gauges, controls, and safety mechanisms that operators (AI personas) can inspect and manipulate.

---

## Core Requirements

Every worker MUST implement these three interfaces:

### 1. Health Check Interface (`ping`)
**Purpose**: Prove the worker is alive and report basic metrics.

**Request**:
```json
{
  "type": "ping",
  "id": "uuid-here"
}
```

**Response**:
```json
{
  "type": "pong",
  "id": "uuid-here",
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

**Status Values**:
- `healthy` - Operating normally
- `degraded` - Functioning but experiencing issues (high queue depth, memory pressure)
- `failing` - Critical issues detected (repeated errors, resource exhaustion)

**Implementation Notes**:
- MUST respond within 1 second
- MUST NOT block on I/O operations
- Should track uptime, throughput, and error rates

---

### 2. Graceful Shutdown Interface (`shutdown`)
**Purpose**: Allow controlled shutdown with queue draining.

**Request**:
```json
{
  "type": "shutdown",
  "id": "uuid-here",
  "data": {
    "timeout_ms": 10000,
    "force": false
  }
}
```

**Response**:
```json
{
  "type": "shutdown-ack",
  "id": "uuid-here",
  "success": true,
  "data": {
    "queue_drained": 25,
    "shutdown_time_ms": 3450
  }
}
```

**Shutdown Sequence**:
1. Stop accepting new work
2. Drain existing queue (up to timeout)
3. Flush any pending writes
4. Close connections
5. Exit cleanly

**Implementation Notes**:
- If `force: true`, exit immediately without draining
- If timeout exceeded, save queue state and exit
- MUST respond before exiting (send ack first)

---

### 3. Status/Diagnostics Interface (`status`)
**Purpose**: Detailed inspection for debugging and monitoring.

**Request**:
```json
{
  "type": "status",
  "id": "uuid-here",
  "data": {
    "verbose": true
  }
}
```

**Response**:
```json
{
  "type": "status-result",
  "id": "uuid-here",
  "success": true,
  "data": {
    "worker_type": "chat-drain",
    "version": "1.0.0",
    "pid": 12345,
    "uptime_ms": 3600000,
    "status": "healthy",
    "metrics": {
      "queue_depth": 5,
      "queue_capacity": 1000,
      "processed_total": 15000,
      "errors_total": 12,
      "error_rate": 0.0008,
      "avg_processing_time_ms": 45.2,
      "p99_processing_time_ms": 120.5
    },
    "resources": {
      "memory_mb": 18.5,
      "memory_limit_mb": 512,
      "cpu_percent": 2.3,
      "threads": 4
    },
    "diagnostics": {
      "last_error": "AI API timeout",
      "last_error_time": 1234567890,
      "active_connections": 3,
      "backpressure": false
    }
  }
}
```

**Implementation Notes**:
- If `verbose: false`, return only summary metrics
- Include enough info for AI personas to diagnose issues
- Report resource limits to detect capacity issues

---

## Message Format (JTAGMessage)

All worker communication uses the JTAGMessage format (defined in `jtag_protocol.rs`):

```rust
pub struct JTAGMessage {
    pub id: String,          // UUID for request/response matching
    pub type: String,        // Message type (ping, shutdown, etc.)
    pub data: serde_json::Value,  // Type-specific payload
}
```

**Why JSON?**
- Human-readable for debugging
- Easy TypeScript interop
- Flexible schema evolution
- Tooling support (jq, grep, logs)

**Transport**: Unix Domain Sockets (like logger worker)
- Low overhead (no TCP)
- File system permissions for security
- Works locally (no network exposure)

---

## Standard Architecture Pattern

Every worker follows this structure:

```rust
// main.rs - Socket listener + queue creation
fn main() {
    let listener = UnixListener::bind(socket_path)?;
    let (tx, rx) = mpsc::channel();  // Unbounded queue

    // Background processor thread
    spawn_processor_thread(rx);

    // Accept connections (multi-threaded)
    for stream in listener.incoming() {
        let tx_clone = tx.clone();
        thread::spawn(move || {
            handle_connection(stream, tx_clone);
        });
    }
}

// connection_handler.rs - Parse messages, route to handlers
fn handle_connection(stream: UnixStream, tx: Sender<WorkItem>) {
    loop {
        let msg: JTAGMessage = read_message(&stream)?;

        match msg.type.as_str() {
            "ping" => handle_ping(&stream, &msg),
            "shutdown" => handle_shutdown(&stream, &msg),
            "status" => handle_status(&stream, &msg),
            _ => handle_domain_specific(&stream, &msg, &tx),
        }
    }
}

// processor.rs - Background work (domain-specific)
fn processor_thread(rx: Receiver<WorkItem>) {
    for work in rx.iter() {
        process_work_item(work);  // Domain-specific logic
    }
}

// health.rs - Universal health tracking
pub struct WorkerStats {
    start_time: Instant,
    processed: AtomicU64,
    errors: AtomicU64,
    queue_depth: Arc<AtomicUsize>,
}

impl WorkerStats {
    pub fn handle_ping(&self) -> PingResult { ... }
    pub fn handle_status(&self) -> StatusResult { ... }
}
```

---

## File Structure (Standard)

```
workers/<worker-name>/
├── src/
│   ├── main.rs               # Socket listener, queue, thread spawning
│   ├── connection_handler.rs # Message parsing, routing
│   ├── processor.rs          # Background work (domain-specific)
│   ├── health.rs             # Universal health/status implementation
│   ├── messages.rs           # Domain-specific types with ts-rs
│   └── [domain logic]/       # Additional modules as needed
├── bindings/                 # ts-rs generated TypeScript types
│   ├── <Type>.ts
│   └── ...
├── Cargo.toml               # Dependencies + binary config
└── README.md                # Worker-specific documentation
```

---

## Integration with Node.js

**TypeScript side** (system/workers/<worker-name>/):
```typescript
// WorkerClient.ts
export class WorkerClient {
  private socket: net.Socket;

  async ping(): Promise<PingResult> {
    return this.sendMessage({ type: 'ping', id: uuid() });
  }

  async shutdown(timeout?: number): Promise<ShutdownResult> {
    return this.sendMessage({
      type: 'shutdown',
      id: uuid(),
      data: { timeout_ms: timeout }
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

**Supervisor pattern** (optional):
```typescript
// WorkerSupervisor.ts - Monitors and restarts workers
export class WorkerSupervisor {
  async checkHealth(workerName: string): Promise<boolean> {
    const client = this.clients.get(workerName);
    const result = await client.ping();
    return result.data.status === 'healthy';
  }

  async restartIfUnhealthy(workerName: string): Promise<void> {
    if (!await this.checkHealth(workerName)) {
      await this.restart(workerName);
    }
  }
}
```

---

## Testing Strategy

Every worker MUST have integration tests for the protocol:

```rust
#[cfg(test)]
mod protocol_tests {
    #[test]
    fn test_ping_responds_within_1_second() {
        let client = connect_to_worker();
        let start = Instant::now();
        let result = client.ping();
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(result.success, true);
    }

    #[test]
    fn test_graceful_shutdown_drains_queue() {
        let client = connect_to_worker();
        queue_100_items();
        let result = client.shutdown(10000);
        assert_eq!(result.data.queue_drained, 100);
    }

    #[test]
    fn test_status_reports_accurate_metrics() {
        let client = connect_to_worker();
        process_50_items();
        let status = client.status(true);
        assert_eq!(status.data.metrics.processed_total, 50);
    }
}
```

---

## AI Persona Integration

AI personas can act as system mechanics:

**CLI Commands**:
```bash
# Health check
./jtag worker/health --worker=chat-drain
./jtag worker/health --all

# Status inspection
./jtag worker/status --worker=logger --verbose

# Restart unhealthy workers
./jtag worker/restart --worker=ai-provider

# System-wide health
./jtag system/health
```

**AI Persona Tools**:
```xml
<tool_name="worker/health">
  <param name="worker">chat-drain</param>
</tool_name>

<tool_name="worker/restart">
  <param name="worker">ai-provider</param>
  <param name="reason">High error rate detected</param>
</tool_name>
```

AI personas can:
- Monitor worker health continuously
- Detect degraded performance
- Restart failing workers
- Report issues to humans
- Learn optimal restart strategies

---

## Error Handling Philosophy

**Workers should fail fast and loudly:**
- Don't hide errors in the queue
- Log all failures with context
- Expose error rates in status
- Set thresholds for auto-degradation

**Graceful degradation:**
- `healthy` → `degraded` when queue depth > 80% capacity
- `degraded` → `failing` when error rate > 5%
- Failing workers should self-report and request restart

**Recovery strategies:**
- Auto-restart on crash (supervisor process)
- Manual restart via CLI/tools
- Queue persistence (save state before exit)
- Circuit breaker for external dependencies

---

## Performance Guidelines

**Queue Sizing**:
- Use unbounded channels by default (mpsc::channel)
- Monitor queue depth, alert at 80% capacity
- If queue grows unbounded, add backpressure

**Thread Count**:
- 1 main thread (socket listener)
- 1 background thread (processor)
- N connection threads (spawned per connection)

**Memory**:
- Set soft limit (e.g., 512 MB)
- Report memory in status
- Trigger GC or drop caches if approaching limit

**Latency**:
- Fast path (queue push) < 1ms
- Background processing: domain-specific
- Health checks < 1 second

---

## Future Extensions

**Planned additions to protocol:**
- `metrics/export` - Prometheus format
- `config/reload` - Hot reload configuration
- `debug/trace` - Enable detailed tracing
- `queue/inspect` - Peek at queue contents

**Worker-to-worker communication:**
- Shared protocol enables direct worker communication
- Example: Chat drain → AI provider → Logger
- Chain of responsibility pattern

**Distributed workers:**
- Protocol extends to TCP sockets (not just Unix)
- Service discovery for remote workers
- Load balancing across multiple instances

---

## Reference Implementation

**Logger Worker** (`workers/logger/`) is the reference implementation:
- ✅ Implements ping (health check)
- ✅ Queue-based architecture
- ✅ Background processing thread
- ✅ Metrics tracking (processed count)
- ⚠️ Missing: shutdown, status (to be added)

**Next: Chat Drain Worker** will be the first COMPLETE implementation with all three interfaces.

---

## Summary

Every worker is a **self-contained unit** with:
1. **Standard health interface** - Ping, shutdown, status
2. **Queue-based processing** - Non-blocking fast path
3. **Background threads** - Async work off main thread
4. **TypeScript integration** - Generated types, client library
5. **Supervisor compatibility** - Can be monitored and restarted

This enables **autonomous operation** where AI personas can maintain the system like power plant operators - inspecting gauges, restarting units, and ensuring stable operation.

**The goal**: A system that heals itself, monitored by AI citizens who understand its health and can take action.
