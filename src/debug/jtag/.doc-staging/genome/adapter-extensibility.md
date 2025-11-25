# Recipe Prompt Adapter Extensibility

## The Adapter Pattern for Multi-Domain AI

The RecipePromptBuilder uses the **Adapter Pattern** to enable easy extension to new domains without modifying core code.

### Current Adapters (Chat Domain)
- `GatingPromptAdapter` - AI decides whether to respond
- `GenerationPromptAdapter` - AI generates response text

### Future Domain Adapters

#### 🎮 Game Domain
```typescript
class GamePromptAdapter implements PromptAdapter<GamePromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: GamePromptContext): string {
    return [
      PromptSectionBuilder.buildHeader(
        context.personaName,
        context.conversationPattern,
        'Analyze the game state and decide your next move.'
      ),
      this.buildGameStateSection(context.gameState),
      this.buildValidMovesSection(context.validMoves),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      this.buildMoveOutputFormat()
    ].join('\n\n');
  }

  private buildGameStateSection(gameState: GameState): string {
    return `**Current Game State:**
Board: ${gameState.board}
Your pieces: ${gameState.myPieces}
Opponent pieces: ${gameState.opponentPieces}
Score: ${gameState.score}`;
  }
}
```

#### 🤖 Robotics Domain
```typescript
class RoboticsPromptAdapter implements PromptAdapter<RoboticsPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: RoboticsPromptContext): string {
    return [
      PromptSectionBuilder.buildHeader(
        context.personaName,
        context.conversationPattern,
        'Control the robot based on sensor data and mission objectives.'
      ),
      this.buildSensorDataSection(context.sensorData),
      this.buildMissionObjectivesSection(context.mission),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      this.buildCommandOutputFormat()
    ].join('\n\n');
  }
}
```

#### 🎥 Video/3D Domain
```typescript
class Video3DPromptAdapter implements PromptAdapter<Video3DPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: Video3DPromptContext): string {
    return [
      PromptSectionBuilder.buildHeader(
        context.personaName,
        context.conversationPattern,
        'Understand the 3D scene and generate camera movements.'
      ),
      this.buildSceneDescriptionSection(context.sceneGraph),
      this.buildCameraStateSection(context.cameraState),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      this.buildCameraCommandFormat()
    ].join('\n\n');
  }
}
```

#### 🎓 Academy LoRA Training Domain (GAN-like Teacher/Student)
```typescript
class AcademyTeacherPromptAdapter implements PromptAdapter<AcademyPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: AcademyPromptContext): string {
    return [
      PromptSectionBuilder.buildHeader(
        context.personaName,
        'teaching', // conversation pattern
        'Evaluate the student response and provide targeted feedback.'
      ),
      this.buildLearningObjectivesSection(context.objectives),
      this.buildStudentResponseSection(context.studentResponse),
      this.buildPerformanceMetricsSection(context.previousAttempts),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      this.buildFeedbackOutputFormat()
    ].join('\n\n');
  }

  private buildLearningObjectivesSection(objectives: LearningObjective[]): string {
    return `**Learning Objectives:**
${objectives.map((obj, i) => `${i + 1}. ${obj.skill} (Target: ${obj.targetAccuracy}%)`).join('\n')}`;
  }

  private buildStudentResponseSection(response: StudentResponse): string {
    return `**Student Response:**
Question: ${response.question}
Answer: ${response.answer}
Confidence: ${response.confidence}%`;
  }

  private buildPerformanceMetricsSection(attempts: TrainingAttempt[]): string {
    const recentAccuracy = attempts.slice(-5).filter(a => a.correct).length / 5 * 100;
    return `**Performance Metrics:**
Recent Accuracy: ${recentAccuracy.toFixed(1)}%
Total Attempts: ${attempts.length}
Improvement Trend: ${this.calculateTrend(attempts)}`;
  }

  private buildFeedbackOutputFormat(): string {
    return `**Your Feedback (JSON):**
{
  "isCorrect": true,
  "score": 85,  // 0-100
  "feedback": "Detailed explanation of what was good/bad",
  "hint": "Next step for improvement (optional)",
  "adjustDifficulty": "increase" | "decrease" | "maintain"
}`;
  }
}

class AcademyStudentPromptAdapter implements PromptAdapter<AcademyPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: AcademyPromptContext): string {
    return [
      PromptSectionBuilder.buildHeader(
        context.personaName,
        'teaching',
        'Answer the training question to the best of your ability.'
      ),
      this.buildLearningObjectivesSection(context.objectives),
      this.buildCurrentQuestionSection(context.currentQuestion),
      this.buildRecentFeedbackSection(context.recentFeedback),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      this.buildAnswerOutputFormat()
    ].join('\n\n');
  }

  private buildRecentFeedbackSection(feedback: TeacherFeedback[]): string {
    if (feedback.length === 0) {
      return '**Recent Feedback:** None yet.';
    }

    const latest = feedback[feedback.length - 1];
    return `**Recent Feedback:**
Score: ${latest.score}/100
Feedback: ${latest.feedback}
${latest.hint ? `Hint: ${latest.hint}` : ''}`;
  }
}
```

**Academy Recipe Example** (GAN-like dynamics):
```json
{
  "uniqueId": "academy-typescript-training",
  "name": "TypeScript Mastery Training",
  "conversationPattern": "teaching",

  "pipeline": [
    {
      "command": "academy/generate-question",
      "params": { "difficulty": "$currentDifficulty", "skill": "typescript" },
      "outputTo": "question"
    },
    {
      "command": "academy/student-answer",
      "params": { "question": "$question", "studentId": "$personaId" },
      "outputTo": "studentResponse"
    },
    {
      "command": "academy/teacher-evaluate",
      "params": {
        "response": "$studentResponse",
        "objectives": "$learningObjectives"
      },
      "outputTo": "evaluation"
    },
    {
      "command": "academy/update-lora-weights",
      "params": {
        "evaluation": "$evaluation",
        "genomeId": "$genomeId"
      },
      "condition": "evaluation.score >= 80"
    }
  ],

  "strategy": {
    "responseRules": [
      "Teacher: Be constructive, not punitive",
      "Teacher: Adjust difficulty based on student performance",
      "Student: Show your reasoning, not just answers",
      "Student: Learn from mistakes iteratively"
    ],
    "decisionCriteria": [
      "Is the student improving over time?",
      "Is the difficulty level appropriate?",
      "Should LoRA weights be updated?"
    ]
  }
}
```

The GAN-like dynamic:
- **Teacher (Discriminator)**: Evaluates student responses, provides feedback
- **Student (Generator)**: Attempts to improve responses based on feedback
- **LoRA Updates**: Student's genome (LoRA weights) updated when performance threshold met
- **Adaptive Difficulty**: Teacher adjusts question difficulty based on student performance

#### 💻 Programming Domain
```typescript
class ProgrammingPromptAdapter implements PromptAdapter<ProgrammingPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: ProgrammingPromptContext): string {
    return [
      PromptSectionBuilder.buildHeader(
        context.personaName,
        context.conversationPattern,
        'Write code to solve the programming task.'
      ),
      this.buildCodeContextSection(context.codebase),
      this.buildTaskSpecSection(context.taskDescription),
      this.buildCompilerErrorsSection(context.compilerErrors),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      this.buildCodeOutputFormat()
    ].join('\n\n');
  }
}
```

### How to Add a New Domain

1. **Define Context Type**
```typescript
export interface YourDomainPromptContext extends BasePromptContext {
  domainSpecificField: YourType;
}
```

2. **Create Adapter**
```typescript
export class YourDomainPromptAdapter implements PromptAdapter<YourDomainPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: YourDomainPromptContext): string {
    // Build sections specific to your domain
    return sections.join('\n\n');
  }
}
```

3. **Register Adapter**
```typescript
RecipePromptBuilder.registerYourDomainAdapter(new YourDomainPromptAdapter());
```

4. **Use It**
```typescript
const prompt = yourDomainAdapter.buildPrompt(strategy, context);
```

### Key Benefits

✅ **Zero Modification** - Add new domains without touching existing code
✅ **Shared Components** - Reuse PromptSectionBuilder for common sections
✅ **Type Safety** - Each adapter has its own context type
✅ **Testable** - Unit test each adapter in isolation
✅ **Composable** - Adapters can delegate to section builders

### Architectural Pattern

```
PromptAdapter<TContext> (interface)
    ↑
    ├── GatingPromptAdapter (chat gating)
    ├── GenerationPromptAdapter (chat generation)
    ├── GamePromptAdapter (game moves)
    ├── RoboticsPromptAdapter (robot commands)
    ├── Video3DPromptAdapter (camera control)
    └── ProgrammingPromptAdapter (code generation)

All adapters share:
    - PromptSectionBuilder (reusable sections)
    - RecipeStrategy (recipe rules)
    - BasePromptContext (common fields)
```

This pattern enables the system to scale to ANY domain (games, video, 3D worlds, programming, robotics, etc.) without architectural rewrites.
