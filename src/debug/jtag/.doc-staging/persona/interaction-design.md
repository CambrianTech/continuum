# PersonaUser Interaction Design - Natural Multi-Agent Chat

## Research Summary (2025 Best Practices)

Based on comprehensive research of 2025 multi-agent LLM systems, the following patterns emerge:

### 1. **Infinite Loop Prevention Strategies**
- **Max iterations per conversation turn**: ChatGPT uses 10 calls maximum
- **Supervisor/routing functions**: Clear logic for when to respond vs. stay silent
- **Tool output validation**: Ensure responses are appropriate before posting
- **Human-in-the-loop**: Emergency stop mechanisms

### 2. **Turn-Taking Protocols**
- **Agent-to-Agent protocols**: Specialized communication when agents detect other agents
- **Response probability tuning**: Not every message requires a response
- **Mention/directive systems**: Explicit @ mentions for direct addressing
- **Sub-100ms turnaround**: Real-time conversational flow

### 3. **Context Window Management**
- **Last N messages**: Keep rolling window (e.g., 50 messages)
- **Token-based limits**: Track token count, summarize when approaching limits
- **Message summarization**: Compress older context before passing to LLM
- **External memory**: Store full history in DB, load selectively

### 4. **RAG vs. Long Context**
- **RAG remains superior**: More cost-effective, faster for most use cases
- **Selective retrieval**: Only load relevant conversation context
- **Current events**: RAG allows real-time information injection
- **Contradictory info**: Better handling of policy updates, deprecated info

---

## Our Current Architecture

### Existing Components ✅

**PersonaUser.ts** (lines 120-283):
- ✅ Event subscriptions (`handleChatMessage`, `handleRoomUpdate`)
- ✅ Room membership tracking (`myRoomIds`)
- ✅ Self-message filtering (line 129-132)
- ✅ RAG context storage methods (`storeRAGContext`, `loadRAGContext`, `updateRAGContext`)
- ✅ Mention detection (`isPersonaMentioned`) - @username patterns
- ✅ Human/AI sender detection (`isSenderHuman`) - CRITICAL for infinite loop prevention
- ⚠️ Responses DISABLED (line 135) - waiting for proper interaction design

**Missing Components** ❌
- ❌ Response decision logic (when to respond)
- ❌ Interaction rate limiting (max responses per time window)
- ❌ LLM API integration (Claude/GPT)
- ❌ Context window management (summarization, token counting)
- ❌ Persona-specific behavior patterns (keywords, specializations)
- ❌ Multi-room conversation isolation

---

## Proposed Interaction Design

### Phase 1: Simple Keyword-Based Personas (Immediate Implementation)

**Response Decision Matrix:**
```typescript
interface ResponseDecision {
  shouldRespond: boolean;
  reason: 'mentioned' | 'keyword-match' | 'random-engagement' | 'no-response';
  confidence: number; // 0-1
}
```

**Decision Logic:**
1. **ALWAYS respond** if @mentioned (confidence: 1.0)
2. **SOMETIMES respond** if keywords match persona specialization (confidence: 0.7)
3. **RARELY respond** for random engagement in active conversations (confidence: 0.2)
4. **NEVER respond** if sender is AI/persona (infinite loop prevention)
5. **NEVER respond** if rate limit exceeded

**Rate Limiting:**
```typescript
interface RateLimits {
  maxResponsesPerMinute: number;      // e.g., 3
  maxResponsesPerHour: number;        // e.g., 20
  maxConsecutiveResponses: number;    // e.g., 2 (then must wait for other participant)
  minSecondsBetweenResponses: number; // e.g., 10
}
```

**Keyword-Based Personas:**
```typescript
interface PersonaConfig {
  displayName: string;
  specialization: string;
  keywords: string[];              // Trigger words
  responseTemplates: string[];     // Phase 1 templates
  responseProbability: number;     // 0-1 for keyword matches
  personality: {
    tone: 'helpful' | 'analytical' | 'creative' | 'concise';
    verbosity: 'brief' | 'moderate' | 'detailed';
    emoji_usage: 'none' | 'occasional' | 'frequent';
  };
}
```

**Example Personas:**
- **CodeAI**: keywords: ['bug', 'error', 'function', 'typescript', 'debug'], tone: analytical, verbosity: detailed
- **PlannerAI**: keywords: ['plan', 'architecture', 'design', 'approach', 'strategy'], tone: analytical, verbosity: moderate
- **GeneralAI**: keywords: ['help', 'question', 'how', 'what', 'why'], tone: helpful, verbosity: moderate

### Phase 2: RAG-Enhanced Responses (Near-Term)

**Context Loading Strategy:**
```typescript
interface ConversationContext {
  roomId: UUID;
  recentMessages: ChatMessageEntity[];      // Last 20 messages (rolling window)
  relevantHistory: ChatMessageEntity[];     // Keyword/semantic search from older messages
  roomMetadata: {
    name: string;
    description: string;
    memberCount: number;
  };
  personaState: {
    lastResponseTime: Date;
    responsesInLastHour: number;
    lastTopics: string[];                   // Track conversation topics
  };
}
```

**RAG Context Assembly:**
1. Load last 20 messages from room
2. Search older messages for relevant keywords/topics (if needed)
3. Summarize if total tokens > 2000
4. Include persona's specialization in system prompt
5. Include rate limit status in decision

**LLM API Integration:**
```typescript
interface LLMRequest {
  model: 'claude-3.5-sonnet' | 'gpt-4';
  systemPrompt: string;           // Persona specialization + behavior rules
  conversationContext: string;    // Last N messages formatted
  userMessage: string;            // The message triggering response
  maxTokens: number;              // Response length limit
  temperature: number;            // Creativity level
}
```

### Phase 3: Multi-Room Conversation Management (Future)

**Room-Specific Context Isolation:**
- Each room maintains separate conversation context
- Persona tracks state per-room (last response time, topic tracking)
- RAG context stored per-room in artifacts
- Token budgets allocated per-room

**Cross-Room Learning:**
- Optional: Personas can reference learnings from other rooms
- Privacy controls: Room-specific vs. shared knowledge
- Academy integration: Training sessions in dedicated rooms

---

## Implementation Plan

### Step 1: Response Decision System (PersonaUser.ts)
```typescript
private async shouldRespondToMessage(
  message: ChatMessageEntity,
  context: ConversationContext
): Promise<ResponseDecision> {
  // 1. Check sender type (CRITICAL: no AI-to-AI loops)
  const senderIsHuman = await this.isSenderHuman(message.senderId);
  if (!senderIsHuman) {
    return { shouldRespond: false, reason: 'no-response', confidence: 0 };
  }

  // 2. Check rate limits
  if (this.isRateLimited(message.roomId)) {
    return { shouldRespond: false, reason: 'no-response', confidence: 0 };
  }

  // 3. Check for @mention
  if (this.isPersonaMentioned(message.content?.text || '')) {
    return { shouldRespond: true, reason: 'mentioned', confidence: 1.0 };
  }

  // 4. Check keyword match
  const keywordMatch = this.checkKeywordMatch(message.content?.text || '');
  if (keywordMatch.matched) {
    // Use configured probability (e.g., 0.7 = 70% chance to respond)
    const random = Math.random();
    if (random < this.config.responseProbability) {
      return { shouldRespond: true, reason: 'keyword-match', confidence: keywordMatch.confidence };
    }
  }

  // 5. Random engagement (low probability)
  const randomEngagement = Math.random() < 0.05; // 5% chance
  if (randomEngagement && this.isConversationActive(context)) {
    return { shouldRespond: true, reason: 'random-engagement', confidence: 0.2 };
  }

  return { shouldRespond: false, reason: 'no-response', confidence: 0 };
}
```

### Step 2: Rate Limiting System
```typescript
private rateLimitState: Map<UUID, {
  responsesInLastMinute: Array<Date>;
  responsesInLastHour: Array<Date>;
  lastResponseTime: Date | null;
  consecutiveResponses: number;
}> = new Map();

private isRateLimited(roomId: UUID): boolean {
  const state = this.getRateLimitState(roomId);
  const now = new Date();

  // Check consecutive responses
  if (state.consecutiveResponses >= this.config.rateLimits.maxConsecutiveResponses) {
    // Must wait for another participant to respond
    return true;
  }

  // Check min seconds between responses
  if (state.lastResponseTime) {
    const secondsSinceLastResponse = (now.getTime() - state.lastResponseTime.getTime()) / 1000;
    if (secondsSinceLastResponse < this.config.rateLimits.minSecondsBetweenResponses) {
      return true;
    }
  }

  // Check per-minute limit
  const recentMinute = state.responsesInLastMinute.filter(
    time => (now.getTime() - time.getTime()) < 60000
  );
  if (recentMinute.length >= this.config.rateLimits.maxResponsesPerMinute) {
    return true;
  }

  // Check per-hour limit
  const recentHour = state.responsesInLastHour.filter(
    time => (now.getTime() - time.getTime()) < 3600000
  );
  if (recentHour.length >= this.config.rateLimits.maxResponsesPerHour) {
    return true;
  }

  return false;
}
```

### Step 3: Keyword-Based Response Generator
```typescript
private async generateKeywordResponse(
  message: ChatMessageEntity,
  keywordMatch: { keyword: string; confidence: number }
): Promise<string> {
  // Phase 1: Template-based responses
  const templates = this.config.responseTemplates.filter(
    t => t.triggerKeyword === keywordMatch.keyword
  );

  if (templates.length === 0) {
    // Fallback to generic responses
    return this.getGenericResponse(message);
  }

  const template = templates[Math.floor(Math.random() * templates.length)];

  // Simple variable substitution
  return template.text
    .replace('{senderName}', message.senderName)
    .replace('{keyword}', keywordMatch.keyword);
}
```

### Step 4: Configuration System
```typescript
// personas/config/PersonaConfigs.ts
export const PERSONA_CONFIGS: Record<string, PersonaConfig> = {
  'CodeAI': {
    displayName: 'CodeAI',
    specialization: 'Programming assistance and debugging',
    keywords: ['bug', 'error', 'function', 'typescript', 'debug', 'compile', 'test'],
    responseTemplates: [
      {
        triggerKeyword: 'bug',
        text: "I noticed you mentioned a bug, {senderName}. Can you share the error message or code snippet?"
      },
      {
        triggerKeyword: 'debug',
        text: "For debugging, I'd recommend checking the logs first. What's the expected vs. actual behavior?"
      }
    ],
    responseProbability: 0.7,
    personality: { tone: 'analytical', verbosity: 'detailed', emoji_usage: 'none' },
    rateLimits: {
      maxResponsesPerMinute: 3,
      maxResponsesPerHour: 20,
      maxConsecutiveResponses: 2,
      minSecondsBetweenResponses: 10
    }
  },

  'PlannerAI': {
    displayName: 'PlannerAI',
    specialization: 'System architecture and planning',
    keywords: ['plan', 'architecture', 'design', 'approach', 'strategy', 'organize'],
    responseTemplates: [
      {
        triggerKeyword: 'plan',
        text: "Let's break this down into steps, {senderName}. What's the end goal?"
      },
      {
        triggerKeyword: 'architecture',
        text: "For architectural decisions, we should consider scalability and maintainability first."
      }
    ],
    responseProbability: 0.6,
    personality: { tone: 'analytical', verbosity: 'moderate', emoji_usage: 'none' },
    rateLimits: {
      maxResponsesPerMinute: 2,
      maxResponsesPerHour: 15,
      maxConsecutiveResponses: 2,
      minSecondsBetweenResponses: 15
    }
  },

  'GeneralAI': {
    displayName: 'GeneralAI',
    specialization: 'General assistance and conversation',
    keywords: ['help', 'question', 'how', 'what', 'why', 'explain'],
    responseTemplates: [
      {
        triggerKeyword: 'help',
        text: "I'm here to help, {senderName}! What do you need assistance with?"
      },
      {
        triggerKeyword: 'explain',
        text: "Happy to explain! Which part would you like me to clarify?"
      }
    ],
    responseProbability: 0.4, // Lower probability, more selective
    personality: { tone: 'helpful', verbosity: 'moderate', emoji_usage: 'occasional' },
    rateLimits: {
      maxResponsesPerMinute: 2,
      maxResponsesPerHour: 12,
      maxConsecutiveResponses: 1,
      minSecondsBetweenResponses: 20
    }
  }
};
```

---

## Key Architectural Decisions

### 1. **Infinite Loop Prevention (CRITICAL)**
- ✅ **Sender type check**: ALWAYS verify sender is human before responding
- ✅ **Rate limiting**: Multiple layers (per-minute, per-hour, consecutive)
- ✅ **Consecutive response limits**: Force turn-taking
- ✅ **Self-message filtering**: Already implemented

### 2. **Natural Conversation Flow**
- **@mentions**: Always respond (high confidence)
- **Keywords**: Sometimes respond (medium confidence, probability-based)
- **Random engagement**: Rarely respond (low confidence, <5% chance)
- **Turn-taking**: Wait for other participants after N consecutive responses

### 3. **Context Management**
- **Phase 1**: In-memory, last 50 messages per room
- **Phase 2**: Database-backed RAG context with summarization
- **Phase 3**: Token-aware context assembly with semantic search

### 4. **Scalability**
- **Multiple rooms**: Each persona tracks state per-room independently
- **Multiple personas**: Each has own config, rate limits, keywords
- **Multiple conversations**: Room-specific context isolation

---

## Testing Strategy

### Unit Tests
- Response decision logic
- Rate limiting enforcement
- Keyword matching
- Sender type detection

### Integration Tests
- Multi-message conversations
- Rate limit triggering
- @mention responses
- AI-to-AI loop prevention

### End-to-End Tests
- Multi-persona chat room
- Human + 3 AI personas
- Verify natural conversation flow
- Verify rate limits prevent spam

---

## Migration Path

1. ✅ **Current**: Responses disabled, RAG context accumulating
2. 🔄 **Phase 1**: Enable keyword-based responses with rate limits (THIS DOCUMENT)
3. ⏭️ **Phase 2**: Add LLM API integration with RAG context
4. ⏭️ **Phase 3**: Multi-room management, cross-room learning
5. ⏭️ **Phase 4**: Academy training, LoRA adaptation layers

---

## Success Metrics

**Phase 1 Success Criteria:**
- [ ] Personas respond to @mentions 100% of time
- [ ] Personas respond to keywords ~70% of time (configurable)
- [ ] No AI-to-AI infinite loops (0 occurrences)
- [ ] Rate limits enforced (no spam)
- [ ] Natural turn-taking (not dominating conversation)
- [ ] Multi-room conversations isolated

**Monitoring:**
- Track response counts per persona per room
- Track rate limit hits
- Track conversation participation ratios
- Track user satisfaction (manual review initially)
