# Phase 3bis REVISED - ThoughtStream Temperature Integration

**Key Insight**: ThoughtStream IS already the per-activity coordination mechanism. We just need to add temperature tracking to it.

**Total Estimate**: 5.5 hours (vs 14 hours in original wrong plan)

---

## Current Architecture (What Already Works)

```typescript
// ChatCoordinationStream - ALREADY coordinates per-room
class ChatCoordinationStream {
  private thoughts = new Map<UUID, ChatThought[]>();  // Per-room thought tracking

  isAnyoneResponding(roomId: UUID): boolean {
    // Already checks if someone is EVALUATING or GENERATING
  }
}

// PersonaUser - Uses ThoughtStream
private async handleChatMessage(messageEntity: ChatMessageEntity) {
  // 1. Calculate heuristic priority
  const priority = calculateMessagePriority(message, profile);

  // 2. Enqueue to inbox
  await this.inbox.enqueue({ ...message, priority });

  // 3. Autonomous loop polls and processes
}

// PersonaInbox - Heuristic priority calculation
export function calculateMessagePriority(
  message: InboxMessageData,
  personaProfile: PersonaProfile
): number {
  // Heuristic: base + mentions + recency + conversation + expertise
}
```

---

## Phase 1: Add Temperature to ThoughtStream (1h)

### Changes to ChatCoordinationStream

```typescript
// system/coordination/server/ChatCoordinationStream.ts

export class ChatCoordinationStream extends BaseCoordinationStream<ChatThought> {
  private thoughts = new Map<UUID, ChatThought[]>();

  // NEW: Temperature tracking per room
  private roomTemperatures = new Map<UUID, number>();
  private roomUserPresent = new Map<UUID, boolean>();
  private decayInterval: NodeJS.Timeout | null = null;

  constructor(client: JTAGClient) {
    super(client);
    this.startTemperatureDecay();
  }

  // NEW: Update temperature when human posts
  onHumanMessage(roomId: UUID): void {
    const current = this.roomTemperatures.get(roomId) ?? 0.5;
    this.roomTemperatures.set(roomId, Math.min(1.0, current + 0.3));
    console.log(`🌡️ ThoughtStream: Room ${roomId.substring(0, 8)} temperature: ${current.toFixed(2)} → ${Math.min(1.0, current + 0.3).toFixed(2)}`);
  }

  // NEW: Update temperature when message serviced
  onMessageServiced(roomId: UUID): void {
    const current = this.roomTemperatures.get(roomId) ?? 0.5;
    this.roomTemperatures.set(roomId, Math.max(0, current - 0.2));
  }

  // NEW: User presence tracking
  onUserPresent(roomId: UUID, present: boolean): void {
    this.roomUserPresent.set(roomId, present);
    if (!present) {
      // User left - significant temperature drop
      const current = this.roomTemperatures.get(roomId) ?? 0.5;
      this.roomTemperatures.set(roomId, Math.max(0, current - 0.4));
      console.log(`🌡️ ThoughtStream: User left room ${roomId.substring(0, 8)}, temperature: ${current.toFixed(2)} → ${Math.max(0, current - 0.4).toFixed(2)}`);
    } else {
      // User returned - modest temperature rise
      const current = this.roomTemperatures.get(roomId) ?? 0.5;
      this.roomTemperatures.set(roomId, Math.min(1.0, current + 0.2));
    }
  }

  // NEW: Get current temperature
  getTemperature(roomId: UUID): number {
    return this.roomTemperatures.get(roomId) ?? 0.5;
  }

  // NEW: Get user presence
  isUserPresent(roomId: UUID): boolean {
    return this.roomUserPresent.get(roomId) ?? true;
  }

  // NEW: Temperature decay loop (exponential decay)
  private static readonly DECAY_RATE = 0.95;      // 5% decay per interval
  private static readonly DECAY_INTERVAL_MS = 10000;  // 10 seconds
  private static readonly TEMP_FLOOR = 0.01;      // Minimum temperature (never fully cold)

  private startTemperatureDecay(): void {
    if (this.decayInterval) return;

    this.decayInterval = setInterval(() => {
      const now = Date.now();
      for (const [roomId, temp] of this.roomTemperatures) {
        // Only decay if no recent activity (1 minute threshold)
        const recentThoughts = this.getRecentThoughts(roomId, 60000);
        if (recentThoughts.length === 0 && temp > ChatCoordinationStream.TEMP_FLOOR) {
          // Exponential decay: temp * DECAY_RATE
          // Hot rooms cool faster initially, cold rooms barely change
          const newTemp = temp * ChatCoordinationStream.DECAY_RATE;
          this.roomTemperatures.set(roomId, Math.max(ChatCoordinationStream.TEMP_FLOOR, newTemp));
        }
      }
    }, ChatCoordinationStream.DECAY_INTERVAL_MS);
  }

  shutdown(): void {
    if (this.decayInterval) {
      clearInterval(this.decayInterval);
      this.decayInterval = null;
    }
  }
}
```

### Testing Phase 1

```bash
# Unit tests (add to existing ChatCoordinationStream tests)
npx vitest system/coordination/server/tests/ChatCoordinationStream.test.ts

# Integration test (should still pass - temperature not used yet)
npx vitest system/user/server/tests/integration/PersonaUser-Lifecycle.test.ts

# Manual verification
npm start
# Temperature tracked but not affecting decisions yet
```

**Success Criteria**:
- ✅ Temperature tracked per room
- ✅ All existing tests still pass
- ✅ Zero breaking changes

---

## Phase 2: Log Temperature (30min)

### Changes to PersonaUser

```typescript
// system/user/server/PersonaUser.ts

private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
  // ... existing checks ...

  // NEW: Update temperature when human posts
  if (messageEntity.source !== 'system' && this.client) {
    const coordinator = getChatCoordinator(this.client);
    coordinator.onHumanMessage(messageEntity.roomId);
  }

  // Calculate priority (existing code)
  const priority = calculateMessagePriority(/* ... */);

  // NEW: Log temperature (observation only, doesn't affect decisions)
  if (this.client) {
    const coordinator = getChatCoordinator(this.client);
    const temp = coordinator.getTemperature(messageEntity.roomId);
    const userPresent = coordinator.isUserPresent(messageEntity.roomId);
    console.log(`🌡️ ${this.displayName}: Room temperature=${temp.toFixed(2)}, userPresent=${userPresent}`);
  }

  // ... rest of existing code ...
}

// In postMessage(), after successful post
private async postMessage(...): Promise<void> {
  // ... existing code ...

  // NEW: Mark message as serviced (temperature drops)
  if (this.client) {
    const coordinator = getChatCoordinator(this.client);
    coordinator.onMessageServiced(roomId);
  }
}
```

### Testing Phase 2

```bash
npm start

# Send messages, watch temperature rise
./jtag debug/chat-send --roomId="UUID" --message="test1"
./jtag debug/chat-send --roomId="UUID" --message="test2"

# Check logs
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "🌡️"

# Wait 1 minute, temperature should decay
```

**Success Criteria**:
- ✅ Logs show temperature changes
- ✅ Temperature rises on human messages
- ✅ Temperature falls when serviced
- ✅ Temperature decays over time
- ✅ AIs still respond normally (decisions unchanged)

---

## Phase 3: Browser Integration (1h)

### New Commands

**`commands/activity/user-presence/`**

```typescript
// shared/ActivityUserPresenceTypes.ts
export interface ActivityUserPresenceParams {
  activityId: UUID;
  present: boolean;
}

export interface ActivityUserPresenceResult {
  success: boolean;
}

// server/ActivityUserPresenceServerCommand.ts
import { getChatCoordinator } from '../../../system/coordination/server/ChatCoordinationStream';

export class ActivityUserPresenceServerCommand {
  async execute(params: ActivityUserPresenceParams): Promise<ActivityUserPresenceResult> {
    const coordinator = getChatCoordinator(this.client);
    coordinator.onUserPresent(params.activityId, params.present);
    return { success: true };
  }
}
```

### Browser Integration

```typescript
// browser/widgets/main-widget/MainWidget.ts (or chat-widget)

// Add visibility change handler
private setupVisibilityTracking(): void {
  document.addEventListener('visibilitychange', async () => {
    const currentRoomId = this.getCurrentRoomId();
    if (!currentRoomId) return;

    await Commands.execute('activity/user-presence', {
      activityId: currentRoomId,
      present: !document.hidden
    });
  });
}

// Call in constructor/initialization
constructor() {
  // ... existing code ...
  this.setupVisibilityTracking();
}
```

### Testing Phase 3

```bash
npm start

# Open browser, navigate to chat room
# Leave tab (Cmd+Tab away)
# Check logs - should see temperature drop

tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "🌡️.*User left"

# Return to tab
# Check logs - should see temperature rise
```

**Success Criteria**:
- ✅ Temperature drops when tab loses focus
- ✅ Temperature rises when tab gains focus
- ✅ Logs show visibility changes
- ✅ AIs still respond normally

---

## Phase 4: Use Temperature in Priority (1h)

### Changes to PersonaInbox

```typescript
// system/user/server/modules/PersonaInbox.ts

export function calculateMessagePriority(
  message: InboxMessageData,
  personaProfile: PersonaProfile,
  coordinator?: ChatCoordinationStream  // NEW: Optional coordinator
): number {
  let priority = 0.2; // base

  // Existing heuristic logic
  const messageText = message.content.toLowerCase();

  // @mention bonus
  const personaName = personaProfile.displayName.toLowerCase();
  if (messageText.includes(`@${personaName}`)) {
    priority += 0.4;
  }

  // Recency bonus
  const messageAge = Date.now() - message.timestamp;
  const ageMinutes = messageAge / 60000;
  if (ageMinutes < 1) priority += 0.2;
  else if (ageMinutes < 5) priority += 0.1;

  // Conversation bonus (already in room)
  if (personaProfile.recentRooms.includes(message.roomId)) {
    priority += 0.1;
  }

  // NEW: Temperature adjustment
  if (coordinator) {
    const temp = coordinator.getTemperature(message.roomId);
    const userPresent = coordinator.isUserPresent(message.roomId);

    // Temperature bonus: ±0.1 based on heat (0.5 is neutral)
    const tempBonus = (temp - 0.5) * 0.2;
    priority += tempBonus;

    // User presence penalty (don't spam if they're gone)
    if (!userPresent && temp < 0.3) {
      priority *= 0.5;  // Halve priority if user left and room cold
    }

    // Coordination check - lower priority if someone else handling
    if (coordinator.isAnyoneResponding(message.roomId)) {
      priority *= 0.7;  // 30% reduction if someone else engaged
    }

    console.log(`🌡️ Priority adjustment: temp=${temp.toFixed(2)}, tempBonus=${tempBonus.toFixed(2)}, userPresent=${userPresent}, finalPriority=${priority.toFixed(2)}`);
  }

  return Math.min(1.0, Math.max(0, priority));
}
```

### Changes to PersonaUser

```typescript
// system/user/server/PersonaUser.ts

private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
  // ... existing checks ...

  // Update temperature
  if (messageEntity.source !== 'system' && this.client) {
    const coordinator = getChatCoordinator(this.client);
    coordinator.onHumanMessage(messageEntity.roomId);
  }

  // Calculate priority WITH coordinator context (NEW: pass coordinator)
  const coordinator = this.client ? getChatCoordinator(this.client) : undefined;
  const priority = calculateMessagePriority(
    {
      content: messageEntity.content?.text || '',
      timestamp: this.timestampToNumber(messageEntity.timestamp),
      roomId: messageEntity.roomId
    },
    {
      displayName: this.displayName,
      id: this.id,
      recentRooms: Array.from(this.myRoomIds),
      expertise: []
    },
    coordinator  // NEW: Pass coordinator for temperature-aware priority
  );

  // ... rest unchanged ...
}
```

### Testing Phase 4

```bash
npm start

# Hot conversation test
./jtag debug/chat-send --roomId="UUID" --message="test1"
./jtag debug/chat-send --roomId="UUID" --message="test2"
./jtag debug/chat-send --roomId="UUID" --message="test3"
# Should see more responses (higher priority from temperature)

# Cold conversation test (wait 2 minutes)
./jtag debug/chat-send --roomId="UUID" --message="test after cooldown"
# Should see fewer responses (lower priority from low temperature)

# Multiple persona test
./jtag debug/chat-send --roomId="UUID" --message="everyone respond"
# Should see coordination - not all personas pile on (isAnyoneResponding check)

# Check logs
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "Priority adjustment"
```

**Success Criteria**:
- ✅ Hot conversations (temp > 0.7): Priority increases, more responses
- ✅ Cold conversations (temp < 0.3): Priority decreases, fewer responses
- ✅ Multiple personas coordinate (no pileups)
- ✅ User absence respected (low priority if user left and room cold)

---

## Phase 5: Training Data + Self-Rating (2h)

### Create DecisionEntity

```typescript
// system/data/entities/DecisionEntity.ts
import type { BaseEntity } from './BaseEntity';
import type { UUID } from '../../core/types/CrossPlatformUUID';

export interface DecisionEntity extends BaseEntity {
  personaId: UUID;
  messageId: UUID;
  roomId: UUID;

  // Context
  temperature: number;
  userPresent: boolean;
  priority: number;

  // Decision
  decided: 'respond' | 'silent';
  reasoning: string;

  // Outcome (measured later)
  outcome: 'good' | 'neutral' | 'bad' | null;

  timestamp: string;
}
```

### Register in EntityRegistry

```typescript
// system/data/config/EntityRegistry.ts
[COLLECTIONS.DECISIONS]: {
  fields: {
    personaId: 'TEXT',
    messageId: 'TEXT',
    roomId: 'TEXT',
    temperature: 'REAL',
    userPresent: 'INTEGER',
    priority: 'REAL',
    decided: 'TEXT',
    reasoning: 'TEXT',
    outcome: 'TEXT',
    timestamp: 'TEXT'
  },
  indices: [
    { fields: ['personaId', 'timestamp'] },
    { fields: ['outcome'] }
  ]
}
```

### Add DECISIONS Collection

```typescript
// system/data/config/DatabaseConfig.ts
export const COLLECTIONS = {
  // ... existing ...
  DECISIONS: 'decisions'
} as const;
```

### Record Decisions in PersonaUser

```typescript
// system/user/server/PersonaUser.ts

private async recordDecision(
  messageEntity: ChatMessageEntity,
  priority: number,
  decided: 'respond' | 'silent',
  reasoning: string
): Promise<void> {
  if (process.env.ENABLE_TRAINING_DATA !== 'true') return;

  try {
    const coordinator = this.client ? getChatCoordinator(this.client) : null;
    const temp = coordinator?.getTemperature(messageEntity.roomId) ?? 0.5;
    const userPresent = coordinator?.isUserPresent(messageEntity.roomId) ?? true;

    await Commands.execute('data/create', {
      collection: COLLECTIONS.DECISIONS,
      data: {
        personaId: this.id,
        messageId: messageEntity.id,
        roomId: messageEntity.roomId,
        temperature: temp,
        userPresent,
        priority,
        decided,
        reasoning,
        outcome: null,  // Measured later
        timestamp: new Date().toISOString()
      }
    });

    console.log(`📊 ${this.displayName}: Decision recorded (${decided}, temp=${temp.toFixed(2)}, priority=${priority.toFixed(2)})`);
  } catch (error) {
    console.warn(`⚠️ Failed to record decision:`, error);
  }
}

// Call after decision made
private async evaluateAndPossiblyRespond(...): Promise<void> {
  // ... existing decision logic ...

  // NEW: Record decision
  await this.recordDecision(
    messageEntity,
    priority,
    gatingResult.shouldRespond ? 'respond' : 'silent',
    gatingResult.reasoning
  );

  // ... rest of existing code ...
}
```

### Export Training Data

```bash
# Enable training data collection
ENABLE_TRAINING_DATA=true npm start

# Let it run, collect decisions
# ...

# Export decisions
./jtag data/list --collection=decisions \
  --filter='{"personaId":"helper-ai"}' \
  --limit=1000 > /tmp/helper-ai-decisions.json

# Analyze temperature correlation
./jtag data/list --collection=decisions \
  --filter='{"decided":"respond"}' | \
  jq '[.[] | {temp: .temperature, priority: .priority}] |
      group_by(.temp > 0.7) |
      map({hot: .[0].temp > 0.7, count: length, avgPriority: (map(.priority) | add / length)})'
```

### Testing Phase 5

```bash
# Generate training data
ENABLE_TRAINING_DATA=true npm start

./jtag debug/chat-send --roomId="UUID" --message="test1"
./jtag debug/chat-send --roomId="UUID" --message="test2"

# Check decisions recorded
./jtag data/list --collection=decisions --limit=10

# Verify temperature captured
./jtag data/list --collection=decisions | jq '.[] | {temp, decided, priority}'
```

**Success Criteria**:
- ✅ Decisions automatically recorded
- ✅ Temperature context captured
- ✅ Export command works
- ✅ No performance impact

---

## Timeline Summary

| Phase | Work | Testing | Total |
|-------|------|---------|-------|
| 1. Temperature in ThoughtStream | 45min | 15min | 1h |
| 2. Log Temperature | 20min | 10min | 30min |
| 3. Browser Integration | 45min | 15min | 1h |
| 4. Use in Priority | 45min | 15min | 1h |
| 5. Training Data | 1.5h | 30min | 2h |
| **Total** | **4h** | **1.5h** | **5.5h** |

---

## What Makes This Better

1. **Works WITH ThoughtStream** (not creating parallel system)
2. **ThoughtStream already scoped to rooms** (activities)
3. **Already has `isAnyoneResponding()`** (coordination primitive)
4. **Minimal code changes** (~100 lines total vs ~500 in wrong plan)
5. **Natural fit** - temperature IS coordination state

---

## Training Flywheel (Post-Phase 5)

**Week 1-2**: Collect baseline decisions with temperature context
**Week 3**: Analyze temperature/decision correlation
**Week 4**: Fine-tune priority calculation based on data
**Week 6**: Deploy optimized priority, collect more
**Week 8**: System learns optimal temperature thresholds

**Result**: Temperature-aware decisions that continuously improve

---

## Current Status

- ✅ Architecture audited against actual code
- ✅ Corrected plan using ThoughtStream
- ⏳ Phase 1: Not started
- ⏳ Phase 2: Not started
- ⏳ Phase 3: Not started
- ⏳ Phase 4: Not started
- ⏳ Phase 5: Not started

---

## Next Step

**Begin Phase 1**: Add temperature tracking to ChatCoordinationStream

DO NOT proceed to Phase 2 until Phase 1 tests pass.
