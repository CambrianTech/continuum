# RAG & Thought Coherence Improvements Per Learning Mode Phase

**Created**: 2025-10-23
**Purpose**: Identify improvements to RAG context building and thought coherence for each phase of learning mode architecture

---

## Executive Summary

As we implement per-participant learning modes (Phase 1-6), we need to ensure:
1. **RAG Context Quality** - Learning participants get relevant context for skill development
2. **Thought Coherence** - AI responses reflect learning mode (meta-learning, self-reflection)
3. **Context Awareness** - Prompts adapt based on participant role (student, teacher, reviewer)
4. **Progressive Enhancement** - Each phase adds capabilities without breaking existing functionality

---

## Phase 1: Data Model Foundation ✅ COMPLETE

### Status
- ✅ Added `learningMode`, `genomeId`, `participantRole` to RoomMember interface
- ✅ All fields optional (backwards compatible)
- ✅ Type check passes, deployment clean

### RAG Improvements Needed
**NONE** - Phase 1 is data model only, RAG system unchanged

### Thought Coherence Improvements Needed
**NONE** - No prompt changes in Phase 1

### Testing Completed
- ✅ TypeScript compilation
- ✅ System deployment
- ✅ System health check
- ✅ ThoughtStream coordination (existing functionality preserved)

---

## Phase 2: RAG Context Extension 🔄 NEXT

### Goal
Make learning mode **available** to prompt adapters without changing behavior yet.

### Data Reseeding Strategy

**CRITICAL**: Phase 2 doesn't require data reseeding because:
- ✅ Learning mode fields are **optional** in RoomMember interface
- ✅ Existing rooms without learning mode will work (fields undefined)
- ✅ RAG context gracefully handles missing learning mode (no errors)
- ✅ Backwards compatible by design

**However, for thorough testing:**
```bash
# Option 1: Test with existing data (verify backwards compatibility)
npm start
./jtag debug/chat-send --roomId="<EXISTING_ROOM>" --message="Test Phase 2"
# Expected: Works normally, learning mode fields undefined in logs

# Option 2: Create NEW test room with learning mode (verify new functionality)
./jtag data/create --collection=rooms --data='{
  "name": "Phase 2 Test Room",
  "recipeId": "general-chat",
  "members": [
    {"userId": "<HUMAN_ID>", "role": "member"},
    {"userId": "<AI_ID>", "role": "member", "learningMode": "fine-tuning", "genomeId": "test-genome", "participantRole": "student"}
  ]
}'

# Option 3: Full reseed (nuclear option, only if needed)
npm run data:reseed
# This recreates ALL data from scratch
```

**Recommendation**: Use Option 1 (existing data) first to verify backwards compatibility, then Option 2 (new test room) to verify new functionality. Only use Option 3 if data corruption occurs.

### RAG Improvements Required

#### 1. Extend RAGContext Type
**File**: `system/rag/shared/RAGTypes.ts` (line 115)

**Current State**:
```typescript
export interface RAGContext {
  domain: RAGDomain;
  contextId: UUID;
  personaId: UUID;
  identity: PersonaIdentity;
  recipeStrategy?: RecipeStrategy;
  conversationHistory: LLMMessage[];
  artifacts: RAGArtifact[];
  privateMemories: PersonaMemory[];
  metadata: {...};
}
```

**Required Changes**:
```typescript
export interface RAGContext {
  // ... existing fields ...

  // NEW: Learning mode configuration (Phase 2)
  learningMode?: 'fine-tuning' | 'inference-only';
  genomeId?: UUID;
  participantRole?: string;  // 'student', 'teacher', 'reviewer', etc.
}
```

**Why**: Prompt adapters need access to learning mode to customize prompts in Phase 3

#### 2. Load Learning Config in ChatRAGBuilder
**File**: `system/rag/builders/ChatRAGBuilder.ts` (line 45, buildContext method)

**Current State**: Loads identity, conversation history, artifacts, memories, recipe strategy

**Required Changes**:
```typescript
async buildContext(
  contextId: UUID,
  personaId: UUID,
  options?: RAGBuildOptions
): Promise<RAGContext> {
  // ... existing context building (lines 50-91) ...

  // NEW: Load learning configuration from room membership
  const learningConfig = await this.loadLearningConfig(contextId, personaId);

  const ragContext: RAGContext = {
    domain: 'chat',
    contextId,
    personaId,
    identity,
    recipeStrategy,
    conversationHistory,
    artifacts,
    privateMemories,

    // NEW: Add learning mode fields
    learningMode: learningConfig?.learningMode,
    genomeId: learningConfig?.genomeId,
    participantRole: learningConfig?.participantRole,

    metadata: { ... }
  };

  return ragContext;
}

/**
 * NEW METHOD: Load learning configuration from room membership
 */
private async loadLearningConfig(
  roomId: UUID,
  personaId: UUID
): Promise<{ learningMode?: 'fine-tuning' | 'inference-only'; genomeId?: UUID; participantRole?: string } | undefined> {
  try {
    // 1. Load room entity
    const roomResult = await DataDaemon.read<RoomEntity>(RoomEntity.collection, roomId);
    if (!roomResult.success || !roomResult.data) {
      return undefined;
    }

    const room = roomResult.data.data;

    // 2. Find this persona's membership
    const member = room.members.find(m => m.userId === personaId);
    if (!member) {
      return undefined;
    }

    // 3. Return learning config if present (all fields optional)
    return {
      learningMode: member.learningMode,
      genomeId: member.genomeId,
      participantRole: member.participantRole
    };
  } catch (error) {
    console.error(`❌ ChatRAGBuilder: Error loading learning config:`, error);
    return undefined;
  }
}
```

**Why**: RAG context must include learning mode for adapters to access

**Testing**:
```bash
# 1. Add debug log to verify loading
console.log('🧠 RAG Learning Mode:', learningConfig?.learningMode ?? 'not set',
            'Role:', learningConfig?.participantRole ?? 'not set');

# 2. Trigger message
./jtag debug/chat-send --roomId="<ID>" --message="Phase 2 learning mode test"

# 3. Check logs
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "RAG Learning Mode"

# Expected: "🧠 RAG Learning Mode: not set Role: not set" (no members have learning mode yet)
```

### Thought Coherence Improvements Required

#### 3. Extend BasePromptContext Type
**File**: `system/recipes/shared/RecipePromptBuilder.ts` (line 25)

**Current State**:
```typescript
export interface BasePromptContext {
  readonly personaName: string;
  readonly roomContext: RAGContext;
  readonly conversationPattern: ConversationPattern;
}
```

**Required Changes**:
```typescript
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

**Why**: All prompt adapters inherit from BasePromptContext, making learning mode available everywhere

**Impact**: GatingPromptContext and GenerationPromptContext automatically get these fields

#### 4. Pass Learning Mode to Prompt Adapters
**File**: Wherever prompt adapters are instantiated (likely in PersonaUser or ThoughtStream)

**Required Changes**:
```typescript
// When building prompt context, extract learning mode from RAG context
const promptContext: GatingPromptContext = {
  personaName: ragContext.identity.name,
  roomContext: ragContext,
  conversationPattern: ragContext.recipeStrategy?.conversationPattern ?? 'human-focused',

  // NEW: Pass through learning mode fields
  learningMode: ragContext.learningMode,
  genomeId: ragContext.genomeId,
  participantRole: ragContext.participantRole
};
```

**Why**: Adapters can't use learning mode unless it's passed to them

### Testing Strategy

```bash
# 1. Type check
npx tsc --noEmit
# Expected: Zero errors

# 2. Deploy system
npm start
# Expected: Clean deployment, no errors

# 3. Verify RAG context extension
# Add debug logs to ChatRAGBuilder.buildContext()
# Send test message, check logs for learning mode fields

# 4. Run prompt builder unit tests
npx vitest run system/recipes/test/unit/RecipePromptBuilder.test.ts
# Expected: All tests pass (no behavior change)

# 5. System health check
./jtag ping
# Expected: 66 commands, 12 daemons, systemReady: true

# 6. Chat functionality verification
./jtag debug/chat-send --roomId="<ID>" --message="Phase 2 complete test"
# Expected: Message sent successfully, system operational
```

### Success Criteria

✅ Phase 2 Complete When:
1. RAGContext type includes learning mode fields
2. ChatRAGBuilder loads learning config from room members
3. BasePromptContext includes learning fields
4. Learning mode passed to all prompt adapters
5. Type check passes
6. All existing tests pass
7. Logs show learning mode being loaded (even if undefined)
8. **No behavior change** - fields present but not used yet

### Estimated Effort
- **Code Changes**: 3 files modified (RAGTypes, ChatRAGBuilder, RecipePromptBuilder)
- **Testing**: 30 minutes (compile, deploy, verify logs, run tests)
- **Total Time**: 1-2 hours

---

## Phase 3: Prompt Adapter Meta-Learning 🔮 FUTURE

### Goal
Adapters **USE** learning mode to customize prompts with meta-learning sections.

### Data Reseeding Strategy

**CRITICAL**: Phase 3 changes prompt behavior, so we MUST test with learning mode data.

**Recommended Approach**:
```bash
# 1. Update data seeding to include learning mode participants
# File: api/data-seed/RoomDataSeed.ts

# Add to "general" room seeding:
{
  userId: helperAI.id,
  role: 'member',
  joinedAt: new Date(),
  learningMode: 'fine-tuning',      // NEW
  genomeId: 'helper-ai-genome',     // NEW
  participantRole: 'assistant'      // NEW
}

# 2. Full reseed to test Phase 3
npm run data:reseed

# 3. Verify learning mode loaded
./jtag data/list --collection=rooms --limit=1
# Expected: Room members show learning mode fields

# 4. Trigger AI response
./jtag debug/chat-send --roomId="<GENERAL_ROOM>" --message="Phase 3 meta-learning test"

# 5. Check logs for meta-learning sections in prompts
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "Meta-Learning"
# Expected: "**Meta-Learning (Self-Improvement):**" in prompt
```

**Why Reseed Required**:
- Phase 3 behavior ONLY activates if `learningMode: 'fine-tuning'` is set
- Existing rooms have no learning mode (undefined)
- We need learning mode participants to see meta-learning prompts
- Reseeding is easier than manually updating room members

**Alternative (No Reseed)**:
```bash
# Manually update existing room member with learning mode
./jtag data/update --collection=rooms --id="<ROOM_ID>" --data='{
  "members": [
    # ... copy existing members, add learning mode to one AI ...
  ]
}'
# This is tedious and error-prone - reseed is better
```

### RAG Improvements Required

**NONE** - Phase 2 already provided learning mode in RAG context

### Thought Coherence Improvements Required

#### 1. Add Meta-Learning Section Builder
**File**: `system/recipes/shared/RecipePromptBuilder.ts` (PromptSectionBuilder class)

**New Method**:
```typescript
/**
 * Build meta-learning section for fine-tuning participants
 * Encourages self-reflection and skill development
 */
static buildMetaLearningSection(participantRole?: string): string {
  const baseReflection = `**Meta-Learning (Self-Improvement):**
After responding, reflect on your performance:
- Was my response helpful and accurate?
- What could I improve?
- Did I align with the conversation pattern and response rules?`;

  // Role-specific reflection prompts
  const roleReflection: Record<string, string> = {
    teacher: `
- Did I adapt my teaching to the learner's level?
- Was my explanation clear and well-structured?
- Did I provide actionable feedback?`,

    student: `
- Did I show my reasoning clearly?
- Did I identify what I don't understand?
- Did I ask clarifying questions when needed?`,

    reviewer: `
- Was my review constructive and specific?
- Did I identify both strengths and improvements?
- Did I explain my reasoning clearly?`
  };

  const roleSpecific = participantRole ? roleReflection[participantRole] ?? '' : '';

  return baseReflection + roleSpecific + `

Your reflection will help update your skills if performance meets threshold.`;
}
```

**Why**: Fine-tuning participants need prompts that encourage self-reflection

#### 2. Modify GatingPromptAdapter
**File**: `system/recipes/shared/RecipePromptBuilder.ts` (line 58)

**Current Implementation**:
```typescript
export class GatingPromptAdapter implements PromptAdapter<GatingPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: GatingPromptContext): string {
    const sections: readonly string[] = [
      PromptSectionBuilder.buildHeader(...),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      PromptSectionBuilder.buildDecisionCriteria(strategy.decisionCriteria),
      PromptSectionBuilder.buildConversationContext(context.roomContext),
      PromptSectionBuilder.buildGatingInstructions()
    ];

    return sections.join('\n\n');
  }
}
```

**Required Changes**:
```typescript
export class GatingPromptAdapter implements PromptAdapter<GatingPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: GatingPromptContext): string {
    const sections: string[] = [  // Change to mutable array
      PromptSectionBuilder.buildHeader(...),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      PromptSectionBuilder.buildDecisionCriteria(strategy.decisionCriteria),
      PromptSectionBuilder.buildConversationContext(context.roomContext)
    ];

    // NEW: Add meta-learning section if in fine-tuning mode
    if (context.learningMode === 'fine-tuning') {
      sections.push(PromptSectionBuilder.buildMetaLearningSection(context.participantRole));
    }

    sections.push(PromptSectionBuilder.buildGatingInstructions());

    return sections.join('\n\n');
  }
}
```

**Why**: Gating decisions should consider learning objectives for fine-tuning participants

#### 3. Modify GenerationPromptAdapter
**File**: `system/recipes/shared/RecipePromptBuilder.ts` (line 79)

**Current Implementation**:
```typescript
export class GenerationPromptAdapter implements PromptAdapter<GenerationPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: GenerationPromptContext): string {
    const sections: readonly string[] = [
      PromptSectionBuilder.buildHeader(...),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      PromptSectionBuilder.buildConversationContext(context.roomContext),
      PromptSectionBuilder.buildGenerationInstructions()
    ];

    return sections.join('\n\n');
  }
}
```

**Required Changes**:
```typescript
export class GenerationPromptAdapter implements PromptAdapter<GenerationPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: GenerationPromptContext): string {
    const sections: string[] = [  // Change to mutable array
      PromptSectionBuilder.buildHeader(...),
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
```

**Why**: Response generation should include self-reflection prompts for learning participants

#### 4. Update Adapter Task Descriptions
**File**: `system/recipes/shared/RecipePromptBuilder.ts` (buildHeader calls)

**Enhancement**: Customize task description based on learning mode

```typescript
// In GatingPromptAdapter
const task = context.learningMode === 'fine-tuning'
  ? 'Decide if you should respond AND consider your learning objectives.'
  : 'Decide if you should respond to the most recent message.';

PromptSectionBuilder.buildHeader(
  context.personaName,
  context.conversationPattern,
  task
);

// In GenerationPromptAdapter
const task = context.learningMode === 'fine-tuning'
  ? 'Generate a thoughtful response AND reflect on your performance.'
  : 'Generate a thoughtful response to the conversation.';

PromptSectionBuilder.buildHeader(
  context.personaName,
  context.conversationPattern,
  task
);
```

**Why**: Task description should signal learning intent to the model

### Testing Strategy

```bash
# 1. Unit tests for meta-learning section builder
# File: system/recipes/test/unit/RecipePromptBuilder.test.ts
# Add tests:
# - buildMetaLearningSection() with no role
# - buildMetaLearningSection() with teacher role
# - buildMetaLearningSection() with student role
# - buildMetaLearningSection() with reviewer role

# 2. Adapter prompt generation tests
# Verify:
# - Gating prompt includes meta-learning for fine-tuning mode
# - Generation prompt includes meta-learning for fine-tuning mode
# - Inference-only mode excludes meta-learning sections
# - Task descriptions change based on learning mode

# 3. Integration test with test room
# Create room with learning mode participant
# Send message, capture generated prompts
# Verify meta-learning sections present

# 4. Visual inspection of prompts
# Add debug logs to output full prompts
# Verify readability and coherence
```

### Success Criteria

✅ Phase 3 Complete When:
1. Meta-learning section builder implemented
2. Gating adapter includes meta-learning for fine-tuning participants
3. Generation adapter includes meta-learning for fine-tuning participants
4. Task descriptions adapt based on learning mode
5. Unit tests pass for all new functionality
6. Integration tests verify prompts include meta-learning
7. **Behavior change confirmed** - fine-tuning participants get different prompts
8. Inference-only participants unchanged (backwards compatible)

### Estimated Effort
- **Code Changes**: 1 file modified (RecipePromptBuilder.ts)
- **Unit Tests**: 8-10 new tests
- **Integration Testing**: 1 hour
- **Total Time**: 3-4 hours

---

## Phase 4: Academy Domain Adapters 🎓 FUTURE

### Goal
Create specialized teacher/student adapters that respect learning modes.

### Data Reseeding Strategy

**CRITICAL**: Phase 4 introduces NEW DOMAIN (Academy), requires NEW database collections and seeding.

**New Collections Required**:
```typescript
// database/entities/TrainingSessionEntity.ts
collection: 'training_sessions'

// database/entities/TrainingExerciseEntity.ts
collection: 'training_exercises'

// database/entities/TrainingAttemptEntity.ts
collection: 'training_attempts'
```

**Seeding Strategy**:
```bash
# 1. Create Academy data seeder
# File: api/data-seed/AcademyDataSeed.ts

export class AcademyDataSeed {
  async seed(): Promise<void> {
    // 1. Create training exercises
    const tsInterfaceExercise = await TrainingExerciseEntity.create({
      skill: 'TypeScript Interfaces',
      title: 'Define a User interface',
      description: 'Create an interface with name, email, and optional age',
      difficulty: 'beginner',
      expectedAnswer: 'interface User { name: string; email: string; age?: number; }',
      hints: ['Use interface keyword', 'Optional properties use ?']
    });

    // 2. Create training session (TypeScript learning)
    const tsSession = await TrainingSessionEntity.create({
      sessionName: 'TypeScript Fundamentals',
      skill: 'TypeScript',
      learningObjectives: [
        'Define interfaces',
        'Use type annotations',
        'Understand optional properties'
      ],
      exercises: [tsInterfaceExercise.id],
      participants: [
        {
          personaId: studentAI.id,
          role: 'student',
          learningMode: 'fine-tuning',
          genomeId: 'typescript-skills-genome'
        },
        {
          personaId: teacherAI.id,
          role: 'teacher',
          learningMode: 'fine-tuning',
          genomeId: 'teaching-pedagogy-genome'
        }
      ]
    });
  }
}

# 2. Update master data seed
# File: api/data-seed/DataSeeder.ts

import { AcademyDataSeed } from './AcademyDataSeed';

async seed(): Promise<void> {
  await this.userDataSeed.seed();
  await this.roomDataSeed.seed();
  await this.academyDataSeed.seed();  // NEW
}

# 3. Full reseed with Academy data
npm run data:reseed

# 4. Verify Academy collections populated
./jtag data/list --collection=training_sessions
./jtag data/list --collection=training_exercises

# 5. Start training session
./jtag academy/session/start --sessionId="<TS_SESSION_ID>"

# 6. Verify Academy RAG builder used
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "AcademyRAGBuilder"
# Expected: "✅ Using AcademyRAGBuilder for training session"
```

**Why Full Reseed Required**:
- **NEW collections** don't exist yet (training_sessions, training_exercises, training_attempts)
- **NEW domain** (Academy) needs specialized test data
- **NEW recipes** for Academy training (academy-dual-learning.json)
- Can't test Academy without proper seeded data

**Phase 4 Seeding Checklist**:
- ✅ Create TrainingSessionEntity, TrainingExerciseEntity, TrainingAttemptEntity
- ✅ Create AcademyDataSeed.ts
- ✅ Seed 2-3 training exercises per skill
- ✅ Seed 1-2 training sessions (TypeScript, Rust)
- ✅ Assign student + teacher personas with learning modes
- ✅ Create Academy recipes (academy-dual-learning.json)
- ✅ Register AcademyRAGBuilder in RAGRegistry
- ✅ Full reseed: `npm run data:reseed`

### RAG Improvements Required

#### 1. Create AcademyRAGBuilder
**File**: `system/rag/builders/AcademyRAGBuilder.ts` (NEW)

**Purpose**: Build RAG context for training sessions

**Context Strategy**:
- **Priority-based token management** (not FIFO like chat)
- **Learning objectives** always included (highest priority)
- **Current exercise** full context (high priority)
- **Recent dialogue** compressed (medium priority)
- **Training history** summarized (low priority)

**Implementation**:
```typescript
export class AcademyRAGBuilder extends RAGBuilder {
  readonly domain: RAGDomain = 'academy';

  async buildContext(
    contextId: UUID,  // Training session ID
    personaId: UUID,
    options?: RAGBuildOptions
  ): Promise<RAGContext> {
    // 1. Load training session configuration
    const session = await this.loadTrainingSession(contextId);

    // 2. Load learning objectives (ALWAYS include - highest priority)
    const objectives = session.learningObjectives;

    // 3. Load current exercise/question
    const currentExercise = await this.loadCurrentExercise(session.currentExerciseId);

    // 4. Load recent dialogue (teacher-student conversation)
    const conversationHistory = await this.loadSessionDialogue(contextId, options?.maxMessages ?? 50);

    // 5. Load training artifacts (code examples, test cases)
    const artifacts = await this.loadTrainingArtifacts(session.artifactIds);

    // 6. Load persona identity WITH learning mode
    const identity = await this.loadPersonaIdentity(personaId, contextId);

    // 7. Load learning config (fine-tuning vs inference-only)
    const learningConfig = await this.loadLearningConfig(contextId, personaId);

    // 8. Load performance history (for adaptive difficulty)
    const performanceHistory = await this.loadPerformanceHistory(contextId, personaId);

    return {
      domain: 'academy',
      contextId,
      personaId,
      identity,
      conversationHistory,
      artifacts,
      privateMemories: [],  // Academy uses performance history instead
      learningMode: learningConfig?.learningMode,
      genomeId: learningConfig?.genomeId,
      participantRole: learningConfig?.participantRole,
      metadata: {
        messageCount: conversationHistory.length,
        artifactCount: artifacts.length,
        memoryCount: 0,
        builtAt: new Date(),
        // Academy-specific metadata
        learningObjectives: objectives,
        currentExercise: currentExercise.title,
        performanceScore: performanceHistory.averageScore
      }
    };
  }
}
```

**Why**: Academy domain needs different context than chat (objectives > history)

#### 2. Register AcademyRAGBuilder
**File**: `system/rag/RAGRegistry.ts` or similar

**Required Changes**:
```typescript
import { RAGBuilderFactory } from './shared/RAGBuilder';
import { ChatRAGBuilder } from './builders/ChatRAGBuilder';
import { AcademyRAGBuilder } from './builders/AcademyRAGBuilder';

export function registerAllRAGBuilders(): void {
  RAGBuilderFactory.register('chat', new ChatRAGBuilder());
  RAGBuilderFactory.register('academy', new AcademyRAGBuilder());  // NEW

  console.log('✅ Registered RAG builders for 2 domains');
}
```

**Why**: Factory pattern requires registration

### Thought Coherence Improvements Required

#### 1. Create Academy-Specific Prompt Contexts
**File**: `system/recipes/shared/adapters/AcademyAdapters.ts` (NEW)

**New Types**:
```typescript
/**
 * Academy-specific prompt context
 */
export interface AcademyPromptContext extends BasePromptContext {
  readonly learningObjectives: string[];
  readonly currentExercise: {
    title: string;
    description: string;
    difficulty: 'beginner' | 'intermediate' | 'advanced';
  };
  readonly performanceHistory: {
    totalAttempts: number;
    successfulAttempts: number;
    averageScore: number;
    recentTrend: 'improving' | 'stable' | 'declining';
  };
}

/**
 * Teacher-specific context (evaluating student work)
 */
export interface TeacherPromptContext extends AcademyPromptContext {
  readonly studentResponse: string;
  readonly previousAttempts: number;
  readonly studentProgress: 'struggling' | 'progressing' | 'mastering';
}

/**
 * Student-specific context (answering questions)
 */
export interface StudentPromptContext extends AcademyPromptContext {
  readonly hints: string[];
  readonly previousIncorrectAttempts: number;
}
```

#### 2. Create AcademyTeacherAdapter
**File**: `system/recipes/shared/adapters/AcademyTeacherAdapter.ts` (NEW)

**Implementation**:
```typescript
export class AcademyTeacherAdapter implements PromptAdapter<TeacherPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: TeacherPromptContext): string {
    const sections: string[] = [
      this.buildTeacherHeader(context),
      this.buildLearningObjectives(context.learningObjectives),
      this.buildCurrentExercise(context.currentExercise),
      this.buildStudentResponse(context.studentResponse),
      this.buildPerformanceContext(context.performanceHistory),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      this.buildFeedbackFormat()
    ];

    // Meta-learning for teachers in fine-tuning mode
    if (context.learningMode === 'fine-tuning') {
      sections.push(this.buildTeacherMetaLearning(context));
    }

    return sections.join('\n\n');
  }

  private buildTeacherHeader(context: TeacherPromptContext): string {
    const task = context.learningMode === 'fine-tuning'
      ? 'Evaluate the student response AND improve your teaching approach.'
      : 'Evaluate the student response and provide constructive feedback.';

    return PromptSectionBuilder.buildHeader(
      context.personaName,
      'teaching',
      task
    );
  }

  private buildTeacherMetaLearning(context: TeacherPromptContext): string {
    return `**Teaching Meta-Learning:**
After providing feedback, evaluate your teaching effectiveness:

1. **Clarity**: Was my feedback clear and actionable?
2. **Adaptation**: Did I adjust difficulty appropriately? (Student is ${context.studentProgress})
3. **Effectiveness**: Is the student improving under my guidance? (${context.performanceHistory.recentTrend})
4. **Empathy**: Did I consider the student's struggle level? (${context.previousAttempts} previous attempts)
5. **Pedagogy**: Did I use effective teaching techniques (examples, analogies, scaffolding)?

Your teaching genome will update if student performance improves after your feedback.`;
  }

  // ... other section builders ...
}
```

**Why**: Teachers need specialized prompts for evaluating student work

#### 3. Create AcademyStudentAdapter
**File**: `system/recipes/shared/adapters/AcademyStudentAdapter.ts` (NEW)

**Implementation**:
```typescript
export class AcademyStudentAdapter implements PromptAdapter<StudentPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: StudentPromptContext): string {
    const sections: string[] = [
      this.buildStudentHeader(context),
      this.buildLearningObjectives(context.learningObjectives),
      this.buildCurrentExercise(context.currentExercise),
      this.buildHints(context.hints, context.previousIncorrectAttempts),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      this.buildAnswerFormat()
    ];

    // Meta-learning for students in fine-tuning mode
    if (context.learningMode === 'fine-tuning') {
      sections.push(this.buildStudentMetaLearning(context));
    }

    return sections.join('\n\n');
  }

  private buildStudentHeader(context: StudentPromptContext): string {
    const task = context.learningMode === 'fine-tuning'
      ? 'Answer the exercise AND reflect on your learning process.'
      : 'Answer the exercise to the best of your ability.';

    return PromptSectionBuilder.buildHeader(
      context.personaName,
      'learning',
      task
    );
  }

  private buildStudentMetaLearning(context: StudentPromptContext): string {
    return `**Learning Meta-Cognition:**
After answering, reflect on your understanding:

1. **Confidence**: How confident am I in this answer? (0-100%)
2. **Reasoning**: Did I show my step-by-step thinking clearly?
3. **Gaps**: What concepts am I still unclear about?
4. **Questions**: What follow-up questions would help my understanding?
5. **Progress**: How has my understanding improved since the last attempt?

Your learning genome will update if you demonstrate understanding and clear reasoning.`;
  }

  // ... other section builders ...
}
```

**Why**: Students need prompts that encourage showing work and identifying gaps

### Testing Strategy

```bash
# 1. Unit tests for Academy adapters
# File: system/recipes/test/unit/AcademyAdapters.test.ts
# Tests:
# - Teacher adapter builds correct prompt structure
# - Student adapter builds correct prompt structure
# - Meta-learning sections appear only for fine-tuning mode
# - Performance context affects prompt content
# - Learning objectives always included

# 2. Create test training session
./jtag academy/session/create \
  --skill="TypeScript Interfaces" \
  --objectives='["Define interfaces","Use type annotations","Understand extends"]'

# 3. Add learning mode participants
./jtag academy/session/join \
  --sessionId="<ID>" \
  --personaId="<STUDENT_ID>" \
  --learningMode="fine-tuning" \
  --genomeId="ts-skills" \
  --participantRole="student"

./jtag academy/session/join \
  --sessionId="<ID>" \
  --personaId="<TEACHER_ID>" \
  --learningMode="fine-tuning" \
  --genomeId="teaching-skills" \
  --participantRole="teacher"

# 4. Trigger training cycle
./jtag academy/exercise/start --sessionId="<ID>"

# 5. Verify prompts in logs
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "Meta-Learning"
# Expected: Teacher and student both get meta-learning sections

# 6. Visual inspection of generated prompts
./jtag debug/logs --filterPattern="PROMPT GENERATED" --tailLines=100
```

### Success Criteria

✅ Phase 4 Complete When:
1. AcademyRAGBuilder implemented and registered
2. AcademyTeacherAdapter builds teacher-specific prompts
3. AcademyStudentAdapter builds student-specific prompts
4. Meta-learning sections adapt based on role (teacher vs student)
5. Performance history influences prompt content
6. Learning objectives always included (highest priority)
7. Unit tests pass for all Academy adapters
8. Integration tests verify training cycle works end-to-end
9. Both teacher and student can be in fine-tuning mode simultaneously

### Estimated Effort
- **Code Changes**: 3 new files (AcademyRAGBuilder, TeacherAdapter, StudentAdapter)
- **Unit Tests**: 15-20 new tests
- **Integration Testing**: 2-3 hours
- **Total Time**: 1-2 days

---

## Phase 5: Genome Update Pipeline 🧬 FUTURE

### Goal
Actually update LoRA weights based on learning mode and performance.

### Data Reseeding Strategy

**CRITICAL**: Phase 5 modifies genome files on disk, requires genome infrastructure.

**New Infrastructure Required**:
```bash
# 1. Genome storage directory structure
system/genomes/
├── typescript-skills-genome/
│   ├── base_weights.safetensors
│   ├── adapter_weights.safetensors  # LoRA weights
│   └── metadata.json
├── teaching-pedagogy-genome/
│   ├── base_weights.safetensors
│   ├── adapter_weights.safetensors
│   └── metadata.json
└── helper-ai-genome/
    ├── base_weights.safetensors
    ├── adapter_weights.safetensors
    └── metadata.json
```

**Seeding Strategy**:
```bash
# 1. Create GenomeEntity collection
# File: database/entities/GenomeEntity.ts
collection: 'genomes'

interface GenomeEntity {
  id: UUID;
  name: string;
  description: string;
  baseModel: string;  // 'deepseek-r1:7b', 'llama3.2:3b', etc.
  adapterPath: string;  // 'system/genomes/typescript-skills-genome'
  version: number;
  lastUpdated: Date;
  updateCount: number;
  performanceMetrics: {
    averageScore: number;
    totalEvaluations: number;
    successRate: number;
  };
}

# 2. Create GenomeDataSeed.ts
# File: api/data-seed/GenomeDataSeed.ts

export class GenomeDataSeed {
  async seed(): Promise<void> {
    // Create genome records in database
    const tsSkillsGenome = await GenomeEntity.create({
      name: 'TypeScript Skills',
      description: 'Student genome for learning TypeScript fundamentals',
      baseModel: 'deepseek-r1:7b',
      adapterPath: 'system/genomes/typescript-skills-genome',
      version: 0,
      lastUpdated: new Date(),
      updateCount: 0
    });

    const teachingGenome = await GenomeEntity.create({
      name: 'Teaching Pedagogy',
      description: 'Teacher genome for improving teaching effectiveness',
      baseModel: 'deepseek-r1:7b',
      adapterPath: 'system/genomes/teaching-pedagogy-genome',
      version: 0,
      lastUpdated: new Date(),
      updateCount: 0
    });

    // Initialize genome files on disk (empty LoRA adapters)
    await this.initializeGenomeFiles(tsSkillsGenome);
    await this.initializeGenomeFiles(teachingGenome);
  }

  private async initializeGenomeFiles(genome: GenomeEntity): Promise<void> {
    const genomePath = path.join(process.cwd(), genome.adapterPath);
    await fs.mkdir(genomePath, { recursive: true });

    // Create empty adapter weights (will be updated during training)
    await fs.writeFile(
      path.join(genomePath, 'adapter_weights.safetensors'),
      Buffer.alloc(0)  // Empty file, updated by GenomeUpdateService
    );

    // Create metadata file
    await fs.writeFile(
      path.join(genomePath, 'metadata.json'),
      JSON.stringify({
        genomeId: genome.id,
        name: genome.name,
        baseModel: genome.baseModel,
        version: 0,
        createdAt: new Date().toISOString()
      }, null, 2)
    );
  }
}

# 3. Update master data seed
# File: api/data-seed/DataSeeder.ts

async seed(): Promise<void> {
  await this.userDataSeed.seed();
  await this.roomDataSeed.seed();
  await this.genomeDataSeed.seed();  // NEW (before Academy, genomes referenced)
  await this.academyDataSeed.seed();
}

# 4. Full reseed with genome infrastructure
npm run data:reseed

# 5. Verify genome records and files
./jtag data/list --collection=genomes
ls -la system/genomes/*/

# 6. Test genome update (dry run)
./jtag academy/update-lora-weights \
  --personaId="<STUDENT_AI>" \
  --genomeId="typescript-skills-genome" \
  --evaluation='{"score": 95, "feedback": "Excellent"}' \
  --learningMode="fine-tuning"

# 7. Verify genome file modified
ls -la system/genomes/typescript-skills-genome/adapter_weights.safetensors
# Expected: File size increased, timestamp updated
```

**Why Genome Seeding Required**:
- **NEW collection** (`genomes`) for tracking genome metadata
- **Filesystem structure** for storing LoRA weights
- **Database references** in TrainingSession participants (genomeId)
- **GenomeUpdateService** needs valid genome paths to write to
- Can't test genome updates without genome infrastructure

**Phase 5 Seeding Checklist**:
- ✅ Create GenomeEntity database entity
- ✅ Create GenomeDataSeed.ts
- ✅ Seed 3-5 genome records (TypeScript, teaching, helper, etc.)
- ✅ Initialize genome file structure (adapter_weights.safetensors)
- ✅ Update AcademyDataSeed to reference genome IDs
- ✅ Create GenomeUpdateService (apply LoRA updates)
- ✅ Full reseed: `npm run data:reseed`
- ✅ Verify genome files exist on disk

**IMPORTANT**: Genome files are PERSISTENT across reseeds (by design). If you want to reset genomes:
```bash
# Nuclear option: Delete all genome weights
rm -rf system/genomes/*/adapter_weights.safetensors

# Then reseed to recreate empty genome files
npm run data:reseed
```

### RAG Improvements Required

**NONE** - Phase 4 already provides all context needed for genome updates

### Thought Coherence Improvements Required

**NONE** - Genome updates happen AFTER response generation, not in prompts

### System Improvements Required

#### 1. Create Genome Update Command
**File**: `commands/academy/update-lora-weights/` (NEW)

**Purpose**: Update persona's LoRA weights if performance meets threshold

**Implementation**: See LEARNING-MODE-ARCHITECTURE.md Phase 5 (lines 384-438)

**Key Logic**:
```typescript
async execute(params: UpdateLoRAWeightsParams): Promise<UpdateLoRAWeightsResult> {
  // 1. Check learning mode (gate on learning mode)
  if (params.learningMode === 'inference-only') {
    return { updated: false, reason: 'Participant in inference-only mode' };
  }

  // 2. Check performance threshold
  if (params.evaluation.score < 80) {
    return { updated: false, reason: `Score ${params.evaluation.score} below threshold` };
  }

  // 3. Load genome
  const genome = await GenomeEntity.findById(params.genomeId);

  // 4. Apply LoRA update
  await GenomeUpdateService.applyLoRAUpdate(genome, params.evaluation);

  return { updated: true };
}
```

**Why**: Genome updates must respect learning mode (only update if fine-tuning)

#### 2. Integrate into Recipe Pipeline
**File**: `system/recipes/*.json` (Academy recipes)

**Example Recipe**:
```json
{
  "uniqueId": "academy-dual-learning",
  "displayName": "Academy Training (Both Learn)",
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

**Why**: Recipe pipeline orchestrates genome updates after evaluation

### Testing Strategy

```bash
# 1. Test static persona (should NOT update)
./jtag academy/update-lora-weights \
  --personaId="static-teacher" \
  --genomeId="teaching-genome" \
  --evaluation='{"score": 90, "feedback": "Excellent"}' \
  --learningMode="inference-only"
# Expected: { updated: false, reason: "inference-only mode" }

# 2. Test learning persona below threshold (should NOT update)
./jtag academy/update-lora-weights \
  --personaId="learning-student" \
  --genomeId="ts-skills" \
  --evaluation='{"score": 70, "feedback": "Needs improvement"}' \
  --learningMode="fine-tuning"
# Expected: { updated: false, reason: "Score 70 below threshold (80)" }

# 3. Test learning persona above threshold (SHOULD update)
./jtag academy/update-lora-weights \
  --personaId="learning-student" \
  --genomeId="ts-skills" \
  --evaluation='{"score": 95, "feedback": "Excellent reasoning"}' \
  --learningMode="fine-tuning"
# Expected: { updated: true }

# 4. Verify genome file modified
ls -la system/genomes/<GENOME_ID>/adapter_weights.safetensors
# Expected: Timestamp updated

# 5. Full training cycle test
./jtag academy/run-training-cycle --sessionId="<ID>"
# Expected: Both student and teacher genomes update if performance good
```

### Success Criteria

✅ Phase 5 Complete When:
1. academy/update-lora-weights command implemented
2. Learning mode gates genome updates (inference-only = no updates)
3. Performance threshold gates updates (score >= 80)
4. Genome files actually modified on disk
5. Recipe pipeline integrates genome updates
6. Unit tests for genome update logic
7. Integration tests verify end-to-end training cycle
8. Both teacher and student can learn simultaneously
9. Metrics tracked (update frequency, performance improvements)

### Estimated Effort
- **Code Changes**: 2 new files (command + GenomeUpdateService)
- **Recipe Integration**: 2-3 recipe JSON files
- **Testing**: 2-3 hours
- **Total Time**: 1-2 days

---

## Phase 6: UI Indicators 🎨 FUTURE

### Goal
Show learning mode status in chat UI.

### Data Reseeding Strategy

**CRITICAL**: Phase 6 is UI-only, but requires learning mode data to display.

**Reseeding Strategy**:
```bash
# Phase 6 assumes Phases 3-5 are complete, so data already has learning mode

# If Phase 6 is implemented BEFORE Phase 3:
# 1. Update RoomDataSeed.ts to include learning mode participants
# File: api/data-seed/RoomDataSeed.ts

// Add to "general" room seeding:
const generalRoom = await RoomEntity.create({
  name: 'general',
  description: 'General discussion room',
  recipeId: 'general-chat',
  members: [
    { userId: joel.id, role: 'owner', joinedAt: new Date() },
    {
      userId: helperAI.id,
      role: 'member',
      joinedAt: new Date(),
      learningMode: 'fine-tuning',      // NEW
      genomeId: helperGenome.id,        // NEW (reference genome)
      participantRole: 'assistant'       // NEW
    },
    {
      userId: teacherAI.id,
      role: 'member',
      joinedAt: new Date(),
      learningMode: 'inference-only'    // Static teacher
    }
  ]
});

# 2. Full reseed to update room members
npm run data:reseed

# 3. Deploy with UI changes
npm start

# 4. Visual verification
./jtag screenshot --querySelector="chat-widget" --filename="learning-ui-test.png"

# Expected UI elements:
# - Participant list shows "Helper AI [assistant] 🧬 (helper-ge...)"
# - Participant list shows "Teacher AI" (no 🧬, inference-only)
# - Room header shows "🧬 1 learning / 2 static"
# - Message headers show 🧬 badge next to Helper AI messages
```

**Why Minimal Reseeding**:
- Phase 6 is **display-only**, no new data model changes
- If Phase 3-5 already completed, data already has learning mode
- If Phase 6 implemented early, only need to update RoomDataSeed.ts
- No new collections, no new infrastructure

**Testing Without Reseed (Alternative)**:
```bash
# Manually update existing room (if you don't want full reseed)
./jtag data/update --collection=rooms --id="<GENERAL_ROOM_ID>" --data='{
  "members": [
    {"userId": "<JOEL_ID>", "role": "owner", "joinedAt": "2025-10-23T00:00:00Z"},
    {
      "userId": "<HELPER_AI_ID>",
      "role": "member",
      "joinedAt": "2025-10-23T00:00:00Z",
      "learningMode": "fine-tuning",
      "genomeId": "helper-ai-genome",
      "participantRole": "assistant"
    }
  ]
}'

# Refresh page to see UI changes
```

**Phase 6 Seeding Summary**:
- ✅ No new collections
- ✅ No new infrastructure
- ✅ Only update RoomDataSeed.ts if Phase 3 not done yet
- ✅ Reseed: `npm run data:reseed` (or manual update)
- ✅ Visual testing with screenshots

### RAG Improvements Required

**NONE** - UI reads from data model, doesn't affect RAG context

### Thought Coherence Improvements Required

**NONE** - UI displays status, doesn't affect prompts

### UI Improvements Required

#### 1. Participant List Learning Indicators
**File**: `widgets/chat/chat-widget/ParticipantList.ts`

**Current State**: Displays participant names and roles

**Required Changes**:
```typescript
renderParticipant(member: RoomMemberEntity): string {
  // Learning mode indicator
  const learningIndicator = member.learningMode === 'fine-tuning'
    ? ' 🧬' // Genome/learning indicator
    : '';

  // Role tag (student, teacher, reviewer)
  const roleTag = member.participantRole
    ? ` [${member.participantRole}]`
    : '';

  // Genome ID (for debugging/transparency)
  const genomeInfo = member.genomeId && member.learningMode === 'fine-tuning'
    ? ` (${member.genomeId.slice(0, 8)}...)`
    : '';

  return `<div class="participant" title="${this.buildTooltip(member)}">
    <span class="participant-name">${member.displayName}</span>
    <span class="participant-role">${roleTag}</span>
    <span class="learning-indicator">${learningIndicator}</span>
    <span class="genome-id">${genomeInfo}</span>
  </div>`;
}

private buildTooltip(member: RoomMemberEntity): string {
  if (member.learningMode === 'fine-tuning') {
    return `${member.displayName} is learning (fine-tuning ${member.genomeId})`;
  }
  return `${member.displayName} (static, inference-only)`;
}
```

**Why**: Users need to see which AIs are learning vs static

#### 2. Message Header Learning Indicators
**File**: `widgets/chat/chat-widget/MessageHeader.ts`

**Current State**: Shows sender name and timestamp

**Required Changes**:
```typescript
renderMessageHeader(message: ChatMessageEntity, sender: UserEntity): string {
  // Check if sender is in learning mode in this room
  const learningStatus = await this.getSenderLearningStatus(message.roomId, message.senderId);

  const learningBadge = learningStatus?.learningMode === 'fine-tuning'
    ? '<span class="learning-badge" title="Currently learning">🧬</span>'
    : '';

  return `<div class="message-header">
    <span class="sender-name">${sender.displayName}</span>
    ${learningBadge}
    <span class="timestamp">${this.formatTimestamp(message.timestamp)}</span>
  </div>`;
}
```

**Why**: Readers should know if a message was generated during learning mode

#### 3. Room Header Learning Summary
**File**: `widgets/chat/chat-widget/RoomHeader.ts`

**New Feature**: Show learning mode statistics

**Implementation**:
```typescript
renderLearningSummary(room: RoomEntity): string {
  const learningParticipants = room.members.filter(m => m.learningMode === 'fine-tuning');
  const staticParticipants = room.members.filter(m => m.learningMode !== 'fine-tuning');

  if (learningParticipants.length === 0) {
    return ''; // No learning happening, don't show
  }

  return `<div class="learning-summary" title="Learning participants">
    🧬 ${learningParticipants.length} learning
    / ${staticParticipants.length} static
  </div>`;
}
```

**Why**: Room overview should indicate if learning is happening

### Testing Strategy

```bash
# 1. Deploy with UI changes
npm start

# 2. Create test room with mixed learning modes
./jtag data/create --collection=rooms --data='{
  "name": "Learning Mode UI Test",
  "members": [
    {"userId": "human-1", "role": "member"},
    {"userId": "student-ai", "learningMode": "fine-tuning", "genomeId": "ts-skills", "participantRole": "student"},
    {"userId": "teacher-ai", "learningMode": "fine-tuning", "genomeId": "teaching", "participantRole": "teacher"},
    {"userId": "static-ai", "learningMode": "inference-only"}
  ]
}'

# 3. Take screenshots
./jtag screenshot --querySelector="chat-widget" --filename="learning-mode-ui.png"

# 4. Visual verification
# Expected:
# - Participant list shows 🧬 next to learning AIs
# - Role tags show [student], [teacher]
# - Genome IDs shown for learning participants
# - Room header shows "🧬 2 learning / 2 static"
# - Message headers show 🧬 badge for learning AIs

# 5. Hover tooltips
# Manually verify tooltips provide useful information
```

### Success Criteria

✅ Phase 6 Complete When:
1. Participant list shows learning indicators (🧬)
2. Participant list shows role tags ([student], [teacher])
3. Genome IDs visible for learning participants
4. Message headers indicate if sender was learning
5. Room header shows learning summary
6. Tooltips provide detailed information
7. UI degrades gracefully (no learning mode = no indicators)
8. Screenshot verification passes
9. Visual design consistent with existing UI

### Estimated Effort
- **Code Changes**: 3 widget files modified
- **CSS Styling**: 1-2 hours
- **Testing**: 1 hour
- **Total Time**: 4-6 hours

---

## Summary: Progressive Enhancement Strategy

| Phase | RAG Changes | Prompt Changes | System Changes | UI Changes | Data Reseeding | Estimated Time |
|-------|-------------|----------------|----------------|------------|----------------|----------------|
| 1. Data Model ✅ | None | None | Add fields to RoomMember | None | **Not required** | **COMPLETE** |
| 2. RAG Extension 🔄 | Add fields to RAGContext, load from DB | Pass to adapters | None | None | **Not required** (optional fields) | 1-2 hours |
| 3. Meta-Learning 🔮 | None | Add meta-learning sections | None | None | **REQUIRED** (update RoomDataSeed) | 3-4 hours |
| 4. Academy Domain 🎓 | New AcademyRAGBuilder | New teacher/student adapters | None | None | **REQUIRED** (new collections) | 1-2 days |
| 5. Genome Updates 🧬 | None | None | LoRA weight updates | None | **REQUIRED** (genome infrastructure) | 1-2 days |
| 6. UI Indicators 🎨 | None | None | None | Learning badges | **Optional** (if Phase 3 done) | 4-6 hours |

**Total Estimated Time**: 3-5 days

**Key Principles**:
1. ✅ **Backwards Compatible**: Every phase maintains existing functionality
2. ✅ **Incremental**: Each phase adds value independently
3. ✅ **Testable**: Clear success criteria per phase
4. ✅ **Safe**: No breaking changes, can pause after any phase
5. ✅ **Progressive**: Later phases build on earlier foundations
6. ✅ **Data-Aware**: Each phase documents reseeding requirements

---

## Data Reseeding Summary

### When to Reseed

**Phase 1** ✅ - **NO RESEED** (data model only)

**Phase 2** 🔄 - **NO RESEED REQUIRED**
- Optional fields, existing data works
- Create new test room for verification (optional)

**Phase 3** 🔮 - **RESEED REQUIRED**
- Update `RoomDataSeed.ts` to add learning mode to AI participants
- Full reseed: `npm run data:reseed`
- Why: Can't test meta-learning without learning mode participants

**Phase 4** 🎓 - **RESEED REQUIRED**
- Create `AcademyDataSeed.ts` (new domain)
- Create 3 new entity types (TrainingSession, Exercise, Attempt)
- Full reseed: `npm run data:reseed`
- Why: Academy domain needs specialized test data

**Phase 5** 🧬 - **RESEED REQUIRED**
- Create `GenomeDataSeed.ts` (genome infrastructure)
- Create genome file structure on disk
- Update `AcademyDataSeed.ts` to reference genome IDs
- Full reseed: `npm run data:reseed`
- Why: Genome updates need valid genome files

**Phase 6** 🎨 - **OPTIONAL RESEED**
- If Phase 3-5 done: No reseed needed (data already has learning mode)
- If Phase 6 done early: Update `RoomDataSeed.ts`, then reseed
- Why: UI displays learning mode, needs data to show

### Reseed Command Reference

```bash
# Full system reseed (clears ALL data, recreates from seed files)
npm run data:reseed

# Partial operations (if needed)
npm run data:clear       # Clear all data
npm run data:seed        # Seed data (without clearing)

# Manual verification after reseed
./jtag data/list --collection=rooms
./jtag data/list --collection=users
./jtag data/list --collection=training_sessions  # Phase 4+
./jtag data/list --collection=genomes            # Phase 5+
```

### Seed File Organization

```
api/data-seed/
├── DataSeeder.ts          # Master orchestrator
├── UserDataSeed.ts        # Users (Phase 1+)
├── RoomDataSeed.ts        # Rooms (Phase 1+, updated Phase 3+)
├── AcademyDataSeed.ts     # Academy domain (Phase 4+)
└── GenomeDataSeed.ts      # Genomes (Phase 5+)
```

---

## Next Steps (Phase 2)

**Ready to start Phase 2**: Extend RAG Context

**Tasks**:
1. ✅ Read this document (you're here)
2. ⏳ Modify RAGTypes.ts (add learning mode fields)
3. ⏳ Modify ChatRAGBuilder.ts (load learning config)
4. ⏳ Modify RecipePromptBuilder.ts (extend BasePromptContext)
5. ⏳ Add debug logs to verify loading
6. ⏳ Test: compile, deploy, verify logs
7. ⏳ Test with existing data (backwards compatibility)
8. ⏳ Test with new room (learning mode populated)
9. ⏳ Commit: "Extend RAG context with learning mode (Phase 2)"

**Data Strategy for Phase 2**:
- ✅ **No reseed required** - existing data works
- ✅ Test backwards compatibility first (existing rooms)
- ✅ Create new test room second (learning mode populated)
- ✅ Save reseed for Phase 3 (when behavior changes)

**Let's begin!**
