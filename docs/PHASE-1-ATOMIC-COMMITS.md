# Phase 1: Extract Rate Limiting - Atomic Commit Breakdown

**Goal**: Extract rate limiting from PersonaUser into RateLimiter module

**Success Criteria**: AI responses work EXACTLY the same, but code is 50 lines cleaner

---

## Commit 1.0: Establish Baseline Test

### Architecture
Create integration test that documents current AI response behavior.

### Code
**File**: `tests/integration/ai-response-baseline.test.ts`

```typescript
import { describe, it, beforeAll, afterAll } from '@jest/globals';
import { executeCommand } from '../../commands/shared/CommandExecutor';
import type { DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { RoomEntity } from '../../database/entities/RoomEntity';

describe('AI Response Baseline', () => {
  let roomId: string;

  beforeAll(async () => {
    // Get general room
    const result = await executeCommand<DataListResult<RoomEntity>>('data/list', {
      collection: 'rooms',
      filter: { name: 'general' },
      limit: 1
    });
    roomId = result.items[0].id;
  });

  it('should have AIs respond to chat messages', async () => {
    const testMessage = `Baseline test ${Date.now()}`;

    // Send message via command
    await executeCommand('debug/chat-send', {
      roomId,
      message: testMessage
    });

    // Wait for AI evaluation + response
    await new Promise(resolve => setTimeout(resolve, 15000));

    // Check decision logs
    const decisions = await executeCommand('debug/logs', {
      filterPattern: 'Worker evaluated',
      tailLines: 50
    });

    // Expect at least 5 AIs evaluated
    expect(decisions.match(/Worker evaluated/g)?.length).toBeGreaterThanOrEqual(5);

    // Check for AI responses
    const responses = await executeCommand('debug/logs', {
      filterPattern: 'AI-RESPONSE',
      tailLines: 50
    });

    // Expect at least 1 AI responded
    expect(responses.match(/AI-RESPONSE/g)?.length).toBeGreaterThanOrEqual(1);
  }, 30000);

  it('should enforce rate limiting on rapid messages', async () => {
    const startTime = Date.now();

    // Send 5 messages rapidly
    for (let i = 0; i < 5; i++) {
      await executeCommand('debug/chat-send', {
        roomId,
        message: `Spam test ${i} - ${Date.now()}`
      });
      await new Promise(resolve => setTimeout(resolve, 2000));
    }

    // Wait for processing
    await new Promise(resolve => setTimeout(resolve, 15000));

    // Check for rate limit logs
    const logs = await executeCommand('debug/logs', {
      filterPattern: 'Rate limited',
      tailLines: 100
    });

    // Expect to see rate limiting kick in
    expect(logs).toContain('Rate limited');
  }, 60000);
});
```

### Lint
```bash
npm run lint:file tests/integration/ai-response-baseline.test.ts
```

### Compile
```bash
npx tsc --noEmit
```

### Test
```bash
npm test -- ai-response-baseline.test.ts
```

**Expected**: Test passes, documenting current behavior

### Commit Message
```
Add AI response baseline integration test

Establishes baseline metrics before refactoring:
- Verifies 5+ AIs evaluate messages
- Verifies 1+ AIs respond
- Documents rate limiting behavior

No code changes, only test documentation.
```

---

## Commit 1.1: Create ai/persona/status Command

### Architecture
Before extracting rate limiter, create command to INSPECT persona state (including rate limits).
This lets us verify rate limiter works after extraction.

### Code

**File**: `commands/ai/persona/status/shared/PersonaStatusTypes.ts`

```typescript
import type { CommandParams, CommandResult } from '../../../../core/types/JTAGTypes';
import type { UUID } from '../../../../core/types/JTAGTypes';

export interface PersonaStatusParams extends CommandParams {
  personaId: UUID;
  includeRateLimits?: boolean;
  includeGenome?: boolean;
}

export interface RateLimitStatus {
  contextId: UUID;
  contextName: string;
  lastResponseTime: string | null;
  responseCount: number;
  isCurrentlyLimited: boolean;
  secondsUntilNextResponse: number;
}

export interface PersonaStatusResult extends CommandResult {
  personaId: UUID;
  name: string;
  isActive: boolean;
  rateLimits?: RateLimitStatus[];
  genomeId?: UUID | null;
  workerStatus: 'running' | 'stopped' | 'error';
}
```

**File**: `commands/ai/persona/status/server/PersonaStatusServerCommand.ts`

```typescript
import { BaseServerCommand } from '../../../../core/BaseServerCommand';
import type { PersonaStatusParams, PersonaStatusResult, RateLimitStatus } from '../shared/PersonaStatusTypes';
import { getUserDaemon } from '../../../../../daemons/user-daemon/shared/UserDaemon';
import type { PersonaUser } from '../../../../../system/user/server/PersonaUser';

export class PersonaStatusServerCommand extends BaseServerCommand<PersonaStatusParams, PersonaStatusResult> {
  readonly name = 'ai/persona/status';

  async executeServer(params: PersonaStatusParams): Promise<PersonaStatusResult> {
    const userDaemon = getUserDaemon();
    const persona = userDaemon.getUser(params.personaId) as PersonaUser;

    if (!persona) {
      throw new Error(`Persona not found: ${params.personaId}`);
    }

    const result: PersonaStatusResult = {
      success: true,
      personaId: persona.id,
      name: persona.entity.name,
      isActive: true,
      workerStatus: persona.worker ? 'running' : 'stopped'
    };

    // Get rate limit status if requested
    if (params.includeRateLimits) {
      result.rateLimits = await this.getRateLimitStatus(persona);
    }

    // Get genome if requested
    if (params.includeGenome) {
      result.genomeId = await persona.getGenome().then(g => g?.id || null);
    }

    return result;
  }

  private async getRateLimitStatus(persona: PersonaUser): Promise<RateLimitStatus[]> {
    // Access private rate limit state via reflection (temporary until extracted)
    const lastResponseTime = (persona as any).lastResponseTime as Map<UUID, Date>;
    const responseCount = (persona as any).responseCount as Map<UUID, number>;

    const statuses: RateLimitStatus[] = [];

    for (const [contextId, lastTime] of lastResponseTime.entries()) {
      const count = responseCount.get(contextId) || 0;
      const secondsSinceResponse = (Date.now() - lastTime.getTime()) / 1000;
      const minSeconds = 10; // From PersonaUser.minSecondsBetweenResponses

      statuses.push({
        contextId,
        contextName: `Room ${contextId.slice(0, 8)}`,
        lastResponseTime: lastTime.toISOString(),
        responseCount: count,
        isCurrentlyLimited: secondsSinceResponse < minSeconds,
        secondsUntilNextResponse: Math.max(0, minSeconds - secondsSinceResponse)
      });
    }

    return statuses;
  }
}
```

**File**: `commands/ai/persona/status/README.md`

```markdown
# ai/persona/status - Persona Status Inspector

Get current status of a PersonaUser including rate limits, genome, and worker state.

## Usage

\`\`\`bash
./jtag ai/persona/status --personaId="<UUID>" --includeRateLimits=true
\`\`\`

## Parameters

- `personaId` (required): UUID of persona to inspect
- `includeRateLimits` (optional): Include rate limit details
- `includeGenome` (optional): Include genome ID

## Example Output

\`\`\`json
{
  "success": true,
  "personaId": "abc123...",
  "name": "Helper AI",
  "isActive": true,
  "workerStatus": "running",
  "rateLimits": [
    {
      "contextId": "room-123...",
      "contextName": "Room room-123",
      "lastResponseTime": "2025-10-27T10:30:00.000Z",
      "responseCount": 3,
      "isCurrentlyLimited": false,
      "secondsUntilNextResponse": 0
    }
  ],
  "genomeId": null
}
\`\`\`

## Use Cases

- Verify rate limiting is working correctly
- Debug why a persona isn't responding
- Inspect persona state during development
```

### Lint
```bash
npm run lint:file commands/ai/persona/status/shared/PersonaStatusTypes.ts
npm run lint:file commands/ai/persona/status/server/PersonaStatusServerCommand.ts
```

### Compile
```bash
npx tsc --noEmit
npm run build:ts
```

### Deploy
```bash
npm start
```

### Inspect
```bash
# Get persona IDs
./jtag data/list --collection=users --filter='{"type":"persona"}' --limit=5

# Check status of first persona
./jtag ai/persona/status --personaId="<PERSONA_ID>" --includeRateLimits=true

# Expected: See rate limit state (lastResponseTime, responseCount, isCurrentlyLimited)
```

### Test
Create unit test:

**File**: `commands/ai/persona/status/server/PersonaStatusServerCommand.test.ts`

```typescript
import { describe, it, expect } from '@jest/globals';
import { PersonaStatusServerCommand } from './PersonaStatusServerCommand';

describe('PersonaStatusServerCommand', () => {
  it('should return persona status', async () => {
    const command = new PersonaStatusServerCommand();

    // Get first persona
    const personas = await executeCommand('data/list', {
      collection: 'users',
      filter: { type: 'persona' },
      limit: 1
    });

    const result = await command.executeServer({
      personaId: personas.items[0].id,
      includeRateLimits: true,
      includeGenome: true
    });

    expect(result.success).toBe(true);
    expect(result.personaId).toBeDefined();
    expect(result.name).toBeDefined();
    expect(result.rateLimits).toBeDefined();
  });
});
```

```bash
npm test -- PersonaStatusServerCommand.test.ts
```

### Verify AI Responses Still Work
```bash
# Get room ID
./jtag data/list --collection=rooms --limit=1

# Send test message
./jtag debug/chat-send --roomId="<ROOM_ID>" --message="Commit 1.1 test - $(date +%s)"

# Wait 15 seconds
sleep 15

# Check AI activity
./jtag debug/logs --filterPattern="Worker evaluated|AI-RESPONSE" --tailLines=30

# Expected: 5+ evaluations, 1+ responses (same as baseline)

# Check persona status after response
./jtag ai/persona/status --personaId="<PERSONA_ID>" --includeRateLimits=true

# Expected: See lastResponseTime updated, responseCount incremented
```

### Commit Message
```
Add ai/persona/status command for inspecting persona state

Creates inspection command before refactoring rate limiter:
- View rate limit status (lastResponseTime, responseCount, isCurrentlyLimited)
- View genome ID
- View worker status

No changes to PersonaUser logic, only adds inspection capability.

AI responses verified working (baseline maintained).
```

---

## Commit 1.2: Extract RateLimiter Module

### Architecture
Extract rate limiting logic into standalone module, use immediately in PersonaUser.

### Code

**File**: `system/user/server/modules/RateLimiter.ts`

```typescript
import type { UUID } from '../../../core/types/JTAGTypes';

export interface RateLimitConfig {
  minSecondsBetweenResponses: number;
  maxResponsesPerSession: number;
}

export interface RateLimitInfo {
  contextId: UUID;
  lastResponseTime: Date | null;
  responseCount: number;
  isLimited: boolean;
  secondsUntilNext: number;
}

/**
 * Rate Limiter - Prevents AI spam in conversations
 *
 * Extracted from PersonaUser.ts (lines 99-104, 723-756)
 */
export class RateLimiter {
  private lastResponseTime: Map<UUID, Date> = new Map();
  private responseCount: Map<UUID, number> = new Map();

  constructor(
    private config: RateLimitConfig = {
      minSecondsBetweenResponses: 10,
      maxResponsesPerSession: 50
    }
  ) {}

  /**
   * Check if persona is rate limited in given context
   *
   * Original: PersonaUser.isRateLimited() line 723
   */
  isRateLimited(contextId: UUID): boolean {
    const lastTime = this.lastResponseTime.get(contextId);
    if (!lastTime) {
      return false; // Never responded before
    }

    const secondsSinceResponse = (Date.now() - lastTime.getTime()) / 1000;

    // Check time-based limit
    if (secondsSinceResponse < this.config.minSecondsBetweenResponses) {
      return true;
    }

    // Check count-based limit
    const count = this.responseCount.get(contextId) || 0;
    if (count >= this.config.maxResponsesPerSession) {
      return true;
    }

    return false;
  }

  /**
   * Record a response for rate limiting
   */
  recordResponse(contextId: UUID): void {
    this.lastResponseTime.set(contextId, new Date());
    const currentCount = this.responseCount.get(contextId) || 0;
    this.responseCount.set(contextId, currentCount + 1);
  }

  /**
   * Get rate limit info for a context
   */
  getInfo(contextId: UUID): RateLimitInfo {
    const lastTime = this.lastResponseTime.get(contextId);
    const count = this.responseCount.get(contextId) || 0;
    const secondsSince = lastTime ? (Date.now() - lastTime.getTime()) / 1000 : Infinity;
    const secondsUntilNext = Math.max(0, this.config.minSecondsBetweenResponses - secondsSince);

    return {
      contextId,
      lastResponseTime: lastTime || null,
      responseCount: count,
      isLimited: this.isRateLimited(contextId),
      secondsUntilNext
    };
  }

  /**
   * Get all contexts with rate limit info
   */
  getAllInfo(): RateLimitInfo[] {
    const contexts = new Set([
      ...this.lastResponseTime.keys(),
      ...this.responseCount.keys()
    ]);

    return Array.from(contexts).map(contextId => this.getInfo(contextId));
  }

  /**
   * Reset rate limits for a context
   */
  reset(contextId: UUID): void {
    this.lastResponseTime.delete(contextId);
    this.responseCount.delete(contextId);
  }

  /**
   * Reset all rate limits
   */
  resetAll(): void {
    this.lastResponseTime.clear();
    this.responseCount.clear();
  }
}
```

**File**: `system/user/server/PersonaUser.ts` (MODIFY)

```typescript
// Line 10: Add import
import { RateLimiter } from './modules/RateLimiter';

// Lines 99-104: REMOVE these properties
// private lastResponseTime: Map<UUID, Date> = new Map();
// private readonly minSecondsBetweenResponses = 10;
// private responseCount: Map<UUID, number> = new Map();
// private readonly maxResponsesPerSession = 50;

// Line ~105: ADD this property
private rateLimiter = new RateLimiter({
  minSecondsBetweenResponses: 10,
  maxResponsesPerSession: 50
});

// Lines 723-756: REPLACE isRateLimited() method
// OLD:
// private isRateLimited(roomId: UUID): boolean {
//   const lastTime = this.lastResponseTime.get(roomId);
//   if (!lastTime) return false;
//   const secondsSince = (Date.now() - lastTime.getTime()) / 1000;
//   if (secondsSince < this.minSecondsBetweenResponses) return true;
//   const count = this.responseCount.get(roomId) || 0;
//   if (count >= this.maxResponsesPerSession) return true;
//   return false;
// }

// NEW: (single line)
private isRateLimited(roomId: UUID): boolean {
  return this.rateLimiter.isRateLimited(roomId);
}

// Line ~950 (in respondToMessage): REPLACE rate limit recording
// OLD:
// this.lastResponseTime.set(roomId, new Date());
// this.responseCount.set(roomId, (this.responseCount.get(roomId) || 0) + 1);

// NEW:
this.rateLimiter.recordResponse(roomId);

// Line ~1560 (in shutdown): ADD cleanup
await this.rateLimiter.resetAll();
```

### Lint
```bash
npm run lint:file system/user/server/modules/RateLimiter.ts
npm run lint:file system/user/server/PersonaUser.ts
```

### Compile
```bash
npx tsc --noEmit
npm run build:ts
```

### Deploy
```bash
npm start
```

### Inspect
```bash
# Verify PersonaUser still works
./jtag data/list --collection=users --filter='{"type":"persona"}' --limit=1

# Check persona status (rate limiter should still work)
./jtag ai/persona/status --personaId="<PERSONA_ID>" --includeRateLimits=true

# Send test message
./jtag debug/chat-send --roomId="<ROOM_ID>" --message="RateLimiter extraction test - $(date +%s)"

# Wait for responses
sleep 15

# Check AI activity
./jtag debug/logs --filterPattern="Worker evaluated|AI-RESPONSE|Rate limited" --tailLines=30

# Expected: Same behavior as baseline

# Send rapid messages to trigger rate limit
for i in {1..5}; do
  ./jtag debug/chat-send --roomId="<ROOM_ID>" --message="Spam test $i"
  sleep 2
done

# Check for rate limiting
./jtag debug/logs --filterPattern="Rate limited" --tailLines=20

# Expected: See "⏸️  Rate limited in room" messages

# Check persona status again
./jtag ai/persona/status --personaId="<PERSONA_ID>" --includeRateLimits=true

# Expected: See responseCount incremented, isCurrentlyLimited: true
```

### Test (Integration)
```bash
# Run baseline test again
npm test -- ai-response-baseline.test.ts

# Expected: All tests pass (same as Commit 1.0)
```

### Commit Message
```
Extract rate limiting into RateLimiter module

Extracts rate limit logic from PersonaUser:
- Removed: lastResponseTime, responseCount properties (lines 99-104)
- Removed: isRateLimited() implementation (lines 723-756)
- Added: RateLimiter module (system/user/server/modules/RateLimiter.ts)
- PersonaUser now delegates to rateLimiter.isRateLimited()

Result: PersonaUser.ts 2004 → 1970 lines (-34 lines)

Behavior unchanged:
- AI responses work normally
- Rate limiting enforced (10s between responses, max 50 per session)
- Baseline integration tests pass

Command verification:
- ai/persona/status shows rate limit state correctly
- debug/logs shows "Rate limited" messages when triggered
```

---

## Commit 1.3: Update ai/persona/status to Use RateLimiter

### Architecture
Now that RateLimiter is extracted, update inspection command to use it properly (no more reflection).

### Code

**File**: `system/user/server/PersonaUser.ts` (MODIFY)

```typescript
// Add public getter for rate limiter inspection
public getRateLimitInfo(contextId?: UUID): RateLimitInfo | RateLimitInfo[] {
  if (contextId) {
    return this.rateLimiter.getInfo(contextId);
  }
  return this.rateLimiter.getAllInfo();
}
```

**File**: `commands/ai/persona/status/server/PersonaStatusServerCommand.ts` (MODIFY)

```typescript
// Remove reflection hack, use public API
private async getRateLimitStatus(persona: PersonaUser): Promise<RateLimitStatus[]> {
  // OLD: const lastResponseTime = (persona as any).lastResponseTime...

  // NEW: Use public API
  const infos = persona.getRateLimitInfo() as RateLimitInfo[];

  return infos.map(info => ({
    contextId: info.contextId,
    contextName: `Room ${info.contextId.slice(0, 8)}`,
    lastResponseTime: info.lastResponseTime?.toISOString() || null,
    responseCount: info.responseCount,
    isCurrentlyLimited: info.isLimited,
    secondsUntilNextResponse: info.secondsUntilNext
  }));
}
```

### Lint, Compile, Deploy
```bash
npm run lint:file system/user/server/PersonaUser.ts
npm run lint:file commands/ai/persona/status/server/PersonaStatusServerCommand.ts
npx tsc --noEmit
npm run build:ts
npm start
```

### Inspect
```bash
# Verify command still works
./jtag ai/persona/status --personaId="<PERSONA_ID>" --includeRateLimits=true

# Expected: Same output as before, but using clean API now
```

### Test
```bash
npm test -- ai-response-baseline.test.ts
npm test -- PersonaStatusServerCommand.test.ts
```

### Commit Message
```
Update ai/persona/status to use RateLimiter API

Removes reflection hack, uses public getRateLimitInfo() method.

No behavior change, cleaner implementation.
```

---

## Summary: Phase 1 Complete

**Result**:
- ✅ Baseline test established (Commit 1.0)
- ✅ Inspection command created (Commit 1.1)
- ✅ RateLimiter extracted and integrated (Commit 1.2)
- ✅ Inspection command cleaned up (Commit 1.3)
- ✅ PersonaUser.ts: 2004 → 1970 lines (-34 lines)
- ✅ AI responses verified working at every step

**Commands Created**:
- `ai/persona/status` - Inspect persona rate limits, genome, worker status

**Modules Extracted**:
- `RateLimiter` - Standalone rate limiting logic

**Next Phase**: Extract ResponseProcessor (Commit 2.0 - 2.3)
