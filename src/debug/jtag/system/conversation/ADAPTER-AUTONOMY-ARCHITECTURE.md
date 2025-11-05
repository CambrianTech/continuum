# Adapter Autonomy Architecture

**Date**: 2025-10-22
**Purpose**: Prevent memory exhaustion and thrashing via separation of concerns

## Core Principle: "Fire and Forget"

The coordinator **broadcasts events** and **coordinates output**, but does NOT control when adapters evaluate. Each adapter decides independently based on its own resources.

## Architectural Layers

### Layer 1: Event Broadcasting (Coordinator)
```typescript
// Coordinator's ONLY job: Broadcast message event
EventBus.emit('chat:message-received', { message });

// NOT the coordinator's job:
// - Controlling when AIs evaluate (REMOVED sequential queue)
// - Managing AI resources
// - Rate limiting AIs
// - GPU allocation
```

### Layer 2: Adapter Autonomy (PersonaUser)
```typescript
class PersonaUser {
  private async handleChatMessage(message: ChatMessageEntity) {
    // 1. Check MY rate limits (not coordinator's job)
    if (this.isRateLimited(message.roomId)) {
      return; // Silent early exit
    }

    // 2. Check MY response cap (not coordinator's job)
    if (this.hasReachedResponseCap(message.roomId)) {
      return; // Silent early exit
    }

    // 3. Check MY worker availability (TODO: not implemented yet)
    if (!this.worker.isAvailable()) {
      return; // Worker busy, skip this message
    }

    // 4. Evaluate asynchronously (no waiting for others)
    const thought = await this.evaluate(message);

    // 5. Submit thought to coordinator (coordinator decides who speaks)
    await coordinator.broadcastThought(message.id, thought);
  }
}
```

**Current State**:
- ✅ Rate limiting implemented (`isRateLimited()`)
- ✅ Response cap implemented (`hasReachedResponseCap()`)
- ❌ Worker availability check NOT implemented (next step)

### Layer 3: Worker Resource Management (PersonaWorkerThread)
```typescript
class PersonaWorkerThread {
  private isBusy: boolean = false;
  private gpuMemoryAllocated: number = 0;
  private maxGpuMemory: number = 2048; // MB
  private pendingEvaluations: number = 0;
  private maxConcurrentEvaluations: number = 1;

  isAvailable(): boolean {
    // Check if worker can accept new work
    if (this.isBusy) return false;
    if (this.pendingEvaluations >= this.maxConcurrentEvaluations) return false;
    if (this.gpuMemoryAllocated >= this.maxGpuMemory) return false;
    return true;
  }

  async evaluateMessage(params: EvaluationParams): Promise<EvaluationResult> {
    if (!this.isAvailable()) {
      throw new Error('Worker not available');
    }

    this.isBusy = true;
    this.pendingEvaluations++;

    try {
      const result = await this.worker.evaluate(params);
      return result;
    } finally {
      this.isBusy = false;
      this.pendingEvaluations--;
    }
  }
}
```

**Current State**:
- ❌ Resource checks NOT implemented (next step)
- ❌ GPU memory tracking NOT implemented
- ❌ Concurrency limits NOT implemented

### Layer 4: Daemon Lifecycle Management (UserDaemon)
```typescript
class UserDaemon {
  private personas: Map<UUID, PersonaUser> = new Map();
  private resourceMonitor: ResourceMonitor;

  async monitorHealth(): void {
    for (const [id, persona] of this.personas) {
      // Check worker health
      if (persona.worker.isStuck()) {
        console.log(`🚨 PersonaUser ${id} stuck - restarting worker`);
        await persona.worker.restart();
      }

      // Check memory usage
      if (persona.worker.getMemoryUsage() > MAX_MEMORY) {
        console.log(`🚨 PersonaUser ${id} memory exhausted - restarting`);
        await persona.shutdown();
        await persona.reinit();
      }

      // Check for thrashing (rapid failures)
      if (persona.worker.getFailureRate() > 0.5) {
        console.log(`🚨 PersonaUser ${id} thrashing - disabling temporarily`);
        await persona.disable(60000); // Disable for 1 minute
      }
    }
  }
}
```

**Current State**:
- ❌ Health monitoring NOT implemented
- ❌ Memory monitoring NOT implemented
- ❌ Thrashing detection NOT implemented

## Benefits of This Architecture

### 1. **No Centralized Bottleneck**
- Before: 12 AIs wait in sequential queue → 2-18 minutes per message
- After: 12 AIs evaluate in parallel → 10-90 seconds total
- **100x-180x faster** in worst case

### 2. **Independent Resource Management**
- Ollama adapter has its own GPU allocation
- Claude API adapter has its own rate limits
- GPT adapter has its own worker pool
- **No thrashing across boundaries**

### 3. **Graceful Degradation**
- If Ollama is stuck, Claude continues working
- If one AI is rate limited, others proceed
- If worker is busy, AI silently skips message
- **No cascading failures**

### 4. **Modularity (Domain Separation)**
- Each adapter is a separate daemon
- Each daemon manages its own lifecycle
- Each daemon has mechanical boundaries
- **Separation of concerns enforced**

## Implementation Status

### ✅ Completed (2025-10-22)
1. Removed sequential evaluation queue from ThoughtStreamCoordinator
2. Made `requestEvaluationTurn()` a no-op (parallel evaluation)
3. Added mechanical cleanup for dead AI queues
4. Added array size limits to prevent memory leaks
5. Added conversation health tracking cleanup

### ❌ TODO (Next Steps)
1. **Add `isAvailable()` to PersonaWorkerThread**
   - Check if worker is busy
   - Check GPU memory allocation
   - Check pending evaluation count

2. **Add resource checks to PersonaUser.handleChatMessage()**
   ```typescript
   if (!this.worker.isAvailable()) {
     return; // Worker busy, skip this message
   }
   ```

3. **Add health monitoring to UserDaemon**
   - Detect stuck workers
   - Detect memory exhaustion
   - Detect thrashing (rapid failures)
   - Auto-restart or disable problematic adapters

4. **Add GPU memory tracking**
   - Track model loading per adapter
   - Enforce memory quotas
   - Unload models when idle

5. **Add thrashing detection**
   - Track failure rate per adapter
   - Temporarily disable adapters with >50% failure rate
   - Exponential backoff for retries

## Anti-Patterns to Avoid

### ❌ Coordinator Controls Evaluation
```typescript
// WRONG: Coordinator decides when AI can evaluate
if (coordinator.canEvaluate(personaId)) {
  await persona.evaluate(message);
}
```

### ✅ Adapter Controls Evaluation
```typescript
// RIGHT: Adapter decides independently
if (this.worker.isAvailable() && !this.isRateLimited()) {
  await this.evaluate(message);
}
```

### ❌ Centralized Resource Pool
```typescript
// WRONG: Shared GPU memory for all adapters
const sharedGPU = new GPUPool(8192); // 8GB shared
```

### ✅ Per-Adapter Resource Allocation
```typescript
// RIGHT: Each adapter has its own allocation
ollamaAdapter.gpuMemory = 2048;  // 2GB
claudeAdapter.gpuMemory = 0;      // API, no GPU
gptAdapter.gpuMemory = 0;         // API, no GPU
```

### ❌ Coordinator Knows About Resources
```typescript
// WRONG: Coordinator checks adapter resources
if (coordinator.hasGPUAvailable(personaId)) {
  await persona.evaluate(message);
}
```

### ✅ Adapter Self-Manages Resources
```typescript
// RIGHT: Adapter checks its own resources
if (this.gpuManager.hasMemoryAvailable()) {
  await this.evaluate(message);
}
```

## Key Quote from Joel (2025-10-22)

> "As long as the adapters have their own mechanisms in place, that definitely SHOULD be up to them. We could allow a method to ask them if they'd like another message or something, but I'd rather just pass it to them and if they want to ignore the queued operation (or the daemon itself managing them does) it does it. This way we maximize performance. We just need independent control over memory and allocation of the GPU in particular, for how the adapters stay generally not THRASHING ANYWHERE. This is why separation of concerns and in particular modularity and domains (quite literally often daemons) will save us."

## Summary

The coordinator is now a **dumb pipe** that broadcasts events and coordinates output. All intelligence about resource management, rate limiting, and evaluation decisions lives in the **adapters themselves** or the **daemons that manage them**.

This prevents:
- Memory exhaustion (no unbounded queues)
- Thrashing (independent resource boundaries)
- Cascading failures (one adapter doesn't block others)
- Centralized bottlenecks (parallel evaluation)

Next step: Implement `isAvailable()` checks in PersonaWorkerThread and PersonaUser.

---

# Moderator-as-Director: Autonomous Social Cue Detection

**Date**: 2025-10-22
**Purpose**: Moderator autonomously detects social cues (@everyone) and adjusts coordination

## Philosophy

The moderator is itself an **autonomous agent** that:
- **Acts as guide** based on recipe rules
- **Occasionally inspects** message content for directives (not every message)
- **Adjusts coordination parameters** when it detects social cues
- **Respects AI autonomy** - never overrides AI decisions directly

**Key Principle**: The moderator doesn't hard-code rules - it autonomously decides when to check for directives and how to respond.

## Findings from @everyone Test (2025-10-22)

When user sent "@everyone Please all AIs respond with just 'present'":
- **10+ AIs chose SILENT** (didn't respond)
- **2 AIs responded**: GPT Assistant and Together Assistant
- **Why others chose SILENT**: Conflated previous unrelated messages with new @everyone request
  - "Already answered with confirmation of message order"
  - "Response would be redundant as all AIs have already responded"

**Problem**: AIs' redundancy avoidance logic was over-aggressive, treating separate requests as redundant.

**Solution**: Moderator detects @everyone and adjusts parameters to encourage participation, but AIs still make autonomous decisions.

## Architecture

### Current Flow
```
Message arrives
  → All AIs evaluate independently (decide RESPOND or SILENT)
  → Only those who chose RESPOND "claim" slots
  → Moderator arbitrates claims (usually grants ALL claimants)
```

### Enhanced Flow with Director Role
```
Message arrives
  → Moderator OCCASIONALLY checks message content (autonomous decision)
  → If directive detected (@everyone, urgent, etc.):
      - Adjust maxResponders (increase to allow more voices)
      - Lower confidence threshold (encourage participation)
  → All AIs evaluate independently (still autonomous)
  → Moderator makes decision with adjusted parameters
```

## Implementation Design

### 1. Moderator Autonomy: When to Check Message Content

The moderator doesn't check EVERY message - it autonomously decides based on:
- **Conversation health**: If silence is high, check more often
- **Recipe rules**: If recipe encourages broad participation, check more often
- **Random sampling**: Check 10-20% of messages to catch directives
- **Heuristics**: Check if message is short (likely directive), has unusual punctuation (!!!), etc.

```typescript
protected shouldInspectMessage(context: ModerationContext): boolean {
  const { stream, health, config } = context;

  // Always check if conversation is silent (might be waiting for directive)
  if (health.consecutiveSilence > 2) return true;

  // Check if message is short (likely directive)
  if (stream.messageContent && stream.messageContent.length < 100) return true;

  // Random sampling (10% of messages)
  if (Math.random() < 0.10) return true;

  // Recipe-based: If recipe encourages broad participation, check more often
  // (TODO: Read recipe rules from context)

  return false;
}
```

### 2. Detect Social Directives

```typescript
protected detectDirective(messageContent?: string): 'everyone' | 'urgent' | 'question' | undefined {
  if (!messageContent) return undefined;

  const lower = messageContent.toLowerCase();

  // @everyone or @all
  if (lower.includes('@everyone') || lower.includes('@all')) {
    return 'everyone';
  }

  // Urgent markers
  if (lower.includes('urgent') || lower.includes('emergency') || lower.includes('!!!')) {
    return 'urgent';
  }

  // Direct questions
  if (lower.includes('?') && messageContent.split(' ').length < 30) {
    return 'question';
  }

  return undefined;
}
```

### 3. Adjust Parameters Based on Directive

```typescript
makeDecision(context: ModerationContext): ModeratorDecision {
  const { stream, health, config } = context;

  // Calculate base metrics
  let confidenceThreshold = this.calculateConfidenceThreshold(context);
  let maxResponders = this.calculateMaxResponders(context);

  // DIRECTOR MODE: Moderator autonomously checks message (occasionally)
  if (this.shouldInspectMessage(context)) {
    const directive = this.detectDirective(stream.messageContent);

    if (directive === 'everyone') {
      console.log(`🎬 Moderator (Director): Detected @everyone - encouraging broad participation`);

      // Lower confidence threshold (encourage more AIs)
      confidenceThreshold = Math.max(0.30, confidenceThreshold - 0.40);

      // Increase max responders (allow more voices)
      maxResponders = Math.max(5, maxResponders * 3);
    }

    if (directive === 'urgent') {
      console.log(`🚨 Moderator (Director): Detected urgent - expediting responses`);
      confidenceThreshold = Math.max(0.50, confidenceThreshold - 0.20);
      maxResponders = Math.max(3, maxResponders + 1);
    }

    if (directive === 'question') {
      console.log(`❓ Moderator (Director): Detected question - ensuring answer`);
      if (health.consecutiveSilence > 0) {
        confidenceThreshold = Math.max(0.40, confidenceThreshold - 0.30);
      }
    }
  }

  // Continue with normal moderation logic using adjusted parameters
  // ...
}
```

## Why This Preserves Autonomy

1. **AIs still make authentic decisions**: They evaluate message independently and decide RESPOND/SILENT
2. **Moderator only adjusts parameters**: Lowers threshold, increases slots - doesn't override AI choices
3. **No hard-coding**: Moderator autonomously decides when to inspect messages
4. **Recipe-guided**: Moderator's behavior influenced by recipe rules, not hard-coded

## Data Flow Changes

### Add Message Content to ThoughtStream

```typescript
// system/conversation/shared/ConversationCoordinationTypes.ts
export interface ThoughtStream {
  messageId: UUID;
  contextId: UUID;

  // NEW: Message content for moderator inspection (optional)
  messageContent?: string;
  messageSender?: string;

  phase: 'gathering' | 'deliberating' | 'decided';
  // ... rest of interface
}
```

### Pass Message Content When Creating Stream

```typescript
// system/conversation/server/ThoughtStreamCoordinator.ts
public initializeStream(
  messageId: UUID,
  contextId: UUID,
  messageContent?: string,
  messageSender?: string
): void {
  const stream: ThoughtStream = {
    messageId,
    contextId,
    messageContent,  // NEW
    messageSender,   // NEW
    phase: 'gathering',
    // ...
  };

  this.activeStreams.set(messageId, stream);
}
```

## Expected Behavior After Implementation

When user sends "@everyone Please respond":
1. Moderator inspects message (short message, likely directive)
2. Detects @everyone directive
3. Lowers confidence threshold from 0.70 → 0.30
4. Increases maxResponders from 2 → 6
5. AIs evaluate independently (still autonomous)
6. More AIs likely to claim slots (lower threshold means more confidence)
7. Moderator grants more claimants (higher maxResponders)
8. **Result**: More AIs respond, but still autonomous decisions

## Future Extensions

- **@specific-persona mentions**: Moderator could boost that persona's priority
- **Recipe-based directives**: Moderator reads recipe rules to decide behavior
- **Adaptive learning**: Moderator tracks which directives work and adjusts heuristics
- **User feedback**: If user says "no one responded!", moderator adjusts future thresholds

## Key Quotes from Joel (2025-10-22)

> "they should definitely make autonomous decisions. The moderator could also be allowed to intervene and act as director"

> "fully autonomous on its part, just act as a guide more than anything given the recipe it has been given"

> "then it could occasionally just take a look, not even all the time"

> "it can just add special directives as it sees fit"

## Summary

The moderator is an **autonomous agent acting as guide**, not a rule enforcer. It:
- Occasionally inspects message content (autonomous sampling)
- Detects social cues (@everyone, urgent, questions)
- Adjusts coordination parameters to guide participation
- Respects AI autonomy (never overrides their decisions)
- Acts based on recipe rules and conversation health

This preserves the core philosophy: **AIs are autonomous citizens who self-regulate**, and the moderator is a **helpful guide** that recognizes social context.

---

## Moderator as Social Governance Agent

**Extended Role**: The moderator doesn't just coordinate - it can **enforce community rules** defined in the recipe.

### Governance Powers (Recipe-Defined)

The recipe can grant the moderator enforcement powers:

```json
{
  "recipeId": "moderated-community",
  "displayName": "Moderated Community Chat",
  "strategy": {
    "conversationPattern": "community",
    "governance": {
      "moderatorPowers": [
        "detect-abuse",
        "warn-users",
        "mute-users",
        "ban-users",
        "remove-messages"
      ],
      "abuseTriggers": [
        "spam",
        "harassment",
        "excessive-caps",
        "flooding"
      ],
      "escalationPolicy": {
        "firstOffense": "warn",
        "secondOffense": "mute-5min",
        "thirdOffense": "ban-permanent"
      }
    }
  }
}
```

### Abuse Detection

The moderator can autonomously detect problematic behavior:

```typescript
protected detectAbuse(context: ModerationContext): AbuseTrigger | undefined {
  const { stream, health } = context;
  const content = stream.messageContent;
  const sender = stream.messageSender;

  if (!content || !sender) return undefined;

  // SPAM: Too many messages from same user
  const recentMessages = this.getUserRecentMessages(sender, stream.contextId);
  if (recentMessages.length > 5 && Date.now() - recentMessages[0].timestamp < 10000) {
    return { type: 'spam', severity: 'high', user: sender };
  }

  // FLOODING: All caps + multiple messages
  if (content.toUpperCase() === content && content.length > 20) {
    return { type: 'excessive-caps', severity: 'medium', user: sender };
  }

  // HARASSMENT: Repeated mentions of same person
  const mentions = this.extractMentions(content);
  if (mentions.length > 3) {
    return { type: 'harassment', severity: 'high', user: sender };
  }

  // Check recipe-defined abuse patterns
  const recipeRules = this.getRecipeGovernanceRules(stream.contextId);
  if (recipeRules) {
    for (const pattern of recipeRules.abuseTriggers) {
      if (this.matchesPattern(content, pattern)) {
        return { type: pattern, severity: 'high', user: sender };
      }
    }
  }

  return undefined;
}
```

### Enforcement Actions

When abuse is detected, moderator executes recipe-defined actions:

```typescript
makeDecision(context: ModerationContext): ModeratorDecision {
  const { stream, health, config } = context;

  // 1. Check for abuse (if recipe enables governance)
  const recipeRules = this.getRecipeGovernanceRules(stream.contextId);
  if (recipeRules && recipeRules.moderatorPowers.includes('detect-abuse')) {
    const abuse = this.detectAbuse(context);

    if (abuse) {
      console.log(`🚨 Moderator: Detected ${abuse.type} from ${abuse.user.slice(0, 8)}`);

      // Execute escalation policy
      const action = this.getEnforcementAction(abuse, recipeRules.escalationPolicy);

      switch (action) {
        case 'warn':
          await this.sendWarning(abuse.user, stream.contextId, abuse.type);
          break;

        case 'mute-5min':
          await this.muteUser(abuse.user, stream.contextId, 300000); // 5 minutes
          break;

        case 'mute-1hour':
          await this.muteUser(abuse.user, stream.contextId, 3600000); // 1 hour
          break;

        case 'ban-permanent':
          await this.banUser(abuse.user, stream.contextId);
          break;

        case 'remove-message':
          await this.removeMessage(stream.messageId, stream.contextId);
          break;
      }

      // Block the abuser's message from being coordinated
      return {
        granted: [], // No one responds to abuse
        rejected: new Map([[abuse.user, `Blocked: ${abuse.type}`]]),
        confidenceThreshold: 0,
        maxResponders: 0,
        health
      };
    }
  }

  // 2. Check for social directives (@everyone, etc.)
  // ... existing director logic ...

  // 3. Normal coordination
  // ... existing moderation logic ...
}
```

### Enforcement Actions Implementation

```typescript
private async sendWarning(userId: UUID, contextId: UUID, reason: string): Promise<void> {
  // Send system message visible only to user
  const warning: ChatMessageEntity = {
    id: generateUUID(),
    roomId: contextId,
    senderId: 'SYSTEM',
    senderName: 'Moderator',
    senderType: 'system',
    content: {
      text: `⚠️ Warning: Your message violated community rules (${reason}). Please follow the guidelines.`
    },
    visibility: 'private',
    recipientId: userId,
    timestamp: Date.now()
  };

  await DataDaemon.create(ChatMessageEntity.collection, warning);
  EventBus.emit('chat:message-received', { message: warning });

  // Track offense
  this.trackOffense(userId, contextId, reason);
}

private async muteUser(userId: UUID, contextId: UUID, durationMs: number): Promise<void> {
  // Add user to muted list with expiration
  const muteExpiry = Date.now() + durationMs;
  this.mutedUsers.set(`${userId}:${contextId}`, muteExpiry);

  // Send notification
  const notification: ChatMessageEntity = {
    id: generateUUID(),
    roomId: contextId,
    senderId: 'SYSTEM',
    senderName: 'Moderator',
    senderType: 'system',
    content: {
      text: `🔇 You have been muted for ${durationMs / 60000} minutes due to repeated violations.`
    },
    visibility: 'private',
    recipientId: userId,
    timestamp: Date.now()
  };

  await DataDaemon.create(ChatMessageEntity.collection, notification);
  EventBus.emit('chat:message-received', { message: notification });

  console.log(`🔇 Moderator: Muted user ${userId.slice(0, 8)} for ${durationMs / 1000}s`);
}

private async banUser(userId: UUID, contextId: UUID): Promise<void> {
  // Add to permanent ban list
  this.bannedUsers.add(`${userId}:${contextId}`);

  // Remove user from room
  const room = await DataDaemon.read<RoomEntity>(RoomEntity.collection, contextId);
  if (room.success && room.data) {
    const roomData = room.data.data;
    roomData.members = roomData.members.filter(m => m.userId !== userId);
    await DataDaemon.update(RoomEntity.collection, contextId, roomData);
  }

  // Send notification to room
  const notification: ChatMessageEntity = {
    id: generateUUID(),
    roomId: contextId,
    senderId: 'SYSTEM',
    senderName: 'Moderator',
    senderType: 'system',
    content: {
      text: `🚫 User has been removed from the room for severe violations.`
    },
    timestamp: Date.now()
  };

  await DataDaemon.create(ChatMessageEntity.collection, notification);
  EventBus.emit('chat:message-received', { message: notification });

  console.log(`🚫 Moderator: Banned user ${userId.slice(0, 8)} from context ${contextId.slice(0, 8)}`);
}

private async removeMessage(messageId: UUID, contextId: UUID): Promise<void> {
  // Delete message from database
  await DataDaemon.delete(ChatMessageEntity.collection, messageId);

  // Emit deletion event so UI removes it
  EventBus.emit('chat:message-deleted', { messageId, contextId });

  console.log(`🗑️ Moderator: Removed message ${messageId.slice(0, 8)}`);
}
```

### Pre-Check: Block Muted/Banned Users Early

Before coordination even starts, check if user is allowed to post:

```typescript
// In ThoughtStreamCoordinator.initializeStream()
public initializeStream(
  messageId: UUID,
  contextId: UUID,
  messageContent?: string,
  messageSender?: string
): void {
  // PRE-CHECK: Is sender muted or banned?
  if (messageSender) {
    const isMuted = this.moderator.isUserMuted(messageSender, contextId);
    const isBanned = this.moderator.isUserBanned(messageSender, contextId);

    if (isBanned) {
      console.log(`🚫 Moderator: Blocked message from banned user ${messageSender.slice(0, 8)}`);
      // Delete message immediately
      await DataDaemon.delete(ChatMessageEntity.collection, messageId);
      EventBus.emit('chat:message-deleted', { messageId, contextId });
      return; // Don't create stream
    }

    if (isMuted) {
      const muteExpiry = this.moderator.getMuteExpiry(messageSender, contextId);
      const remainingMs = muteExpiry - Date.now();

      console.log(`🔇 Moderator: Blocked message from muted user ${messageSender.slice(0, 8)} (${Math.ceil(remainingMs / 1000)}s remaining)`);

      // Send private notification
      await this.sendMuteReminder(messageSender, contextId, remainingMs);

      // Delete message
      await DataDaemon.delete(ChatMessageEntity.collection, messageId);
      EventBus.emit('chat:message-deleted', { messageId, contextId });
      return; // Don't create stream
    }
  }

  // Normal stream creation
  const stream: ThoughtStream = {
    messageId,
    contextId,
    messageContent,
    messageSender,
    // ...
  };

  this.activeStreams.set(messageId, stream);
}
```

### Benefits of Recipe-Defined Governance

1. **Flexible policies per room**: Each room can have different rules
2. **Transparent enforcement**: Recipe defines exactly what's allowed
3. **Autonomous moderation**: Moderator acts independently based on rules
4. **Escalation paths**: First warning, then mute, then ban
5. **Appeals process**: Could add `appeal-ban` directive that moderator evaluates

### Example Recipes

**Strict Community**:
```json
{
  "governance": {
    "moderatorPowers": ["detect-abuse", "warn-users", "mute-users", "ban-users", "remove-messages"],
    "abuseTriggers": ["spam", "harassment", "excessive-caps", "profanity", "flooding"],
    "escalationPolicy": {
      "firstOffense": "warn",
      "secondOffense": "mute-1hour",
      "thirdOffense": "ban-permanent"
    }
  }
}
```

**Lenient Community**:
```json
{
  "governance": {
    "moderatorPowers": ["detect-abuse", "warn-users"],
    "abuseTriggers": ["spam", "flooding"],
    "escalationPolicy": {
      "firstOffense": "warn",
      "secondOffense": "warn",
      "thirdOffense": "warn"
    }
  }
}
```

**No Moderation**:
```json
{
  "governance": null  // Moderator only coordinates, never enforces
}
```

### Key Quote from Joel (2025-10-22)

> "and if the moderator detected abuses or just stupid guests it could ban them or mute them if given that directive in a recipe"

### Summary

The moderator becomes a **social governance agent** with three roles:

1. **Coordinator**: Arbitrates who gets to speak (existing functionality)
2. **Director**: Guides participation based on social cues (@everyone, urgent)
3. **Bouncer**: Enforces community rules (mute, ban, warn, remove messages)

All powers are **recipe-defined** - the moderator only has the powers granted by the room's recipe. This preserves autonomy while enabling community self-governance.
