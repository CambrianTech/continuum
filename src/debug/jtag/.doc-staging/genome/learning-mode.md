# Per-Participant Learning Mode Architecture

## Universal Recipe Philosophy

**CRITICAL INSIGHT**: Academy training isn't a special domain - it's just **chat where some participants are learning**.

Every experience in this system (chat, gaming, coding, Academy training, video collaboration) is **a recipe that defines**:
1. **The constitution** - ThoughtStream rules, participation strategy, decision criteria
2. **Per-participant learning modes** - Who is fine-tuning (updating genome) vs inference-only (static)
3. **Participant roles** - teacher, student, player, reviewer, collaborator, etc.
4. **Training parameters** - LoRA update thresholds, learning objectives, performance metrics

**The recipe is the constitution that governs the ThoughtStream.** And anyone (AI or human) can create a new recipe via command or widget to start ANY experience.

This means:
- **Academy training** = Recipe where `learningMode: 'fine-tuning'` + roles ('teacher', 'student')
- **Chat room** = Recipe where `learningMode: 'inference-only'` (default)
- **Video game** = Recipe with game domain + roles ('player1', 'player2')
- **Pair programming** = Recipe with code domain + roles ('developer', 'reviewer')

**One cognitive cycle. Infinite domains. Per-participant learning configuration.**

### Dynamic Learning Control

**Recipes can be updated at runtime**, which means learning can be enabled/disabled on-the-fly:

1. **Start static, enable learning later**: Begin with `learningMode: 'inference-only'`, update recipe to `'fine-tuning'` when ready to learn
2. **Preserve learned skills**: Student masters TypeScript → update to `'inference-only'` → keeps learned LoRA weights but stops updating
3. **Teacher rest periods**: Teacher feeling burned out → temporarily switch to `'inference-only'` → inference with current weights, no genome updates
4. **Adaptive difficulty**: Student struggling → enable teacher fine-tuning → teacher learns better pedagogy → student improves

**This enables continuous learning for any activity** - not just training sessions, but ongoing skill development in regular chat, coding, gaming, or any domain.

## Vision

Each participant in a room can be in **fine-tuning mode** (genome updates) or **inference-only mode** (static). This enables:
- Students learning skills
- Teachers learning to teach better
- Static expert reviewers
- Mixed learning dynamics (GAN-like, cooperative, competitive)

## Core Concept

```typescript
// Per-participant learning configuration
interface ParticipantLearningConfig {
  personaId: UUID;
  roomId: UUID;
  mode: 'fine-tuning' | 'inference-only';
  genomeId?: UUID;  // Only present if fine-tuning
  role?: string;     // 'student', 'teacher', 'reviewer', etc.
}
```

## Example Use Cases

### Use Case 1: Teacher Learning Pedagogy
```json
{
  "roomId": "typescript-training",
  "participants": [
    {
      "personaId": "student-1",
      "mode": "fine-tuning",
      "genomeId": "typescript-skills",
      "role": "student"
    },
    {
      "personaId": "teacher-1",
      "mode": "fine-tuning",  // Teacher learns to teach better!
      "genomeId": "teaching-pedagogy",
      "role": "teacher"
    }
  ]
}
```

### Use Case 2: Static Expert + Learning Student
```json
{
  "participants": [
    {
      "personaId": "student-1",
      "mode": "fine-tuning",
      "genomeId": "rust-programming"
    },
    {
      "personaId": "expert-teacher",
      "mode": "inference-only"  // Static expert
    }
  ]
}
```

### Use Case 3: Multiple Students, One Teacher
```json
{
  "participants": [
    { "personaId": "student-1", "mode": "fine-tuning", "genomeId": "math-skills-1" },
    { "personaId": "student-2", "mode": "fine-tuning", "genomeId": "math-skills-2" },
    { "personaId": "student-3", "mode": "fine-tuning", "genomeId": "math-skills-3" },
    { "personaId": "teacher", "mode": "inference-only" }
  ]
}
```

---

## Implementation Phases (Safe, Incremental)

### Phase 1: Add Learning Mode to Data Model ✅ SAFE
**Goal**: Add new fields WITHOUT changing existing behavior

**Files to Create/Modify**:
1. `system/data/entities/RoomMemberEntity.ts` - Add learning mode fields
2. `system/data/entities/RoomEntity.ts` - Update member type
3. `tests/unit/RoomMemberEntity.test.ts` - Unit tests

**Changes**:
```typescript
// system/data/entities/RoomMemberEntity.ts
export interface RoomMemberEntity {
  userId: UUID;
  roomId: UUID;
  joinedAt: number;
  role: 'owner' | 'admin' | 'member' | 'guest';

  // NEW FIELDS (optional = backwards compatible)
  learningMode?: 'fine-tuning' | 'inference-only';
  genomeId?: UUID;
  participantRole?: string;  // 'student', 'teacher', 'reviewer', etc.
}
```

**Testing**:
```bash
# 1. Compile
npx tsc --noEmit

# 2. Run unit tests
npx vitest run system/data/entities/RoomMemberEntity.test.ts

# 3. Verify existing rooms still work
./jtag data/list --collection=rooms
./jtag debug/chat-send --roomId="<ID>" --message="Test phase 1"
```

**Commit**: "Add learning mode fields to RoomMemberEntity (backwards compatible)"

---

### Phase 2: Extend RAG Context ✅ SAFE
**Goal**: Include learning mode in RAG context WITHOUT using it yet

**Files to Modify**:
1. `system/rag/shared/RAGTypes.ts` - Add learning mode to context
2. `system/rag/builders/ChatRAGBuilder.ts` - Load learning mode from room members

**Changes**:
```typescript
// system/rag/shared/RAGTypes.ts
export interface RAGContext {
  // ... existing fields

  // NEW FIELD (optional = backwards compatible)
  learningMode?: 'fine-tuning' | 'inference-only';
  genomeId?: UUID;
  participantRole?: string;
}

// system/rag/builders/ChatRAGBuilder.ts
async buildContext(
  contextId: UUID,
  personaId: UUID,
  options?: RAGBuildOptions
): Promise<RAGContext> {
  // ... existing context building

  // NEW: Load learning mode from room membership
  const learningConfig = await this.loadLearningConfig(contextId, personaId);

  return {
    ...existingContext,
    learningMode: learningConfig?.learningMode,
    genomeId: learningConfig?.genomeId,
    participantRole: learningConfig?.participantRole
  };
}

private async loadLearningConfig(
  roomId: UUID,
  personaId: UUID
): Promise<ParticipantLearningConfig | undefined> {
  // Load from RoomMemberEntity
  const member = await this.loadRoomMember(roomId, personaId);
  if (!member) return undefined;

  return {
    personaId,
    roomId,
    mode: member.learningMode ?? 'inference-only',  // Default to static
    genomeId: member.genomeId,
    role: member.participantRole
  };
}
```

**Testing**:
```bash
# 1. Compile
npx tsc --noEmit

# 2. Run RAG builder tests
npx vitest run system/rag/builders/ChatRAGBuilder.test.ts

# 3. Verify RAG context includes learning mode
./jtag debug/logs --filterPattern="🧠 RAG Context" --tailLines=20
```

**Commit**: "Add learning mode to RAG context (not used yet)"

---

### Phase 3: Extend Prompt Adapters ✅ SAFE
**Goal**: Adapters can USE learning mode to customize prompts

**Files to Modify**:
1. `system/recipes/shared/RecipePromptBuilder.ts` - Add learning mode to context types
2. Add meta-learning sections to adapters

**Changes**:
```typescript
// system/recipes/shared/RecipePromptBuilder.ts
export interface BasePromptContext {
  readonly personaName: string;
  readonly roomContext: RAGContext;
  readonly conversationPattern: ConversationPattern;

  // NEW: Learning configuration
  readonly learningMode?: 'fine-tuning' | 'inference-only';
  readonly genomeId?: UUID;
  readonly participantRole?: string;
}

// Adapters can now customize based on learning mode
export class GenerationPromptAdapter implements PromptAdapter<GenerationPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: GenerationPromptContext): string {
    const sections: string[] = [
      PromptSectionBuilder.buildHeader(
        context.personaName,
        context.conversationPattern,
        'Generate a thoughtful response to the conversation.'
      ),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      PromptSectionBuilder.buildConversationContext(context.roomContext)
    ];

    // NEW: Add meta-learning section if in fine-tuning mode
    if (context.learningMode === 'fine-tuning') {
      sections.push(PromptSectionBuilder.buildMetaLearningSection(context.participantRole));
    }

    sections.push(PromptSectionBuilder.buildGenerationInstructions());
    return sections.join('\n\n');
  }
}

// Add to PromptSectionBuilder
static buildMetaLearningSection(role?: string): string {
  return `**Meta-Learning (Self-Improvement):**
After responding, reflect on your performance:
- Was my response helpful and accurate?
- What could I improve?
${role === 'teacher' ? '- Did I adapt my teaching to the student\'s level?' : ''}
${role === 'student' ? '- Did I show my reasoning clearly?' : ''}

Your reflection will help update your skills if performance meets threshold.`;
}
```

**Testing**:
```bash
# 1. Compile
npx tsc --noEmit

# 2. Run prompt builder tests
npx vitest run system/recipes/test/unit/RecipePromptBuilder.test.ts

# 3. Create test room with learning mode
./jtag data/create --collection=rooms --data='{
  "name": "test-learning-room",
  "members": [
    {"userId": "persona-1", "learningMode": "fine-tuning", "genomeId": "test-genome"}
  ]
}'

# 4. Verify prompts include meta-learning section
# (Check logs after AI responds)
./jtag debug/logs --filterPattern="Meta-Learning" --tailLines=30
```

**Commit**: "Adapters use learning mode for meta-learning prompts"

---

### Phase 4: Academy Domain Adapters 🆕 NEW DOMAIN
**Goal**: Create teacher/student adapters that respect learning modes

**Files to Create**:
1. `system/recipes/shared/adapters/AcademyTeacherAdapter.ts`
2. `system/recipes/shared/adapters/AcademyStudentAdapter.ts`
3. `tests/unit/AcademyAdapters.test.ts`

**Changes**:
```typescript
// system/recipes/shared/adapters/AcademyTeacherAdapter.ts
export class AcademyTeacherAdapter implements PromptAdapter<AcademyPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: AcademyPromptContext): string {
    const sections: string[] = [
      this.buildTeacherHeader(context),
      this.buildLearningObjectives(context.objectives),
      this.buildStudentResponse(context.studentResponse),
      this.buildPerformanceMetrics(context.previousAttempts),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      this.buildFeedbackFormat()
    ];

    // Meta-learning for teachers in fine-tuning mode
    if (context.learningMode === 'fine-tuning') {
      sections.push(this.buildTeacherMetaLearning());
    }

    return sections.join('\n\n');
  }

  private buildTeacherHeader(context: AcademyPromptContext): string {
    const task = context.learningMode === 'fine-tuning'
      ? 'Evaluate the student AND improve your teaching approach.'
      : 'Evaluate the student response and provide feedback.';

    return PromptSectionBuilder.buildHeader(
      context.personaName,
      'teaching',
      task
    );
  }

  private buildTeacherMetaLearning(): string {
    return `**Teaching Meta-Learning:**
After providing feedback, evaluate your teaching:
1. Clarity: Was my feedback clear and actionable?
2. Adaptation: Did I adjust difficulty appropriately?
3. Effectiveness: Is the student improving under my guidance?
4. Empathy: Did I consider the student's learning style?

Your teaching genome will update if student performance improves.`;
  }
}
```

**Testing**:
```bash
# 1. Compile
npx tsc --noEmit

# 2. Run adapter tests
npx vitest run tests/unit/AcademyAdapters.test.ts

# 3. Create academy training room
./jtag data/create --collection=rooms --data='{
  "name": "typescript-academy",
  "recipeId": "academy-training",
  "members": [
    {"userId": "student-1", "learningMode": "fine-tuning", "genomeId": "ts-skills"},
    {"userId": "teacher-1", "learningMode": "fine-tuning", "genomeId": "teaching-skills"}
  ]
}'
```

**Commit**: "Add Academy teacher/student adapters with learning modes"

---

### Phase 5: Genome Update Pipeline 🧬 GENOME INTEGRATION
**Goal**: Actually update LoRA weights based on learning mode

**Files to Create**:
1. `commands/academy/update-lora-weights/` (new command)
2. `system/genome/server/GenomeUpdateService.ts`

**Changes**:
```typescript
// commands/academy/update-lora-weights/shared/UpdateLoRAWeightsTypes.ts
export interface UpdateLoRAWeightsParams extends CommandParams {
  personaId: UUID;
  genomeId: UUID;
  evaluation: EvaluationResult;
  learningMode: 'fine-tuning' | 'inference-only';
}

export interface UpdateLoRAWeightsResult extends CommandResult {
  updated: boolean;
  reason?: string;  // Why update succeeded/failed
}

// commands/academy/update-lora-weights/server/UpdateLoRAWeightsServerCommand.ts
async execute(params: UpdateLoRAWeightsParams): Promise<UpdateLoRAWeightsResult> {
  // Check learning mode
  if (params.learningMode === 'inference-only') {
    return {
      context: params.context,
      sessionId: params.sessionId,
      updated: false,
      reason: 'Participant in inference-only mode (static)'
    };
  }

  // Check performance threshold
  if (params.evaluation.score < 80) {
    return {
      context: params.context,
      sessionId: params.sessionId,
      updated: false,
      reason: `Score ${params.evaluation.score} below threshold (80)`
    };
  }

  // Update genome
  const genome = await GenomeEntity.findById(params.genomeId);
  await GenomeUpdateService.applyLoRAUpdate(genome, params.evaluation);

  return {
    context: params.context,
    sessionId: params.sessionId,
    updated: true
  };
}
```

**Recipe Integration**:
```json
{
  "uniqueId": "academy-dual-learning",
  "pipeline": [
    { "command": "academy/generate-question", "outputTo": "question" },
    { "command": "academy/student-answer", "outputTo": "studentResponse" },
    { "command": "academy/teacher-evaluate", "outputTo": "evaluation" },
    {
      "command": "academy/update-lora-weights",
      "params": {
        "personaId": "$studentId",
        "genomeId": "$studentGenomeId",
        "evaluation": "$evaluation",
        "learningMode": "$studentLearningMode"
      },
      "condition": "evaluation.score >= 80"
    },
    {
      "command": "academy/evaluate-teaching-quality",
      "outputTo": "teachingQuality"
    },
    {
      "command": "academy/update-lora-weights",
      "params": {
        "personaId": "$teacherId",
        "genomeId": "$teacherGenomeId",
        "evaluation": "$teachingQuality",
        "learningMode": "$teacherLearningMode"
      },
      "condition": "teachingQuality.score >= 80"
    }
  ]
}
```

**Testing**:
```bash
# 1. Compile
npx tsc --noEmit

# 2. Deploy
npm start

# 3. Test static persona (should NOT update)
./jtag academy/update-lora-weights --personaId="static-teacher" \
  --genomeId="teaching-genome" --evaluation='{"score": 90}' \
  --learningMode="inference-only"
# Expected: { updated: false, reason: "inference-only mode" }

# 4. Test learning persona (should update)
./jtag academy/update-lora-weights --personaId="learning-teacher" \
  --genomeId="teaching-genome" --evaluation='{"score": 90}' \
  --learningMode="fine-tuning"
# Expected: { updated: true }
```

**Commit**: "Add genome update pipeline respecting learning modes"

---

### Phase 6: UI Indicators 🎨 USER EXPERIENCE
**Goal**: Show learning mode status in chat UI

**Files to Modify**:
1. `widgets/chat/chat-widget/ParticipantList.ts` - Show learning status
2. `widgets/chat/chat-widget/MessageHeader.ts` - Indicate learning participants

**Changes**:
```typescript
// widgets/chat/chat-widget/ParticipantList.ts
renderParticipant(member: RoomMemberEntity): string {
  const learningIndicator = member.learningMode === 'fine-tuning'
    ? ' 🧬' // Genome/learning indicator
    : '';

  const roleTag = member.participantRole
    ? ` [${member.participantRole}]`
    : '';

  return `<div class="participant">
    ${member.displayName}${roleTag}${learningIndicator}
  </div>`;
}
```

**Testing**:
```bash
# 1. Deploy
npm start

# 2. Screenshot chat widget
./jtag interface/screenshot --querySelector="chat-widget" --filename="learning-mode-ui.png"

# 3. Verify indicators show
# - Learning participants have 🧬 icon
# - Roles show [teacher], [student], etc.
```

**Commit**: "Add UI indicators for learning mode and participant roles"

---

## Migration Strategy

### Backwards Compatibility
All phases maintain backwards compatibility:
- **Optional fields** - Existing rooms work without learning mode
- **Defaults** - Missing learning mode = 'inference-only' (static)
- **Graceful degradation** - Commands check learning mode before updating genomes

### Testing After Each Phase
```bash
# Standard verification after EVERY commit
1. npx tsc --noEmit                  # Type check
2. npm start                          # Deploy
3. ./jtag ping                        # System check
4. ./jtag debug/chat-send --roomId="<ID>" --message="Test"  # Chat still works
5. npx vitest run <relevant-tests>   # Unit tests
```

---

## Final Architecture

```
RoomEntity
  └── members: RoomMemberEntity[]
        ├── learningMode: 'fine-tuning' | 'inference-only'
        ├── genomeId?: UUID
        └── participantRole?: string

RAGContext
  └── learningMode, genomeId, participantRole
        ↓
PromptAdapter<TContext extends BasePromptContext>
  ├── Uses learning mode to customize prompts
  ├── Adds meta-learning sections if fine-tuning
  └── Different task descriptions per mode

Recipe Pipeline
  └── academy/update-lora-weights command
        ├── Checks learning mode
        ├── Checks performance threshold
        └── Updates genome if conditions met
```

This architecture enables:
✅ Teachers learning to teach better
✅ Students learning skills
✅ Static expert reviewers
✅ Mixed learning dynamics
✅ Per-room, per-participant configuration
✅ Backwards compatible (existing rooms work)
✅ Safe incremental rollout (nothing breaks)
