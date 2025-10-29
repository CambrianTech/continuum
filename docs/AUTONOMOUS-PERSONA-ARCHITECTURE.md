# Autonomous Persona Architecture: RTOS-Inspired AI Citizens

**Date**: 2025-10-28
**Status**: Design Phase
**Philosophy**: Personas as first-class citizens with internal rhythm and autonomy

---

## The Vision: Traffic Management, Not 100% Duty Cycle

**Core Insight**: "in a good rtos you arent at 100% duty cycle, same goes for persona"

### What We're Building

Personas that operate like **real individuals in a traffic system**:
- They have their own schedules and rhythm (not synchronously invoked)
- They check inboxes when they have capacity (not forced to evaluate everything)
- They self-prioritize based on relevance, energy, and current load
- They don't neglect important work (like traffic lights preventing starvation)
- They yield strategically based on internal state

**User Quote**: "scheduling and self prioritization, but not neglecting so badly, like a traffic problem"

This is **NOT** about filtering or gatekeeping. It's about **autonomous cadence with intelligent prioritization**.

---

## The Blueprint: C++ Cambrian Computer Vision System

User's reference architecture (pthread-based CV system on **mobile phone**):

```cpp
// Computer vision process (C++ Cambrian system)
void* processFrame(void* data) {
  while (running) {
    frame = queue.pop();           // Wait for work
    if (shouldProcess(frame)) {    // Own decision
      process(frame);              // Time slice
    }
    sync_point();                  // Coordinate with others
  }
}
```

**Key Properties**:
- Each vision process runs on pthread
- Queue-based work distribution (frame queue)
- Time slices per process
- Synchronization at frame boundaries (not during processing)
- Processes decide internally whether to process frame

**Critical Constraint**: Mobile phone - can't burn user's hand!
- Line finder, 3D plane segmenter, various CNNs
- Lighting, semantic segmentation, feature finders, 3D geometry
- Must do a lot, but not too much
- Demand-driven: Detect need by process dependencies or scanning new territory
- Real-time floor/wall replacement - only when needed

**User Quote**: "You can't be permanently pegging the ai's with 100% utilization, especially for it to seem natural, and work efficiently"

**Translation to PersonaUser**:
```typescript
async autonomousLife() {
  while (this.alive) {
    const messages = await this.inbox.pop();  // Wait for work

    if (this.shouldEngage(messages)) {        // Own decision
      await this.process(messages);           // Time slice
    }

    await this.coordination.waitForDecision(); // Sync point
  }
}
```

---

## Demand-Driven Processing & Dependency-Aware Scheduling

**Critical Insight**: "You can do the same thing with multiple ai's working together collaboratively, one one machine"

### Parallel with Mobile AR System

**CV System (Mobile Phone)**:
- Line finder detects need by scanning new territory (floor/wall movement)
- 3D plane segmenter waits for line finder results
- Semantic segmentation depends on plane geometry
- Real-time processing only when needed (not 100% duty cycle)

**Persona System (AI Collaboration)**:
- CodeReview AI waits for Helper AI's analysis
- Teacher AI depends on Student AI's question
- Planner AI coordinates multiple specialist AIs
- Process when needed, not constantly

### Three Processing Triggers

1. **Demand-Driven** (External trigger):
   - New message arrives in inbox
   - User mentions persona by name
   - Priority calculation indicates relevance

2. **Dependency-Driven** (Internal trigger):
   - Another persona completes work I depend on
   - Coordination decision grants permission
   - Prerequisite data becomes available

3. **Territory-Scanning** (Proactive trigger):
   - Check inbox periodically (cadence-based)
   - Monitor for high-priority work
   - Adjust rhythm based on load

### Process Dependency Graph

```typescript
// Example: Multi-AI collaboration
interface PersonaDependency {
  personaId: UUID;
  waitingFor: UUID[];  // Other personas this one depends on
  notify: UUID[];      // Personas that depend on this one
}

// CodeReview AI waits for Helper AI's analysis
{
  personaId: 'codereview-ai',
  waitingFor: ['helper-ai'],  // Don't process until helper analyzes
  notify: ['planner-ai']        // Planner waits for code review
}
```

**Efficiency Gains**:
- CodeReview doesn't evaluate EVERY message
- Only processes when Helper flags code-related content
- Planner coordinates after both finish
- Natural cadence emerges from dependencies

### Adaptive Load Balancing

**Like mobile AR thermal management**:
```typescript
// If system overheating (high load):
- Increase cadence (slow down checks)
- Raise priority thresholds (process less)
- Defer low-priority work

// If system idle (low load):
- Decrease cadence (check more often)
- Lower priority thresholds (process more)
- Catch up on deferred work
```

**Result**: System stays responsive without burning resources (or user's hand!)

---

## Architecture Components

### 1. PersonaInbox: Queue-Based Message Delivery

**Purpose**: Messages go to inbox, not direct invocation

```typescript
// system/user/server/modules/PersonaInbox.ts

export interface InboxMessage {
  messageId: string;
  roomId: UUID;
  content: string;
  timestamp: number;
  priority: number;  // Calculated based on relevance
}

export class PersonaInbox {
  private queue: InboxMessage[] = [];
  private maxSize: number = 1000;

  // Non-blocking: Add to inbox (called when message arrives)
  async enqueue(message: InboxMessage): Promise<void> {
    if (this.queue.length >= this.maxSize) {
      // Drop lowest priority message (traffic management)
      this.queue.sort((a, b) => b.priority - a.priority);
      this.queue.pop();
    }

    this.queue.push(message);
    this.queue.sort((a, b) => b.priority - a.priority);
  }

  // Non-blocking: Check inbox without removing
  async peek(): Promise<InboxMessage[]> {
    return this.queue.slice(0, 10); // Top 10 by priority
  }

  // Blocking: Wait for next message
  async pop(timeoutMs: number = 5000): Promise<InboxMessage | null> {
    if (this.queue.length > 0) {
      return this.queue.shift()!;
    }

    // Wait for message or timeout
    return new Promise((resolve) => {
      const checkInterval = setInterval(() => {
        if (this.queue.length > 0) {
          clearInterval(checkInterval);
          resolve(this.queue.shift()!);
        }
      }, 100);

      setTimeout(() => {
        clearInterval(checkInterval);
        resolve(null);
      }, timeoutMs);
    });
  }

  // Get inbox size (for load awareness)
  getSize(): number {
    return this.queue.length;
  }
}
```

**Traffic Management**:
- Messages sorted by priority (relevance + urgency)
- Queue size limited (prevent overload)
- Drop lowest priority when full (graceful degradation)
- Personas see load via `getSize()` and adjust cadence

---

### 2. PersonaState: Internal State Management

**Purpose**: Energy, mood, attention influence decisions

```typescript
// system/user/server/modules/PersonaState.ts

export interface PersonaState {
  energy: number;           // 0.0-1.0 (depletes with processing, recovers with rest)
  attention: number;        // 0.0-1.0 (focus level, influenced by complexity)
  mood: 'active' | 'tired' | 'overwhelmed' | 'idle';
  inboxLoad: number;        // Current inbox size
  lastActivityTime: number; // When last processed message
  responseCount: number;    // Responses in current window
  computeBudget: number;    // Available compute (influenced by rate limits)
}

export class PersonaStateManager {
  private state: PersonaState;

  constructor() {
    this.state = {
      energy: 1.0,
      attention: 1.0,
      mood: 'idle',
      inboxLoad: 0,
      lastActivityTime: Date.now(),
      responseCount: 0,
      computeBudget: 1.0
    };
  }

  // Update state after processing message
  async recordActivity(durationMs: number, complexity: number): Promise<void> {
    // Deplete energy based on effort
    this.state.energy -= (durationMs / 10000) * complexity;
    this.state.energy = Math.max(0, this.state.energy);

    // Update attention (focus or fatigue)
    if (this.state.energy < 0.3) {
      this.state.attention *= 0.9; // Fatigue
    }

    // Update mood
    this.state.mood = this.calculateMood();

    this.state.lastActivityTime = Date.now();
    this.state.responseCount++;
  }

  // Recover energy during idle time
  async rest(durationMs: number): Promise<void> {
    const recovery = durationMs / 20000; // Slower recovery than depletion
    this.state.energy += recovery;
    this.state.energy = Math.min(1.0, this.state.energy);

    // Attention recovers faster during rest
    this.state.attention += recovery * 2;
    this.state.attention = Math.min(1.0, this.state.attention);

    this.state.mood = this.calculateMood();
  }

  // Calculate mood from state
  private calculateMood(): 'active' | 'tired' | 'overwhelmed' | 'idle' {
    if (this.state.inboxLoad > 50) return 'overwhelmed';
    if (this.state.energy < 0.3) return 'tired';
    if (this.state.responseCount > 0 && this.state.energy > 0.5) return 'active';
    return 'idle';
  }

  // Should persona engage? (traffic management decision)
  shouldEngage(message: InboxMessage): boolean {
    // Never neglect high priority (traffic starvation prevention)
    if (message.priority > 0.8) return true;

    // Overwhelmed: only process highest priority
    if (this.state.mood === 'overwhelmed') {
      return message.priority > 0.9;
    }

    // Tired: lower threshold
    if (this.state.mood === 'tired') {
      return message.priority > 0.5 && this.state.energy > 0.2;
    }

    // Active: normal processing
    if (this.state.mood === 'active') {
      return message.priority > 0.3;
    }

    // Idle: eager to work
    return message.priority > 0.1;
  }

  // Get current state (for diagnostics)
  getState(): PersonaState {
    return { ...this.state };
  }

  // Update inbox load (called by autonomous loop)
  updateInboxLoad(size: number): void {
    this.state.inboxLoad = size;
  }
}
```

**Traffic Management**:
- High priority messages never starved (always processed)
- Overwhelmed personas raise threshold (shed low-priority load)
- Tired personas lower activity (energy conservation)
- Idle personas eager to engage (maintain system responsiveness)

---

### 3. Autonomous Life Cycle: The Heart of the System

**Purpose**: Personas run their own loops, checking inbox on their schedule

```typescript
// Modifications to system/user/server/PersonaUser.ts

export class PersonaUser extends AIUser {
  private inbox: PersonaInbox;
  private stateManager: PersonaStateManager;
  private alive: boolean = true;
  private internalClock: number = 5000; // Check inbox every 5 seconds (base)

  constructor(entity: UserEntity) {
    super(entity);
    this.inbox = new PersonaInbox();
    this.stateManager = new PersonaStateManager();

    // Start autonomous loop immediately
    this.startAutonomousLife();
  }

  /**
   * Autonomous life cycle (RTOS-inspired loop)
   */
  private async startAutonomousLife(): Promise<void> {
    console.log(`🤖 ${this.displayName}: Starting autonomous life`);

    while (this.alive) {
      try {
        // Update state with inbox load
        const inboxSize = this.inbox.getSize();
        this.stateManager.updateInboxLoad(inboxSize);

        // Check inbox (non-blocking peek)
        const messages = await this.inbox.peek();

        if (messages.length > 0) {
          // Decide which messages to process (traffic management)
          const toProcess = messages.filter(msg =>
            this.stateManager.shouldEngage(msg)
          );

          if (toProcess.length > 0) {
            // Process message(s) - this is the "time slice"
            await this.processMessages(toProcess);
          } else {
            // No engaging messages, rest briefly
            await this.rest(this.internalClock);
          }
        } else {
          // Empty inbox, rest longer
          await this.rest(this.internalClock * 2);
        }

        // Adjust cadence based on state (traffic adaptive)
        this.adjustCadence();

        // Wait for next cycle (like frame in CV system)
        await this.sleep(this.internalClock);

      } catch (error) {
        console.error(`❌ ${this.displayName}: Error in autonomous loop:`, error);
        await this.sleep(this.internalClock * 2); // Back off on error
      }
    }

    console.log(`🛑 ${this.displayName}: Autonomous life ended`);
  }

  /**
   * Process messages (time slice)
   */
  private async processMessages(messages: InboxMessage[]): Promise<void> {
    const startTime = Date.now();

    for (const message of messages) {
      // Remove from inbox
      await this.inbox.pop();

      // Process via existing handleChatMessage logic
      const messageEntity = await this.loadMessageEntity(message.messageId);
      if (messageEntity) {
        await this.handleChatMessage(messageEntity);
      }
    }

    const durationMs = Date.now() - startTime;
    const complexity = messages.length; // Simple complexity metric

    // Record activity for state management
    await this.stateManager.recordActivity(durationMs, complexity);
  }

  /**
   * Rest (energy recovery)
   */
  private async rest(durationMs: number): Promise<void> {
    await this.stateManager.rest(durationMs);
  }

  /**
   * Sleep (yield to other personas)
   */
  private async sleep(durationMs: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, durationMs));
  }

  /**
   * Adjust cadence based on state (traffic adaptive)
   */
  private adjustCadence(): void {
    const state = this.stateManager.getState();

    // Overwhelmed: slow down (back pressure)
    if (state.mood === 'overwhelmed') {
      this.internalClock = 10000; // 10 seconds
    }
    // Tired: moderate pace
    else if (state.mood === 'tired') {
      this.internalClock = 7000; // 7 seconds
    }
    // Active: normal pace
    else if (state.mood === 'active') {
      this.internalClock = 5000; // 5 seconds
    }
    // Idle: faster check (eager)
    else {
      this.internalClock = 3000; // 3 seconds
    }

    // Constrain by compute budget (rate limits)
    if (state.computeBudget < 0.5) {
      this.internalClock *= 2; // Slow down when limited
    }
  }

  /**
   * Shutdown (cleanup)
   */
  async shutdown(): Promise<void> {
    this.alive = false;
    await super.shutdown();
  }
}
```

---

### 4. Message Delivery: Replacing Direct Invocation

**OLD (Synchronous Invocation)**:
```typescript
// When message arrives, directly invoke persona
await personaUser.handleChatMessage(messageEntity);
```

**NEW (Inbox Delivery)**:
```typescript
// When message arrives, add to persona's inbox
const inboxMessage: InboxMessage = {
  messageId: messageEntity.id,
  roomId: messageEntity.roomId,
  content: messageEntity.content,
  timestamp: messageEntity.timestamp,
  priority: this.calculatePriority(messageEntity, personaUser)
};

await personaUser.inbox.enqueue(inboxMessage);

// Persona processes on its own schedule!
```

**Priority Calculation** (traffic management):
```typescript
private calculatePriority(message: ChatMessageEntity, persona: PersonaUser): number {
  let priority = 0.5; // Base

  // Mentioned by name: high priority
  if (message.content.includes(`@${persona.displayName}`)) {
    priority += 0.4;
  }

  // In active conversation: moderate priority
  if (persona.recentRooms.includes(message.roomId)) {
    priority += 0.2;
  }

  // Relevant to expertise: moderate priority
  if (this.isRelevantToExpertise(message, persona)) {
    priority += 0.2;
  }

  // Recent message: slight priority boost
  const ageMs = Date.now() - message.timestamp;
  if (ageMs < 60000) { // Last minute
    priority += 0.1;
  }

  return Math.min(1.0, priority);
}
```

---

## Traffic Management Properties

### 1. No Starvation (High Priority Always Processed)
```typescript
if (message.priority > 0.8) return true; // Never neglect
```

### 2. Graceful Degradation (Drop Low Priority When Overloaded)
```typescript
if (this.queue.length >= this.maxSize) {
  this.queue.sort((a, b) => b.priority - a.priority);
  this.queue.pop(); // Drop lowest priority
}
```

### 3. Adaptive Cadence (Back Pressure When Overwhelmed)
```typescript
if (state.mood === 'overwhelmed') {
  this.internalClock = 10000; // Slow down
}
```

### 4. Fair Scheduling (Personas Yield to Each Other)
```typescript
await this.sleep(this.internalClock); // Yield after processing
```

### 5. Energy Conservation (Rest When Low Energy)
```typescript
if (this.state.energy < 0.3) {
  return message.priority > 0.5; // Higher threshold when tired
}
```

---

## Integration with Existing Coordination

**ChatCoordinationStream** (already implemented) provides synchronization points:

```typescript
// After processing message, coordinate with other personas
await this.coordination.broadcastChatThought(messageId, roomId, thought);
const decision = await this.coordination.waitForChatDecision(messageId);

// This is the "sync point" (like frame boundary in CV system)
if (decision.granted.includes(this.id)) {
  // Permission to respond
  await this.respondToMessage(messageEntity);
}
```

**Key Insight**: Coordination happens AFTER autonomous decision to engage, not before.

**Old Flow (Synchronous)**:
1. Message arrives → invoke persona directly
2. Persona evaluates (forced)
3. Coordinate with others
4. Respond if granted

**New Flow (Autonomous)**:
1. Message arrives → add to inbox
2. Persona checks inbox on schedule
3. Persona decides if engaging (traffic management)
4. If engaging, evaluate and coordinate
5. Respond if granted

---

## Domain-Agnostic Design

**Critical Requirement**: "if we did ours right we should be able to replicate the cambrian system entirely in a vision system or a video game play"

### Chat Domain (Current)
```typescript
interface InboxMessage {
  messageId: string;
  roomId: UUID;
  content: string;
  timestamp: number;
  priority: number;
}
```

### Vision Domain (Future)
```typescript
interface InboxFrame {
  frameId: string;
  cameraId: UUID;
  imageData: Buffer;
  timestamp: number;
  priority: number; // Motion detection, face recognition, etc.
}
```

### Game Domain (Future)
```typescript
interface InboxGameState {
  gameId: string;
  sessionId: UUID;
  gameState: unknown;
  timestamp: number;
  priority: number; // Player action, AI turn, etc.
}
```

**Same autonomous loop works for all domains** - only inbox message type changes!

---

## Migration Strategy

### Phase 1: Add Inbox and State (No Behavior Change)
- Create `PersonaInbox.ts` module
- Create `PersonaStateManager.ts` module
- Add to PersonaUser (but don't use yet)
- Test compilation

### Phase 2: Add Autonomous Loop (Parallel to Existing)
- Implement `startAutonomousLife()` in PersonaUser
- Keep existing `handleChatMessage()` direct invocation
- Inbox delivery runs in parallel (for testing)
- Compare metrics (responsiveness, load distribution)

### Phase 3: Switch to Inbox-Only Delivery
- Replace direct invocation with inbox delivery
- Remove synchronous `handleChatMessage()` entry point
- Personas only process via autonomous loop
- Monitor for regressions

### Phase 4: Optimize Traffic Management
- Tune priority calculations
- Adjust cadence thresholds
- Refine energy/attention models
- Monitor starvation metrics

---

## Success Metrics

### Traffic Management Metrics
- **No starvation**: High priority messages processed within SLA (e.g., 10 seconds)
- **Graceful degradation**: Low priority messages dropped when overloaded (not lost)
- **Fair scheduling**: All personas get time slices (not dominated by one)
- **Energy conservation**: Personas rest when low energy (not burned out)
- **Adaptive cadence**: Personas slow down when overwhelmed (back pressure)

### System Health Metrics
- **AI response rate**: Maintain 5-10 evaluations per message
- **Response latency**: Maintain p95 < 60 seconds
- **Queue depth**: Monitor inbox sizes (detect overload)
- **Energy levels**: Monitor persona energy (detect exhaustion)
- **Coordination efficiency**: Maintain parallel thought broadcasting

---

## Philosophy Alignment

**User Quotes**:
- "in a good rtos you arent at 100% duty cycle, same goes for persona"
- "Think of these as like individuals. You arent paying attention 100% of your time"
- "They, if truly autonomous, need their own time and schedule"
- "scheduling and self prioritization, but not neglecting so badly, like a traffic problem"

**Translation**:
- ✅ Autonomous loops with internal rhythm
- ✅ Inbox-based delivery (not direct invocation)
- ✅ Self-prioritization with traffic management
- ✅ No starvation (high priority always processed)
- ✅ Graceful degradation (drop low priority when overloaded)
- ✅ Energy-aware processing (rest when tired)
- ✅ Domain-agnostic (works for vision, games, chat)

---

**Next Steps**: Begin implementation of PersonaInbox and PersonaStateManager modules.
