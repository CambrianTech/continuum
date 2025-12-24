# Logger Timing & Inspection Features

**Status**: Planned for alpha release
**Priority**: High - essential for debugging and performance analysis

## Overview

Enhance ComponentLogger with timing, variable inspection, and checkpoint capabilities for AI-readable debugging output.

## Proposed API

### 1. Performance Timing

```typescript
// Start a timer
log.time('database-query');

// ... operation ...

// End timer and log duration
log.timeEnd('database-query');
// Output: ⏱️ database-query: 42.31ms
```

### 2. Variable Inspection (AI-Readable)

```typescript
// Dump variables in structured format
log.inspect('user-state', {
  userId: user.id,
  sessionCount: sessions.length,
  lastActive: user.lastActiveAt
});
// Output: 🔍 INSPECT [user-state]
// {
//   "userId": "abc123",
//   "sessionCount": 3,
//   "lastActive": "2025-12-24T..."
// }
```

### 3. Checkpoints (Non-blocking Breakpoints)

```typescript
// Mark execution point with optional context
log.checkpoint('after-auth-check', { authenticated: true, role: 'admin' });
// Output: 📍 CHECKPOINT [after-auth-check] @ PersonaUser.ts:142
// { authenticated: true, role: 'admin' }
```

## Implementation

Add to `ComponentLogger.ts`:

```typescript
export class ComponentLogger {
  private timers: Map<string, number> = new Map();

  /**
   * Start a performance timer
   */
  time(label: string): void {
    this.timers.set(label, performance.now());
  }

  /**
   * End timer and log duration
   */
  timeEnd(label: string): void {
    const start = this.timers.get(label);
    if (start) {
      const ms = (performance.now() - start).toFixed(2);
      this.info(`⏱️ ${label}: ${ms}ms`);
      this.timers.delete(label);
    }
  }

  /**
   * Structured variable dump for AI analysis
   */
  inspect(label: string, vars: Record<string, unknown>): void {
    this.debug(`🔍 INSPECT [${label}]`, JSON.stringify(vars, null, 2));
  }

  /**
   * Checkpoint marker with stack location
   */
  checkpoint(label: string, context?: Record<string, unknown>): void {
    const stack = new Error().stack?.split('\n')[2]?.trim() || '';
    const location = stack.replace(/^at\s+/, '');
    this.info(`📍 CHECKPOINT [${label}] @ ${location}`, context || {});
  }

  /**
   * Timed async operation wrapper
   */
  async timed<T>(label: string, fn: () => Promise<T>): Promise<T> {
    this.time(label);
    try {
      return await fn();
    } finally {
      this.timeEnd(label);
    }
  }
}
```

## Usage Examples

### Database Query Timing
```typescript
const log = Logger.create('SqliteQueryExecutor');

log.time('complex-join');
const results = await db.query(complexSql);
log.timeEnd('complex-join');
```

### AI Response Debugging
```typescript
const log = Logger.create('PersonaUser');

log.checkpoint('before-llm-call', {
  messageCount: messages.length,
  model: this.model
});

const response = await this.llm.complete(prompt);

log.inspect('llm-response', {
  tokenCount: response.usage.total,
  finishReason: response.finish_reason,
  firstChars: response.content.slice(0, 100)
});
```

### Async Operation Wrapper
```typescript
const result = await log.timed('fetch-user-data', async () => {
  return await userService.getFullProfile(userId);
});
```

## Log Output Format

All timing/inspection logs follow consistent format for AI parsing:

```
[2025-12-24T03:00:00.000Z] ⏱️ ComponentName: operation-label: 42.31ms
[2025-12-24T03:00:00.000Z] 🔍 ComponentName: INSPECT [label] {...}
[2025-12-24T03:00:00.000Z] 📍 ComponentName: CHECKPOINT [label] @ file:line {...}
```

## Dependencies

- `performance.now()` for high-resolution timing (available in Node.js)
- Existing ComponentLogger infrastructure
- No external dependencies

## Notes

- Timers are per-ComponentLogger instance (not global)
- Checkpoint captures call stack for location
- inspect() uses debug level (won't spam production logs)
- timed() wrapper handles errors gracefully
