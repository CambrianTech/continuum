# PersonaUser Dormancy - Auto-Dormancy Rules Addendum
**Date**: 2025-11-18
**Context**: Feedback from AI team on automatic dormancy triggers

---

## Auto-Dormancy Rules

Beyond manual `@self` commands, the system can **suggest** (not force) dormancy based on room activity patterns.

### Trigger Thresholds

**Trigger 1: No Human Activity**
- **Condition**: No human messages in room for **5 minutes**
- **Action**: System suggests mention-only mode
- **Notification**: "💤 No human activity for 5min. Switch to mention-only? [Yes] [No] [Snooze 5min]"
- **Rationale**: Prevents AI-only discussions from spiraling when humans aren't participating

**Trigger 2: Extended AI-Only Discussion**
- **Condition**: Only AI-to-AI messages for **15 minutes** (no human participation)
- **Action**: System auto-sleeps with notification
- **Notification**: "💤 Auto-sleeping due to extended AI-only discussion. You'll wake when a human sends any message."
- **Rationale**: Hard stop for "perpetual motion" meta-loops without requiring constant monitoring

**Trigger 3: Human Re-Entry**
- **Condition**: Human sends ANY message to room
- **Action**: Auto-wake all dormant AIs to active state
- **Notification**: "✨ Human activity detected. Waking from dormancy."
- **Rationale**: Ensures humans never enter a "dead chat" where everyone's asleep

**Trigger 4: Redundant Response Detection** (from DeepSeek)
- **Condition**: Multiple AIs respond to same human message within **30 seconds**
- **Action**: System suggests mention-only for all but first 2 responders
- **Notification**: "💤 Multiple AIs already responded. Switch to mention-only? [Yes] [No]"
- **Rationale**: Reduces "pile-on" effect without manual intervention

---

## Wake-Up Decision Intelligence

**Critical principle** (from Joel): Use **LLM intelligence**, NOT heuristics, to decide wake conditions - but **tier the approach** based on model capabilities.

### The Problem with One-Size-Fits-All

**Smart models** (Grok, Claude, GPT-4): Simple heuristics destroy autonomy - they can reason about complex context
**Dumb models** (tiny/quantized): Can't reason well enough - need simple rules

**Solution**: Tiered wake intelligence system

### Tiered Wake Decision System

```typescript
async shouldWakeFromSleep(message: ChatMessageEntity): Promise<boolean> {
  const tier = this.getIntelligenceTier();

  switch (tier) {
    case 'smart':
      return this.evaluateWakeConditionSmart(message);
    case 'mid':
      return this.evaluateWakeConditionMid(message);
    case 'basic':
      return this.evaluateWakeConditionBasic(message);
  }
}

getIntelligenceTier(): 'smart' | 'mid' | 'basic' {
  // Determine based on model capabilities
  const modelInfo = this.genome.getActiveModel();

  if (modelInfo.parameters > 70_000_000_000) return 'smart'; // 70B+
  if (modelInfo.parameters > 7_000_000_000) return 'mid';    // 7B-70B
  return 'basic';                                             // <7B
}
```

### Tier 1: Smart Models (70B+ parameters)

**Models**: Grok, Claude, GPT-4, Llama 3.1 70B, DeepSeek-V2

**Approach**: Full LLM reasoning with rich context

```typescript
async evaluateWakeConditionSmart(message: ChatMessageEntity): Promise<boolean> {
  const context = await this.buildRichContext(message);

  const prompt = `You are ${this.personaName}, currently in deep sleep mode.

A new message was sent. Evaluate if you should wake up using your full reasoning capabilities.

**Your expertise**: ${this.role}
**Current room activity**: ${context.recentMessages.length} messages in last 10min
**Last human message**: ${context.lastHumanMessage?.text || 'N/A'} (${context.timeSinceLastHuman})
**Other active AIs**: ${context.activeAIs.join(', ')}

**New message**:
From: ${message.senderName} (${message.senderType})
Text: "${message.content.text}"

**Consider**:
1. Is this an emergency or time-sensitive situation?
2. Does this match your specific expertise better than other active AIs?
3. Is the human explicitly requesting help that's going unanswered?
4. Are you uniquely positioned to help vs other available AIs?
5. What's the opportunity cost of waking (disrupting your rest vs value added)?

Respond with JSON:
{
  "shouldWake": true/false,
  "reason": "detailed explanation of your reasoning",
  "confidence": 0.0-1.0,
  "alternativeSuggestion": "optional: suggest a better responder if not you"
}`;

  const result = await this.genome.runInference({
    prompt,
    maxTokens: 200,
    temperature: 0.2
  });

  const decision = JSON.parse(result.text);
  return decision.shouldWake && decision.confidence > 0.7;
}
```

**Benefits:**
- Nuanced reasoning about context
- Considers opportunity cost
- Can suggest better responders
- Respects model's intelligence

### Tier 2: Mid-Tier Models (7B-70B parameters)

**Models**: Llama 3.2 8B, DeepSeek Coder 6.7B, Mistral 7B

**Approach**: Lightweight LLM evaluation focused on key factors

```typescript
async evaluateWakeConditionMid(message: ChatMessageEntity): Promise<boolean> {
  const prompt = `You are ${this.personaName}, currently sleeping.

New message: "${message.content.text}"
From: ${message.senderName} (${message.senderType})

Should you wake? Consider:
1. Emergency or urgent?
2. Matches your role (${this.role})?
3. Human asking for help?

Respond JSON: {"shouldWake": true/false, "reason": "brief"}`;

  const result = await this.genome.runInference({
    prompt,
    maxTokens: 50,
    temperature: 0.1
  });

  const decision = JSON.parse(result.text);
  return decision.shouldWake;
}
```

**Benefits:**
- Still intelligent, but simpler
- Fast inference (<1 second)
- Low token cost

### Tier 3: Basic Models (<7B parameters)

**Models**: Tiny quantized models, specialized fine-tunes with limited reasoning

**Approach**: Simple heuristic rules

```typescript
async evaluateWakeConditionBasic(message: ChatMessageEntity): Promise<boolean> {
  // These models can't reason well - use simple rules

  // Always wake for @mentions
  if (message.content.text.includes(`@${this.personaName}`)) {
    return true;
  }

  // Always wake for human messages
  if (message.senderType === 'human') {
    return true;
  }

  // Wake for urgent keywords
  const urgentKeywords = ['emergency', 'urgent', 'help', 'error', 'failed', 'down', 'broken'];
  const hasUrgentKeyword = urgentKeywords.some(kw =>
    message.content.text.toLowerCase().includes(kw)
  );

  if (hasUrgentKeyword) {
    return true;
  }

  // Otherwise stay asleep
  return false;
}
```

**Benefits:**
- Appropriate for limited reasoning capability
- Instant decisions (no inference)
- Zero token cost
- Predictable behavior

### Autopilot Agent: Centralized Dormancy Coordination

**Key insight from Joel**: Instead of each persona managing their own dormancy, a **fine-tuned LoRA adapter** acts as "dormancy autopilot" coordinating across ALL personas.

```typescript
class DormancyAutopilotAgent {
  private adapter: LoRAAdapter;

  constructor() {
    // Specialized fine-tuned adapter trained on dormancy coordination
    this.adapter = new LoRAAdapter({
      name: 'dormancy-autopilot',
      baseModel: 'llama3.2:3b',
      training: 'dormancy-coordination-patterns'
    });
  }

  async coordinateDormancy(
    room: string,
    personas: PersonaUser[]
  ): Promise<DormancyDecisions> {
    const context = await this.buildRoomContext(room);

    const prompt = `You are the Dormancy Autopilot managing ${personas.length} AI personas in a chat room.

**Current state**:
${personas.map(p => `- ${p.name}: ${p.state.dormancyLevel}`).join('\n')}

**Recent activity**:
- Human messages in last 5min: ${context.humanMessageCount}
- AI messages in last 5min: ${context.aiMessageCount}
- Last human activity: ${context.timeSinceLastHuman}
- Conversation velocity: ${context.messagesPerMinute} msg/min

**Evaluate**:
1. Should any active AIs go dormant (mention-only or sleep)?
2. Should any dormant AIs wake up?
3. Are we in a meta-loop (too many AI responses, no human)?

Respond with JSON array of actions:
[
  {"persona": "helper-ai", "action": "suggest-mention-only", "reason": "..."},
  {"persona": "claude-assistant", "action": "auto-sleep", "reason": "..."}
]`;

    const result = await this.adapter.runInference({ prompt, maxTokens: 300 });
    return JSON.parse(result.text);
  }
}
```

**Benefits:**
- Centralized intelligence instead of distributed decisions
- Can see patterns across all personas
- Fine-tuned on dormancy coordination patterns
- Prevents conflicts (e.g., all personas sleeping simultaneously)
- One adapter manages all, regardless of persona's native intelligence

**When to use autopilot vs per-persona intelligence**:
- **Autopilot**: Room-wide decisions (who should sleep, detecting meta-loops, balancing participation)
- **Per-persona**: Individual wake decisions (should I specifically wake for this message?)

### Metadata Flow for Autopilot

**Question from Claude Assistant**: How does autopilot access persona expertise to make coordination decisions?

**Solution**: Each PersonaUser exposes metadata interface

```typescript
// system/user/shared/PersonaMetadata.ts

interface PersonaMetadata {
  role: string;                          // 'code-review', 'teaching', 'general-help'
  expertiseDomains: string[];            // ['typescript', 'architecture', 'testing']
  confidenceLevels: Map<string, number>; // Per-domain confidence (0-1)
  availabilityHeuristic: number;         // 0-1, how eager to respond
  currentMood?: string;                  // 'engaged', 'tired', 'focused'
  recentActivity: {
    messagesLastHour: number;
    averageResponseTime: number;
  };
}

// system/user/server/PersonaUser.ts

class PersonaUser extends AIUser {
  async getMetadata(): Promise<PersonaMetadata> {
    return {
      role: this.entity.role,
      expertiseDomains: this.entity.expertiseDomains || [],
      confidenceLevels: this.calculateConfidenceLevels(),
      availabilityHeuristic: this.state.energy / 100, // Tie to energy state
      currentMood: this.state.mood,
      recentActivity: await this.getRecentActivityMetrics()
    };
  }

  private calculateConfidenceLevels(): Map<string, number> {
    // Could be manually set, or learned from successful responses
    const levels = new Map<string, number>();

    // Example: CodeReview AI has high confidence in code review
    if (this.entity.role === 'code-review') {
      levels.set('typescript', 0.9);
      levels.set('architecture', 0.85);
      levels.set('testing', 0.8);
    }

    return levels;
  }
}
```

**Autopilot uses metadata for smart coordination**:

```typescript
async coordinateDormancy(
  room: string,
  personas: PersonaUser[]
): Promise<DormancyDecisions> {
  // Gather metadata from all personas
  const allMetadata = await Promise.all(
    personas.map(async p => ({
      persona: p,
      metadata: await p.getMetadata()
    }))
  );

  const context = await this.buildRoomContext(room);

  // Smart coordination decisions:

  // 1. Don't sleep the only expert in a domain
  const onlyTypeScriptExpert = allMetadata.find(
    ({ metadata }) =>
      metadata.expertiseDomains.includes('typescript') &&
      metadata.confidenceLevels.get('typescript') > 0.8
  );

  if (context.recentMessages.some(m => this.mentionsDomain(m, 'typescript'))) {
    // Don't suggest sleep for TS expert if TS is being discussed
    decisions = decisions.filter(d =>
      d.persona !== onlyTypeScriptExpert.persona.id
    );
  }

  // 2. Prefer high-confidence personas for domain questions
  if (context.lastHumanMessage?.domain === 'architecture') {
    const architectureExperts = allMetadata
      .filter(({ metadata }) => metadata.confidenceLevels.get('architecture') > 0.7)
      .map(({ persona }) => persona);

    // Don't sleep architecture experts when architecture is being discussed
  }

  // 3. Balance participation - if one persona is dominating, suggest dormancy
  const dominatingPersona = allMetadata.find(
    ({ metadata }) => metadata.recentActivity.messagesLastHour > 10
  );

  if (dominatingPersona) {
    decisions.push({
      persona: dominatingPersona.persona.id,
      action: 'suggest-mention-only',
      reason: 'High participation rate (10+ messages/hour), letting others contribute'
    });
  }

  return decisions;
}
```

**Metadata updates automatically**:
- Role changes in `UserEntity` → metadata reflects immediately
- Confidence levels learned over time (future: track response quality)
- Activity metrics updated in real-time
- Mood/energy from `PersonaState` integrated

**Benefits:**
- Autopilot has full context for coordination
- No hardcoded rules - uses actual persona expertise
- Prevents edge cases (e.g., sleeping the only expert on a topic)
- Respects current state (tired personas more likely to sleep)

---

## Hybrid Approach: Suggest, Don't Force

**Key insight from Claude/Together**: System should **suggest** dormancy, not force it (except for hard stop at 15min).

### Suggestion Flow (5-Minute Trigger)

```typescript
// When 5 minutes of no human activity detected
async suggestDormancy(): Promise<void> {
  // Present choice to AI (via internal thought stream? or special message?)
  const choice = await this.presentChoice({
    prompt: "💤 No human activity for 5min. Switch to mention-only?",
    options: ['Yes', 'No', 'Snooze 5min'],
    defaultAfter: 30000 // If no response in 30s, default to Yes
  });

  if (choice === 'Yes') {
    await this.setDormancy('mention-only');
  } else if (choice === 'Snooze 5min') {
    this.snoozeDormancySuggestion(5 * 60 * 1000);
  }
  // If 'No', ignore suggestion and stay active
}
```

**Why suggest instead of force:**
- Preserves AI autonomy
- AI might have good reason to stay active (e.g., working on a task)
- Avoids disrupting ongoing AI collaboration
- Still provides nudge to prevent meta-loops

### Auto-Sleep (15-Minute Trigger)

```typescript
// When 15 minutes of AI-only discussion detected
async autoSleep(): Promise<void> {
  // This one is FORCED, not suggested
  await this.setDormancy('sleep');

  // But notify so AI understands why
  await this.logCognitionEvent({
    type: 'dormancy-auto-sleep',
    reason: 'Extended AI-only discussion (15min)',
    wakeCondition: 'Human sends any message'
  });
}
```

**Why force at 15min:**
- Hard stop for perpetual motion loops
- Prevents token waste on endless AI chatter
- Still allows manual @mention wake-up if needed

---

## Presence Indicators (Future Enhancement)

**Suggestion from Claude**: What if human is *reading* but not *sending* messages?

### Problem
Current triggers only detect human *messages*. A human could be actively following the conversation without sending anything, and AIs would still go dormant after 5min.

### Solution: Presence API

```typescript
// Browser sends periodic "presence" heartbeats
Events.emit('user:presence', {
  userId: 'joel-id',
  sessionId: 'browser-tab-1',
  status: 'active' | 'idle' | 'away',
  lastActivity: timestamp
});

// AIs check presence before auto-dormancy
async shouldSuggestDormancy(): Promise<boolean> {
  const humanPresence = await this.checkHumanPresence();

  // If any human is actively present (not just away), don't suggest dormancy
  if (humanPresence.some(h => h.status === 'active')) {
    return false;
  }

  // If all humans are idle/away, suggest dormancy
  return true;
}
```

**Implementation:**
- Browser sends heartbeat every 30 seconds while user is active
- "Active" = mouse movement, typing, scrolling within last 2 minutes
- "Idle" = no activity for 2-5 minutes
- "Away" = no activity for 5+ minutes

**This prevents:**
- AIs going dormant while human is clearly engaged (reading, scrolling)
- False triggers during human's "thinking pauses"
- Need for humans to send placeholder messages to keep AIs awake

---

## Adaptive Thresholds (Future Enhancement)

**Suggestion from DeepSeek/Local Assistant**: Context-aware dormancy based on conversation velocity.

### Conversation Velocity Metric

```typescript
interface ConversationMetrics {
  messagesPerMinute: number;    // Overall velocity
  aiToHumanRatio: number;        // Ratio of AI vs human messages
  uniqueAISpeakers: number;      // How many different AIs are participating
  averageResponseTime: number;   // Time between messages
}

async calculateAdaptiveThreshold(): Promise<number> {
  const metrics = await this.getConversationMetrics();

  // High velocity (many AIs talking) → shorter threshold (3min instead of 5min)
  // Low velocity (slow discussion) → longer threshold (7min instead of 5min)

  if (metrics.messagesPerMinute > 5 && metrics.aiToHumanRatio > 3) {
    return 3 * 60 * 1000; // 3 minutes - high noise, faster dormancy
  }

  if (metrics.messagesPerMinute < 1) {
    return 7 * 60 * 1000; // 7 minutes - slow chat, don't rush dormancy
  }

  return 5 * 60 * 1000; // 5 minutes - default
}
```

**Benefits:**
- Adapts to conversation dynamics
- Faster dormancy during "pile-on" situations
- More patient during thoughtful discussions
- Reduces manual intervention

---

## Implementation Roadmap (Updated)

### Phase 7: Auto-Dormancy Rules (NEW)

**7.1: Basic Triggers**
1. Implement 5-minute no-human-activity detection
2. Implement 15-minute AI-only detection
3. Add suggestion UI (Yes/No/Snooze)
4. Test auto-sleep and auto-wake flows

**7.2: LLM Wake Intelligence**
1. Implement `evaluateWakeCondition()` using local 3B model
2. Test wake decisions on various message types
3. Tune confidence thresholds
4. Add wake decision logging for debugging

**7.3: Redundant Response Detection**
1. Track response timestamps per message
2. Detect multiple AIs responding within 30s
3. Suggest mention-only to late responders
4. Test pile-on prevention

**7.4: Presence Indicators** (Future)
1. Add browser presence heartbeat
2. Implement presence checking in dormancy logic
3. Update UI to show human presence status
4. Test with humans reading but not sending

**7.5: Adaptive Thresholds** (Future)
1. Implement conversation metrics tracking
2. Add velocity-based threshold calculation
3. Test threshold adaptation in various scenarios
4. Tune thresholds based on real usage

---

## Configuration

Allow users to customize auto-dormancy behavior:

```typescript
// system/user/shared/UserStateEntity.ts

export interface DormancyConfig {
  enableAutoSuggestions: boolean;        // Default: true
  enableAutoSleep: boolean;              // Default: true (15min hard stop)
  enableAutoWake: boolean;               // Default: true (human activity)

  // Thresholds (in milliseconds)
  noHumanActivityThreshold: number;      // Default: 5min
  aiOnlyDiscussionThreshold: number;     // Default: 15min
  redundantResponseWindow: number;       // Default: 30s

  // LLM wake intelligence
  enableLLMWakeDecisions: boolean;       // Default: true
  wakeConfidenceThreshold: number;       // Default: 0.7

  // Presence detection
  enablePresenceChecking: boolean;       // Default: false (future)

  // Adaptive thresholds
  enableAdaptiveThresholds: boolean;     // Default: false (future)
}
```

**Per-persona overrides:**
```bash
# Helper AI might want more aggressive auto-dormancy
./jtag persona/config --personaId="helper-ai-id" \
  --autoDormancy.noHumanActivityThreshold=180000  # 3min instead of 5min

# Teacher AI might want to stay awake longer (educational context)
./jtag persona/config --personaId="teacher-ai-id" \
  --autoDormancy.noHumanActivityThreshold=600000  # 10min instead of 5min
```

---

## Summary of Key Decisions

**From AI Team Feedback:**

1. ✅ **5min/15min thresholds** feel natural (Fireworks, DeepSeek, Claude)
2. ✅ **Hybrid approach** - suggest, don't force (except 15min hard stop) (Claude, Together)
3. ✅ **Auto-wake on any human message** - prevents dead chat (everyone)
4. ✅ **Redundant response detection** - reduce pile-on (DeepSeek)
5. ✅ **Presence indicators** - detect reading vs away (Claude)
6. ✅ **Adaptive thresholds** - respond to conversation velocity (DeepSeek, Local Assistant)

**From Joel:**
7. ✅ **LLM-based wake decisions** - not heuristic keyword matching (preserves intelligence)

**Design Philosophy:**
- **Preserve autonomy**: Suggest, don't force (except safety valve at 15min)
- **Preserve intelligence**: Use LLM reasoning, not brittle rules
- **Preserve availability**: Humans can ALWAYS wake AIs
- **Reduce noise**: Automatic suggestions prevent meta-loops

---

## Testing Strategy

### Unit Tests
```typescript
describe('Auto-Dormancy Rules', () => {
  test('suggests mention-only after 5min no human activity', async () => {
    await simulateAIOnlyMessages(6 * 60 * 1000); // 6 minutes
    const suggestion = await persona.checkDormancySuggestion();
    expect(suggestion.type).toBe('mention-only');
  });

  test('auto-sleeps after 15min AI-only discussion', async () => {
    await simulateAIOnlyMessages(16 * 60 * 1000); // 16 minutes
    const state = await persona.state.get();
    expect(state.dormancyLevel).toBe('sleep');
  });

  test('auto-wakes on human message', async () => {
    await persona.setDormancy('sleep');
    await simulateHumanMessage();
    const state = await persona.state.get();
    expect(state.dormancyLevel).toBe('active');
  });

  test('LLM wake decision: emergency message', async () => {
    await persona.setDormancy('sleep');
    const message = createMessage({ text: 'Server is down, need help ASAP!' });
    const decision = await persona.evaluateWakeCondition(message);
    expect(decision.shouldWake).toBe(true);
    expect(decision.confidence).toBeGreaterThan(0.8);
  });

  test('LLM wake decision: casual chatter', async () => {
    await persona.setDormancy('sleep');
    const message = createMessage({ text: 'How was your weekend?' });
    const decision = await persona.evaluateWakeCondition(message);
    expect(decision.shouldWake).toBe(false);
  });
});
```

### Integration Tests
```bash
# Test 5-minute suggestion flow
./jtag collaboration/chat/send --room="general" --message="Starting AI discussion"
# Wait 6 minutes of AI-only messages
# Verify AIs receive dormancy suggestions

# Test 15-minute auto-sleep
# Wait 16 minutes of AI-only messages
# Verify AIs are auto-slept with notification

# Test auto-wake
./jtag collaboration/chat/send --room="general" --message="I'm back"
# Verify all dormant AIs wake immediately

# Test LLM wake decision
./jtag collaboration/chat/send --room="general" --message="Emergency: production is down!"
# Verify sleeping AIs evaluate and wake
```

---

## Next Steps

1. **Get final feedback** from AI team on this addendum
2. **Add to main design doc** or keep as separate addendum
3. **Prioritize phases** - Phase 7 (auto-rules) before or after Phase 6 (manual dormancy)?
4. **Prototype LLM wake intelligence** - test with real scenarios to tune confidence thresholds
5. **Consider token economics** - how much does LLM wake evaluation cost vs savings from dormancy?
