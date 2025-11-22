# Phase 2: Extend RAG Context with Learning Mode

## Status: READY TO START

**Phase 1 Complete** ✅:
- Added learning mode fields to `RoomMember` interface (system/data/entities/RoomEntity.ts:29-39)
- Fields: `learningMode?`, `genomeId?`, `participantRole?`
- Backwards compatible (all optional)
- Type check passes
- System deploys successfully

## Phase 2 Goals

**Make learning mode available to prompt adapters WITHOUT changing behavior yet.**

The fields exist in the data model, now we need to:
1. Include them in RAG context
2. Pass them to prompt adapters
3. Verify they're accessible (but not yet used)

## Implementation Steps

### Step 1: Extend RAGContext Type

**File**: `system/rag/shared/RAGTypes.ts`

**Changes**:
```typescript
export interface RAGContext {
  // ... existing fields

  // NEW: Learning mode configuration (Phase 2)
  learningMode?: 'fine-tuning' | 'inference-only';
  genomeId?: UUID;
  participantRole?: string;
}
```

**Testing**:
```bash
npx tsc --noEmit  # Verify compilation
```

### Step 2: Load Learning Config in ChatRAGBuilder

**File**: `system/rag/builders/ChatRAGBuilder.ts`

**Add Method**:
```typescript
/**
 * Load learning configuration for persona from room membership
 */
private async loadLearningConfig(
  roomId: UUID,
  personaId: UUID
): Promise<{ learningMode?: 'fine-tuning' | 'inference-only'; genomeId?: UUID; participantRole?: string } | undefined> {
  // Load room entity
  const room = await RoomEntity.findById(roomId);
  if (!room) return undefined;

  // Find this persona's membership
  const member = room.members.find(m => m.userId === personaId);
  if (!member) return undefined;

  // Return learning config if present
  return {
    learningMode: member.learningMode,
    genomeId: member.genomeId,
    participantRole: member.participantRole
  };
}
```

**Integrate into buildContext()**:
```typescript
async buildContext(
  contextId: UUID,
  personaId: UUID,
  options?: RAGBuildOptions
): Promise<RAGContext> {
  // ... existing context building

  // NEW: Load learning configuration
  const learningConfig = await this.loadLearningConfig(contextId, personaId);

  return {
    ...existingContext,
    learningMode: learningConfig?.learningMode,
    genomeId: learningConfig?.genomeId,
    participantRole: learningConfig?.participantRole
  };
}
```

**Testing**:
```bash
# 1. Compile
npx tsc --noEmit

# 2. Deploy
npm start

# 3. Add log to verify learning mode is loaded
# In ChatRAGBuilder.ts buildContext():
console.log('🧠 RAG Context Learning Mode:', learningConfig?.learningMode ?? 'not set');

# 4. Trigger message in chat
./jtag debug/chat-send --roomId="<ID>" --message="Test learning mode loading"

# 5. Check logs
./jtag debug/logs --filterPattern="RAG Context Learning Mode" --tailLines=10

# Expected: "🧠 RAG Context Learning Mode: not set" (no members have learning mode yet)
```

### Step 3: Extend Prompt Context Types

**File**: `system/recipes/shared/RecipePromptBuilder.ts`

**Changes**:
```typescript
/**
 * Base prompt context - shared across all adapters
 */
export interface BasePromptContext {
  readonly personaName: string;
  readonly roomContext: RAGContext;
  readonly conversationPattern: ConversationPattern;

  // NEW: Learning configuration (from RAG context)
  readonly learningMode?: 'fine-tuning' | 'inference-only';
  readonly genomeId?: UUID;
  readonly participantRole?: string;
}
```

**Update Adapter Calls**:
```typescript
// GatingPromptAdapter.buildPrompt()
buildPrompt(strategy: RecipeStrategy, context: GatingPromptContext): string {
  // Context now includes learningMode, genomeId, participantRole
  // (Not used yet, but accessible)

  const sections: readonly string[] = [
    // ... existing sections
  ];

  return sections.join('\n\n');
}
```

**Testing**:
```bash
# 1. Compile
npx tsc --noEmit

# 2. Run prompt builder tests
npx vitest run system/recipes/test/unit/RecipePromptBuilder.test.ts

# All tests should pass (no behavior change)
```

### Step 4: Verification

**Create test room with learning mode**:
```bash
# This command doesn't exist yet, but shows the goal:
./jtag data/update --collection=rooms --id="<ROOM_ID>" --data='{
  "members": [
    {
      "userId": "persona-1",
      "role": "member",
      "joinedAt": "2025-10-23T00:00:00Z",
      "learningMode": "fine-tuning",
      "genomeId": "test-genome-id",
      "participantRole": "student"
    }
  ]
}'

# Or manually update in database for now
```

**Verify in logs**:
```bash
# Send message to room
./jtag debug/chat-send --roomId="<ID>" --message="Learning mode test"

# Check RAG context includes learning mode
./jtag debug/logs --filterPattern="RAG Context" --tailLines=20

# Expected: Learning mode fields present in context
```

## Success Criteria

✅ **Phase 2 Complete When**:
1. RAGContext type includes learning mode fields
2. ChatRAGBuilder loads learning config from room members
3. BasePromptContext includes learning fields
4. Type check passes
5. All existing tests pass
6. Logs show learning mode being loaded (even if undefined)
7. No behavior change (fields present but not used)

## Commit Message Template

```
Extend RAG context with learning mode (Phase 2 - no behavior change)

- Added learningMode, genomeId, participantRole to RAGContext
- ChatRAGBuilder loads learning config from room membership
- BasePromptContext includes learning fields for adapters
- All fields optional, defaults to undefined
- No behavior change (fields accessible but not used yet)
- Type check passes, all tests pass

Phase 2 of Learning Mode Architecture implementation
```

## Next Phase Preview (Phase 3)

**Goal**: Adapters USE learning mode to customize prompts

**Example**: Add meta-learning sections for fine-tuning participants
```typescript
if (context.learningMode === 'fine-tuning') {
  sections.push(PromptSectionBuilder.buildMetaLearningSection(context.participantRole));
}
```

This is when behavior actually changes - personas in fine-tuning mode get different prompts.
