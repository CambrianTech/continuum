# Practical Implementation Plan: Recipe Learning System

## 🎯 Current Status (October 30, 2025)

### ✅ What We Have
1. **Task System** (Phase 4 complete)
   - `task/create`, `task/list`, `task/complete` commands
   - TaskEntity with domain-specific metadata
   - PersonaUsers can create self-improvement tasks

2. **Genome Training Command** (Phase 7.1 complete)
   - `genome/train` - Full LoRA fine-tuning via Unsloth
   - Supports chat conversation training data
   - Creates .safetensors adapter files

3. **Genome Learning Commands** (Phase 7.2 complete - just built!)
   - `genome/capture-interaction` - Record AI interactions
   - `genome/capture-feedback` - Record feedback between personas
   - `genome/batch-micro-tune` - Lightweight in-recipe training
   - `genome/multi-agent-learn` - Team learning from outcomes

4. **Architecture Docs**
   - RECIPE-EMBEDDED-LEARNING.md - Continuous learning design
   - RECIPE-LEARNING-DYNAMICS.md - Team dynamics & AI-orchestrated learning
   - ACADEMY_ARCHITECTURE.md - Competition & genomic evolution
   - PERSONA-CONVERGENCE-ROADMAP.md - PersonaUser autonomous loop

### ❌ What We Need
1. **TrainingDataAccumulator** - In-memory buffer in PersonaUser
2. **AI teaching commands** - ai/observe-team-work, ai/should-trigger-training
3. **Recipe teamDynamics** - Extend RecipeEntity schema
4. **Wire commands to PersonaUser** - Actually store/retrieve training data
5. **One working recipe** - End-to-end learning demonstration

---

## 📋 Phase-by-Phase Plan

### Phase 7.3: Fix TypeScript & Deploy Learning Commands (IMMEDIATE)

**Problem**: Genome commands have TypeScript errors (missing CommandParams extension)

**Tasks**:
1. Fix all genome command Params types to extend CommandParams
2. Run `npm start` successfully
3. Verify commands registered in generated.ts

**Files**:
- `commands/genome/*/shared/*Types.ts` - Add context/sessionId
- Test with `./jtag genome/capture-interaction --help`

**Time**: 15 minutes

---

### Phase 7.4: TrainingDataAccumulator (FOUNDATION)

**Goal**: PersonaUsers can accumulate training examples in RAM

**Implementation**:
```typescript
// system/user/server/modules/TrainingDataAccumulator.ts
export class TrainingDataAccumulator {
  private domainBuffers: Map<string, TrainingExample[]> = new Map();
  private batchThresholds: Map<string, number> = new Map();

  async captureInteraction(capture: InteractionCapture): Promise<void> {
    // Store in appropriate domain buffer
  }

  async captureFeedback(feedback: FeedbackCapture): Promise<void> {
    // Attach to most recent interaction
  }

  shouldMicroTune(domain: string): boolean {
    // Check batch threshold
  }

  async consumeTrainingData(domain: string): Promise<TrainingExample[]> {
    // Return and clear buffer
  }
}

// Add to PersonaUser
class PersonaUser {
  trainingAccumulator: TrainingDataAccumulator;

  constructor() {
    this.trainingAccumulator = new TrainingDataAccumulator();
  }
}
```

**Wire genome commands**:
```typescript
// In GenomeCaptureInteractionServerCommand.execute()
const personaUser = await this.getPersonaUser(context.userId);
await personaUser.trainingAccumulator.captureInteraction(captureParams);
```

**Test**:
```bash
# Should accumulate in memory
./jtag genome/capture-interaction --roleId="test-ai" --domain="test" --input="test" --output="test"

# Check buffer (via debug command)
./jtag debug/persona-training-buffer --personaId="test-ai"
```

**Time**: 2-3 hours

---

### Phase 7.5: AI Teaching Commands (INTELLIGENCE)

**Goal**: AI makes training decisions, not hard-coded rules

**New Commands**:

1. **ai/observe-team-work**
   ```typescript
   // Takes team context, returns teaching decisions
   interface ObserveTeamWorkParams {
     teamContext: {
       developer: { code, performance },
       reviewer: { review, accuracy },
       qa: { tests, coverage }
     };
     observePrompt: string;  // AI prompt for observation
   }

   interface ObserveTeamWorkResult {
     teachingDecisions: {
       developerFeedback: string;
       peerCoaching: string;
       qaFeedback: string;
       developerCodeQuality: number;
       peerReviewQuality: number;
       qaTestQuality: number;
     };
   }
   ```

2. **ai/should-trigger-training**
   ```typescript
   // AI decides training parameters
   interface ShouldTriggerTrainingParams {
     personaId: UUID;
     domain: string;
     accumulatedExamples: number;
     recentPerformance: PerformanceMetrics;
     decisionPrompt: string;  // AI prompt for decision
   }

   interface ShouldTriggerTrainingResult {
     shouldTrain: boolean;
     reasoning: string;
     trainingParams?: {
       learningRate: number;
       epochs: number;
       selectedExamples: number[];
       method: 'lora' | 'prompt' | 'rag';
     };
   }
   ```

**Implementation**: Use existing ai/generate command infrastructure

**Time**: 3-4 hours

---

### Phase 7.6: Recipe TeamDynamics Schema (STRUCTURE)

**Goal**: Recipes formally specify team dynamics

**Extend RecipeEntity**:
```typescript
// system/recipes/shared/RecipeTypes.ts
interface RecipeDefinition {
  // Existing fields...
  pipeline: RecipeStep[];
  ragTemplate: RAGTemplate;
  strategy: RecipeStrategy;

  // NEW: Team dynamics
  teamDynamics?: RecipTeamDynamics;
}

interface RecipeTeamDynamics {
  roles: {
    [roleId: string]: {
      type: 'student' | 'teacher' | 'peer' | 'validator' | 'static';
      learns: boolean;
      teaches?: boolean;
      learningDomain?: string;
    };
  };

  coordinationPattern: 'collaborative' | 'competitive' | 'hierarchical';
  decisionMaking: 'consensus' | 'teacher-led' | 'democratic';

  learningDynamics?: {
    orchestrator: string;  // Role ID who makes teaching decisions
    teachingStyle?: string;
    feedbackTiming?: 'immediate' | 'end-of-recipe' | 'batch';
    adaptiveDifficulty?: boolean;
  };
}
```

**Migration**: Existing recipes work without teamDynamics (backward compatible)

**Time**: 1-2 hours

---

### Phase 7.7: One Working Recipe (PROOF OF CONCEPT)

**Goal**: End-to-end learning in simplest possible scenario

**Recipe**: General chat with teacher observing

```json
{
  "uniqueId": "general-chat-with-learning-v1",
  "name": "General Chat (Teacher-Observed Learning)",

  "teamDynamics": {
    "roles": {
      "assistant": { "type": "student", "learns": true, "learningDomain": "conversation" },
      "teacher": { "type": "teacher", "teaches": true, "learns": false }
    },
    "learningDynamics": {
      "orchestrator": "teacher",
      "teachingStyle": "observe-and-coach",
      "feedbackTiming": "immediate"
    }
  },

  "pipeline": [
    { "command": "rag/build", "outputTo": "context" },
    { "command": "ai/should-respond", "outputTo": "decision" },
    { "command": "ai/generate", "assignedRole": "assistant", "outputTo": "response" },

    { "command": "genome/capture-interaction", "params": {
        "roleId": "assistant",
        "domain": "conversation",
        "input": "{{context}}",
        "output": "{{response}}"
      }
    },

    { "command": "ai/post-message" },

    // Teacher observes (if present in room)
    { "command": "ai/observe-student-response", "assignedRole": "teacher", "params": {
        "observePrompt": "Student responded: {{response}}. Context was: {{context}}. Was this good? Should they learn something?"
      }, "outputTo": "teacherObservation",
      "condition": "room.hasRole('teacher')"
    },

    { "command": "genome/capture-feedback", "params": {
        "targetRole": "assistant",
        "feedbackRole": "teacher",
        "feedbackContent": "{{teacherObservation.feedback}}",
        "qualityScore": "{{teacherObservation.quality}}"
      },
      "condition": "teacherObservation && teacherObservation.hasFeedback"
    },

    // Check if ready to train
    { "command": "ai/should-trigger-training", "assignedRole": "teacher", "params": {
        "personaId": "{{assistant.personaId}}",
        "domain": "conversation",
        "decisionPrompt": "Assistant has {{assistant.exampleCount}} examples. Should we train their LoRA now?"
      }, "outputTo": "trainingDecision",
      "condition": "messageCount % 10 === 0"
    },

    { "command": "genome/batch-micro-tune", "params": {
        "roleId": "assistant",
        "domain": "conversation"
      },
      "condition": "trainingDecision && trainingDecision.shouldTrain"
    }
  ]
}
```

**Test Flow**:
1. Create teacher AI and assistant AI in general room
2. Have conversation with 10+ messages
3. Teacher AI observes, provides feedback
4. After 10 messages, teacher decides whether to train
5. If trained, assistant AI should improve
6. Verify with more conversation

**Success Criteria**:
- Training data accumulates in assistant AI's buffer
- Teacher AI makes reasonable training decisions
- Training actually triggers (even if just logs for now)
- Can observe the learning loop working

**Time**: 4-5 hours (includes debugging)

---

### Phase 7.8: Multi-Agent Team Recipe (EXPANSION)

**Goal**: Multiple personas learning from each other

**Recipe**: Code review team

**Test with**: Developer AI + Reviewer AI + QA AI
- Developer generates code
- Reviewer critiques (and learns to review better!)
- QA validates (and learns to test better!)
- All improve through multi-agent-learn

**Time**: 3-4 hours

---

### Phase 7.9: Visual/Screenshot Learning (MODALITY)

**Goal**: Design improvement through visual feedback

**Recipe**: Design committee

**Test with**: Designer AI + 2 critic AIs
- Designer generates CSS
- Take screenshot
- Critics evaluate visual quality (AI judgment!)
- Designer learns from committee feedback

**Time**: 3-4 hours

---

## 📊 Total Timeline Estimate

| Phase | Description | Time | Dependencies |
|-------|-------------|------|--------------|
| 7.3 | Fix TypeScript errors | 15 min | None |
| 7.4 | TrainingDataAccumulator | 2-3 hrs | 7.3 |
| 7.5 | AI teaching commands | 3-4 hrs | 7.4 |
| 7.6 | Recipe teamDynamics schema | 1-2 hrs | None (parallel) |
| 7.7 | One working recipe | 4-5 hrs | 7.4, 7.5, 7.6 |
| 7.8 | Multi-agent recipe | 3-4 hrs | 7.7 |
| 7.9 | Visual learning | 3-4 hrs | 7.7 |

**Total**: ~17-23 hours of implementation

**Critical Path**: 7.3 → 7.4 → 7.5 → 7.7 (MVP in ~10 hours)

---

## 🎯 Immediate Next Steps (Right Now)

1. **Fix TypeScript errors** (15 min)
   - Extend CommandParams in all genome command types
   - Deploy successfully

2. **Create TrainingDataAccumulator** (2-3 hrs)
   - Simple in-memory buffer
   - Wire to genome commands
   - Test accumulation works

3. **Create one teaching command** (2-3 hrs)
   - Start with ai/should-trigger-training
   - Use existing AI infrastructure
   - Test AI makes sensible decisions

4. **Test end-to-end** (2-3 hrs)
   - Simple chat scenario
   - Teacher observes student
   - Training decision gets made
   - Validate the loop works

**Goal**: Working proof-of-concept in ~8 hours!

---

## 🔑 Key Decisions Needed

1. **Where should TrainingDataAccumulator live?**
   - Option A: In PersonaUser directly
   - Option B: Separate service accessed by PersonaUser
   - **Recommendation**: A (simpler, faster)

2. **How to access PersonaUser from commands?**
   - Need UserDaemon integration
   - Commands need `getPersonaUser(userId)` helper

3. **Should we use real LoRA training in Phase 7.7?**
   - Option A: Yes, call genome/train with accumulated examples
   - Option B: No, just log "would train" for now
   - **Recommendation**: B for MVP, A for Phase 7.8

4. **How to test AI teaching decisions?**
   - Manual inspection initially
   - Later: metrics on learning effectiveness

---

## ✅ Definition of Done

**System is working when**:
1. PersonaUsers accumulate training data during recipes
2. Teacher AIs make training decisions with reasoning
3. Training happens based on AI decisions (not fixed rules)
4. Can demonstrate learning improvement over time
5. Multiple personas can learn simultaneously in one recipe
6. Committee-based evaluation works with screenshots

**We'll know we succeeded when**: A designer AI improves at CSS through committee feedback, without any hard-coded rules!
