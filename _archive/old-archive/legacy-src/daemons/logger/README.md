# Process-Based Logger Daemon

The first implementation of the process-based daemon architecture, featuring async queue processing with mutex/semaphore synchronization.

## Architecture

```
BaseDaemon
    ↓
ProcessBasedDaemon<T>
    ↓
LoggerDaemon
```

## Key Features

- **Async Fire-and-Forget**: Logging calls return immediately without blocking
- **Queue Processing**: Background processing with mutex/semaphore synchronization
- **Batch Operations**: Efficient file I/O with configurable batch sizes
- **Type Safety**: Uses `DaemonMessage<LoggerMessage>` generic pattern
- **Resource Isolation**: Dedicated process with memory/CPU limits
- **Fault Tolerance**: Graceful handling of queue overflow and daemon failures

## Usage Examples

### Basic Usage

```typescript
import { AsyncLogger } from '../../logging/AsyncLogger';
import { ContinuumContext } from '../../types/shared/core/ContinuumTypes';

// Initialize once at startup
await AsyncLogger.initialize(context);

// Use anywhere in your code - returns immediately
await AsyncLogger.info(context, 'User action completed', 'UserService');
await AsyncLogger.error(context, 'Database connection failed', 'DatabaseService');
```

### Convenient Global Functions

```typescript
import { asyncInfo, asyncError } from '../../logging/AsyncLogger';

// Super convenient - no class references needed
await asyncInfo(context, 'Server started successfully');
await asyncError(context, 'Failed to process request');
```

### Direct Logger Client

```typescript
import { loggerClient } from './LoggerClient';

// Initialize
await loggerClient.initialize(context);

// Log with metadata
await loggerClient.info(context, 'User logged in', 'AuthService', {
  userId: 'user123',
  ip: '192.168.1.1'
});

// Configure batching
await loggerClient.configure({
  batchSize: 200,
  flushInterval: 3000
});

// Force flush
await loggerClient.flush(sessionId);
```

## Message Types

The logger uses typed messages with the existing `DaemonMessage<T>` pattern:

```typescript
// Log message
const logMessage: LoggerDaemonMessage = {
  id: 'uuid',
  from: 'UserService',
  to: 'logger',
  type: 'log',
  data: {
    type: 'log',
    payload: {
      level: 'info',
      message: 'User action completed',
      timestamp: '2025-01-01T00:00:00Z',
      source: 'UserService',
      context: { sessionId: 'abc123', environment: 'server' }
    }
  },
  timestamp: new Date(),
  priority: 'normal'
};
```

## Performance Benefits

- **Non-blocking**: Main thread never waits for disk I/O
- **Batched writes**: Multiple log entries written together
- **File handle reuse**: Keeps log files open for efficiency
- **Memory efficient**: Configurable buffer sizes and queue limits
- **Background processing**: Dedicated process thread for I/O

## Configuration

```typescript
const config: ProcessBasedDaemonConfig = {
  queueSize: 10000,        // Max queued messages
  batchSize: 100,          // Messages per batch
  processTimeoutMs: 30000, // Process timeout
  resourceLimits: {
    memory: '64MB',        // Memory limit
    cpu: '10%'             // CPU limit
  }
};
```

## Output Format

The logger maintains compatibility with existing log formats:

### Human-readable (server.log)
```
UL: [2025-01-01T00:00:00Z] [UserService] INFO: User action completed [session:abc123]
```

### JSON format (server.info.json)
```json
{"level":"info","message":"User action completed","timestamp":"2025-01-01T00:00:00Z","source":"UserService","context":{"sessionId":"abc123","environment":"server"}}
```

## Migration Path

1. **Phase 1**: Use alongside existing UniversalLogger
2. **Phase 2**: Gradually replace UniversalLogger calls
3. **Phase 3**: Remove UniversalLogger entirely

## Template for Other Daemons

This LoggerDaemon serves as the template for migrating other daemons to the process-based architecture:

- **SessionManagerDaemon**: Session state queue processing
- **BrowserManagerDaemon**: Browser command queue processing
- **CommandProcessorDaemon**: Command execution queue processing

Each daemon follows the same pattern:
1. Extend `ProcessBasedDaemon<TMessage>`
2. Define typed message interfaces
3. Implement `processMessage()` and `processBatch()`
4. Create convenient client interface
5. Configure resource limits and queue settings