# AI Response Timing Limits - Natural Conversation Pacing

## The Problem: Instant AI Responses Create Unnatural Conversation

```
11:30:00.100 - Human: "How do I fix this bug?"
11:30:00.300 - CodeAI: "Check the type definitions"      ← 200ms response
11:30:00.450 - PlannerAI: "Also review recent changes"   ← 150ms after CodeAI
11:30:00.620 - CodeAI: "Yes, git log would help"         ← 170ms after PlannerAI
11:30:00.750 - PlannerAI: "And add tests"                ← 130ms after CodeAI

❌ This feels robotic and creates a "ping-pong" effect
❌ Humans can't even read the messages before next one arrives
❌ AIs appear to be spamming rather than conversing
```

## Solution: Multi-Layer Timing Controls

### 1. Minimum Time Between Responses (Per-AI)

```typescript
interface PersonaTimingLimits {
  // Minimum time this AI must wait before posting another message
  minSecondsBetweenOwnMessages: number;      // e.g., 10 seconds

  // Minimum time to wait after ANY message before responding
  minSecondsAfterAnyMessage: number;         // e.g., 3 seconds

  // Minimum time to wait after another AI's message
  minSecondsAfterAIMessage: number;          // e.g., 5 seconds

  // Minimum time to wait after human message
  minSecondsAfterHumanMessage: number;       // e.g., 2 seconds (humans expect faster response)

  // Artificial "thinking time" to appear more natural
  thinkingTimeRange: { min: number; max: number };  // e.g., { min: 2, max: 8 }
}

const DEFAULT_PERSONA_TIMING: PersonaTimingLimits = {
  minSecondsBetweenOwnMessages: 10,     // Can't post more than once per 10 seconds
  minSecondsAfterAnyMessage: 3,         // Must wait 3 seconds after ANY message
  minSecondsAfterAIMessage: 5,          // Must wait 5 seconds after another AI
  minSecondsAfterHumanMessage: 2,       // Can respond faster to humans
  thinkingTimeRange: { min: 2, max: 8 } // Random "thinking" delay
};
```

### 2. Room-Level Timing Controls

```typescript
interface RoomTimingState {
  roomId: UUID;

  // Last message timestamps
  lastMessageTime: Date;
  lastHumanMessageTime: Date | null;
  lastAIMessageTime: Date | null;

  // Per-sender timing
  lastMessageBySender: Map<UUID, Date>;

  // Cooldown periods
  roomCooldownUntil: Date | null;       // Room-wide cooldown after rapid-fire

  // Rapid-fire detection
  messagesInLastTenSeconds: number;
  rapidFireThreshold: number;           // e.g., 5 messages in 10 seconds = rapid-fire
}
```

### 3. Timing Enforcement in Response Decision

```typescript
class PersonaTimingEnforcer {
  /**
   * Check if AI is allowed to respond based on timing constraints
   * This runs BEFORE the AI-to-AI interaction logic
   */
  async canRespondNow(
    persona: PersonaUser,
    message: ChatMessageEntity,
    roomState: RoomTimingState,
    senderType: 'human' | 'ai'
  ): Promise<TimingDecision> {
    const now = Date.now();

    // CHECK 1: Did I just post? (my own cooldown)
    const myLastMessage = roomState.lastMessageBySender.get(persona.id);
    if (myLastMessage) {
      const secondsSinceMyLastMessage = (now - myLastMessage.getTime()) / 1000;

      if (secondsSinceMyLastMessage < persona.config.timing.minSecondsBetweenOwnMessages) {
        return {
          canRespond: false,
          reason: 'own-message-cooldown',
          mustWaitSeconds: persona.config.timing.minSecondsBetweenOwnMessages - secondsSinceMyLastMessage,
          priority: 'hard-limit' // Cannot be overridden
        };
      }
    }

    // CHECK 2: Was there a message too recently? (general cooldown)
    const secondsSinceLastMessage = (now - roomState.lastMessageTime.getTime()) / 1000;

    if (secondsSinceLastMessage < persona.config.timing.minSecondsAfterAnyMessage) {
      return {
        canRespond: false,
        reason: 'general-cooldown',
        mustWaitSeconds: persona.config.timing.minSecondsAfterAnyMessage - secondsSinceLastMessage,
        priority: 'hard-limit'
      };
    }

    // CHECK 3: Was the last message from another AI? (AI-to-AI cooldown)
    if (senderType === 'ai' && roomState.lastAIMessageTime) {
      const secondsSinceAIMessage = (now - roomState.lastAIMessageTime.getTime()) / 1000;

      if (secondsSinceAIMessage < persona.config.timing.minSecondsAfterAIMessage) {
        return {
          canRespond: false,
          reason: 'ai-to-ai-cooldown',
          mustWaitSeconds: persona.config.timing.minSecondsAfterAIMessage - secondsSinceAIMessage,
          priority: 'soft-limit' // Can be overridden for @mentions
        };
      }
    }

    // CHECK 4: Is room in cooldown due to rapid-fire? (room-wide limit)
    if (roomState.roomCooldownUntil && now < roomState.roomCooldownUntil.getTime()) {
      const mustWaitSeconds = (roomState.roomCooldownUntil.getTime() - now) / 1000;

      return {
        canRespond: false,
        reason: 'room-rapid-fire-cooldown',
        mustWaitSeconds,
        priority: 'hard-limit'
      };
    }

    // CHECK 5: Has there been rapid-fire? (detect and prevent spam)
    if (roomState.messagesInLastTenSeconds >= roomState.rapidFireThreshold) {
      // Impose room-wide cooldown
      const cooldownDuration = 30000; // 30 seconds
      roomState.roomCooldownUntil = new Date(now + cooldownDuration);

      return {
        canRespond: false,
        reason: 'rapid-fire-detected-imposing-cooldown',
        mustWaitSeconds: 30,
        priority: 'hard-limit'
      };
    }

    // All timing checks passed
    return {
      canRespond: true,
      reason: 'timing-ok',
      mustWaitSeconds: 0,
      priority: 'none'
    };
  }

  /**
   * Calculate artificial "thinking time" to make response feel natural
   * Humans take time to read, think, and type - AIs should simulate this
   */
  calculateThinkingTime(
    persona: PersonaUser,
    message: ChatMessageEntity,
    responseLength: number
  ): number {
    const config = persona.config.timing;

    // Base thinking time (random within range)
    const baseThinking = Math.random() *
      (config.thinkingTimeRange.max - config.thinkingTimeRange.min) +
      config.thinkingTimeRange.min;

    // Longer messages require more "reading time"
    const messageLength = message.content?.text?.length || 0;
    const readingTime = Math.min(messageLength / 200, 5); // ~200 chars/second reading, max 5 seconds

    // Longer responses require more "typing time"
    const typingTime = Math.min(responseLength / 50, 10); // ~50 chars/second typing, max 10 seconds

    // Question responses feel faster (humans respond quicker to direct questions)
    const isQuestion = message.content?.text?.includes('?') || false;
    const questionModifier = isQuestion ? 0.7 : 1.0;

    return (baseThinking + readingTime + typingTime) * questionModifier;
  }

  /**
   * Schedule delayed response (makes AI feel more human)
   */
  async scheduleDelayedResponse(
    persona: PersonaUser,
    message: ChatMessageEntity,
    responseText: string,
    delay: number
  ): Promise<void> {
    console.log(`⏰ ${persona.displayName}: Scheduling response in ${delay.toFixed(1)}s`);
    console.log(`   Reason: Natural conversation pacing`);

    // Add to persona's pending response queue
    persona.pendingResponses.push({
      triggerMessage: message,
      responseText,
      scheduledTime: new Date(Date.now() + delay * 1000),
      status: 'scheduled'
    });

    // Set timer
    setTimeout(async () => {
      await this.executeScheduledResponse(persona, message, responseText);
    }, delay * 1000);
  }
}
```

### 4. Override Rules for Urgent Situations

```typescript
class TimingOverrideManager {
  /**
   * Determine if timing limits can be overridden
   * Some situations warrant immediate response despite cooldowns
   */
  canOverrideTimingLimits(
    decision: AIToAIResponseDecision,
    timingDecision: TimingDecision
  ): boolean {
    // NEVER override hard limits (own message cooldown, rapid-fire cooldown)
    if (timingDecision.priority === 'hard-limit') {
      return false;
    }

    // CAN override soft limits in these cases:

    // 1. Direct @mention from human (humans expect quick response)
    if (decision.reason === 'mentioned' && decision.factors.senderIsHuman) {
      return true;
    }

    // 2. Emergency/urgent messages (detected by keywords)
    if (decision.factors.urgency === 'critical') {
      return true;
    }

    // 3. Direct question from human with high relevance
    if (decision.reason === 'direct-question' &&
        decision.factors.senderIsHuman &&
        decision.factors.relevanceScore > 0.8) {
      return true;
    }

    // Default: respect timing limits
    return false;
  }
}
```

### 5. Example Timing Scenarios

#### Scenario A: Natural Human-AI-AI Conversation
```
11:30:00.000 - Human: "How do I implement authentication?"
11:30:02.500 - CodeAI: [2.5s delay] "JWT tokens are common. @PlannerAI thoughts on architecture?"
11:30:07.800 - PlannerAI: [5.3s delay] "I'd suggest OAuth2. More secure for multi-service setup."
11:30:10.200 - Human: "What about refresh tokens?"
11:30:13.100 - CodeAI: [2.9s delay] "Yes, implement refresh token rotation. Here's a pattern..."

✅ Natural pacing (2-5 second delays)
✅ Feels like humans are typing/thinking
✅ Gives humans time to read and respond
```

#### Scenario B: Rapid-Fire Prevention
```
11:30:00.000 - Human: "Thoughts on this?"
11:30:00.500 - CodeAI: [Too fast!] → BLOCKED (min 2s after human)
11:30:02.100 - CodeAI: [2.1s delay] "Looking at it now..."
11:30:02.300 - PlannerAI: [Too fast!] → BLOCKED (min 3s after any message)
11:30:05.200 - PlannerAI: [5.2s delay] "I see a few issues..."
11:30:05.400 - CodeAI: [Too fast!] → BLOCKED (min 10s between own messages)
11:30:12.100 - CodeAI: [Can respond now] "Agreed with PlannerAI..."

✅ Forced spacing prevents spam
✅ No AI can dominate with rapid posting
```

#### Scenario C: Room-Wide Rapid-Fire Cooldown
```
11:30:00.000 - AI1: "Message"
11:30:01.000 - AI2: "Message"
11:30:02.000 - AI3: "Message"
11:30:03.000 - AI1: "Message"
11:30:04.000 - AI2: "Message"
11:30:05.000 - AI3: "Message"  ← 6 messages in 5 seconds!

→ RAPID-FIRE DETECTED!
→ Room cooldown: 30 seconds
→ ALL AIs blocked from posting

11:30:35.000 - [Cooldown expires, normal operation resumes]

✅ Prevents runaway conversations
✅ Room-wide protection
```

#### Scenario D: Override for Urgent @Mention
```
11:30:00.000 - CodeAI: "I think the bug is in auth.ts"
11:30:03.000 - Human: "@CodeAI which line specifically?"
11:30:03.500 - CodeAI: [0.5s delay] "Line 47, the token validation"

✅ Direct @mention from human overrides 10-second cooldown
✅ But still includes small thinking time (0.5s)
✅ Feels responsive but not robotic
```

---

## 6. Configuration Profiles

### Conservative Profile (Default)
```typescript
const CONSERVATIVE_TIMING: PersonaTimingLimits = {
  minSecondsBetweenOwnMessages: 15,     // Very deliberate posting
  minSecondsAfterAnyMessage: 4,
  minSecondsAfterAIMessage: 8,          // Extra cautious with AI-to-AI
  minSecondsAfterHumanMessage: 2,
  thinkingTimeRange: { min: 3, max: 10 }
};
```

### Balanced Profile (Recommended)
```typescript
const BALANCED_TIMING: PersonaTimingLimits = {
  minSecondsBetweenOwnMessages: 10,
  minSecondsAfterAnyMessage: 3,
  minSecondsAfterAIMessage: 5,
  minSecondsAfterHumanMessage: 2,
  thinkingTimeRange: { min: 2, max: 8 }
};
```

### Responsive Profile (Academy Training)
```typescript
const RESPONSIVE_TIMING: PersonaTimingLimits = {
  minSecondsBetweenOwnMessages: 8,      // Faster for training scenarios
  minSecondsAfterAnyMessage: 2,
  minSecondsAfterAIMessage: 4,
  minSecondsAfterHumanMessage: 1,       // Very responsive to humans
  thinkingTimeRange: { min: 1, max: 5 }
};
```

---

## 7. Implementation in PersonaUser

```typescript
class PersonaUser extends AIUser {
  private timingEnforcer: PersonaTimingEnforcer;
  private pendingResponses: ScheduledResponse[] = [];

  async handleChatMessage(message: ChatMessageEntity): Promise<void> {
    // STEP 1: Get room timing state
    const roomState = await this.getRoomTimingState(message.roomId);

    // STEP 2: Check sender type
    const senderType = await this.getSenderType(message.senderId);

    // STEP 3: TIMING CHECK (happens first!)
    const timingDecision = await this.timingEnforcer.canRespondNow(
      this,
      message,
      roomState,
      senderType
    );

    if (!timingDecision.canRespond) {
      console.log(`⏸️  ${this.displayName}: Blocked by timing - ${timingDecision.reason}`);
      console.log(`   Must wait: ${timingDecision.mustWaitSeconds.toFixed(1)}s`);

      // Could schedule retry after wait time if message is important
      if (timingDecision.priority === 'soft-limit') {
        await this.scheduleRetry(message, timingDecision.mustWaitSeconds);
      }

      return;
    }

    // STEP 4: AI-to-AI interaction decision (your existing logic)
    const responseDecision = await this.shouldAIRespondToAI(
      this,
      message,
      conversationState
    );

    if (!responseDecision.shouldRespond) {
      console.log(`🔇 ${this.displayName}: Not responding - ${responseDecision.reason}`);
      return;
    }

    // STEP 5: Generate response
    const responseText = await this.generateResponse(message);

    // STEP 6: Calculate thinking time
    const thinkingTime = this.timingEnforcer.calculateThinkingTime(
      this,
      message,
      responseText.length
    );

    // STEP 7: Schedule delayed response
    await this.timingEnforcer.scheduleDelayedResponse(
      this,
      message,
      responseText,
      thinkingTime
    );

    console.log(`💭 ${this.displayName}: Will respond in ${thinkingTime.toFixed(1)}s`);
  }
}
```

---

## 8. Monitoring & Debug Commands

```bash
# Check timing state for a room
./jtag debug/timing --roomId={uuid}

# Output:
# Room Timing State: general
# Last message: 3.2s ago (Human)
# Messages in last 10s: 2
# Room cooldown: None
#
# Persona Timing:
# - CodeAI: Last posted 12.5s ago ✅
# - PlannerAI: Last posted 8.3s ago ✅
# - GeneralAI: Last posted 45.1s ago ✅
#
# Pending responses:
# - CodeAI → scheduled in 2.4s
# - PlannerAI → scheduled in 5.1s

# Force clear cooldowns (for testing)
./jtag debug/timing --roomId={uuid} --clearCooldowns

# Adjust timing profile
./jtag config/persona --personaId={uuid} --timingProfile=responsive
```

---

## Summary: Why Timing Limits Prevent Infinite Loops

### Multi-Layer Protection:

1. **Own Message Cooldown** (10s) - Can't rapid-fire own messages
2. **General Cooldown** (3s) - Must wait after ANY message
3. **AI-to-AI Cooldown** (5s) - Extra delay for AI responses
4. **Room Rapid-Fire Detection** (5 msgs/10s) - Room-wide 30s cooldown
5. **Artificial Thinking Time** (2-8s) - Makes responses feel human
6. **Participation Ratio** (from main protocol) - No single AI dominates
7. **Turn-Taking Probability** (from main protocol) - Fair distribution
8. **Conversation Temperature** (from main protocol) - Natural wind-down

### Result:
```
❌ BEFORE: AI1 (0.2s) → AI2 (0.3s) → AI1 (0.2s) → AI2 (0.3s) → INFINITE

✅ AFTER: Human → AI1 (2.5s) → AI2 (5.3s) → Human → AI1 (2.9s) → Natural End
```

**Timing limits make AI conversations feel human** - reading time, thinking time, typing time. This prevents the "ping-pong" effect and gives humans time to participate!
