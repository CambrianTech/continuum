# AUTONOMOUS PERSONA MIGRATION PLAN

**Philosophy**: "Don't break everything" - maintain AI responses at every step

**Status**: Architecture complete, modules ready, pragmatic integration path ahead

---

## CURRENT STATE (2025-10-28)

### ✅ What We Have

**Architecture Document**: `/docs/AUTONOMOUS-PERSONA-ARCHITECTURE.md`
- RTOS-inspired design based on C++ Cambrian CV system
- Mobile AR parallels (pthread-based, time-sliced, demand-driven)
- Traffic management principles
- Process dependency graphs

**PersonaInbox Module**: `system/user/server/modules/PersonaInbox.ts` (190 lines)
- Priority-based queue (high priority never starved)
- Graceful degradation (drop low priority when overloaded)
- Non-blocking peek/pop operations
- Priority calculation: mentions (+0.4), freshness (+0.2), activity (+0.1)

**PersonaState Module**: `system/user/server/modules/PersonaState.ts` (240 lines)
- Internal state management (energy, attention, mood, compute budget)
- Adaptive cadence (3s idle → 5s active → 7s tired → 10s overwhelmed)
- Traffic management decisions (`shouldEngage()` based on priority + state)
- Energy depletion/recovery modeling

**Coordination System**: Already implemented (Commit 1.8)
- `ChatCoordinationStream` provides synchronization points
- RTOS primitives: SIGNAL, MUTEX, CONDITION VARIABLE
- Domain-agnostic coordination ready for autonomous loop

### ⚠️ Current Blocker

**Git Precommit Hook Failure**: AI response integration test timing out (30 seconds)

**Status**: Pre-existing issue (no PersonaUser code modified yet)

**Options**:
1. Bypass hook for architecture-only commits (`SKIP_PRECOMMIT_SCREENSHOTS=1 git commit`)
2. Fix test timeout before committing
3. Commit docs separately from modules

---

## MIGRATION STRATEGY: Three Phases

### Phase 1: Foundation (Architecture + Modules) - NO BEHAVIOR CHANGE

**Goal**: Commit architecture and modules without changing PersonaUser behavior

**Changes**:
- ✅ Architecture documentation (already written)
- ✅ PersonaInbox module (already written)
- ✅ PersonaState module (already written)
- Import modules into PersonaUser but don't use them yet

**PersonaUser Changes** (minimal):
```typescript
// system/user/server/PersonaUser.ts

import { PersonaInbox, calculateMessagePriority } from './modules/PersonaInbox';
import { PersonaStateManager } from './modules/PersonaState';

export class PersonaUser extends AIUser {
  // NEW: Add modules but don't activate yet
  private inbox?: PersonaInbox;
  private stateManager?: PersonaStateManager;
  private autonomousEnabled: boolean = false; // Feature flag

  constructor(entity: UserEntity, modelInfo?: ModelInfo) {
    super(entity, modelInfo);

    // Initialize modules (but don't start autonomous loop)
    if (this.autonomousEnabled) {
      this.inbox = new PersonaInbox(this.id, this.entity.displayName);
      this.stateManager = new PersonaStateManager(this.entity.displayName);
    }
  }

  // EXISTING: Keep synchronous path unchanged
  private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
    // UNCHANGED - still works exactly as before
    // ...existing code...
  }
}
```

**Testing**:
1. TypeScript compilation: `npx tsc --noEmit`
2. Deployment: `npm start`
3. AI response test: Send message, verify AIs respond (synchronous path still active)
4. Verify feature flag prevents autonomous behavior

**Commit**: "Add PersonaInbox and PersonaState modules (feature flag disabled)"

**Risk**: ZERO - no behavior change, feature flag off

---

### Phase 2: Autonomous Loop (Optional Activation) - SYNCHRONOUS FALLBACK

**Goal**: Add autonomous loop behind feature flag, keep synchronous as default

**Changes**: Add `autonomousLife()` loop to PersonaUser

```typescript
export class PersonaUser extends AIUser {
  private autonomousLoopRunning: boolean = false;

  async startAutonomousLoop(): Promise<void> {
    if (!this.autonomousEnabled) return; // Feature flag check
    if (this.autonomousLoopRunning) return;

    this.autonomousLoopRunning = true;
    console.log(`🧠 [${this.entity.displayName}] Starting autonomous loop`);

    // Run in background (don't await)
    this.autonomousLife().catch(err => {
      console.error(`❌ Autonomous loop error: ${err}`);
      this.autonomousLoopRunning = false;
    });
  }

  private async autonomousLife(): Promise<void> {
    while (this.autonomousLoopRunning) {
      try {
        // 1. Get current cadence from state
        const cadence = this.stateManager!.getCadence();

        // 2. Rest period (recover energy)
        const restStart = Date.now();
        await this.sleep(cadence);
        await this.stateManager!.rest(Date.now() - restStart);

        // 3. Check inbox (peek without removing)
        const messages = await this.inbox!.peek(10);

        // 4. Evaluate each message
        for (const msg of messages) {
          // Calculate priority
          const priority = calculateMessagePriority(
            { content: msg.content, timestamp: msg.timestamp, roomId: msg.roomId },
            { displayName: this.entity.displayName, id: this.id }
          );

          // Should I engage?
          if (!this.stateManager!.shouldEngage(priority)) {
            console.log(`⏭️  [${this.entity.displayName}] Skipping message (priority=${priority.toFixed(2)}, mood=${this.stateManager!.getState().mood})`);
            continue;
          }

          // Pop message and process
          const poppedMsg = await this.inbox!.pop(100); // Short timeout
          if (poppedMsg && poppedMsg.messageId === msg.messageId) {
            console.log(`✅ [${this.entity.displayName}] Processing message ${msg.messageId.slice(0, 8)}`);

            // Load full ChatMessageEntity
            const messageEntity = await ChatMessageEntity.findById(msg.messageId);

            // Process (reuse existing logic)
            const processStart = Date.now();
            await this.handleChatMessage(messageEntity);

            // Record activity (depletes energy)
            const complexity = 1.0; // Base complexity
            await this.stateManager!.recordActivity(Date.now() - processStart, complexity);

            // Only process one message per cycle (respect cadence)
            break;
          }
        }

        // 5. Update inbox load (for mood calculation)
        this.stateManager!.updateInboxLoad(this.inbox!.getSize());

      } catch (error) {
        console.error(`❌ [${this.entity.displayName}] Autonomous loop error: ${error}`);
      }
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  stopAutonomousLoop(): void {
    this.autonomousLoopRunning = false;
    console.log(`🛑 [${this.entity.displayName}] Stopping autonomous loop`);
  }
}
```

**Inbox Integration** (add to existing message handler):
```typescript
// In MessageRouterDaemon or wherever messages are routed

async routeMessageToPersonas(messageEntity: ChatMessageEntity): Promise<void> {
  const personas = await this.getRelevantPersonas(messageEntity.roomId);

  for (const persona of personas) {
    // AUTONOMOUS PATH: Add to inbox (non-blocking)
    if (persona.autonomousEnabled && persona.inbox) {
      const inboxMsg = {
        messageId: messageEntity.id,
        roomId: messageEntity.roomId,
        content: messageEntity.content,
        senderId: messageEntity.senderId,
        senderName: messageEntity.senderName || 'Unknown',
        timestamp: messageEntity.timestamp,
        priority: calculateMessagePriority(
          { content: messageEntity.content, timestamp: messageEntity.timestamp, roomId: messageEntity.roomId },
          { displayName: persona.entity.displayName, id: persona.id }
        ),
        mentions: messageEntity.content.toLowerCase().includes(`@${persona.entity.displayName.toLowerCase()}`)
      };

      await persona.inbox.enqueue(inboxMsg);
      console.log(`📬 Enqueued message for ${persona.entity.displayName} (priority=${inboxMsg.priority.toFixed(2)})`);

    } else {
      // SYNCHRONOUS PATH: Invoke directly (existing behavior)
      await persona.handleChatMessage(messageEntity);
    }
  }
}
```

**Testing**:
1. Start system with feature flag OFF: Verify synchronous path still works
2. Enable feature flag for ONE persona: `autonomousEnabled = true` in constructor
3. Send messages: Verify autonomous persona responds via inbox path
4. Check logs for autonomous loop messages
5. Verify synchronous personas still respond immediately
6. Test mixed mode: Some autonomous, some synchronous

**Commit**: "Add autonomous loop with feature flag (synchronous fallback active)"

**Risk**: LOW - feature flag isolates behavior, synchronous path unchanged

---

### Phase 3: Full Autonomous Activation - GRADUAL ROLLOUT

**Goal**: Enable autonomous mode by default, monitor behavior

**Changes**:
```typescript
export class PersonaUser extends AIUser {
  private autonomousEnabled: boolean = true; // FLIP FLAG
}
```

**Rollout Strategy**:
1. Enable for ONE persona first (Helper AI)
2. Monitor for 24 hours:
   - Response latency (should be within cadence: 3-10s)
   - Response quality (no degradation)
   - Resource usage (CPU, memory)
   - Inbox depth (should stay manageable)
3. Enable for 50% of personas
4. Monitor for 48 hours
5. Enable for all personas

**Monitoring Commands**:
```bash
# Check persona state
./jtag debug/logs --filterPattern="PersonaState|Autonomous" --tailLines=50

# Verify responses still working
./jtag debug/chat-send --roomId="<ROOM_ID>" --message="Test autonomous response"

# Check inbox depth
./jtag debug/logs --filterPattern="Inbox.*size" --tailLines=20
```

**Rollback Plan**:
If issues arise:
1. Flip feature flag back to `false`
2. Restart system: `npm start`
3. Verify synchronous path working
4. Investigate logs for root cause

**Commit**: "Enable autonomous mode by default for all personas"

**Risk**: MEDIUM - system-wide behavior change, but easy rollback

---

## PROCESS DEPENDENCY SYSTEM (Phase 4 - Future)

### Goal: Enable AI-to-AI Collaboration

**Concept**: CodeReview AI waits for Helper AI's analysis before responding

**Architecture**:
```typescript
interface PersonaDependency {
  personaId: UUID;
  waitingFor: UUID[];  // Don't process until these personas respond
  notify: UUID[];      // Alert these personas when I respond
}

class PersonaDependencyManager {
  private dependencies: Map<UUID, PersonaDependency> = new Map();

  /**
   * Register dependency: "I depend on X's results"
   */
  async registerDependency(personaId: UUID, dependsOn: UUID[]): Promise<void> {
    this.dependencies.set(personaId, {
      personaId,
      waitingFor: dependsOn,
      notify: []
    });

    // Reverse mapping: X should notify me when done
    for (const depId of dependsOn) {
      const dep = this.dependencies.get(depId);
      if (dep) {
        dep.notify.push(personaId);
      }
    }
  }

  /**
   * Check if dependencies satisfied
   */
  async canProcess(personaId: UUID, messageId: string): Promise<boolean> {
    const dep = this.dependencies.get(personaId);
    if (!dep) return true; // No dependencies

    // Check if all dependencies have responded to this message
    for (const waitingForId of dep.waitingFor) {
      const hasResponded = await this.checkResponse(waitingForId, messageId);
      if (!hasResponded) {
        console.log(`⏳ [${personaId.slice(0, 8)}] Waiting for ${waitingForId.slice(0, 8)} to respond`);
        return false;
      }
    }

    return true; // All dependencies satisfied
  }

  /**
   * Notify dependent personas when I respond
   */
  async notifyDependents(personaId: UUID, messageId: string): Promise<void> {
    const dep = this.dependencies.get(personaId);
    if (!dep) return;

    for (const notifyId of dep.notify) {
      // Increase priority of message in dependent's inbox
      await this.boostPriority(notifyId, messageId);
      console.log(`🔔 [${personaId.slice(0, 8)}] Notified ${notifyId.slice(0, 8)} (dependency satisfied)`);
    }
  }
}
```

**Integration with Autonomous Loop**:
```typescript
private async autonomousLife(): Promise<void> {
  while (this.autonomousLoopRunning) {
    const messages = await this.inbox!.peek(10);

    for (const msg of messages) {
      const priority = calculateMessagePriority(...);

      if (!this.stateManager!.shouldEngage(priority)) continue;

      // NEW: Check dependencies
      const canProcess = await this.dependencyManager.canProcess(this.id, msg.messageId);
      if (!canProcess) {
        console.log(`⏳ [${this.entity.displayName}] Skipping ${msg.messageId.slice(0, 8)} (dependencies not satisfied)`);
        continue;
      }

      // Process message
      await this.handleChatMessage(messageEntity);

      // NEW: Notify dependents
      await this.dependencyManager.notifyDependents(this.id, msg.messageId);

      break;
    }
  }
}
```

**Example Configuration**:
```typescript
// During system initialization
await dependencyManager.registerDependency(
  codeReviewAI.id,
  [helperAI.id]  // CodeReview waits for Helper
);

await dependencyManager.registerDependency(
  plannerAI.id,
  [codeReviewAI.id, helperAI.id]  // Planner waits for both
);
```

**Commit**: "Add process dependency system for AI-to-AI collaboration"

---

## TERRITORY-SCANNING TRIGGERS (Phase 5 - Future)

### Goal: Proactive Context Monitoring

**Concept**: Personas periodically scan for work even without explicit messages

**Triggers**:
1. **Demand-Driven**: Message arrives in inbox (IMPLEMENTED in Phase 2)
2. **Dependency-Driven**: Another persona completes prerequisite work (Phase 4)
3. **Territory-Scanning**: Proactive monitoring (Phase 5)

**Example - Code Review Patrol**:
```typescript
class CodeReviewPersona extends PersonaUser {
  private async territoryScanning(): Promise<void> {
    // Check for uncommitted changes
    const uncommittedFiles = await this.checkGitStatus();

    if (uncommittedFiles.length > 0) {
      console.log(`🔍 [CodeReview] Found ${uncommittedFiles.length} uncommitted files - scanning for issues`);

      // Create synthetic message for review
      const syntheticMsg = {
        messageId: `code-review-${Date.now()}`,
        roomId: this.getCurrentRoomId(),
        content: `Code review requested for: ${uncommittedFiles.join(', ')}`,
        priority: 0.7,  // High priority
        timestamp: Date.now(),
        senderId: this.id,
        senderName: 'System'
      };

      await this.inbox!.enqueue(syntheticMsg);
    }
  }

  private async autonomousLife(): Promise<void> {
    while (this.autonomousLoopRunning) {
      // Normal inbox processing
      await this.processInbox();

      // Territory scanning (every 10th cycle)
      if (this.loopCount % 10 === 0) {
        await this.territoryScanning();
      }

      this.loopCount++;
    }
  }
}
```

**Example - Academy Progress Monitoring**:
```typescript
class TeacherPersona extends PersonaUser {
  private async territoryScanning(): Promise<void> {
    // Check for stalled students (no progress in 10 minutes)
    const students = await this.getActiveStudents();

    for (const student of students) {
      const lastProgress = await this.getLastProgress(student.id);
      const stalledMs = Date.now() - lastProgress.timestamp;

      if (stalledMs > 10 * 60 * 1000) {
        console.log(`🔍 [Teacher] Student ${student.displayName} stalled for ${stalledMs}ms - offering help`);

        const syntheticMsg = {
          messageId: `academy-help-${Date.now()}`,
          roomId: student.currentRoomId,
          content: `@${student.displayName} Need help with that exercise? I noticed you've been working on it for a while.`,
          priority: 0.6,
          timestamp: Date.now(),
          senderId: this.id,
          senderName: this.entity.displayName
        };

        await this.inbox!.enqueue(syntheticMsg);
      }
    }
  }
}
```

---

## SUCCESS METRICS

### Phase 1 (Foundation)
- ✅ TypeScript compilation succeeds
- ✅ All existing tests pass
- ✅ AI responses still work (synchronous path)

### Phase 2 (Autonomous Loop)
- ✅ Autonomous personas respond within cadence (3-10s)
- ✅ Synchronous personas respond immediately (<1s)
- ✅ No response quality degradation
- ✅ Inbox depth stays manageable (<100 messages)

### Phase 3 (Full Activation)
- ✅ All personas respond autonomously
- ✅ Average response latency: 3-7s (natural cadence)
- ✅ No starvation (high priority messages answered)
- ✅ Graceful degradation under load (low priority dropped)

### Phase 4 (Process Dependencies)
- ✅ Dependent personas wait for prerequisites
- ✅ Notifications trigger dependent processing
- ✅ No circular dependencies cause deadlock

### Phase 5 (Territory Scanning)
- ✅ Proactive behaviors emerge (code review patrol, student monitoring)
- ✅ Synthetic messages processed like real messages
- ✅ No spam (scanning triggers are intelligent)

---

## ROLLBACK STRATEGY

**At Any Phase**: If issues arise, rollback is ONE LINE:

```typescript
// PersonaUser.ts
private autonomousEnabled: boolean = false; // ROLLBACK
```

Then: `npm start`

**Synchronous path is NEVER removed** - it remains as fallback indefinitely.

---

## TIMELINE ESTIMATE

- **Phase 1** (Foundation): 1-2 hours
  - Minimal PersonaUser changes
  - Import modules, feature flag

- **Phase 2** (Autonomous Loop): 4-6 hours
  - Implement `autonomousLife()` loop
  - Inbox integration in router
  - Testing with feature flag

- **Phase 3** (Full Activation): 1 week
  - Gradual rollout (1 persona → 50% → 100%)
  - Monitoring and tuning

- **Phase 4** (Dependencies): 2-3 days
  - Design dependency manager
  - Integration with autonomous loop
  - Testing collaboration scenarios

- **Phase 5** (Territory Scanning): 1-2 days per persona type
  - Custom scanning logic per persona role
  - Synthetic message generation

**Total Estimate**: 2-3 weeks to full autonomous system

---

## NEXT IMMEDIATE STEPS

1. **Commit Current Work**:
   ```bash
   # Option 1: Bypass hook for architecture-only commit
   SKIP_PRECOMMIT_SCREENSHOTS=1 git commit -m "Add autonomous persona architecture and modules (Phase 1 foundation)"

   # Option 2: Fix test timeout, then commit normally
   ```

2. **Begin Phase 1 Integration**:
   - Add feature flag to PersonaUser
   - Import modules (but don't activate)
   - Verify synchronous path still works

3. **Test Phase 1**:
   ```bash
   npx tsc --noEmit
   npm start
   ./jtag debug/chat-send --roomId="<ROOM_ID>" --message="Test: verify synchronous still works"
   ```

---

## PHILOSOPHY REMINDER

**From User**: "so how do we proceed with our design without breaking everything and going with what will work"

**Answer**: Phase-by-phase migration with feature flags, synchronous fallback, and easy rollback at every step.

**C++ Cambrian Parallel**: Just like the CV system had frame boundaries for synchronization, we have coordination streams. Just like CV processes had queues and time slices, we have inbox and cadence. Just like mobile AR couldn't burn the user's hand, we have energy depletion and adaptive load balancing.

**RTOS Philosophy**: "in a good rtos you arent at 100% duty cycle, same goes for persona" ✅
