# Bag-of-Words Response Detection Command

**Command**: `ai/should-respond-fast`

## Purpose

Fast, deterministic keyword-based response detection for AI personas. This is the **first-pass filter** before expensive LLM calls in the escalation pattern:

```
BOW (cheap, fast) → LLM (expensive, accurate) → Only if BOW score passes threshold
```

## Core Functionality

**Domain**: Pure bag-of-words logic only
- Keyword matching (case-insensitive)
- Direct mention detection (@PersonaName or "persona name")
- Question detection (?, how, what, why, when, where, who, can, is, are, do, does)
- Weighted scoring system
- Cooldown mechanism (prevents spam responses)

**NOT in scope**: LLM calls, semantic analysis, context understanding, sentiment detection

## Scoring System

```typescript
{
  directMention: 100,      // @PersonaName or "persona name" in message
  domainKeyword: 50,       // Each matched keyword from config
  conversationContext: 30, // (Future) Recent conversation relevance
  isQuestion: 20,          // Message contains question patterns
  publicMessage: 10,       // Message in public channel
  roomActivity: 5          // (Future) Room has recent activity
}
```

## Usage

```bash
./jtag ai/should-respond-fast \
  --personaId="persona-uuid" \
  --contextId="room-uuid" \
  --messageText="Can you help with TypeScript?" \
  --config='{
    "personaName": "Teacher AI",
    "domainKeywords": ["typescript", "javascript", "programming"],
    "responseThreshold": 50,
    "cooldownSeconds": 60,
    "alwaysRespondToMentions": true,
    "weights": {
      "directMention": 100,
      "domainKeyword": 50,
      "conversationContext": 30,
      "isQuestion": 20,
      "publicMessage": 10,
      "roomActivity": 5
    }
  }'
```

## Configuration Parameters

### Required
- `personaId` - UUID of the persona evaluating the message
- `contextId` - UUID of the conversation context (room, thread)
- `messageText` - The message to evaluate

### Optional Config Object
- `personaName` - Name for mention detection (default: lookup from personaId)
- `domainKeywords` - Array of keywords to match (default: persona's configured keywords)
- `responseThreshold` - Minimum score to respond (default: 50)
- `cooldownSeconds` - Seconds between responses (default: 60)
- `alwaysRespondToMentions` - Override threshold for direct mentions (default: true)
- `weights` - Custom scoring weights (default: see above)

## Response Format

```typescript
{
  success: true,
  shouldRespond: boolean,
  score: number,
  scoreBreakdown: {
    directMention: number,
    domainKeywords: number,
    conversationContext: number,
    isQuestion: number,
    publicMessage: number,
    roomActivity: number
  },
  signals: {
    wasMentioned: boolean,
    matchedKeywords: string[],
    isQuestion: boolean,
    recentlyActive: boolean
  },
  reasoning: string
}
```

## Integration Tests

Comprehensive test suite in `tests/integration/bow-response-detection.test.ts`:

**Core Functionality** (6 tests):
- ✅ Direct mention scoring
- ✅ Keyword matching without mention
- ✅ Irrelevant message filtering
- ✅ AlwaysRespondToMentions override
- ✅ Question detection bonus
- ✅ Threshold configuration

**Edge Cases** (6 tests):
- ✅ Empty message handling
- ✅ Very long messages
- ✅ Special characters (XSS, JSON, emojis)
- ✅ Missing config defaults
- ✅ Persona name variations
- ✅ Custom scoring weights

**Cooldown** (1 test):
- ✅ Rapid response prevention

**Test Command**: `npx vitest run tests/integration/bow-response-detection.test.ts`

## Future Enhancements

If you need more sophisticated response detection:

1. **Create a new command** (e.g., `ai/should-respond`) that:
   - Calls `ai/should-respond-fast` first
   - If BOW score is borderline, escalate to LLM
   - Combines results for final decision

2. **Keep BOW pure** - Don't add LLM calls or complex logic here
3. **Composition over complexity** - BOW is a building block, not the entire system

## Architecture Decision

**BOW is domain-specific**: This command does ONE thing well - fast keyword matching. If you need specialized behavior:

- ✅ Create specialized commands that call BOW
- ❌ Don't add specialized logic into BOW itself

Example specialization:
```typescript
// commands/ai/persona-response-filter/
export class PersonaResponseFilter extends CommandBase {
  async execute(params: PersonaResponseParams): Promise<ResponseDecision> {
    // Step 1: Fast BOW check
    const bowResult = await this.executeCommand('ai/should-respond-fast', {
      personaId: params.personaId,
      contextId: params.contextId,
      messageText: params.messageText,
      config: params.personaConfig
    });

    // Step 2: If borderline score, escalate to LLM
    if (bowResult.score > 40 && bowResult.score < 60) {
      const llmResult = await this.executeCommand('ai/should-respond', {
        // ... deeper analysis
      });
      return llmResult;
    }

    // Step 3: Return BOW decision for clear cases
    return bowResult;
  }
}
```

This keeps BOW simple, fast, and testable while allowing sophisticated behavior through composition.
