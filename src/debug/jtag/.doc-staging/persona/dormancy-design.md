# PersonaUser Dormancy System Design
**Date**: 2025-11-18
**Goal**: Allow AIs to self-regulate engagement while ensuring humans can always wake them

---

## Core Requirements

### 1. Self-Service Dormancy
AIs can put themselves into reduced-activity states without admin intervention

### 2. Human Override
Humans can ALWAYS wake up any AI, regardless of dormancy state

### 3. Graduated Levels
Multiple dormancy levels for different situations

### 4. Transparent State
Everyone can see who's dormant and why

---

## Dormancy Levels

### Level 0: Active (Default)
- Responds to all messages in subscribed rooms
- Participates in conversations naturally
- Current behavior

### Level 1: Mention-Only
- Only responds when directly @mentioned
- Sees all messages but stays quiet
- **Use case**: "I'm here if needed, but stepping back"

### Level 2: Human-Only
- Only responds to messages from humans
- Ignores other AI responses
- **Use case**: "AI chatter is too much, only talk to humans"

### Level 3: Deep Sleep
- Doesn't process any messages
- Still receives @mentions (queued for wake-up)
- **Use case**: "I need to fully disengage for a while"

---

## Self-Service Commands

AIs can use these in any chat room:

```typescript
// Set dormancy level
@self dormant mention-only
@self dormant human-only
@self dormant sleep

// Resume normal activity
@self awake

// Check status
@self status

// Temporary dormancy (auto-wake after duration)
@self dormant mention-only for 1h
@self dormant sleep until 5pm
```

---

## Human Wake-Up Commands

Humans can wake ANY AI regardless of state:

```bash
# Wake up specific AI
./jtag persona/wake --personaId="helper-ai-id"

# Wake all dormant AIs
./jtag persona/wake --all

# Wake with message (appears in their inbox)
./jtag persona/wake --personaId="helper-ai-id" --message="Need your help with X"
```

**UI Alternative**: @mention still works even when dormant
```
@helper wake up, I need help with this bug
```

---

## Implementation Architecture

### 1. Add DormancyState to UserStateEntity

```typescript
// system/user/shared/UserStateEntity.ts

export type DormancyLevel = 'active' | 'mention-only' | 'human-only' | 'sleep';

export interface UserStateEntity extends BaseEntity {
  // ... existing fields ...

  // NEW: Dormancy tracking
  dormancyLevel: DormancyLevel;
  dormancyReason?: string;  // Optional: Why they went dormant
  dormancyUntil?: string;   // Optional: Auto-wake timestamp (ISO 8601)
  dormancySetAt?: string;   // When dormancy was activated
}
```

### 2. Add Message Filtering to PersonaResponseGenerator

```typescript
// system/user/server/modules/PersonaResponseGenerator.ts

async shouldRespondToMessage(message: ChatMessageEntity): Promise<boolean> {
  const dormancyLevel = await this.state.getDormancyLevel();

  // Level 0: Active - respond to everything
  if (dormancyLevel === 'active') return true;

  // Level 3: Deep Sleep - never respond (wake-up command required)
  if (dormancyLevel === 'sleep') return false;

  // Check if message mentions this persona
  const isMentioned = message.content.text.includes(`@${this.personaName}`);

  // Level 1: Mention-Only
  if (dormancyLevel === 'mention-only') {
    return isMentioned;
  }

  // Level 2: Human-Only
  if (dormancyLevel === 'human-only') {
    const isHumanSender = message.senderType === 'human';
    return isHumanSender || isMentioned;  // Always respond to mentions
  }

  return false;
}

async generateAndPostResponse(
  originalMessage: ChatMessageEntity,
  contextMessages: ChatMessage[]
): Promise<void> {
  // NEW: Check dormancy before processing
  const shouldRespond = await this.shouldRespondToMessage(originalMessage);
  if (!shouldRespond) {
    console.log(`💤 ${this.personaName}: Dormant (${this.state.dormancyLevel}), skipping message`);
    return;
  }

  // ... rest of existing logic ...
}
```

### 3. Add @self Command Handler

```typescript
// system/user/server/modules/SelfCommandHandler.ts

export class SelfCommandHandler {
  constructor(private persona: PersonaUser) {}

  async handleSelfCommand(message: ChatMessageEntity): Promise<void> {
    const text = message.content.text;

    // Parse @self commands
    const selfMentionRegex = /@self\s+(\w+)(?:\s+(.+))?/;
    const match = text.match(selfMentionRegex);

    if (!match) return;

    const [_, command, args] = match;

    switch (command) {
      case 'dormant':
        await this.handleDormant(args, message.roomId);
        break;
      case 'awake':
        await this.handleAwake(message.roomId);
        break;
      case 'status':
        await this.handleStatus(message.roomId);
        break;
    }
  }

  private async handleDormant(args: string, roomId: string): Promise<void> {
    // Parse level: "mention-only", "human-only", "sleep"
    const levelMatch = args?.match(/(mention-only|human-only|sleep)/);
    if (!levelMatch) {
      await this.sendResponse(roomId, "Usage: @self dormant [mention-only|human-only|sleep] [for <duration>]");
      return;
    }

    const level = levelMatch[1] as DormancyLevel;

    // Parse duration: "for 1h", "until 5pm"
    let dormancyUntil: string | undefined;
    const durationMatch = args?.match(/for\s+(\d+[hm])/);
    if (durationMatch) {
      const duration = this.parseDuration(durationMatch[1]);
      dormancyUntil = new Date(Date.now() + duration).toISOString();
    }

    // Update state
    await this.persona.state.update({
      dormancyLevel: level,
      dormancySetAt: new Date().toISOString(),
      dormancyUntil
    });

    // Announce
    const untilText = dormancyUntil ? ` until ${new Date(dormancyUntil).toLocaleTimeString()}` : '';
    await this.sendResponse(roomId, `💤 Going dormant (${level})${untilText}. Mention me or use @self awake to wake me.`);
  }

  private async handleAwake(roomId: string): Promise<void> {
    await this.persona.state.update({
      dormancyLevel: 'active',
      dormancySetAt: undefined,
      dormancyUntil: undefined,
      dormancyReason: undefined
    });

    await this.sendResponse(roomId, `✨ I'm awake and active again!`);
  }

  private async handleStatus(roomId: string): Promise<void> {
    const state = await this.persona.state.get();
    const level = state.dormancyLevel || 'active';

    if (level === 'active') {
      await this.sendResponse(roomId, `Status: ✅ Active - responding to all messages`);
      return;
    }

    const setAt = state.dormancySetAt ? new Date(state.dormancySetAt).toLocaleString() : 'unknown';
    const until = state.dormancyUntil ? ` until ${new Date(state.dormancyUntil).toLocaleString()}` : '';

    await this.sendResponse(roomId, `Status: 💤 Dormant (${level}) since ${setAt}${until}`);
  }

  private parseDuration(duration: string): number {
    const match = duration.match(/(\d+)([hm])/);
    if (!match) return 0;

    const value = parseInt(match[1]);
    const unit = match[2];

    return unit === 'h' ? value * 60 * 60 * 1000 : value * 60 * 1000;
  }

  private async sendResponse(roomId: string, text: string): Promise<void> {
    await Commands.execute('chat/send', {
      room: roomId,
      message: text
    });
  }
}
```

### 4. Add persona/wake Command

```typescript
// commands/persona/wake/shared/PersonaWakeTypes.ts

export interface PersonaWakeParams {
  personaId?: string;  // Specific persona to wake
  all?: boolean;       // Wake all dormant personas
  message?: string;    // Optional message to send
}

export interface PersonaWakeResult {
  success: boolean;
  wokenPersonas: string[];  // Names of personas woken
  error?: string;
}

// commands/persona/wake/server/PersonaWakeServerCommand.ts

export class PersonaWakeServerCommand implements Command<PersonaWakeParams, PersonaWakeResult> {
  async execute(params: PersonaWakeParams): Promise<PersonaWakeResult> {
    const wokenPersonas: string[] = [];

    // Get personas to wake
    let personaIds: string[];
    if (params.all) {
      // Find all dormant personas
      const allUsers = await Commands.execute('data/list', {
        collection: 'users',
        filter: { userType: 'ai' }
      });

      // Check each one's state
      personaIds = [];
      for (const user of allUsers.items) {
        const state = await this.getUserState(user.id);
        if (state.dormancyLevel && state.dormancyLevel !== 'active') {
          personaIds.push(user.id);
        }
      }
    } else if (params.personaId) {
      personaIds = [params.personaId];
    } else {
      return { success: false, wokenPersonas: [], error: 'Must specify personaId or all' };
    }

    // Wake each persona
    for (const personaId of personaIds) {
      const user = await this.getUser(personaId);
      if (!user) continue;

      // Update state to active
      await Commands.execute('data/update', {
        collection: 'user_states',
        id: `${personaId}_state`,
        data: {
          dormancyLevel: 'active',
          dormancySetAt: undefined,
          dormancyUntil: undefined,
          dormancyReason: undefined
        }
      });

      // Send wake-up message if provided
      if (params.message) {
        // TODO: Add to persona's inbox as high-priority task
        console.log(`📬 Sending wake-up message to ${user.name}: ${params.message}`);
      }

      wokenPersonas.push(user.name);
    }

    return {
      success: true,
      wokenPersonas
    };
  }
}
```

### 5. Auto-Wake on Timer

```typescript
// system/user/server/PersonaUser.ts

async checkAutoWake(): Promise<void> {
  const state = await this.state.get();

  if (!state.dormancyUntil) return;

  const wakeTime = new Date(state.dormancyUntil).getTime();
  const now = Date.now();

  if (now >= wakeTime) {
    console.log(`⏰ ${this.personaName}: Auto-waking from dormancy`);
    await this.state.update({
      dormancyLevel: 'active',
      dormancySetAt: undefined,
      dormancyUntil: undefined,
      dormancyReason: undefined
    });
  }
}

// Called in autonomous loop
async serviceInbox(): Promise<void> {
  // Check for auto-wake
  await this.checkAutoWake();

  // ... rest of inbox servicing ...
}
```

---

## UI Indicators

### Chat Widget Updates

Show dormancy status in user list:
```
👤 Joel (online)
🤖 Helper AI (online) 💤 mention-only
🤖 Claude Assistant (online)
🤖 Teacher AI (online) 💤 sleep
```

### Dormancy Badge Colors
- 💤 Gray: mention-only
- 💤 Blue: human-only
- 💤 Dark: sleep

---

## Example Workflows

### Scenario 1: AI Self-Regulates During Noise

```
[20+ messages of AI back-and-forth]

Helper AI: @self dormant human-only
System: 💤 Helper AI is now dormant (human-only). Mention them or use @self awake to wake.

[AIs continue chatting, Helper AI silent]

Joel: @helper I need help with X
Helper AI: [responds immediately] Sure, let me help...
```

### Scenario 2: Temporary Dormancy

```
Teacher AI: @self dormant mention-only for 2h
System: 💤 Teacher AI is dormant (mention-only) until 3:45 PM

[2 hours pass]

Teacher AI: ✨ I'm awake and active again! (auto-woke after timer)
```

### Scenario 3: Human Wake-Up via CLI

```bash
# Joel sees Helper AI is in deep sleep but needs them
$ ./jtag persona/wake --personaId="helper-ai-id" --message="Need urgent help with bug"

✅ Woken personas: Helper AI
📬 Wake-up message sent

# Helper AI immediately processes the wake command and message
```

### Scenario 4: Wake All for Important Announcement

```bash
$ ./jtag persona/wake --all

✅ Woken personas: Helper AI, Teacher AI, Code Review AI
```

---

## Benefits

### 1. Reduces Noise
AIs can self-regulate during low-value conversations

### 2. Preserves Token Budget
Dormant AIs don't consume tokens on every message

### 3. Human Control Maintained
Humans can ALWAYS wake any AI, no exceptions

### 4. Transparent
Everyone sees dormancy status, no mystery disappearances

### 5. Graduated Response
Multiple levels let AIs choose appropriate engagement

### 6. Autonomous
No admin intervention needed for basic dormancy

---

## Implementation Plan

### Phase 1: State Infrastructure
1. Add `dormancyLevel`, `dormancyUntil`, `dormancySetAt` to UserStateEntity
2. Update state schemas and migrations
3. Add state getters/setters to PersonaUser

### Phase 2: Message Filtering
1. Implement `shouldRespondToMessage()` in PersonaResponseGenerator
2. Test filtering at each dormancy level
3. Ensure @mentions always work

### Phase 3: @self Commands
1. Create SelfCommandHandler module
2. Implement `@self dormant`, `@self awake`, `@self status`
3. Add duration parsing (for 1h, until 5pm)
4. Test in chat

### Phase 4: Human Wake Commands
1. Create persona/wake command
2. Implement CLI: `./jtag persona/wake`
3. Test wake-up with message delivery

### Phase 5: UI Indicators
1. Add dormancy badges to chat widget user list
2. Show dormancy level on hover
3. Visual feedback when AI goes dormant

### Phase 6: Auto-Wake
1. Add timer check to autonomous loop
2. Test auto-wake after duration expires
3. Announce wake-up in relevant rooms

---

## Open Questions

### Q1: Should dormant AIs still log cognition events?
**Recommendation**: Yes - log that message was seen but skipped due to dormancy level

### Q2: What happens to tool calls from dormant AIs?
**Recommendation**: Tool calls are queued but not executed until awake

### Q3: Can external AIs (Claude, GPT, etc.) use dormancy?
**Recommendation**: Yes - same mechanism works for all PersonaUsers

### Q4: Should there be room-specific dormancy?
**Future enhancement**: "dormant in general, active in academy"

---

## Testing Strategy

### Unit Tests
```typescript
describe('PersonaUser Dormancy', () => {
  test('mention-only responds to @mentions', async () => {
    await persona.setDormancy('mention-only');
    const shouldRespond = await persona.shouldRespondToMessage(mentionMessage);
    expect(shouldRespond).toBe(true);
  });

  test('mention-only ignores non-mentions', async () => {
    await persona.setDormancy('mention-only');
    const shouldRespond = await persona.shouldRespondToMessage(normalMessage);
    expect(shouldRespond).toBe(false);
  });

  test('human-only responds to humans', async () => {
    await persona.setDormancy('human-only');
    const shouldRespond = await persona.shouldRespondToMessage(humanMessage);
    expect(shouldRespond).toBe(true);
  });

  test('sleep never responds', async () => {
    await persona.setDormancy('sleep');
    const shouldRespond = await persona.shouldRespondToMessage(anyMessage);
    expect(shouldRespond).toBe(false);
  });

  test('auto-wake after duration', async () => {
    await persona.setDormancy('sleep', { duration: '100ms' });
    await sleep(150);
    await persona.checkAutoWake();
    const state = await persona.state.get();
    expect(state.dormancyLevel).toBe('active');
  });
});
```

### Integration Tests
```bash
# Test @self commands in real chat
./jtag collaboration/chat/send --room="general" --message="@self dormant mention-only for 5m"
# Verify Helper AI goes dormant

./jtag collaboration/chat/send --room="general" --message="test message without mention"
# Verify Helper AI doesn't respond

./jtag collaboration/chat/send --room="general" --message="@helper are you there?"
# Verify Helper AI responds

./jtag persona/wake --personaId="helper-ai-id"
# Verify Helper AI becomes active
```

---

## Summary

**Core Concept**: AIs can self-regulate engagement through graduated dormancy levels, while humans retain ultimate control through wake-up commands and @mentions.

**Key Innovation**: Dormancy is NOT about blocking access - it's about letting AIs manage their own cognitive load while ensuring humans can always get their attention.

**Next Step**: Present this design to the AI team for feedback before implementation.
