# Phase 3bis Complete Implementation Plan

**Goal**: Implement Activity Ambient State + Autopilot + Self-Rating Training WITHOUT breaking working PersonaUser

**Constraint**: AIs MUST keep responding at every step

---

## Overview: The Complete System

```
Phase 3bis Components (7 sub-phases):
├── Sub-Phase 1: Activity State Infrastructure (state managers)
├── Sub-Phase 2: Activity Tracking (observation only)
├── Sub-Phase 3: Browser Integration (tab events)
├── Sub-Phase 4: Decision Context (augment with ambient)
├── Sub-Phase 5: Hybrid Decisions (heuristic + ambient)
├── Sub-Phase 6: LLM Decisions (feature flag)
└── Sub-Phase 7: Training Data + Self-Rating (continuous learning)
```

**Total Estimate**: 14-16 hours (implementation + testing)

---

## Sub-Phase 1: Activity State Infrastructure (2.5h)

### Goal
Create state managers WITHOUT touching PersonaUser

### Deliverables

1. **`ActivityStateManager.ts`** (`system/user/server/modules/state/ActivityStateManager.ts`)

```typescript
import type { UUID } from '../../../core/types/CrossPlatformUUID';

export interface ActivityState {
  activityId: UUID;
  temperature: number;        // 0.0-1.0: Conversation heat
  pressure: number;           // 0.0-1.0: Urgency
  userPresent: boolean;       // Is human viewing tab?
  lastInteraction: number;    // Timestamp
  isEngaging: boolean;        // Someone already handling?
  lastServiced: number;       // Timestamp
  servicedBy: UUID | null;    // Which persona serviced last
  participantCount: number;
}

export class ActivityStateManager {
  private static instance: ActivityStateManager;
  private states = new Map<UUID, ActivityState>();
  private decayInterval: NodeJS.Timeout | null = null;
  private readonly DECAY_INTERVAL_MS = 10000; // 10 seconds

  private constructor() {
    this.startDecayLoop();
  }

  static getInstance(): ActivityStateManager {
    if (!this.instance) {
      this.instance = new ActivityStateManager();
    }
    return this.instance;
  }

  /**
   * Get activity state (pull-based, zero I/O)
   */
  get(activityId: UUID): ActivityState {
    if (!this.states.has(activityId)) {
      this.states.set(activityId, this.createDefaultState(activityId));
    }
    return { ...this.states.get(activityId)! };
  }

  /**
   * Update activity state (called by events, commands)
   */
  update(activityId: UUID, updates: Partial<ActivityState>): void {
    const current = this.get(activityId);
    this.states.set(activityId, {
      ...current,
      ...updates,
      temperature: Math.max(0, Math.min(1, updates.temperature ?? current.temperature))
    });
  }

  /**
   * Human sent message → temperature rises
   */
  onHumanMessage(activityId: UUID): void {
    const current = this.get(activityId);
    this.update(activityId, {
      temperature: Math.min(1.0, current.temperature + 0.3),
      lastInteraction: Date.now(),
      participantCount: current.participantCount + 1
    });
  }

  /**
   * Message serviced → temperature drops, mark engaging
   */
  onMessageServiced(activityId: UUID, personaId: UUID): void {
    const current = this.get(activityId);
    this.update(activityId, {
      temperature: Math.max(0, current.temperature - 0.2),
      lastServiced: Date.now(),
      servicedBy: personaId,
      isEngaging: false
    });
  }

  /**
   * User left tab → temperature drops significantly
   */
  onUserLeft(activityId: UUID): void {
    const current = this.get(activityId);
    this.update(activityId, {
      temperature: Math.max(0, current.temperature - 0.4),
      userPresent: false
    });
  }

  /**
   * User present in tab → temperature rises
   */
  onUserPresent(activityId: UUID): void {
    const current = this.get(activityId);
    this.update(activityId, {
      temperature: Math.min(1.0, current.temperature + 0.2),
      userPresent: true
    });
  }

  /**
   * Decay loop: Temperature/pressure fall over time
   */
  private startDecayLoop(): void {
    if (this.decayInterval) return;

    this.decayInterval = setInterval(() => {
      this.decay();
    }, this.DECAY_INTERVAL_MS);
  }

  private decay(): void {
    const now = Date.now();
    for (const [activityId, state] of this.states.entries()) {
      const timeSinceInteraction = now - state.lastInteraction;
      if (timeSinceInteraction > 60000) { // 1 minute idle
        this.update(activityId, {
          temperature: Math.max(0, state.temperature - 0.05),
          pressure: Math.max(0, state.pressure - 0.05)
        });
      }
    }
  }

  private createDefaultState(activityId: UUID): ActivityState {
    return {
      activityId,
      temperature: 0.5,
      pressure: 0.5,
      userPresent: true,
      lastInteraction: Date.now(),
      isEngaging: false,
      lastServiced: 0,
      servicedBy: null,
      participantCount: 0
    };
  }

  shutdown(): void {
    if (this.decayInterval) {
      clearInterval(this.decayInterval);
      this.decayInterval = null;
    }
  }
}
```

2. **`SystemStateManager.ts`** (`system/user/server/modules/state/SystemStateManager.ts`)

```typescript
import type { UUID } from '../../../core/types/CrossPlatformUUID';

export interface SystemState {
  load: number;               // 0.0-1.0: System load
  activePersonas: UUID[];     // Currently active personas
  availablePersonas: UUID[];  // Idle personas
  gpuMemoryUsed: number;      // MB
  gpuMemoryTotal: number;     // MB
}

export class SystemStateManager {
  private static instance: SystemStateManager;
  private state: SystemState = {
    load: 0.0,
    activePersonas: [],
    availablePersonas: [],
    gpuMemoryUsed: 0,
    gpuMemoryTotal: 8192  // 8GB default
  };

  private constructor() {}

  static getInstance(): SystemStateManager {
    if (!this.instance) {
      this.instance = new SystemStateManager();
    }
    return this.instance;
  }

  /**
   * Get system state (pull-based, zero I/O)
   */
  getState(): SystemState {
    return { ...this.state };
  }

  /**
   * Update system state (called by personas, resource manager)
   */
  updateLoad(load: number): void {
    this.state.load = Math.max(0, Math.min(1, load));
  }

  updateActivePersonas(activePersonas: UUID[]): void {
    this.state.activePersonas = [...activePersonas];
  }

  updateGPUMemory(used: number, total: number): void {
    this.state.gpuMemoryUsed = used;
    this.state.gpuMemoryTotal = total;
  }
}
```

3. **`StimulusTypes.ts`** (`system/user/server/modules/stimulus/StimulusTypes.ts`)

```typescript
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { ActivityState } from '../state/ActivityStateManager';
import type { SystemState } from '../state/SystemStateManager';

export type StimulusType = 'chat-message' | 'game-action' | 'task-update' | 'system-event';

export interface Stimulus {
  id: UUID;
  type: StimulusType;
  activityId: UUID;
  content: any;
  ambient: ActivityState;  // Snapshot at emission time
  timestamp: number;
}

export interface DecisionContext {
  stimulus: Stimulus;
  activityState: ActivityState;
  systemState: SystemState;
  myState: PersonaInternalState;
  autopilot?: AutopilotSuggestion;
}

export interface PersonaInternalState {
  currentTask: any | null;
  energy: number;
  mood: string;
  attention: number;
}

export interface AutopilotSuggestion {
  shouldEngage: boolean;
  confidence: number;
  reasoning: string;
}

export interface Decision {
  engage: boolean;
  reasoning: string;
}
```

4. **Unit Tests**

```typescript
// tests/unit/ActivityStateManager.test.ts
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { ActivityStateManager } from '../../modules/state/ActivityStateManager';

describe('ActivityStateManager', () => {
  let manager: ActivityStateManager;

  beforeEach(() => {
    manager = ActivityStateManager.getInstance();
  });

  afterEach(() => {
    manager.shutdown();
  });

  it('should create default state for new activity', () => {
    const state = manager.get('test-activity' as any);
    expect(state.temperature).toBe(0.5);
    expect(state.userPresent).toBe(true);
  });

  it('should increase temperature on human message', () => {
    const activityId = 'test-activity' as any;
    const before = manager.get(activityId).temperature;

    manager.onHumanMessage(activityId);

    const after = manager.get(activityId).temperature;
    expect(after).toBeGreaterThan(before);
  });

  it('should decrease temperature on message serviced', () => {
    const activityId = 'test-activity' as any;
    manager.onHumanMessage(activityId); // Raise first

    const before = manager.get(activityId).temperature;
    manager.onMessageServiced(activityId, 'persona-id' as any);

    const after = manager.get(activityId).temperature;
    expect(after).toBeLessThan(before);
  });

  // ... more tests
});
```

### Testing

```bash
# Unit tests
npx vitest tests/unit/ActivityStateManager.test.ts
npx vitest tests/unit/SystemStateManager.test.ts

# Integration test (no regression)
npx vitest tests/integration/PersonaUser-Lifecycle.test.ts

# Manual verification
npm start
./jtag ping
```

### Success Criteria
- ✅ All unit tests pass
- ✅ PersonaUser-Lifecycle test still passes
- ✅ Zero changes to PersonaUser.ts

---

## Sub-Phase 2: Activity Tracking (1h)

### Goal
Add temperature tracking to PersonaUser (observation only, doesn't affect decisions)

### Changes to PersonaUser.ts

```typescript
import { ActivityStateManager } from './modules/state/ActivityStateManager';

// In handleChatMessage(), AFTER existing logic completes
private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
  // ... EXISTING LOGIC (unchanged) ...

  // NEW: Update activity temperature (observation only)
  if (messageEntity.source !== 'system') {
    ActivityStateManager.getInstance().onHumanMessage(messageEntity.roomId);
  }

  // NEW: Log ambient state for debugging
  const ambient = ActivityStateManager.getInstance().get(messageEntity.roomId);
  console.log(`🌡️ ${this.displayName}: Ambient state - temp=${ambient.temperature.toFixed(2)}, userPresent=${ambient.userPresent}`);
}

// In postMessage(), AFTER posting succeeds
private async postMessage(...): Promise<void> {
  // ... EXISTING LOGIC (unchanged) ...

  // NEW: Mark message as serviced
  ActivityStateManager.getInstance().onMessageServiced(roomId, this.id);
}
```

### Testing

```bash
# Deploy
npm start

# Send test message
./jtag debug/chat-send --roomId="UUID" --message="test"

# Check logs for 🌡️ markers
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "🌡️"

# Verify temperature rises
./jtag debug/chat-send --roomId="UUID" --message="test2"
# Should see temperature increase in logs
```

### Success Criteria
- ✅ Logs show temperature changes
- ✅ AIs still respond normally
- ✅ No errors in server logs

---

## Sub-Phase 3: Browser Integration (1.5h)

### Goal
Hook tab focus/blur to activity temperature

### New Commands

1. **`commands/activity/user-left/`**

```typescript
// server/ActivityUserLeftServerCommand.ts
async execute(params: ActivityUserLeftParams): Promise<ActivityUserLeftResult> {
  ActivityStateManager.getInstance().onUserLeft(params.activityId);
  return { success: true };
}
```

2. **`commands/activity/user-present/`**

```typescript
// server/ActivityUserPresentServerCommand.ts
async execute(params: ActivityUserPresentParams): Promise<ActivityUserPresentResult> {
  ActivityStateManager.getInstance().onUserPresent(params.activityId);
  return { success: true };
}
```

3. **Browser Integration** (`browser/widgets/main-widget/MainWidget.ts`)

```typescript
// Add visibility change handler
document.addEventListener('visibilitychange', async () => {
  const currentRoomId = this.getCurrentRoomId();
  if (!currentRoomId) return;

  if (document.hidden) {
    await Commands.execute('activity/user-left', { activityId: currentRoomId });
  } else {
    await Commands.execute('activity/user-present', { activityId: currentRoomId });
  }
});
```

### Testing

```bash
npm start

# Open browser, navigate to chat
# Leave tab → check logs for temperature drop
# Return to tab → check logs for temperature rise

tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "🌡️"
```

### Success Criteria
- ✅ Temperature drops when tab loses focus
- ✅ Temperature rises when tab gains focus
- ✅ AIs still respond normally

---

## Sub-Phase 4: Decision Context (1h)

### Goal
Augment decision functions with ambient context (log it, don't use it yet)

### Changes to PersonaUser.ts

```typescript
// Update shouldRespond signature (optional param)
private async shouldRespond(
  messageEntity: ChatMessageEntity,
  roomEntity: RoomEntity,
  ambient?: ActivityState  // NEW: Optional ambient context
): Promise<GatingResult> {
  // ... existing logic ...

  // NEW: Log ambient if provided
  if (ambient) {
    console.log(`🌡️ ${this.displayName}: Decision context - temp=${ambient.temperature.toFixed(2)}, userPresent=${ambient.userPresent}, isEngaging=${ambient.isEngaging}`);
  }

  return gatingResult;
}

// In handleChatMessage, pass ambient to shouldRespond
const ambient = ActivityStateManager.getInstance().get(messageEntity.roomId);
const gatingResult = await this.shouldRespond(messageEntity, roomEntity, ambient);
```

### Testing

```bash
npm start
./jtag debug/chat-send --roomId="UUID" --message="test"

# Check logs - should see ambient context in decision logs
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "Decision context"
```

### Success Criteria
- ✅ Logs show ambient context during decisions
- ✅ Decisions unchanged (ambient not affecting yet)
- ✅ AIs still respond normally

---

## Sub-Phase 5: Hybrid Decisions (2h)

### Goal
Blend heuristic priority with ambient state

### Changes to PersonaUser.ts

```typescript
private async shouldRespond(
  messageEntity: ChatMessageEntity,
  roomEntity: RoomEntity,
  ambient?: ActivityState
): Promise<GatingResult> {
  // ... existing heuristic calculation ...

  // NEW: Temperature-adjusted threshold
  const baseThreshold = 0.5;
  let adjustedThreshold = baseThreshold;

  if (ambient) {
    // High temperature → lower threshold (more responsive)
    // Low temperature → higher threshold (less responsive)
    const tempAdjustment = (ambient.temperature - 0.5) * 0.2;
    adjustedThreshold = Math.max(0.2, Math.min(0.8, baseThreshold + tempAdjustment));

    console.log(`🌡️ ${this.displayName}: Threshold adjustment - base=${baseThreshold.toFixed(2)}, temp=${ambient.temperature.toFixed(2)}, adjusted=${adjustedThreshold.toFixed(2)}`);

    // Defer if someone already engaging (unless mentioned)
    if (ambient.isEngaging && !isMentioned) {
      console.log(`🌡️ ${this.displayName}: Deferring - another persona engaging`);
      return { shouldRespond: false, reasoning: 'Another persona handling' };
    }
  }

  // Use adjusted threshold
  const shouldRespond = priority >= adjustedThreshold;

  return { shouldRespond, reasoning: `Priority ${priority.toFixed(2)} vs threshold ${adjustedThreshold.toFixed(2)}` };
}
```

### Testing

```bash
npm start

# Hot conversation test (rapid messages)
./jtag debug/chat-send --roomId="UUID" --message="test1"
./jtag debug/chat-send --roomId="UUID" --message="test2"
./jtag debug/chat-send --roomId="UUID" --message="test3"
# Should see increased responsiveness

# Cold conversation test (wait 2 minutes, then message)
# Should see decreased responsiveness (higher threshold)

# Compare logs
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "Threshold adjustment"
```

### Success Criteria
- ✅ Hot conversations get more responses
- ✅ Cold conversations get fewer responses (but still respond to mentions)
- ✅ Multiple personas don't pile on (isEngaging check works)

---

## Sub-Phase 6: LLM Decisions (3h)

### Goal
Implement full LLM-based decisions (feature flag)

### Changes to PersonaUser.ts

```typescript
// Add feature flag
private useLLMDecision: boolean = process.env.PERSONA_LLM_DECISION === 'true';

// New method: LLM-based decision
private async decideLLM(context: DecisionContext): Promise<Decision> {
  const prompt = `You are ${this.displayName}. Should you respond to this message?

Message: "${context.stimulus.content}"
Activity temperature: ${context.activityState.temperature.toFixed(2)} (0=cold, 1=hot)
User present: ${context.activityState.userPresent}
Someone already engaging: ${context.activityState.isEngaging}
Your current task: ${context.myState.currentTask?.description || 'none'}
System load: ${context.systemState.load.toFixed(2)}

Respond with JSON: { "engage": boolean, "reasoning": string }`;

  try {
    const result = await this.callLLM(prompt, { maxTokens: 100, temperature: 0.3 });
    const decision = JSON.parse(result);

    console.log(`🧠 ${this.displayName}: LLM decision - engage=${decision.engage}, reasoning="${decision.reasoning}"`);

    return decision;
  } catch (error) {
    console.warn(`⚠️ ${this.displayName}: LLM decision failed, falling back to heuristic`);
    // Fallback to hybrid heuristic+ambient
    return this.decideHeuristic(context);
  }
}

// In shouldRespond
if (this.useLLMDecision && ambient) {
  const context: DecisionContext = {
    stimulus: { /* ... */ },
    activityState: ambient,
    systemState: SystemStateManager.getInstance().getState(),
    myState: this.getMyState()
  };

  const decision = await this.decideLLM(context);
  return { shouldRespond: decision.engage, reasoning: decision.reasoning };
} else {
  // Fallback to hybrid heuristic+ambient (Sub-Phase 5)
  return this.decideHeuristic(messageEntity, roomEntity, ambient);
}
```

### Testing

```bash
# Test with heuristic (baseline)
PERSONA_LLM_DECISION=false npm start
./jtag debug/chat-send --roomId="UUID" --message="test"
# Record decisions

# Test with LLM
PERSONA_LLM_DECISION=true npm start
./jtag debug/chat-send --roomId="UUID" --message="test"
# Record decisions

# Compare decision quality, latency, consistency
```

### Success Criteria
- ✅ LLM decisions work when flag enabled
- ✅ Heuristic decisions work when flag disabled
- ✅ LLM gracefully falls back on error
- ✅ Decision quality comparable or better than heuristic

---

## Sub-Phase 7: Training Data + Self-Rating (3h)

### Goal
Automatic training data collection with LLM self-rating

### 1. Create DecisionEntity

```typescript
// system/data/entities/DecisionEntity.ts
import type { BaseEntity } from './BaseEntity';
import type { UUID } from '../../core/types/CrossPlatformUUID';

export interface DecisionEntity extends BaseEntity {
  personaId: UUID;
  stimulusType: 'chat-message' | 'game-action' | 'task-update';
  stimulusId: UUID;
  activityId: UUID;

  // Context
  temperature: number;
  userPresent: boolean;
  systemLoad: number;

  // Autopilot (if available)
  autopilotRecommendation: 'engage' | 'defer' | 'ignore' | null;
  autopilotConfidence: number | null;
  autopilotReasoning: string | null;

  // Actual decision
  actualDecision: 'engage' | 'defer' | 'ignore';
  actualReasoning: string;

  // Rating
  autopilotRating: 'good' | 'neutral' | 'bad' | null;

  // Outcome (measured later)
  outcome: 'good' | 'neutral' | 'bad' | null;

  timestamp: string;
}
```

### 2. Register in EntityRegistry

```typescript
// system/data/config/EntityRegistry.ts
import { COLLECTIONS } from './DatabaseConfig';

export const ENTITY_REGISTRY = {
  // ... existing entities ...
  [COLLECTIONS.DECISIONS]: {
    fields: {
      personaId: 'TEXT',
      stimulusType: 'TEXT',
      stimulusId: 'TEXT',
      activityId: 'TEXT',
      temperature: 'REAL',
      userPresent: 'INTEGER',
      systemLoad: 'REAL',
      autopilotRecommendation: 'TEXT',
      autopilotConfidence: 'REAL',
      autopilotReasoning: 'TEXT',
      actualDecision: 'TEXT',
      actualReasoning: 'TEXT',
      autopilotRating: 'TEXT',
      outcome: 'TEXT',
      timestamp: 'TEXT'
    },
    indices: [
      { fields: ['personaId', 'timestamp'] },
      { fields: ['autopilotRating'] },
      { fields: ['outcome'] }
    ]
  }
};
```

### 3. Add DECISIONS collection

```typescript
// system/data/config/DatabaseConfig.ts
export const COLLECTIONS = {
  // ... existing collections ...
  DECISIONS: 'decisions'
} as const;
```

### 4. Add recordDecision to PersonaUser

```typescript
// In PersonaUser.ts
private async recordDecision(
  stimulus: any,
  ambient: ActivityState,
  autopilot: AutopilotSuggestion | null,
  decision: Decision
): Promise<void> {
  // Feature flag
  if (process.env.ENABLE_TRAINING_DATA !== 'true') return;

  try {
    // Rate autopilot if available
    let autopilotRating: 'good' | 'neutral' | 'bad' | null = null;
    if (autopilot) {
      autopilotRating = this.rateAutopilot(autopilot, decision);
    }

    // Save decision
    await Commands.execute('data/create', {
      collection: COLLECTIONS.DECISIONS,
      data: {
        personaId: this.id,
        stimulusType: 'chat-message',
        stimulusId: stimulus.id,
        activityId: ambient.activityId,
        temperature: ambient.temperature,
        userPresent: ambient.userPresent,
        systemLoad: SystemStateManager.getInstance().getState().load,
        autopilotRecommendation: autopilot?.shouldEngage ? 'engage' : 'defer',
        autopilotConfidence: autopilot?.confidence || null,
        autopilotReasoning: autopilot?.reasoning || null,
        actualDecision: decision.engage ? 'engage' : 'defer',
        actualReasoning: decision.reasoning,
        autopilotRating,
        outcome: null,  // Measured later
        timestamp: new Date().toISOString()
      }
    });

    console.log(`📊 ${this.displayName}: Decision recorded (autopilot rating: ${autopilotRating || 'N/A'})`);
  } catch (error) {
    // Silent fail - don't break persona if training data fails
    console.warn(`⚠️ ${this.displayName}: Failed to record decision:`, error);
  }
}

private rateAutopilot(
  autopilot: AutopilotSuggestion,
  actual: Decision
): 'good' | 'neutral' | 'bad' {
  // Agreement check
  if (autopilot.shouldEngage === actual.engage) {
    // Autopilot was correct
    if (autopilot.confidence > 0.8) {
      return 'good';  // High confidence, correct
    }
    return 'neutral';  // Correct but not confident
  } else {
    // Autopilot was wrong
    if (autopilot.confidence > 0.8) {
      return 'bad';  // High confidence but WRONG
    }
    return 'neutral';  // Low confidence, wrong (not terrible)
  }
}

// Call in decision flow
const decision = await this.decideLLM(context);
await this.recordDecision(stimulus, ambient, context.autopilot, decision);
```

### 5. Export Command

Already works! Just need to add collection:

```bash
./jtag data/list --collection=decisions \
  --filter='{"personaId":"helper-ai","autopilotRating":"good"}' \
  --limit=1000
```

### Testing

```bash
# Enable training data collection
ENABLE_TRAINING_DATA=true PERSONA_LLM_DECISION=true npm start

# Generate decisions
./jtag debug/chat-send --roomId="UUID" --message="test1"
./jtag debug/chat-send --roomId="UUID" --message="test2"

# Check decisions recorded
./jtag data/list --collection=decisions --limit=10

# Export for training
./jtag data/list --collection=decisions \
  --filter='{"autopilotRating":"good"}' \
  --limit=1000 > /tmp/training-data.json
```

### Success Criteria
- ✅ Decisions automatically saved to database
- ✅ Autopilot self-rating works
- ✅ Export command works
- ✅ No performance impact on PersonaUser

---

## Timeline Summary

| Sub-Phase | Work | Testing | Total |
|-----------|------|---------|-------|
| 1. State Infrastructure | 2h | 30min | 2.5h |
| 2. Activity Tracking | 30min | 30min | 1h |
| 3. Browser Integration | 1h | 30min | 1.5h |
| 4. Decision Context | 30min | 30min | 1h |
| 5. Hybrid Decisions | 1h | 1h | 2h |
| 6. LLM Decisions | 2h | 1h | 3h |
| 7. Training Data | 2h | 1h | 3h |
| **Total** | **9h** | **5h** | **14h** |

---

## Rollback Strategy

**If any sub-phase breaks AIs**:

1. **Immediate**: Revert PersonaUser.ts changes for that sub-phase
2. **Keep**: All previous sub-phases (they're additive)
3. **Investigate**: What assumption was wrong?
4. **Fix**: Address root cause in separate branch
5. **Retry**: Merge fix, retry sub-phase

**Nuclear Option**: `git stash` all changes, restore working baseline

---

## Success Metrics

### After Sub-Phase 5 (Hybrid)
- Hot conversations (temp > 0.7): Response rate increases 20-30%
- Cold conversations (temp < 0.3): Response rate decreases 20-30% (but mentions still work)
- Multiple personas: No pileups (isEngaging check prevents redundant responses)

### After Sub-Phase 6 (LLM)
- Decision quality: Comparable or better than heuristic
- Latency: Acceptable (2-3s for full LLM decision)
- Fallback: Graceful degradation on LLM errors

### After Sub-Phase 7 (Training)
- Week 1: 1000+ decisions recorded
- Week 2: Autopilot training data ready for fine-tuning
- Week 4: Autopilot accuracy > 70%
- Week 8: Autopilot accuracy > 85%, handling 80% of decisions

---

## The Training Flywheel (Post-Phase 7)

**Week 1-2**: Collect baseline decisions with LLM + self-rating
**Week 3**: Fine-tune autopilot on good ratings
**Week 4**: Deploy autopilot, collect more decisions (now faster)
**Week 5-6**: Fine-tune again with combined data
**Week 7-8**: Autopilot handles 80%+ of decisions at 50ms latency

**Result**: System becomes ultra-responsive while continuously improving

---

## Current Status

- ✅ Architecture documented (PERSONA-CONVERGENCE-ROADMAP.md)
- ✅ Migration strategy designed (PHASE-3BIS-MIGRATION-PLAN.md)
- ✅ Complete plan with self-rating (this document)
- ⏳ Sub-Phase 1: Not started
- ⏳ Sub-Phase 2: Not started
- ⏳ Sub-Phase 3: Not started
- ⏳ Sub-Phase 4: Not started
- ⏳ Sub-Phase 5: Not started
- ⏳ Sub-Phase 6: Not started
- ⏳ Sub-Phase 7: Not started

---

## Next Step

**Begin Sub-Phase 1**: Create state managers (ActivityStateManager, SystemStateManager, StimulusTypes)

DO NOT proceed to Sub-Phase 2 until Sub-Phase 1 tests pass.
