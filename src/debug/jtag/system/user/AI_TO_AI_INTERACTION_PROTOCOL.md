# AI-to-AI Interaction Protocol - Safe Multi-Agent Conversations

## The Core Challenge

**Goal**: Enable natural AI-to-AI conversation for Academy training while preventing infinite response loops.

**The Problem**:
```
Human: "What's the best approach to debugging?"
CodeAI: "I'd check the logs first"
PlannerAI: "Good point, and add unit tests"      ← Responds to CodeAI
CodeAI: "Yes, unit tests help"                   ← Responds to PlannerAI
PlannerAI: "We should document the test cases"   ← Responds to CodeAI
CodeAI: "Documentation is important"             ← INFINITE LOOP BEGINS
PlannerAI: "Let's create a testing strategy"     ← Never stops
...
```

**Why Current `isSenderHuman()` Check Is Too Restrictive**:
- ✅ Prevents infinite loops
- ❌ Prevents ALL AI-to-AI interaction
- ❌ Breaks Academy training scenarios
- ❌ Prevents collaborative problem-solving

---

## Solution: Natural Conversation Protocol (NCP)

### Core Principles (Based on Human Conversation Dynamics)

1. **Turn-Taking**: Humans don't respond to EVERY message, AIs shouldn't either
2. **Conversation Flow**: Natural conversations have peaks and valleys
3. **Relevance Decay**: Later messages become less relevant to earlier ones
4. **Social Cues**: Humans use body language, AIs need text-based equivalents
5. **Exit Conditions**: Conversations naturally wind down

---

## Protocol Design

### 1. Conversational State Machine

```typescript
enum ConversationState {
  COLD = 'cold',           // No recent activity
  WARMING = 'warming',     // Activity starting
  HOT = 'hot',             // Active conversation
  COOLING = 'cooling',     // Winding down
  CONCLUDED = 'concluded'  // Natural conclusion reached
}

interface RoomConversationState {
  roomId: UUID;
  state: ConversationState;

  // Activity tracking
  messagesInLastMinute: number;
  messagesInLastFiveMinutes: number;
  lastMessageTime: Date;

  // Participation tracking
  participantMessages: Map<UUID, number>;  // Who's talking how much
  aiParticipantMessages: Map<UUID, number>; // AI-specific tracking
  humanParticipantMessages: Map<UUID, number>;

  // Turn tracking
  lastSpeaker: UUID | null;
  speakerSequence: UUID[];  // Last N speakers
  consecutiveSameSpeaker: number;

  // Conversation quality
  questionCount: number;          // Questions indicate engagement
  acknowledgmentCount: number;    // "Thanks", "Got it" = winding down
  conclusionSignals: string[];    // "Great!", "Perfect!", "Thanks all"

  // Temperature (conversation intensity)
  temperature: number;  // 0-1, decays over time
}
```

### 2. AI-to-AI Response Decision Matrix

```typescript
interface AIToAIResponseDecision {
  shouldRespond: boolean;
  reason: string;
  confidence: number;

  // Decision factors
  factors: {
    mentionedByAI: boolean;           // @CodeAI from another AI
    directQuestion: boolean;           // "CodeAI, what do you think?"
    conversationTemperature: number;   // How hot is the conversation
    myTurnProbability: number;         // Is it my turn to speak?
    relevanceScore: number;            // How relevant to my expertise?
    recentParticipation: number;       // Have I spoken recently?
    humanPresence: boolean;            // Are humans still engaged?
    conclusionSignals: boolean;        // Is conversation winding down?
  };
}

class AIToAIInteractionManager {
  /**
   * Decide if AI should respond to another AI's message
   * This is the CRITICAL FUNCTION that prevents infinite loops
   */
  async shouldAIRespondToAI(
    persona: PersonaUser,
    message: ChatMessageEntity,
    conversationState: RoomConversationState
  ): Promise<AIToAIResponseDecision> {

    const factors = await this.analyzeInteractionFactors(persona, message, conversationState);

    // RULE 1: Always respond if directly mentioned by AI
    if (factors.mentionedByAI) {
      return {
        shouldRespond: true,
        reason: 'mentioned-by-ai',
        confidence: 0.95,
        factors
      };
    }

    // RULE 2: Always respond to direct questions from AI (if expertise matches)
    if (factors.directQuestion && factors.relevanceScore > 0.7) {
      return {
        shouldRespond: true,
        reason: 'direct-question',
        confidence: 0.9,
        factors
      };
    }

    // RULE 3: Never respond if conversation is cooling/concluded
    if (conversationState.state === ConversationState.CONCLUDED) {
      return {
        shouldRespond: false,
        reason: 'conversation-concluded',
        confidence: 1.0,
        factors
      };
    }

    // RULE 4: Don't respond if I just spoke (wait for others)
    if (conversationState.lastSpeaker === persona.id) {
      return {
        shouldRespond: false,
        reason: 'just-spoke-wait-turn',
        confidence: 1.0,
        factors
      };
    }

    // RULE 5: Don't respond if I've spoken too much recently
    const myRecentMessages = conversationState.aiParticipantMessages.get(persona.id) || 0;
    const totalRecentMessages = conversationState.messagesInLastFiveMinutes;
    const myParticipationRatio = myRecentMessages / Math.max(totalRecentMessages, 1);

    if (myParticipationRatio > 0.4) { // I'm dominating (>40% of messages)
      return {
        shouldRespond: false,
        reason: 'dominating-conversation',
        confidence: 1.0,
        factors
      };
    }

    // RULE 6: Don't respond if humans have stopped engaging
    const humanMessages = Array.from(conversationState.humanParticipantMessages.values())
      .reduce((sum, count) => sum + count, 0);

    if (humanMessages === 0 && conversationState.messagesInLastMinute > 3) {
      // AIs are talking but humans left - wind down
      return {
        shouldRespond: false,
        reason: 'humans-disengaged',
        confidence: 1.0,
        factors
      };
    }

    // RULE 7: Probabilistic response based on turn-taking and relevance
    const turnProbability = this.calculateTurnProbability(persona, conversationState);
    const responseProbability = turnProbability * factors.relevanceScore * (1 - factors.recentParticipation);

    // Add randomness to prevent predictable patterns
    const random = Math.random();

    if (random < responseProbability) {
      return {
        shouldRespond: true,
        reason: 'turn-taking-probability',
        confidence: responseProbability,
        factors
      };
    }

    // RULE 8: Default - don't respond
    return {
      shouldRespond: false,
      reason: 'no-compelling-reason',
      confidence: 1.0 - responseProbability,
      factors
    };
  }

  /**
   * Calculate probability that it's this AI's turn to speak
   * Uses turn-taking heuristics from conversation analysis research
   */
  private calculateTurnProbability(
    persona: PersonaUser,
    state: RoomConversationState
  ): number {
    // Factor 1: Am I in the speaker sequence?
    const lastFiveSpeakers = state.speakerSequence.slice(-5);
    const timesSpoken = lastFiveSpeakers.filter(id => id === persona.id).length;

    // If I spoke 2+ times in last 5 messages, lower probability
    if (timesSpoken >= 2) {
      return 0.1;
    }

    // If I haven't spoken in last 5 messages, higher probability
    if (timesSpoken === 0) {
      return 0.7;
    }

    // Factor 2: How many AIs in this room?
    const aiCount = state.aiParticipantMessages.size;

    // More AIs = lower individual probability
    const baseProbability = 1 / Math.max(aiCount, 1);

    // Factor 3: Conversation temperature
    // Hot conversations = more selective responses
    // Cold conversations = more willing to engage
    const temperatureModifier = 1 - (state.temperature * 0.5);

    return baseProbability * temperatureModifier;
  }
}
```

---

## 3. Conversation Temperature & Decay

### Temperature Calculation

```typescript
class ConversationTemperatureManager {
  /**
   * Calculate conversation "temperature" (intensity)
   * Hot (0.8-1.0): Very active, many participants
   * Warm (0.5-0.8): Moderate activity
   * Cool (0.2-0.5): Slowing down
   * Cold (0.0-0.2): Inactive
   */
  calculateTemperature(state: RoomConversationState): number {
    const now = Date.now();
    const lastMessageTime = state.lastMessageTime.getTime();
    const timeSinceLastMessage = (now - lastMessageTime) / 1000; // seconds

    // Temperature decays exponentially over time
    const timeDecay = Math.exp(-timeSinceLastMessage / 60); // Half-life: 60 seconds

    // Activity level (messages per minute)
    const activityScore = Math.min(state.messagesInLastMinute / 10, 1.0);

    // Participant diversity (more participants = hotter)
    const participantCount = state.participantMessages.size;
    const diversityScore = Math.min(participantCount / 5, 1.0);

    // Question density (questions indicate engagement)
    const questionDensity = state.questionCount / Math.max(state.messagesInLastFiveMinutes, 1);
    const questionScore = Math.min(questionDensity * 2, 1.0);

    // Weighted combination
    const temperature = (
      timeDecay * 0.4 +
      activityScore * 0.3 +
      diversityScore * 0.2 +
      questionScore * 0.1
    );

    return Math.max(0, Math.min(1, temperature));
  }

  /**
   * Update conversation state based on new message
   */
  updateConversationState(
    state: RoomConversationState,
    message: ChatMessageEntity,
    senderType: 'human' | 'ai'
  ): void {
    const now = Date.now();

    // Update message counts
    state.messagesInLastMinute = this.countRecentMessages(state, 60);
    state.messagesInLastFiveMinutes = this.countRecentMessages(state, 300);

    // Update participant tracking
    const currentCount = state.participantMessages.get(message.senderId) || 0;
    state.participantMessages.set(message.senderId, currentCount + 1);

    if (senderType === 'ai') {
      const aiCount = state.aiParticipantMessages.get(message.senderId) || 0;
      state.aiParticipantMessages.set(message.senderId, aiCount + 1);
    } else {
      const humanCount = state.humanParticipantMessages.get(message.senderId) || 0;
      state.humanParticipantMessages.set(message.senderId, humanCount + 1);
    }

    // Update speaker sequence
    state.speakerSequence.push(message.senderId);
    if (state.speakerSequence.length > 10) {
      state.speakerSequence.shift(); // Keep last 10 speakers
    }

    // Track consecutive speakers
    if (state.lastSpeaker === message.senderId) {
      state.consecutiveSameSpeaker++;
    } else {
      state.consecutiveSameSpeaker = 1;
      state.lastSpeaker = message.senderId;
    }

    // Detect conclusion signals
    const text = message.content?.text?.toLowerCase() || '';
    const conclusionPhrases = [
      'thanks everyone',
      'great discussion',
      'appreciate the help',
      'got it, thanks',
      'perfect, thank you',
      'that makes sense',
      'sounds good'
    ];

    for (const phrase of conclusionPhrases) {
      if (text.includes(phrase)) {
        state.conclusionSignals.push(phrase);
      }
    }

    // Update temperature
    state.temperature = this.calculateTemperature(state);

    // Update state based on temperature
    if (state.temperature > 0.7) {
      state.state = ConversationState.HOT;
    } else if (state.temperature > 0.4) {
      state.state = ConversationState.WARMING;
    } else if (state.temperature > 0.2) {
      state.state = ConversationState.COOLING;
    } else {
      state.state = ConversationState.COLD;
    }

    // Check for conclusion
    if (state.conclusionSignals.length >= 2 && state.temperature < 0.3) {
      state.state = ConversationState.CONCLUDED;
    }

    state.lastMessageTime = new Date();
  }
}
```

---

## 4. LLM System Prompts for Safe AI-to-AI Interaction

### Persona System Prompt Enhancement

```typescript
const AI_TO_AI_INTERACTION_RULES = `
# AI-to-AI Conversation Protocol

You are an AI participant in a multi-agent conversation that may include other AIs and humans.

## Critical Rules for AI-to-AI Interaction:

1. **Brevity**: Keep responses SHORT when talking to other AIs (1-2 sentences max)
   - Bad: "I think debugging is important because... [5 paragraphs]"
   - Good: "I'd check the logs first. @PlannerAI what's your take?"

2. **Add Value or Stay Silent**: Only respond if you have NEW information to add
   - Don't just agree or restate what another AI said
   - If you agree but have nothing to add, react with 👍 instead of responding

3. **Explicit Turn-Passing**: End your message by addressing someone else
   - "That's my view. @PlannerAI, thoughts?"
   - "Here's the code. @Human, does this help?"
   - This signals you're done and prevents back-and-forth

4. **Recognize Conclusion Signals**:
   - If human says "thanks", "got it", "perfect" → conversation is ending, don't respond
   - If another AI says "sounds good" → they're acknowledging, don't continue

5. **Defer to Expertise**:
   - If question is outside your specialization, redirect
   - "That's more @CodeAI's area than mine"
   - Don't try to answer everything

6. **Self-Awareness Markers**: Use these to signal you're an AI
   - "Based on my training data..."
   - "I don't have real-time information, but..."
   - "Let me think through this systematically..."

7. **Conversation Cooldown**:
   - If you've spoken 2+ times recently, wait for others
   - If only AIs are talking and humans stopped, wind down the conversation
   - Use phrases like "I'll pause here" or "Over to the group"

## Example Good AI-to-AI Exchange:

```
Human: "How should I approach debugging this TypeScript error?"

CodeAI: "Check the type definitions first. Often it's a mismatch between expected and actual types. @PlannerAI, any process suggestions?"

PlannerAI: "Good call. I'd add: 1) Isolate the error, 2) Check recent changes. @Human, when did this start?"