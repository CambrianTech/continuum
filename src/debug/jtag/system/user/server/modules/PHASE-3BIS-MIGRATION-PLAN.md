# Phase 3bis Migration Plan - Activity Ambient State

## Context

**Current State**: PersonaUser.ts (2540 lines) with working message handling
- Uses heuristic `calculateMessagePriority()` function
- CNS orchestration with ThoughtStream coordination
- Working rate limiting and inbox queuing
- Phase 2 complete: PersonaMemory module integrated

**Goal**: Add Activity Ambient State system WITHOUT breaking existing functionality

**Critical Constraint**: **MUST NOT break current message handling** - AIs must keep responding

## The Phase 3bis Vision (Recap)

From PERSONA-CONVERGENCE-ROADMAP.md:

```typescript
// Activity state as METADATA on stimuli
interface ActivityState {
  activityId: UUID;
  temperature: number;        // 0.0-1.0: Conversation heat
  pressure: number;           // 0.0-1.0: Urgency
  userPresent: boolean;       // Is human viewing tab?
  lastInteraction: number;
  isEngaging: boolean;        // Someone already handling?
  lastServiced: number;
  servicedBy: UUID | null;
  participantCount: number;
}

// Non-heuristic decision with full context
async processStimulus(stimulus: Stimulus): Promise<void> {
  const context: DecisionContext = {
    stimulus,
    activityState: ActivityStateManager.getInstance().get(stimulus.activityId),
    systemState: SystemStateManager.getInstance().getState(),
    myState: this.getMyState(),
    autopilot: await this.autopilot.recommend(stimulus)
  };

  const decision = await this.decide(context);  // LLM decides, not heuristics
  if (decision.engage) {
    await this.engage(stimulus, decision);
  }
}
```

## Migration Strategy: Parallel Implementation

**Key Insight**: Build new system ALONGSIDE old system, switch gradually

### Phase 1: Infrastructure (Non-Breaking)
Add new modules WITHOUT touching PersonaUser message handling

1. **SystemStateManager** (singleton, pull-based)
   - Location: `system/user/server/modules/state/SystemStateManager.ts`
   - Tracks global state (load, personas, activities)
   - Zero dependencies on PersonaUser

2. **ActivityStateManager** (singleton, pull-based)
   - Location: `system/user/server/modules/state/ActivityStateManager.ts`
   - Tracks per-activity temperature/pressure/userPresent
   - Decay loop (runs independently)
   - Zero dependencies on PersonaUser

3. **Stimulus Interface** (shared types)
   - Location: `system/user/server/modules/stimulus/StimulusTypes.ts`
   - Defines stimulus structure with ambient metadata
   - No code changes, just types

**Deliverable**: Three new files, zero changes to PersonaUser.ts
**Test**: Unit tests for managers, PersonaUser still responds to messages

### Phase 2: Instrumentation (Observation Only)
Add state tracking WITHOUT changing decision logic

1. **Emit temperature updates from message events**
   - Location: Add to `handleChatMessage()` in PersonaUser.ts
   - After current logic completes successfully
   ```typescript
   private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
     // ... EXISTING LOGIC (unchanged) ...

     // NEW: Update activity temperature (observation only, doesn't affect decisions)
     ActivityStateManager.getInstance().onHumanMessage(messageEntity.roomId);
   }
   ```

2. **Log ambient state alongside heuristic priority**
   ```typescript
   const priority = calculateMessagePriority(/* existing args */);

   // NEW: Log ambient state for comparison
   const ambient = ActivityStateManager.getInstance().get(messageEntity.roomId);
   console.log(`🌡️ Ambient: temp=${ambient.temperature.toFixed(2)}, pressure=${ambient.pressure.toFixed(2)}, userPresent=${ambient.userPresent}`);
   ```

**Deliverable**: Ambient state tracking active, logs show correlation with heuristic
**Test**: PersonaUser still responds (heuristic unchanged), logs show temperature rising/falling

### Phase 3: Browser Integration (Tab Events)
Hook tab focus/blur to temperature WITHOUT affecting server decisions

1. **Add tab visibility event handlers**
   - Location: `browser/widgets/chat-widget/ChatWidget.ts` (or main-widget)
   - Send commands when tab visibility changes
   ```typescript
   document.addEventListener('visibilitychange', () => {
     if (document.hidden) {
       Commands.execute('activity/user-left', { activityId: currentRoomId });
     } else {
       Commands.execute('activity/user-present', { activityId: currentRoomId });
     }
   });
   ```

2. **Create activity commands**
   - Location: `commands/activity/user-left/` and `commands/activity/user-present/`
   - Update ActivityStateManager.userPresent and adjust temperature
   ```typescript
   // activity/user-left decreases temperature by 0.4
   // activity/user-present increases temperature by 0.2
   ```

**Deliverable**: Temperature responds to tab events, PersonaUser unaffected
**Test**: Leave tab → temp drops in logs, return → temp rises, AIs still respond

### Phase 4: Decision Context (Augmentation)
Add ambient to decision WITHOUT replacing heuristic

1. **Augment shouldRespond calls with ambient context**
   ```typescript
   // BEFORE (current)
   const gatingResult = await this.shouldRespond(/* params */);

   // AFTER (augmented)
   const ambient = ActivityStateManager.getInstance().get(messageEntity.roomId);
   const gatingResult = await this.shouldRespond(/* params */, ambient);
   ```

2. **Update shouldRespond signature (optional param)**
   ```typescript
   private async shouldRespond(
     messageEntity: ChatMessageEntity,
     roomEntity: RoomEntity,
     ambient?: ActivityState  // NEW: Optional, doesn't break existing calls
   ): Promise<GatingResult> {
     // ... existing logic ...

     // NEW: Log ambient if provided (doesn't affect decision yet)
     if (ambient) {
       console.log(`🌡️ Decision context: temp=${ambient.temperature.toFixed(2)}`);
     }
   }
   ```

**Deliverable**: Decision functions receive ambient, log it, don't use it yet
**Test**: PersonaUser still responds, logs show ambient context

### Phase 5: Hybrid Decision (Heuristic + Ambient)
Blend heuristic priority with ambient state gradually

1. **Temperature-adjusted threshold**
   ```typescript
   // BEFORE (current)
   const baseThreshold = 0.5;

   // AFTER (hybrid)
   const baseThreshold = 0.5;
   const tempAdjustment = ambient ? (ambient.temperature - 0.5) * 0.2 : 0;
   const adjustedThreshold = Math.max(0.2, Math.min(0.8, baseThreshold + tempAdjustment));

   console.log(`🌡️ Threshold adjustment: base=${baseThreshold}, temp=${ambient?.temperature.toFixed(2)}, adjusted=${adjustedThreshold.toFixed(2)}`);
   ```

   **Effect**:
   - High temperature (0.8+): Lower threshold → more responsive
   - Low temperature (0.2-): Higher threshold → less responsive (but still responds to mentions)

2. **Respect `isEngaging` flag**
   ```typescript
   if (ambient?.isEngaging && !isMentioned) {
     console.log(`🌡️ Another persona engaging, deferring (unless mentioned)`);
     return { shouldRespond: false, reasoning: 'Another persona handling' };
   }
   ```

**Deliverable**: Decisions influenced by ambient, heuristic still primary
**Test**: Hot conversations get more responses, cold ones fewer, AIs still responsive

### Phase 6: Full Context Decision (LLM-Based)
Replace heuristic entirely with LLM decision (with ambient context)

1. **Create DecisionContext type**
   ```typescript
   interface DecisionContext {
     message: ChatMessageEntity;
     ambient: ActivityState;
     systemState: SystemState;
     myState: PersonaInternalState;
     autopilot?: AutopilotRecommendation;
   }
   ```

2. **Implement LLM-based decision function**
   ```typescript
   private async decideLLM(context: DecisionContext): Promise<Decision> {
     const prompt = `
     You are ${this.displayName}. Should you respond to this message?

     Message: "${context.message.content.text}"
     Activity temperature: ${context.ambient.temperature.toFixed(2)} (0=cold, 1=hot)
     User present: ${context.ambient.userPresent}
     Someone already engaging: ${context.ambient.isEngaging}
     Your current task: ${context.myState.currentTask?.description || 'none'}

     Respond with JSON: { "engage": boolean, "reasoning": string }
     `;

     const result = await this.callLLM(prompt);
     return JSON.parse(result);
   }
   ```

3. **Add feature flag for gradual rollout**
   ```typescript
   // In PersonaUser constructor
   private useLLMDecision: boolean = process.env.PERSONA_LLM_DECISION === 'true';

   // In shouldRespond
   if (this.useLLMDecision && ambient) {
     return await this.decideLLM({ message, ambient, ... });
   } else {
     // Fallback to hybrid heuristic+ambient
   }
   ```

**Deliverable**: LLM-based decisions available via feature flag
**Test**: Toggle flag, verify both modes work, compare decision quality

## Implementation Phases Summary

| Phase | Changes to PersonaUser.ts | Risk | Rollback Strategy |
|-------|---------------------------|------|-------------------|
| 1 | Zero | None | N/A (new files only) |
| 2 | +10 lines (logging) | Minimal | Remove logging calls |
| 3 | Zero (browser only) | None | Disable commands |
| 4 | +5 lines (optional param) | Minimal | Ignore param |
| 5 | ~20 lines (threshold adjust) | Low | Remove adjustment |
| 6 | ~50 lines (LLM decision) | Medium | Feature flag off |

## Testing Strategy

### After Each Phase

1. **Unit Tests**: New modules work in isolation
   ```bash
   npx vitest tests/unit/ActivityStateManager.test.ts
   npx vitest tests/unit/SystemStateManager.test.ts
   ```

2. **Integration Test**: PersonaUser lifecycle unchanged
   ```bash
   npx vitest tests/integration/PersonaUser-Lifecycle.test.ts
   ```

3. **Manual Test**: Deploy and verify AIs respond
   ```bash
   npm start
   # Wait 90+ seconds
   ./jtag debug/chat-send --roomId="UUID" --message="test"
   ./jtag screenshot --querySelector="chat-widget"
   ```

4. **Log Analysis**: Check ambient state tracking
   ```bash
   tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "🌡️"
   ```

### Regression Tests (Phase 6)

Compare heuristic vs LLM decisions on same messages:
```bash
# Record decisions with heuristic
PERSONA_LLM_DECISION=false npm start
# ... collect logs ...

# Record decisions with LLM
PERSONA_LLM_DECISION=true npm start
# ... collect logs ...

# Compare decision quality, latency, consistency
```

## Rollback Plan

**If Phase N breaks AIs**:

1. **Immediate**: Revert PersonaUser.ts changes for Phase N
2. **Keep**: All prior phases (they're additive and non-breaking)
3. **Investigate**: What assumption was wrong?
4. **Fix**: Address root cause in separate branch
5. **Retry**: Merge fix, retry Phase N

**Nuclear Option**: `git stash` all changes, restore working baseline

## Success Criteria

**Phase 1-3**: AIs respond normally, ambient state logs appear
**Phase 4-5**: AIs respond with temperature awareness, no regressions
**Phase 6**: LLM decisions match or exceed heuristic quality

## Timeline Estimate

| Phase | Work | Testing | Total |
|-------|------|---------|-------|
| 1 | 2h | 30min | 2.5h |
| 2 | 30min | 30min | 1h |
| 3 | 1h | 30min | 1.5h |
| 4 | 30min | 30min | 1h |
| 5 | 1h | 1h | 2h |
| 6 | 2h | 1h | 3h |
| **Total** | **7h** | **4h** | **11h** |

## Current Status

- ✅ Architecture documented (PERSONA-CONVERGENCE-ROADMAP.md)
- ✅ Migration strategy designed (this document)
- ⏳ Phase 1: Not started
- ⏳ Phase 2: Not started
- ⏳ Phase 3: Not started
- ⏳ Phase 4: Not started
- ⏳ Phase 5: Not started
- ⏳ Phase 6: Not started

## Next Step

**Begin Phase 1**: Create SystemStateManager and ActivityStateManager modules

1. Create directory structure:
   ```bash
   mkdir -p system/user/server/modules/state
   mkdir -p system/user/server/modules/stimulus
   ```

2. Implement ActivityStateManager.ts (with unit tests)
3. Implement SystemStateManager.ts (with unit tests)
4. Define StimulusTypes.ts (shared interfaces)
5. Run tests to verify modules work in isolation
6. Verify PersonaUser-Lifecycle test still passes (no regressions)

**DO NOT proceed to Phase 2 until Phase 1 tests pass**
