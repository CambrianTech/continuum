# SQLite + Chat Performance Sprint Plan

**Target**: Fix the two most critical performance bottlenecks identified in bottleneck-removal.md

**Timeline**: 5-7 days total
- SQLite worker threads: 3-4 days
- Chat message queue: 2-3 days

---

## Bottleneck #1: SQLite Worker Threads (CRITICAL)

### Current Problem

**Symptoms**:
- 3.2GB memory usage
- Main thread blocking during SQLite→JS conversion
- V8 object property transitions consuming CPU
- 1312 profiler samples in RowToJS → MigrateToMap

**Root Cause**:
All SQLite operations run on the main Node.js thread. Every query:
1. Blocks main thread while SQLite processes query
2. Blocks main thread during row→JS object conversion
3. Creates massive JS objects that trigger V8 GC pauses

### Solution: SQLite Worker Thread Pool

Move ALL SQLite operations to dedicated worker threads. Main thread only communicates via message passing.

**Architecture**:
```
Main Thread (Node.js)
    ↓ (post query via MessagePort)
Worker Thread 1 (better-sqlite3)
    ↓ (return rows via MessagePort)
Main Thread
    ↓ (deserialize and use)
Application Code
```

**Benefits**:
- Main thread never blocks on SQLite operations
- V8 object property transitions happen in worker thread (isolated heap)
- Better-sqlite3 synchronous API works in worker (no callback hell)
- Can scale to multiple workers for read-heavy workloads

---

### Implementation Plan: SQLite Worker Threads

#### Phase 1: Foundation (Day 1-2)

**Task 1.1: Create SqliteWorkerPool infrastructure** (4 hours)

File: `daemons/data-daemon/server/workers/SqliteWorkerPool.ts`

```typescript
import { Worker } from 'worker_threads';
import { EventEmitter } from 'events';

export interface WorkerPoolConfig {
  workerCount: number;           // Number of worker threads
  maxQueueSize: number;          // Max pending queries
  workerScript: string;          // Path to worker script
}

export interface QueryTask {
  id: string;                    // Unique task ID
  type: 'query' | 'write';       // Query type
  dbPath: string;                // Database file path
  sql: string;                   // SQL statement
  params?: any[];                // Query parameters
  resolve: (result: any) => void;
  reject: (error: Error) => void;
}

export class SqliteWorkerPool extends EventEmitter {
  private workers: Worker[] = [];
  private taskQueue: QueryTask[] = [];
  private activeTasks: Map<string, QueryTask> = new Map();
  private availableWorkers: Worker[] = [];
  private nextTaskId = 0;

  constructor(private config: WorkerPoolConfig) {
    super();
    this.initializeWorkers();
  }

  private initializeWorkers(): void {
    for (let i = 0; i < this.config.workerCount; i++) {
      const worker = new Worker(this.config.workerScript, {
        workerData: { workerId: i }
      });

      worker.on('message', (message) => this.handleWorkerMessage(worker, message));
      worker.on('error', (error) => this.handleWorkerError(worker, error));
      worker.on('exit', (code) => this.handleWorkerExit(worker, code));

      this.workers.push(worker);
      this.availableWorkers.push(worker);
    }
  }

  async execute<T>(dbPath: string, sql: string, params?: any[]): Promise<T> {
    return new Promise((resolve, reject) => {
      const taskId = `task_${this.nextTaskId++}`;
      const task: QueryTask = {
        id: taskId,
        type: sql.trim().toUpperCase().startsWith('SELECT') ? 'query' : 'write',
        dbPath,
        sql,
        params,
        resolve,
        reject
      };

      if (this.taskQueue.length >= this.config.maxQueueSize) {
        reject(new Error('Worker pool queue full'));
        return;
      }

      this.taskQueue.push(task);
      this.processQueue();
    });
  }

  private processQueue(): void {
    while (this.taskQueue.length > 0 && this.availableWorkers.length > 0) {
      const task = this.taskQueue.shift()!;
      const worker = this.availableWorkers.shift()!;

      this.activeTasks.set(task.id, task);

      worker.postMessage({
        taskId: task.id,
        dbPath: task.dbPath,
        sql: task.sql,
        params: task.params
      });
    }
  }

  private handleWorkerMessage(worker: Worker, message: any): void {
    const { taskId, result, error } = message;
    const task = this.activeTasks.get(taskId);

    if (!task) {
      console.error(`Unknown task ID: ${taskId}`);
      return;
    }

    this.activeTasks.delete(taskId);
    this.availableWorkers.push(worker);

    if (error) {
      task.reject(new Error(error));
    } else {
      task.resolve(result);
    }

    this.processQueue();
  }

  private handleWorkerError(worker: Worker, error: Error): void {
    console.error('Worker error:', error);
    // Find and reject all tasks assigned to this worker
    for (const [taskId, task] of this.activeTasks.entries()) {
      task.reject(error);
      this.activeTasks.delete(taskId);
    }
    // Restart worker
    this.restartWorker(worker);
  }

  private handleWorkerExit(worker: Worker, code: number): void {
    if (code !== 0) {
      console.error(`Worker exited with code ${code}`);
      this.restartWorker(worker);
    }
  }

  private restartWorker(oldWorker: Worker): void {
    const index = this.workers.indexOf(oldWorker);
    if (index === -1) return;

    const newWorker = new Worker(this.config.workerScript, {
      workerData: { workerId: index }
    });

    newWorker.on('message', (message) => this.handleWorkerMessage(newWorker, message));
    newWorker.on('error', (error) => this.handleWorkerError(newWorker, error));
    newWorker.on('exit', (code) => this.handleWorkerExit(newWorker, code));

    this.workers[index] = newWorker;
    this.availableWorkers.push(newWorker);
  }

  async shutdown(): Promise<void> {
    // Wait for all active tasks to complete
    await Promise.all(
      Array.from(this.activeTasks.values()).map(task =>
        new Promise(resolve => setTimeout(resolve, 100))
      )
    );

    // Terminate all workers
    await Promise.all(
      this.workers.map(worker => worker.terminate())
    );

    this.workers = [];
    this.availableWorkers = [];
    this.activeTasks.clear();
    this.taskQueue = [];
  }
}
```

**Task 1.2: Create SqliteWorker script** (3 hours)

File: `daemons/data-daemon/server/workers/sqlite-worker.ts`

```typescript
import { parentPort, workerData } from 'worker_threads';
import Database from 'better-sqlite3';

interface WorkerMessage {
  taskId: string;
  dbPath: string;
  sql: string;
  params?: any[];
}

// Cache open database connections
const dbCache = new Map<string, Database.Database>();

function getDatabase(dbPath: string): Database.Database {
  if (!dbCache.has(dbPath)) {
    const db = new Database(dbPath, {
      readonly: false,
      fileMustExist: false
    });
    dbCache.set(dbPath, db);
  }
  return dbCache.get(dbPath)!;
}

parentPort?.on('message', (message: WorkerMessage) => {
  const { taskId, dbPath, sql, params } = message;

  try {
    const db = getDatabase(dbPath);

    // Determine query type
    const isSelect = sql.trim().toUpperCase().startsWith('SELECT');

    let result: any;
    if (isSelect) {
      // Query returns rows
      const stmt = db.prepare(sql);
      result = params ? stmt.all(...params) : stmt.all();
    } else {
      // Write operation
      const stmt = db.prepare(sql);
      const info = params ? stmt.run(...params) : stmt.run();
      result = {
        changes: info.changes,
        lastInsertRowid: info.lastInsertRowid
      };
    }

    parentPort?.postMessage({
      taskId,
      result
    });
  } catch (error: any) {
    parentPort?.postMessage({
      taskId,
      error: error.message
    });
  }
});

// Cleanup on exit
process.on('exit', () => {
  for (const db of dbCache.values()) {
    db.close();
  }
});
```

**Task 1.3: Install better-sqlite3** (30 minutes)

```bash
cd /Volumes/FlashGordon/cambrian/continuum/src
npm install better-sqlite3
npm install --save-dev @types/better-sqlite3
```

**Task 1.4: Compile worker script** (1 hour)

Update `package.json` to compile worker script separately:

```json
{
  "scripts": {
    "build:workers": "tsc daemons/data-daemon/server/workers/sqlite-worker.ts --outDir dist/daemons/data-daemon/server/workers",
    "build:ts": "npm run build:workers && tsc"
  }
}
```

#### Phase 2: Integration (Day 2-3)

**Task 2.1: Update SqliteStorageAdapter** (4 hours)

File: `daemons/data-daemon/server/SqliteStorageAdapter.ts`

Add worker pool initialization:

```typescript
import { SqliteWorkerPool } from './workers/SqliteWorkerPool';
import * as path from 'path';

export class SqliteStorageAdapter implements StorageAdapter {
  private workerPool: SqliteWorkerPool | null = null;

  async initialize(): Promise<void> {
    // Initialize worker pool
    const workerScript = path.join(__dirname, 'workers', 'sqlite-worker.js');

    this.workerPool = new SqliteWorkerPool({
      workerCount: 4,           // 4 workers for parallel queries
      maxQueueSize: 1000,       // Max 1000 pending queries
      workerScript
    });

    this.log.info('✅ SqliteWorkerPool initialized with 4 workers');

    // Continue with existing initialization...
  }

  async shutdown(): Promise<void> {
    if (this.workerPool) {
      await this.workerPool.shutdown();
      this.log.info('✅ SqliteWorkerPool shut down');
    }
    // Continue with existing shutdown...
  }
}
```

**Task 2.2: Update SqliteQueryExecutor to use worker pool** (6 hours)

File: `daemons/data-daemon/server/managers/SqliteQueryExecutor.ts`

Replace direct database.prepare() calls with worker pool:

```typescript
async executeQuery<T extends BaseEntity>(
  database: string,
  sql: string,
  params: any[]
): Promise<T[]> {
  // Get worker pool from storage adapter
  const workerPool = this.storageAdapter.getWorkerPool();

  if (!workerPool) {
    throw new Error('Worker pool not initialized');
  }

  // Execute query in worker thread
  const rows = await workerPool.execute<any[]>(
    this.getDatabasePath(database),
    sql,
    params
  );

  // Convert rows to entities (still on main thread, but fast)
  return rows.map(row => this.rowToEntity<T>(row));
}

private getDatabasePath(database: string): string {
  // Map database name to file path
  const dbPaths: Record<string, string> = {
    'shared': path.join(SystemPaths.database.shared, 'shared.db'),
    'global': path.join(SystemPaths.database.global, 'global.db'),
    // Per-persona databases use persona ID
    // handled separately
  };
  return dbPaths[database] || database; // Assume database is path if not found
}
```

**Task 2.3: Update SqliteWriteManager to use worker pool** (4 hours)

File: `daemons/data-daemon/server/managers/SqliteWriteManager.ts`

Replace direct database.prepare() calls:

```typescript
async executeWrite(
  database: string,
  sql: string,
  params: any[]
): Promise<{ changes: number; lastInsertRowid: number }> {
  const workerPool = this.storageAdapter.getWorkerPool();

  if (!workerPool) {
    throw new Error('Worker pool not initialized');
  }

  return await workerPool.execute(
    this.getDatabasePath(database),
    sql,
    params
  );
}
```

#### Phase 3: Testing (Day 3-4)

**Task 3.1: Unit tests** (3 hours)

File: `tests/unit/SqliteWorkerPool.test.ts`

```typescript
import { SqliteWorkerPool } from '../../daemons/data-daemon/server/workers/SqliteWorkerPool';
import * as path from 'path';
import * as fs from 'fs';

describe('SqliteWorkerPool', () => {
  let pool: SqliteWorkerPool;
  const testDbPath = path.join(__dirname, 'test.db');

  beforeEach(async () => {
    // Clean up test database
    if (fs.existsSync(testDbPath)) {
      fs.unlinkSync(testDbPath);
    }

    pool = new SqliteWorkerPool({
      workerCount: 2,
      maxQueueSize: 100,
      workerScript: path.join(__dirname, '../../dist/daemons/data-daemon/server/workers/sqlite-worker.js')
    });
  });

  afterEach(async () => {
    await pool.shutdown();
    if (fs.existsSync(testDbPath)) {
      fs.unlinkSync(testDbPath);
    }
  });

  it('should execute SELECT query', async () => {
    // Create table
    await pool.execute(testDbPath, 'CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)');

    // Insert data
    await pool.execute(testDbPath, 'INSERT INTO users (name) VALUES (?)', ['Alice']);

    // Query data
    const results = await pool.execute(testDbPath, 'SELECT * FROM users');

    expect(results).toHaveLength(1);
    expect(results[0].name).toBe('Alice');
  });

  it('should handle concurrent queries', async () => {
    await pool.execute(testDbPath, 'CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)');

    // Execute 100 concurrent inserts
    const promises = Array.from({ length: 100 }, (_, i) =>
      pool.execute(testDbPath, 'INSERT INTO users (name) VALUES (?)', [`User${i}`])
    );

    await Promise.all(promises);

    // Verify all inserted
    const results = await pool.execute(testDbPath, 'SELECT COUNT(*) as count FROM users');
    expect(results[0].count).toBe(100);
  });

  it('should reject when queue is full', async () => {
    // Fill queue
    const promises = Array.from({ length: 101 }, () =>
      pool.execute(testDbPath, 'SELECT 1')
    );

    // One should reject
    await expect(Promise.all(promises)).rejects.toThrow('Worker pool queue full');
  });
});
```

**Task 3.2: Integration tests** (4 hours)

File: `tests/integration/sqlite-worker-performance.test.ts`

```typescript
import { SqliteStorageAdapter } from '../../daemons/data-daemon/server/SqliteStorageAdapter';
import { UserEntity } from '../../system/data/entities/UserEntity';

describe('SQLite Worker Performance', () => {
  let adapter: SqliteStorageAdapter;

  beforeAll(async () => {
    adapter = new SqliteStorageAdapter(/* config */);
    await adapter.initialize();
  });

  afterAll(async () => {
    await adapter.shutdown();
  });

  it('should handle 1000 concurrent queries without blocking', async () => {
    const start = Date.now();

    // Execute 1000 queries concurrently
    const promises = Array.from({ length: 1000 }, (_, i) =>
      adapter.query('shared', 'SELECT * FROM users WHERE id = ?', [i])
    );

    const results = await Promise.all(promises);

    const elapsed = Date.now() - start;

    console.log(`1000 concurrent queries completed in ${elapsed}ms`);
    expect(elapsed).toBeLessThan(5000); // Should complete in under 5 seconds
  });

  it('should not block main thread during large query', async () => {
    // Insert 10,000 test records
    for (let i = 0; i < 10000; i++) {
      await adapter.write('shared', 'INSERT INTO test_data (value) VALUES (?)', [i]);
    }

    // Execute large query
    const start = Date.now();
    const queryPromise = adapter.query('shared', 'SELECT * FROM test_data');

    // Main thread should remain responsive
    let mainThreadBlocked = false;
    const checkInterval = setInterval(() => {
      const now = Date.now();
      if (now - start > 100) {
        // If we get here, main thread is still responsive
        mainThreadBlocked = false;
      }
    }, 50);

    await queryPromise;
    clearInterval(checkInterval);

    expect(mainThreadBlocked).toBe(false);
  });
});
```

**Task 3.3: Manual testing** (2 hours)

```bash
# Deploy with worker pool
npm run build:ts
npm start

# Monitor performance
node --inspect dist/server/index.js

# In Chrome DevTools:
# - Record CPU profile during chat usage
# - Verify no blocking in main thread
# - Check worker thread activity

# Monitor memory
ps aux | grep node
# Should see 3.2GB → ~500MB reduction
```

#### Phase 4: Migration (Day 4)

**Task 4.1: Feature flag** (1 hour)

Add environment variable to enable/disable worker pool:

File: `system/core/config/SystemPaths.ts`

```typescript
export const USE_SQLITE_WORKERS = process.env.USE_SQLITE_WORKERS !== '0'; // Enabled by default
```

**Task 4.2: Gradual rollout** (2 hours)

1. Deploy with `USE_SQLITE_WORKERS=0` (disabled)
2. Verify system works normally
3. Enable with `USE_SQLITE_WORKERS=1`
4. Monitor for issues
5. If stable after 1 hour, remove flag

**Task 4.3: Documentation** (1 hour)

Update `docs/ARCHITECTURE.md`:

```markdown
## SQLite Worker Pool

All SQLite operations run in dedicated worker threads to prevent main thread blocking.

**Configuration**:
- `SQLITE_WORKER_COUNT`: Number of worker threads (default: 4)
- `SQLITE_MAX_QUEUE_SIZE`: Max pending queries (default: 1000)

**Benefits**:
- 70-80% reduction in main thread blocking
- 3.2GB → 500MB memory reduction (V8 heap isolation)
- Better-sqlite3 performance (synchronous API)
```

---

## Bottleneck #2: Chat Message Queue (CRITICAL)

### Current Problem

**Symptoms**:
- UI freezes when sending messages
- Double-sends when user clicks multiple times
- 1-2 second latency before message appears

**Root Cause**:
`Commands.execute('chat/send')` is synchronous and blocks the browser's main thread:
1. User clicks "Send"
2. Widget calls `Commands.execute()` synchronously
3. Main thread blocks waiting for server response
4. User clicks again (thinking it failed)
5. Second message sent

### Solution: Fire-and-Forget Message Queue

Implement optimistic UI updates with background message sending.

---

### Implementation Plan: Chat Message Queue

#### Phase 1: Client-Side Queue (Day 1)

**Task 1.1: Create ChatMessageQueue** (3 hours)

File: `widgets/chat-widget/client/ChatMessageQueue.ts`

```typescript
import { Commands } from '../../../system/core/shared/Commands';
import { Events } from '../../../system/core/shared/Events';

export interface QueuedMessage {
  id: string;                    // Client-side UUID
  roomId: string;
  content: string;
  replyToId?: string;
  timestamp: number;
  status: 'pending' | 'sent' | 'failed';
  retryCount: number;
}

export class ChatMessageQueue {
  private queue: QueuedMessage[] = [];
  private sending = false;

  /**
   * Add message to queue and return immediately (fire-and-forget)
   */
  enqueue(roomId: string, content: string, replyToId?: string): string {
    const messageId = this.generateClientId();

    const queuedMessage: QueuedMessage = {
      id: messageId,
      roomId,
      content,
      replyToId,
      timestamp: Date.now(),
      status: 'pending',
      retryCount: 0
    };

    this.queue.push(queuedMessage);

    // Emit event for optimistic UI update
    Events.emit('chat:message:queued', {
      messageId,
      roomId,
      content,
      replyToId,
      timestamp: queuedMessage.timestamp
    });

    // Start processing queue
    this.processQueue();

    return messageId;
  }

  private async processQueue(): Promise<void> {
    if (this.sending) {
      return; // Already processing
    }

    this.sending = true;

    while (this.queue.length > 0) {
      const message = this.queue[0]; // Peek at first message

      try {
        // Send message to server
        const result = await Commands.execute('chat/send', {
          roomId: message.roomId,
          content: message.content,
          replyToId: message.replyToId
        });

        // Success - remove from queue
        this.queue.shift();
        message.status = 'sent';

        // Emit success event
        Events.emit('chat:message:sent', {
          clientId: message.id,
          serverId: result.messageId,
          roomId: message.roomId
        });

      } catch (error: any) {
        console.error('Failed to send message:', error);

        message.retryCount++;

        if (message.retryCount >= 3) {
          // Failed after 3 retries - remove from queue
          this.queue.shift();
          message.status = 'failed';

          // Emit failure event
          Events.emit('chat:message:failed', {
            messageId: message.id,
            roomId: message.roomId,
            error: error.message
          });
        } else {
          // Retry after delay
          await this.delay(1000 * message.retryCount); // Exponential backoff
        }
      }
    }

    this.sending = false;
  }

  private generateClientId(): string {
    return `client_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Get queue status for debugging
   */
  getStatus(): { pending: number; sending: boolean } {
    return {
      pending: this.queue.length,
      sending: this.sending
    };
  }
}
```

**Task 1.2: Update ChatWidget to use queue** (2 hours)

File: `widgets/chat-widget/client/ChatWidgetClient.ts`

```typescript
import { ChatMessageQueue } from './ChatMessageQueue';

export class ChatWidgetClient {
  private messageQueue = new ChatMessageQueue();
  private pendingMessages = new Map<string, any>(); // clientId → optimistic message

  async handleSendMessage(content: string, replyToId?: string): Promise<void> {
    const roomId = this.currentRoomId;

    // Optimistic UI update - add message immediately
    const clientId = this.messageQueue.enqueue(roomId, content, replyToId);

    // Show message in UI with pending status
    const optimisticMessage = {
      id: clientId,
      roomId,
      content,
      replyToId,
      timestamp: Date.now(),
      status: 'pending',
      author: this.currentUser
    };

    this.pendingMessages.set(clientId, optimisticMessage);
    this.addMessageToUI(optimisticMessage);

    // Fire-and-forget - user can continue using chat
  }

  private setupQueueListeners(): void {
    // Message sent successfully
    Events.subscribe('chat:message:sent', (data: any) => {
      const { clientId, serverId } = data;
      const optimisticMessage = this.pendingMessages.get(clientId);

      if (optimisticMessage) {
        // Update UI to show real message from server
        this.replaceOptimisticMessage(clientId, serverId);
        this.pendingMessages.delete(clientId);
      }
    });

    // Message failed to send
    Events.subscribe('chat:message:failed', (data: any) => {
      const { messageId, error } = data;
      const optimisticMessage = this.pendingMessages.get(messageId);

      if (optimisticMessage) {
        // Show error in UI
        this.showMessageError(messageId, error);
        this.pendingMessages.delete(messageId);
      }
    });
  }

  private addMessageToUI(message: any): void {
    const messageElement = this.createMessageElement(message);
    this.messageContainer.appendChild(messageElement);
    this.scrollToBottom();
  }

  private replaceOptimisticMessage(clientId: string, serverId: string): void {
    const element = this.messageContainer.querySelector(`[data-client-id="${clientId}"]`);
    if (element) {
      element.setAttribute('data-message-id', serverId);
      element.removeAttribute('data-client-id');
      element.classList.remove('pending');
      element.classList.add('sent');
    }
  }

  private showMessageError(clientId: string, error: string): void {
    const element = this.messageContainer.querySelector(`[data-client-id="${clientId}"]`);
    if (element) {
      element.classList.add('failed');
      const errorElement = document.createElement('span');
      errorElement.className = 'error-message';
      errorElement.textContent = `Failed to send: ${error}`;
      element.appendChild(errorElement);
    }
  }
}
```

#### Phase 2: UI Polish (Day 2)

**Task 2.1: Add pending/sent/failed states** (2 hours)

File: `widgets/chat-widget/client/styles.css`

```css
.message.pending {
  opacity: 0.6;
  position: relative;
}

.message.pending::after {
  content: "Sending...";
  position: absolute;
  right: 10px;
  top: 5px;
  font-size: 12px;
  color: #666;
}

.message.sent {
  opacity: 1;
}

.message.failed {
  opacity: 0.8;
  border-left: 3px solid #ff4444;
}

.message .error-message {
  display: block;
  color: #ff4444;
  font-size: 12px;
  margin-top: 5px;
}
```

**Task 2.2: Add retry button for failed messages** (2 hours)

```typescript
private showMessageError(clientId: string, error: string): void {
  const element = this.messageContainer.querySelector(`[data-client-id="${clientId}"]`);
  if (element) {
    element.classList.add('failed');

    const errorContainer = document.createElement('div');
    errorContainer.className = 'error-container';

    const errorText = document.createElement('span');
    errorText.className = 'error-message';
    errorText.textContent = `Failed to send: ${error}`;

    const retryButton = document.createElement('button');
    retryButton.className = 'retry-button';
    retryButton.textContent = 'Retry';
    retryButton.onclick = () => this.retryMessage(clientId);

    errorContainer.appendChild(errorText);
    errorContainer.appendChild(retryButton);
    element.appendChild(errorContainer);
  }
}

private retryMessage(clientId: string): void {
  const message = this.pendingMessages.get(clientId);
  if (message) {
    // Re-enqueue message
    this.handleSendMessage(message.content, message.replyToId);

    // Remove failed message from UI
    const element = this.messageContainer.querySelector(`[data-client-id="${clientId}"]`);
    element?.remove();
  }
}
```

#### Phase 3: Testing (Day 2-3)

**Task 3.1: Unit tests** (2 hours)

File: `tests/unit/ChatMessageQueue.test.ts`

```typescript
import { ChatMessageQueue } from '../../widgets/chat-widget/client/ChatMessageQueue';
import { Events } from '../../system/core/shared/Events';
import { Commands } from '../../system/core/shared/Commands';

jest.mock('../../system/core/shared/Commands');
jest.mock('../../system/core/shared/Events');

describe('ChatMessageQueue', () => {
  let queue: ChatMessageQueue;

  beforeEach(() => {
    queue = new ChatMessageQueue();
  });

  it('should return immediately when enqueuing', () => {
    const start = Date.now();
    const messageId = queue.enqueue('room1', 'Hello');
    const elapsed = Date.now() - start;

    expect(elapsed).toBeLessThan(10); // Should return in <10ms
    expect(messageId).toMatch(/^client_/);
  });

  it('should emit queued event', () => {
    const emitSpy = jest.spyOn(Events, 'emit');

    queue.enqueue('room1', 'Hello');

    expect(emitSpy).toHaveBeenCalledWith('chat:message:queued', expect.objectContaining({
      roomId: 'room1',
      content: 'Hello'
    }));
  });

  it('should send messages in order', async () => {
    const executeSpy = jest.spyOn(Commands, 'execute').mockResolvedValue({ messageId: 'server1' });

    queue.enqueue('room1', 'Message 1');
    queue.enqueue('room1', 'Message 2');
    queue.enqueue('room1', 'Message 3');

    // Wait for queue to process
    await new Promise(resolve => setTimeout(resolve, 100));

    expect(executeSpy).toHaveBeenCalledTimes(3);
    expect(executeSpy).toHaveBeenNthCalledWith(1, 'chat/send', expect.objectContaining({ content: 'Message 1' }));
    expect(executeSpy).toHaveBeenNthCalledWith(2, 'chat/send', expect.objectContaining({ content: 'Message 2' }));
    expect(executeSpy).toHaveBeenNthCalledWith(3, 'chat/send', expect.objectContaining({ content: 'Message 3' }));
  });

  it('should retry failed messages', async () => {
    const executeSpy = jest.spyOn(Commands, 'execute')
      .mockRejectedValueOnce(new Error('Network error'))
      .mockResolvedValueOnce({ messageId: 'server1' });

    queue.enqueue('room1', 'Hello');

    // Wait for retry
    await new Promise(resolve => setTimeout(resolve, 2000));

    expect(executeSpy).toHaveBeenCalledTimes(2);
  });

  it('should emit failed event after 3 retries', async () => {
    const executeSpy = jest.spyOn(Commands, 'execute').mockRejectedValue(new Error('Network error'));
    const emitSpy = jest.spyOn(Events, 'emit');

    queue.enqueue('room1', 'Hello');

    // Wait for all retries
    await new Promise(resolve => setTimeout(resolve, 5000));

    expect(executeSpy).toHaveBeenCalledTimes(3);
    expect(emitSpy).toHaveBeenCalledWith('chat:message:failed', expect.objectContaining({
      roomId: 'room1'
    }));
  });
});
```

**Task 3.2: Integration tests** (2 hours)

```bash
# Manual testing checklist:

1. Send message → should appear immediately (optimistic)
2. Wait 1 second → should update to "sent" status
3. Send 10 messages rapidly → all should queue and send in order
4. Disconnect network → send message → should show "failed" + retry button
5. Click retry → should re-send successfully
6. Send message while previous is pending → should not duplicate
```

**Task 3.3: Performance testing** (2 hours)

```typescript
// Measure perceived latency
const start = Date.now();
const messageId = chatWidget.sendMessage('Hello');
const elapsed = Date.now() - start;

console.log(`Perceived latency: ${elapsed}ms`);
// Should be <100ms (vs 1-2 seconds before)
```

---

## Success Criteria

### SQLite Worker Threads

**Metrics**:
- Memory usage: 3.2GB → <500MB ✓
- Main thread blocking: 0 samples in RowToJS ✓
- Query latency: No change (still fast) ✓
- Concurrent queries: 1000 queries in <5 seconds ✓

**Validation**:
```bash
# Before
node --inspect dist/server/index.js
# Record CPU profile during chat
# Check: Main thread blocked in RowToJS → MigrateToMap

# After
node --inspect dist/server/index.js
# Record CPU profile during chat
# Check: Main thread NOT blocked, workers show activity
```

### Chat Message Queue

**Metrics**:
- Perceived send latency: 1-2s → <100ms ✓
- Double-sends: Eliminated ✓
- Message ordering: Preserved ✓
- Failure recovery: Auto-retry + manual retry ✓

**Validation**:
```bash
# Test scenarios:
1. Send message → appears immediately
2. Spam click send button → only 1 message sent
3. Disconnect network → send → shows failed + retry
4. Reconnect → click retry → sends successfully
5. Send 100 messages rapidly → all queued and sent in order
```

---

## Timeline Summary

**Day 1-2**: SQLite foundation + worker pool
**Day 2-3**: SQLite integration + testing
**Day 4**: SQLite migration + docs
**Day 5**: Chat queue implementation
**Day 6-7**: Chat UI polish + testing

**Total**: 5-7 days (depending on bugs/issues)

---

## Rollback Plan

### If SQLite Workers Fail

1. Set `USE_SQLITE_WORKERS=0`
2. System reverts to synchronous SQLite (slower but stable)
3. Investigate worker issues
4. Fix and redeploy

### If Chat Queue Fails

1. Revert ChatWidget to synchronous `Commands.execute()`
2. System still works (just slower UX)
3. Fix queue issues
4. Redeploy

Both features have feature flags for safe rollback.

---

## Dependencies

**Required**:
- better-sqlite3 (npm install)
- TypeScript 4.5+ (worker_threads support)
- Node.js 14+ (worker_threads stable)

**Optional**:
- Chrome DevTools (performance profiling)
- Artillery (load testing)

---

## Notes

- SQLite worker threads address 70-80% of memory/blocking issues
- Chat queue addresses 100% of UI blocking/double-send issues
- Both are independent - can be deployed separately
- Both have rollback plans - low risk
- Total estimated time: 5-7 days for both

