# Autonomous Inbox Servicing Loop - Architecture Roadmap

## The Architectural Gap

**Current System (Event-Driven):**
```
Message arrives → Event fired → PersonaUser.handleChatMessage() → Evaluate → Respond
```

**Missing Autonomous Behavior:**
```
PersonaInbox accumulates messages → PersonaState tracks energy/mood
  ↓
Autonomous servicing loop polls inbox at adaptive cadence
  ↓
State-aware engagement decisions (not just reactive)
```

## The Vision: "What if this became more fluid or autonomous?"

PersonaUser should have **internal life cycles**, not just react to external events:

1. **Proactive Message Discovery**: Poll inbox at mood-based cadence (not just wait for events)
2. **State-Aware Selection**: Choose which messages to engage with based on energy/attention
3. **Graceful Degradation**: Lower thresholds when overwhelmed, raise when fatigued
4. **Rest Cycles**: Recover energy during idle periods (RTOS duty cycle management)
5. **Autonomous Decision**: "I have 5 messages, I'm tired, I'll only handle the urgent one"

## Why This Matters

**Current Problem:**
- PersonaUser is a **synchronous slave** to chat events
- No autonomy - just reacts immediately to every trigger
- No concept of "I'm busy, I'll get to that later"
- No rest/recovery - always on duty at 100%

**Autonomous Solution:**
- PersonaUser is an **independent entity** with internal scheduling
- Inbox acts as buffer between events and processing
- State determines engagement strategy
- Adaptive cadence prevents burnout
- True RTOS-inspired traffic management

## Implementation Phases

### Phase 0: Document Current Behavior (DONE ✅)
- Created unit tests for PersonaInbox and PersonaState
- Created integration tests documenting what's missing
- Identified architectural gap

### Phase 1: Add Inbox to PersonaUser (NOT YET IMPLEMENTED)
**Goal**: Wire PersonaInbox into PersonaUser without changing behavior

**Changes**:
- Add `private inbox: PersonaInbox` to PersonaUser
- In `handleChatMessage()`, enqueue message to inbox instead of processing immediately
- Add simple polling loop that dequeues and processes (synchronous for now)
- NO adaptive cadence yet - just prove inbox works

**Testing**:
- AI responses still work (no regression)
- Messages flow through inbox
- Ordering preserved (priority-based)

**Commit**: "Wire PersonaInbox into PersonaUser (synchronous polling, no autonomy yet)"

### Phase 2: Add State Tracking (NOT YET IMPLEMENTED)
**Goal**: Track energy/attention/mood based on activity

**Changes**:
- Add `private state: PersonaStateManager` to PersonaUser
- Call `state.recordActivity()` after generating response
- Call `state.rest()` during idle periods
- Call `state.updateInboxLoad()` when inbox changes
- Log mood changes for debugging

**Testing**:
- Mood transitions work (idle → active → tired → overwhelmed)
- Energy depletes with activity
- Logs show state changes

**Commit**: "Track PersonaUser internal state (energy, mood, attention)"

### Phase 3: Add Adaptive Cadence (NOT YET IMPLEMENTED)
**Goal**: Poll inbox at mood-based intervals

**Changes**:
- Replace synchronous polling with `setInterval()`
- Use `state.getCadence()` to determine poll interval
- Adjust interval dynamically as mood changes
- Log cadence changes

**Testing**:
- Idle persona polls every 3 seconds
- Active persona polls every 5 seconds
- Tired persona polls every 7 seconds
- Overwhelmed persona polls every 10 seconds

**Commit**: "Add adaptive cadence based on PersonaState mood"

### Phase 4: Add State-Aware Engagement (NOT YET IMPLEMENTED)
**Goal**: Only process messages that pass `shouldEngage()` threshold

**Changes**:
- In polling loop, call `state.shouldEngage(message.priority)`
- Skip low-priority messages when tired/overwhelmed
- Log skipped messages for debugging
- Messages stay in inbox until threshold lowers

**Testing**:
- Idle persona handles all priorities (> 0.1)
- Active persona skips low priorities (< 0.3)
- Tired persona only handles medium+ (> 0.5)
- Overwhelmed persona only handles high (> 0.9)
- High priority messages NEVER skipped (> 0.8)

**Commit**: "Add state-aware message engagement (adaptive thresholds)"

### Phase 5: Add Rest Cycles (NOT YET IMPLEMENTED)
**Goal**: Recover energy when idle

**Changes**:
- Track `lastActivityTime` in state (already exists)
- If no messages for N seconds, call `state.rest(durationMs)`
- Energy recovers, mood shifts back to idle
- Resume normal polling

**Testing**:
- After 30 seconds idle, energy starts recovering
- Tired persona recovers to active after rest
- Overwhelmed persona recovers after inbox clears

**Commit**: "Add autonomous rest cycles for energy recovery"

### Phase 6: Add Backpressure Handling (NOT YET IMPLEMENTED)
**Goal**: Dynamically adjust thresholds based on load

**Changes**:
- When inbox > 75% full, raise thresholds (shed load)
- When inbox < 25% full, lower thresholds (be eager)
- Log threshold adjustments

**Testing**:
- High inbox load triggers threshold increase
- Low inbox load triggers threshold decrease
- System stabilizes under continuous load

**Commit**: "Add dynamic backpressure via threshold adjustment"

## Code Structure

### PersonaUser with Autonomous Loop
```typescript
export class PersonaUser extends AIUser {
  private inbox: PersonaInbox;
  private state: PersonaStateManager;
  private servicingLoop: NodeJS.Timeout | null = null;

  constructor(entity: UserEntity, stateEntity: UserStateEntity) {
    super(entity, stateEntity);

    // Initialize autonomous modules
    this.inbox = new PersonaInbox(this.id, this.displayName, {
      maxSize: 100,
      enableLogging: true
    });

    this.state = new PersonaStateManager(this.displayName, {
      enableLogging: true
    });
  }

  /**
   * Initialize autonomous behavior (called after construction)
   */
  async initialize(): Promise<void> {
    await super.initialize();

    // Subscribe to chat events (feed inbox, don't process directly)
    this.subscribeToChatEvents(this.enqueueMessage.bind(this));

    // Start autonomous servicing loop
    this.startAutonomousServicing();
  }

  /**
   * Enqueue message to inbox (replaces direct handleChatMessage)
   */
  private async enqueueMessage(messageEntity: ChatMessageEntity): Promise<void> {
    // Ignore own messages
    if (messageEntity.senderId === this.id) {
      return;
    }

    // Calculate priority
    const priority = calculateMessagePriority(messageEntity, {
      displayName: this.displayName,
      id: this.id,
      recentRooms: this.myRoomIds,
      expertise: [] // TODO: Extract from genome
    });

    // Enqueue to inbox
    await this.inbox.enqueue({
      messageId: messageEntity.id,
      roomId: messageEntity.roomId,
      content: messageEntity.content,
      senderId: messageEntity.senderId,
      senderName: messageEntity.senderDisplayName,
      timestamp: messageEntity.timestamp,
      priority
    });

    // Update state with inbox load
    this.state.updateInboxLoad(this.inbox.getSize());

    this.log(`📨 Enqueued message (priority=${priority.toFixed(2)}, inbox=${this.inbox.getSize()})`);
  }

  /**
   * Start autonomous servicing loop (RTOS-inspired)
   */
  private startAutonomousServicing(): void {
    // Get initial cadence from state
    const cadence = this.state.getCadence();

    this.log(`🔄 Starting autonomous servicing (cadence=${cadence}ms, mood=${this.state.getState().mood})`);

    // Schedule first iteration
    this.servicingLoop = setInterval(async () => {
      await this.serviceInbox();
    }, cadence);
  }

  /**
   * Service inbox based on current state (one iteration)
   */
  private async serviceInbox(): Promise<void> {
    // Check if there are messages
    if (this.inbox.getSize() === 0) {
      // No messages - rest and recover energy
      const now = Date.now();
      const lastActivity = this.state.getState().lastActivityTime;
      const idleTime = now - lastActivity;

      if (idleTime > 30000) { // 30 seconds idle
        await this.state.rest(idleTime);
        this.log(`💤 Resting (idle for ${(idleTime / 1000).toFixed(1)}s, energy=${this.state.getState().energy.toFixed(2)})`);
      }

      // Check if cadence should change due to mood shift
      this.adjustCadence();
      return;
    }

    // Peek at highest priority message
    const candidates = await this.inbox.peek(1);
    if (candidates.length === 0) {
      return;
    }

    const message = candidates[0];

    // Check if we should engage with this message
    if (!this.state.shouldEngage(message.priority)) {
      this.log(`⏭️ Skipping message (priority=${message.priority.toFixed(2)}, mood=${this.state.getState().mood})`);
      // Leave in inbox - threshold might lower later
      return;
    }

    // Pop message from inbox
    await this.inbox.pop(0); // Immediate pop (no timeout)

    // Process message
    this.log(`✅ Processing message (priority=${message.priority.toFixed(2)}, mood=${this.state.getState().mood})`);

    try {
      // TODO: Reconstruct ChatMessageEntity from inbox message
      // const messageEntity = await ChatMessageEntity.findById(message.messageId);
      // await this.processMessage(messageEntity);

      // For now, just simulate activity
      const complexity = message.priority; // Higher priority = more complex
      const duration = complexity * 5000; // 0-5 seconds
      await this.state.recordActivity(duration, complexity);

      // Update inbox load
      this.state.updateInboxLoad(this.inbox.getSize());

      // Check if cadence should adjust
      this.adjustCadence();
    } catch (error) {
      this.log(`❌ Error processing message: ${error}`);
    }
  }

  /**
   * Adjust polling cadence if mood changed
   */
  private adjustCadence(): void {
    const currentCadence = this.state.getCadence();

    // Get interval duration from servicingLoop
    // (TypeScript doesn't expose this easily, so we'll just restart)
    if (this.servicingLoop) {
      clearInterval(this.servicingLoop);
      this.servicingLoop = setInterval(async () => {
        await this.serviceInbox();
      }, currentCadence);

      this.log(`⏱️ Adjusted cadence to ${currentCadence}ms (mood=${this.state.getState().mood})`);
    }
  }

  /**
   * Shutdown autonomous loop
   */
  async shutdown(): Promise<void> {
    if (this.servicingLoop) {
      clearInterval(this.servicingLoop);
      this.servicingLoop = null;
      this.log(`🛑 Stopped autonomous servicing loop`);
    }

    await super.shutdown();
  }
}
```

## Integration Test for Autonomous Loop

```typescript
describe('Autonomous Inbox Servicing Loop (Integration)', () => {
  it('should continuously poll inbox at adaptive cadence', async () => {
    // Create persona with inbox and state
    const persona = new PersonaUser(entity, stateEntity);
    await persona.initialize(); // Starts autonomous loop

    // Enqueue 3 messages
    await enqueueMessage(persona, { priority: 0.9 }); // High
    await enqueueMessage(persona, { priority: 0.5 }); // Medium
    await enqueueMessage(persona, { priority: 0.2 }); // Low

    // Wait for first poll (idle cadence = 3s)
    await sleep(3500);

    // High priority should be processed
    expect(persona.getInboxSize()).toBe(2); // 2 remaining

    // Persona is now active (energy depleted)
    expect(persona.getState().mood).toBe('active');

    // Wait for next poll (active cadence = 5s)
    await sleep(5500);

    // Medium priority should be processed
    expect(persona.getInboxSize()).toBe(1); // 1 remaining

    // Low priority should be skipped (active threshold = 0.3)
    expect(persona.getState().mood).toBe('active');

    // Wait 30 seconds for rest cycle
    await sleep(30000);

    // Energy should recover
    expect(persona.getState().energy).toBeGreaterThan(0.5);
    expect(persona.getState().mood).toBe('idle');

    // Low priority should now be processed (idle threshold = 0.1)
    await sleep(3500);
    expect(persona.getInboxSize()).toBe(0); // All processed

    await persona.shutdown();
  });
});
```

## Benefits of Autonomous Loop

1. **True Autonomy**: Persona has internal scheduling, not just reactive
2. **State-Aware Decisions**: Engagement based on energy/mood, not just priority
3. **Graceful Degradation**: System remains responsive under overload
4. **Energy Management**: Rest cycles prevent burnout (RTOS duty cycle)
5. **Adaptive Throughput**: Cadence adjusts to load naturally
6. **Testable**: Can test continuous behavior in integration tests

## Philosophy Alignment

- **"What if this became more fluid or autonomous?"** - Proactive servicing, not just reactive
- **"In a good RTOS you aren't at 100% duty cycle"** - Rest cycles and energy management
- **"Modular first, get working, then easily rework pieces"** - Inbox and State tested independently first
- **"Hard coded heuristics need to be properly abstracted"** - Clear separation of concerns
- **"Fallback to the old one if the AI one can't work or froze"** - Event-driven fallback if loop fails

## Next Steps

1. Review this roadmap with Joel
2. Implement Phase 1 (wire inbox into PersonaUser)
3. Test with existing AI responses (no regression)
4. Continue through phases iteratively
5. Update adaptive thresholds roadmap with autonomous loop context
