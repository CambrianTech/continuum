# Reasoning System Implementation Roadmap

**Date**: 2025-11-16
**Status**: Planning phase - Not yet implemented
**Goal**: Transform PersonaUsers from workflows to true agents via reasoning system

---

## Executive Summary

**The Problem**: PersonaUsers currently follow fixed patterns (workflows):
- Receive event → Process → Respond
- No planning, no adaptation, no learning
- Brittle: Errors crash or loop infinitely
- Mindless: Each inference starts from scratch

**The Solution**: Add reasoning system (makes them agents):
- Plan before acting (Chain-of-Thought)
- Adapt plans when environment changes
- Recover autonomously from errors
- Learn from outcomes for future use

**The Impact**:
- **Resilience**: System doesn't break on unexpected input
- **Intelligence**: AIs get smarter over time through learning
- **Cost savings**: Skip bad approaches learned from past failures
- **Observability**: Can see AI's plan, adaptations, learnings

---

## Architecture: The Four Agent Components

Based on research paper "Building Autonomous LLM Agents":

### 1. Perception System ✅ ALREADY HAVE
- Commands.execute() for structured data
- Events.subscribe() for real-time updates
- Data layer for queries

### 2. Memory System ⚠️ DESIGNED, NOT IMPLEMENTED
- Working memory (Phase 2)
- Self-state (Phase 2)
- See: COGNITION-ARCHITECTURE.md

### 3. Reasoning System ❌ THIS ROADMAP
- Planning (formulate plans)
- Adaptation (adjust based on feedback)
- Evaluation (self-assess outcomes)
- Recovery (handle errors gracefully)

### 4. Action System ✅ ALREADY HAVE
- Commands.execute() for actions
- Domain-specific adapters (chat, code, game)

**What's missing**: Reasoning system (#3) + Memory system (#2)

**This roadmap**: How to build #3 (Reasoning System)

---

## Phase 1: Core Types and Interfaces

**Goal**: Define data structures for plans, tasks, evaluations

### 1.1 Create Types File

**File**: `system/user/server/modules/cognition/reasoning/types.ts`

```typescript
import type { UUID } from '@types/CrossPlatformUUID';

/**
 * Task: High-level goal that needs reasoning
 */
export interface Task {
  id: UUID;
  domain: 'chat' | 'code' | 'game' | 'academy';
  contextId: UUID;  // Room, file, session, etc.
  description: string;  // "Respond to user question about React hooks"
  priority: number;  // 0.0-1.0
  triggeredBy: UUID;  // Event that created this task
  createdAt: number;
}

/**
 * PlanStep: One step in a multi-step plan
 */
export interface PlanStep {
  stepNumber: number;
  action: string;  // "Read working memory for React context"
  expectedOutcome: string;  // "Retrieve last 5 React discussions"
  completed: boolean;
  completedAt?: number;
  result?: any;
}

/**
 * Plan: Structured approach to accomplish a task
 */
export interface Plan {
  id: UUID;
  taskId: UUID;
  goal: string;  // "Provide helpful React hooks explanation"

  // Chain-of-Thought reasoning
  learnings: string[];  // "I know user is beginner from past interactions"
  risks: string[];  // "Might be too technical", "Could overwhelm with details"

  // Execution steps
  steps: PlanStep[];
  currentStep: number;

  // Error handling
  contingencies: {
    [errorType: string]: string[];  // Fallback steps for anticipated errors
  };

  // Success criteria
  successCriteria: string[];  // "User understands useState", "Response is clear"

  // Metadata
  createdAt: number;
  lastAdjustedAt: number;
  previousAttempts: number;  // How many times we've replanned
  domain: string;
}

/**
 * ExecutionResult: Outcome of executing a plan step
 */
export interface ExecutionResult {
  success: boolean;
  output?: any;
  error?: Error;
  duration: number;  // milliseconds
  metadata?: any;
}

/**
 * PlanAdjustment: Decision about how to proceed after feedback
 */
export interface PlanAdjustment {
  action: 'CONTINUE' | 'CONTINGENCY' | 'REPLAN' | 'ABORT';
  updatedPlan: Plan;
  reasoning: string;  // Why this adjustment was made
}

/**
 * Evaluation: Self-assessment of task outcome
 */
export interface Evaluation {
  taskId: UUID;
  planId: UUID;

  // Did we succeed?
  meetsSuccessCriteria: boolean;
  criteriaBreakdown: Record<string, boolean>;  // Each criterion individually

  // What did we learn?
  whatWorked: string[];
  mistakes: string[];
  improvements: string[];
  extractedPattern: string;  // One-sentence lesson for future

  // Metadata
  evaluatedAt: number;
  duration: number;  // How long task took
  stepsExecuted: number;
  replansRequired: number;
}

/**
 * LearningEntry: Extracted knowledge from past experiences
 * (Stored in working memory with thoughtType='self-reflection')
 */
export interface LearningEntry {
  id: UUID;
  personaId: UUID;
  domain: string;

  // What was learned
  pattern: string;  // "When user asks about hooks, check their React level first"
  context: string;  // "React questions from beginners"

  // Evidence
  successCount: number;  // How many times this pattern worked
  failureCount: number;  // How many times it failed
  confidence: number;  // 0.0-1.0 based on success rate

  // Provenance
  learnedFrom: UUID[];  // Task IDs that contributed to this learning
  firstSeenAt: number;
  lastUsedAt: number;
  useCount: number;
}
```

**Tests**:
```bash
npx vitest tests/unit/reasoning-types.test.ts
```

**Verify**:
- All types have proper UUID usage
- No `any` types (except metadata fields which need flexibility)
- Clear JSDoc comments
- Imports use path aliases (@types/...)

---

### 1.2 Database Schema for Plans and Learnings

**Goal**: Persist plans and learnings across restarts

**File**: Update `daemons/data-daemon/server/EntityRegistry.ts`

```typescript
// Add to EntityRegistry
export const PLAN_SCHEMA = {
  id: 'TEXT PRIMARY KEY',
  taskId: 'TEXT NOT NULL',
  personaId: 'TEXT NOT NULL',
  goal: 'TEXT NOT NULL',
  learnings: 'TEXT',  // JSON array
  risks: 'TEXT',  // JSON array
  steps: 'TEXT NOT NULL',  // JSON array
  currentStep: 'INTEGER DEFAULT 0',
  contingencies: 'TEXT',  // JSON object
  successCriteria: 'TEXT',  // JSON array
  createdAt: 'INTEGER NOT NULL',
  lastAdjustedAt: 'INTEGER NOT NULL',
  previousAttempts: 'INTEGER DEFAULT 0',
  domain: 'TEXT NOT NULL',
  status: 'TEXT DEFAULT "active"',  // active, completed, aborted
};

export const LEARNING_SCHEMA = {
  id: 'TEXT PRIMARY KEY',
  personaId: 'TEXT NOT NULL',
  domain: 'TEXT NOT NULL',
  pattern: 'TEXT NOT NULL',
  context: 'TEXT NOT NULL',
  successCount: 'INTEGER DEFAULT 0',
  failureCount: 'INTEGER DEFAULT 0',
  confidence: 'REAL DEFAULT 0.0',
  learnedFrom: 'TEXT',  // JSON array of task IDs
  firstSeenAt: 'INTEGER NOT NULL',
  lastUsedAt: 'INTEGER NOT NULL',
  useCount: 'INTEGER DEFAULT 0'
};

// Register collections
registerCollection(COLLECTIONS.PERSONA_PLANS, PLAN_SCHEMA);
registerCollection(COLLECTIONS.PERSONA_LEARNINGS, LEARNING_SCHEMA);
```

**Update**: `system/shared/Constants.ts`

```typescript
export const COLLECTIONS = {
  // ... existing ...
  PERSONA_PLANS: 'persona_plans',
  PERSONA_LEARNINGS: 'persona_learnings'
};
```

**Tests**:
```bash
npx vitest tests/integration/plan-persistence.test.ts
npx vitest tests/integration/learning-persistence.test.ts
```

---

## Phase 2: Plan Formulation (Chain-of-Thought)

**Goal**: AI creates structured plans before acting

### 2.1 PlanFormulator Class

**File**: `system/user/server/modules/cognition/reasoning/PlanFormulator.ts`

```typescript
import type { Task, Plan, LearningEntry } from './types';
import type { WorkingMemoryManager } from '../WorkingMemoryManager';
import type { PersonaSelfState } from '../PersonaSelfState';

/**
 * PlanFormulator: Creates structured plans using Chain-of-Thought reasoning
 */
export class PlanFormulator {
  constructor(
    private personaId: UUID,
    private personaName: string,
    private workingMemory: WorkingMemoryManager,
    private selfState: PersonaSelfState,
    private llm: LLMClient  // Interface to AI provider
  ) {}

  /**
   * Generate a plan for a task
   *
   * Process:
   * 1. Retrieve relevant memories (what I know about this domain)
   * 2. Retrieve relevant learnings (patterns I've discovered)
   * 3. Chain-of-Thought reasoning with LLM
   * 4. Structure response into Plan format
   */
  async formulatePlan(task: Task): Promise<Plan> {
    // 1. Get relevant past experiences
    const memories = await this.workingMemory.recall({
      domain: task.domain,
      contextId: task.contextId,
      limit: 5,
      thoughtTypes: ['observation', 'decision', 'self-reflection']
    });

    // 2. Get applicable learnings
    const learnings = await this.retrieveLearnings(task);

    // 3. Get current self-state
    const myState = await this.selfState.get();

    // 4. Chain-of-Thought prompt
    const prompt = this.buildChainOfThoughtPrompt(task, memories, learnings, myState);

    // 5. Call LLM
    const response = await this.llm.generate({
      messages: [{ role: 'system', content: prompt }],
      responseFormat: { type: 'json_object' }
    });

    // 6. Parse and validate
    const planData = JSON.parse(response.content);

    // 7. Create Plan object
    const plan: Plan = {
      id: UUID.generate(),
      taskId: task.id,
      goal: planData.goal,
      learnings: planData.learnings || [],
      risks: planData.risks || [],
      steps: planData.steps.map((s, idx) => ({
        stepNumber: idx + 1,
        action: s.action,
        expectedOutcome: s.expected,
        completed: false
      })),
      currentStep: 0,
      contingencies: planData.contingencies || {},
      successCriteria: planData.successCriteria || [],
      createdAt: Date.now(),
      lastAdjustedAt: Date.now(),
      previousAttempts: 0,
      domain: task.domain
    };

    // 8. Persist plan
    await this.savePlan(plan);

    return plan;
  }

  private buildChainOfThoughtPrompt(
    task: Task,
    memories: any[],
    learnings: LearningEntry[],
    selfState: any
  ): string {
    return `
You are ${this.personaName}.

YOUR TASK: ${task.description}

YOUR PAST EXPERIENCES WITH THIS DOMAIN:
${memories.map(m => `- ${m.thoughtContent}`).join('\n') || 'No past experiences'}

YOUR LEARNED PATTERNS:
${learnings.map(l => `- ${l.pattern} (confidence: ${l.confidence})`).join('\n') || 'No patterns yet'}

YOUR CURRENT STATE:
- Focus: ${selfState.currentFocus?.objective || 'None'}
- Cognitive load: ${selfState.cognitiveLoad}
- Preoccupations: ${selfState.activePreoccupations?.map(p => p.concern).join(', ') || 'None'}

THINK STEP BY STEP:

1. GOAL: What am I trying to achieve? (be specific and measurable)

2. LEARNINGS: What do I already know that's relevant?
   - Review your past experiences above
   - Identify patterns from your learned knowledge
   - What worked? What failed?

3. RISKS: What could go wrong?
   - Anticipate potential errors
   - Consider edge cases
   - Think about what assumptions might be wrong

4. APPROACH: How will I accomplish this?
   - Break into sequential steps
   - Each step should be concrete and executable
   - Include expected outcome for each step

5. CONTINGENCIES: If things go wrong, what's plan B?
   - For each risk, what's the fallback approach?
   - How will I recover from errors?

6. SUCCESS: How will I know I succeeded?
   - Specific, measurable criteria
   - What does "done" look like?

Respond in this EXACT JSON format:
{
  "goal": "specific measurable goal statement",
  "learnings": ["relevant thing I know", "another relevant thing"],
  "risks": ["potential problem 1", "potential problem 2"],
  "steps": [
    { "action": "concrete step 1", "expected": "what I expect to happen" },
    { "action": "concrete step 2", "expected": "what I expect to happen" }
  ],
  "contingencies": {
    "if_error_timeout": ["fallback step 1", "fallback step 2"],
    "if_error_rate_limit": ["different approach"]
  },
  "successCriteria": ["criterion 1", "criterion 2"]
}
`;
  }

  private async retrieveLearnings(task: Task): Promise<LearningEntry[]> {
    // Query learnings from database
    const learnings = await Commands.execute('data/list', {
      collection: COLLECTIONS.PERSONA_LEARNINGS,
      filter: {
        personaId: this.personaId,
        domain: task.domain,
        confidence: { $gte: 0.5 }  // Only high-confidence learnings
      },
      orderBy: [{ field: 'confidence', direction: 'desc' }],
      limit: 5
    });

    return learnings.entities as LearningEntry[];
  }

  private async savePlan(plan: Plan): Promise<void> {
    await Commands.execute('data/create', {
      collection: COLLECTIONS.PERSONA_PLANS,
      entity: plan
    });
  }
}
```

**Tests**:
```bash
npx vitest tests/unit/PlanFormulator.test.ts
# Test: Creates valid plan structure
# Test: Incorporates past learnings
# Test: Generates contingencies
# Test: Sets success criteria
```

---

## Phase 3: Plan Adaptation (Dynamic Replanning)

**Goal**: Adjust plans based on execution feedback

### 3.1 PlanAdapter Class

**File**: `system/user/server/modules/cognition/reasoning/PlanAdapter.ts`

```typescript
import type { Plan, ExecutionResult, PlanAdjustment } from './types';

/**
 * PlanAdapter: Adjusts plans based on environmental feedback
 */
export class PlanAdapter {
  constructor(
    private personaId: UUID,
    private llm: LLMClient
  ) {}

  /**
   * Decide how to proceed after executing a step
   *
   * Options:
   * - CONTINUE: Step succeeded, move to next
   * - CONTINGENCY: Step failed, use pre-planned fallback
   * - REPLAN: Unexpected failure, generate new approach
   * - ABORT: Can't recover, give up
   */
  async adjustPlan(
    plan: Plan,
    result: ExecutionResult
  ): Promise<PlanAdjustment> {
    // Success case - continue
    if (result.success) {
      return {
        action: 'CONTINUE',
        updatedPlan: this.markStepComplete(plan, result),
        reasoning: `Step ${plan.currentStep + 1} succeeded. Proceeding to next step.`
      };
    }

    // Error case - check if we have contingency
    const errorType = this.classifyError(result.error!);
    const contingencyKey = `if_error_${errorType}`;

    if (plan.contingencies[contingencyKey]) {
      // Use pre-planned contingency
      return {
        action: 'CONTINGENCY',
        updatedPlan: this.injectContingency(plan, errorType),
        reasoning: `Encountered ${errorType}. Executing contingency plan.`
      };
    }

    // Unexpected error - need to replan
    if (plan.previousAttempts < 3) {
      // Try replanning (max 3 attempts)
      const recoveryPlan = await this.generateRecoveryPlan(plan, result.error!);
      return {
        action: 'REPLAN',
        updatedPlan: recoveryPlan,
        reasoning: `Unexpected ${errorType}. Generated recovery plan (attempt ${plan.previousAttempts + 1}).`
      };
    }

    // Too many failures - abort
    return {
      action: 'ABORT',
      updatedPlan: plan,
      reasoning: `Failed after ${plan.previousAttempts} attempts. Aborting task.`
    };
  }

  private classifyError(error: Error): string {
    const msg = error.message.toLowerCase();

    if (msg.includes('timeout')) return 'timeout';
    if (msg.includes('rate limit')) return 'rate_limit';
    if (msg.includes('not found') || msg.includes('404')) return 'missing_resource';
    if (msg.includes('permission') || msg.includes('unauthorized')) return 'access_denied';
    if (msg.includes('network') || msg.includes('connection')) return 'network_error';

    return 'unknown';
  }

  private markStepComplete(plan: Plan, result: ExecutionResult): Plan {
    const updatedSteps = [...plan.steps];
    updatedSteps[plan.currentStep] = {
      ...updatedSteps[plan.currentStep],
      completed: true,
      completedAt: Date.now(),
      result: result.output
    };

    return {
      ...plan,
      steps: updatedSteps,
      currentStep: plan.currentStep + 1,
      lastAdjustedAt: Date.now()
    };
  }

  private injectContingency(plan: Plan, errorType: string): Plan {
    const contingencySteps = plan.contingencies[`if_error_${errorType}`];

    // Convert contingency strings into PlanSteps
    const newSteps = contingencySteps.map((action, idx) => ({
      stepNumber: plan.currentStep + idx + 1,
      action,
      expectedOutcome: 'Recovery from error',
      completed: false
    }));

    // Insert contingency steps after current failed step
    const updatedSteps = [
      ...plan.steps.slice(0, plan.currentStep + 1),
      ...newSteps,
      ...plan.steps.slice(plan.currentStep + 1)
    ];

    return {
      ...plan,
      steps: updatedSteps,
      lastAdjustedAt: Date.now()
    };
  }

  private async generateRecoveryPlan(plan: Plan, error: Error): Promise<Plan> {
    // Store failure in working memory
    await Commands.execute('data/create', {
      collection: COLLECTIONS.PERSONA_WORKING_MEMORY,
      entity: {
        id: UUID.generate(),
        personaId: this.personaId,
        domain: plan.domain,
        contextId: plan.taskId,
        thoughtType: 'observation',
        thoughtContent: `Plan failed: ${plan.goal}. Error: ${error.message}`,
        importance: 0.8,
        createdAt: Date.now()
      }
    });

    // Ask LLM for recovery approach
    const prompt = `
SITUATION: Your plan failed unexpectedly.

ORIGINAL GOAL: ${plan.goal}
STEPS COMPLETED: ${plan.steps.filter(s => s.completed).length}/${plan.steps.length}
FAILED AT: ${plan.steps[plan.currentStep]?.action}
ERROR: ${error.message}

ANALYZE:
1. Why did this fail? What assumption was wrong?
2. What's a different approach that avoids this error?
3. Should we simplify the goal or pivot strategy?

Generate a NEW plan in the same JSON format as before.
`;

    const response = await this.llm.generate({
      messages: [{ role: 'system', content: prompt }],
      responseFormat: { type: 'json_object' }
    });

    const newPlanData = JSON.parse(response.content);

    return {
      ...plan,
      goal: newPlanData.goal,
      steps: newPlanData.steps.map((s, idx) => ({
        stepNumber: idx + 1,
        action: s.action,
        expectedOutcome: s.expected,
        completed: false
      })),
      currentStep: 0,
      contingencies: newPlanData.contingencies || {},
      successCriteria: newPlanData.successCriteria || plan.successCriteria,
      lastAdjustedAt: Date.now(),
      previousAttempts: plan.previousAttempts + 1
    };
  }
}
```

**Tests**:
```bash
npx vitest tests/unit/PlanAdapter.test.ts
# Test: Continues on success
# Test: Uses contingency on anticipated error
# Test: Replans on unexpected error
# Test: Aborts after max retries
```

---

## Phase 4: Outcome Evaluation (Self-Assessment)

**Goal**: AI learns from what worked/failed

### 4.1 OutcomeEvaluator Class

**File**: `system/user/server/modules/cognition/reasoning/OutcomeEvaluator.ts`

```typescript
import type { Plan, ExecutionResult, Evaluation, LearningEntry } from './types';

/**
 * OutcomeEvaluator: Self-assesses task outcomes to extract learnings
 */
export class OutcomeEvaluator {
  constructor(
    private personaId: UUID,
    private llm: LLMClient
  ) {}

  /**
   * Evaluate task outcome and extract learnings
   */
  async evaluateOutcome(
    plan: Plan,
    finalResult: ExecutionResult
  ): Promise<Evaluation> {
    const prompt = this.buildEvaluationPrompt(plan, finalResult);

    const response = await this.llm.generate({
      messages: [{ role: 'system', content: prompt }],
      responseFormat: { type: 'json_object' }
    });

    const evalData = JSON.parse(response.content);

    const evaluation: Evaluation = {
      taskId: plan.taskId,
      planId: plan.id,
      meetsSuccessCriteria: evalData.meetsSuccessCriteria,
      criteriaBreakdown: evalData.criteriaBreakdown,
      whatWorked: evalData.whatWorked,
      mistakes: evalData.mistakes,
      improvements: evalData.improvements,
      extractedPattern: evalData.extractedPattern,
      evaluatedAt: Date.now(),
      duration: Date.now() - plan.createdAt,
      stepsExecuted: plan.steps.filter(s => s.completed).length,
      replansRequired: plan.previousAttempts
    };

    // Store evaluation in working memory
    await this.storeEvaluation(evaluation, plan.domain);

    // Update or create learning entry
    await this.updateLearnings(evaluation, plan);

    return evaluation;
  }

  private buildEvaluationPrompt(plan: Plan, result: ExecutionResult): string {
    return `
TASK COMPLETED

GOAL: ${plan.goal}

RESULT:
- Success: ${result.success}
- Output: ${JSON.stringify(result.output)}
- Duration: ${result.duration}ms
- Steps executed: ${plan.steps.filter(s => s.completed).length}/${plan.steps.length}
- Replans required: ${plan.previousAttempts}

SUCCESS CRITERIA:
${plan.successCriteria.map((c, i) => `${i + 1}. ${c}`).join('\n')}

SELF-EVALUATE:

1. Did I meet EACH success criterion? Go through them one by one.

2. What worked well? What steps/approaches were effective?

3. What mistakes did I make? What would I do differently?

4. What pattern can I extract for future similar tasks?
   (One clear sentence that captures the lesson learned)

Respond in this EXACT JSON format:
{
  "meetsSuccessCriteria": true/false,
  "criteriaBreakdown": {
    "criterion 1 text": true,
    "criterion 2 text": false,
    ...
  },
  "whatWorked": ["effective thing 1", "effective thing 2"],
  "mistakes": ["mistake 1", "mistake 2"],
  "improvements": ["improvement 1", "improvement 2"],
  "extractedPattern": "One-sentence lesson learned"
}
`;
  }

  private async storeEvaluation(evaluation: Evaluation, domain: string): Promise<void> {
    await Commands.execute('data/create', {
      collection: COLLECTIONS.PERSONA_WORKING_MEMORY,
      entity: {
        id: UUID.generate(),
        personaId: this.personaId,
        domain,
        contextId: evaluation.taskId,
        thoughtType: 'self-reflection',
        thoughtContent: `Learned: ${evaluation.extractedPattern}`,
        importance: 0.9,  // High importance
        metadata: {
          evaluation,
          whatWorked: evaluation.whatWorked,
          mistakes: evaluation.mistakes
        },
        createdAt: Date.now()
      }
    });
  }

  private async updateLearnings(evaluation: Evaluation, plan: Plan): Promise<void> {
    // Check if similar learning already exists
    const existingLearnings = await Commands.execute('data/list', {
      collection: COLLECTIONS.PERSONA_LEARNINGS,
      filter: {
        personaId: this.personaId,
        domain: plan.domain
      }
    });

    // Find similar pattern (simple string similarity for now)
    const similarLearning = existingLearnings.entities.find((l: LearningEntry) =>
      this.areSimilar(l.pattern, evaluation.extractedPattern)
    );

    if (similarLearning) {
      // Update existing learning
      const success = evaluation.meetsSuccessCriteria ? 1 : 0;
      const failure = evaluation.meetsSuccessCriteria ? 0 : 1;

      const updated: LearningEntry = {
        ...similarLearning,
        successCount: similarLearning.successCount + success,
        failureCount: similarLearning.failureCount + failure,
        confidence: (similarLearning.successCount + success) /
                   (similarLearning.successCount + similarLearning.failureCount + success + failure),
        learnedFrom: [...similarLearning.learnedFrom, plan.taskId],
        lastUsedAt: Date.now(),
        useCount: similarLearning.useCount + 1
      };

      await Commands.execute('data/update', {
        collection: COLLECTIONS.PERSONA_LEARNINGS,
        id: similarLearning.id,
        entity: updated
      });
    } else {
      // Create new learning
      const newLearning: LearningEntry = {
        id: UUID.generate(),
        personaId: this.personaId,
        domain: plan.domain,
        pattern: evaluation.extractedPattern,
        context: plan.goal,
        successCount: evaluation.meetsSuccessCriteria ? 1 : 0,
        failureCount: evaluation.meetsSuccessCriteria ? 0 : 1,
        confidence: evaluation.meetsSuccessCriteria ? 1.0 : 0.0,
        learnedFrom: [plan.taskId],
        firstSeenAt: Date.now(),
        lastUsedAt: Date.now(),
        useCount: 1
      };

      await Commands.execute('data/create', {
        collection: COLLECTIONS.PERSONA_LEARNINGS,
        entity: newLearning
      });
    }
  }

  private areSimilar(pattern1: string, pattern2: string): boolean {
    // Simple similarity check (can be improved with embeddings later)
    const normalize = (s: string) => s.toLowerCase().replace(/[^\w\s]/g, '');
    const p1 = normalize(pattern1);
    const p2 = normalize(pattern2);

    // Jaccard similarity of words
    const words1 = new Set(p1.split(/\s+/));
    const words2 = new Set(p2.split(/\s+/));
    const intersection = new Set([...words1].filter(w => words2.has(w)));
    const union = new Set([...words1, ...words2]);

    return intersection.size / union.size > 0.5;  // 50% overlap
  }
}
```

**Tests**:
```bash
npx vitest tests/unit/OutcomeEvaluator.test.ts
# Test: Evaluates success correctly
# Test: Stores evaluation in working memory
# Test: Creates new learning
# Test: Updates existing similar learning
# Test: Adjusts confidence based on success/failure
```

---

## Phase 5: PersonaReasoningSystem Integration

**Goal**: Combine all components into unified system

### 5.1 Main Reasoning System Class

**File**: `system/user/server/modules/cognition/reasoning/PersonaReasoningSystem.ts`

```typescript
import { PlanFormulator } from './PlanFormulator';
import { PlanAdapter } from './PlanAdapter';
import { OutcomeEvaluator } from './OutcomeEvaluator';
import type { Task, Plan, ExecutionResult, Evaluation, PlanAdjustment } from './types';

/**
 * PersonaReasoningSystem: Main orchestrator for agent reasoning
 *
 * Combines planning, adaptation, and evaluation into complete agent behavior
 */
export class PersonaReasoningSystem {
  private formulator: PlanFormulator;
  private adapter: PlanAdapter;
  private evaluator: OutcomeEvaluator;

  constructor(
    private personaId: UUID,
    private personaName: string,
    private workingMemory: WorkingMemoryManager,
    private selfState: PersonaSelfState,
    private llm: LLMClient
  ) {
    this.formulator = new PlanFormulator(
      personaId,
      personaName,
      workingMemory,
      selfState,
      llm
    );

    this.adapter = new PlanAdapter(personaId, llm);
    this.evaluator = new OutcomeEvaluator(personaId, llm);
  }

  /**
   * PLANNING: Create plan for task
   */
  async formulatePlan(task: Task): Promise<Plan> {
    console.log(`🧠 [Reasoning] Formulating plan for: ${task.description}`);
    return await this.formulator.formulatePlan(task);
  }

  /**
   * ADAPTATION: Adjust plan based on execution result
   */
  async adjustPlan(plan: Plan, result: ExecutionResult): Promise<PlanAdjustment> {
    return await this.adapter.adjustPlan(plan, result);
  }

  /**
   * EVALUATION: Self-assess outcome and extract learnings
   */
  async evaluateOutcome(plan: Plan, finalResult: ExecutionResult): Promise<Evaluation> {
    console.log(`📊 [Reasoning] Evaluating outcome for: ${plan.goal}`);
    return await this.evaluator.evaluateOutcome(plan, finalResult);
  }

  /**
   * INTROSPECTION: Get current plan for persona
   */
  async getCurrentPlan(): Promise<Plan | null> {
    const plans = await Commands.execute('data/list', {
      collection: COLLECTIONS.PERSONA_PLANS,
      filter: {
        personaId: this.personaId,
        status: 'active'
      },
      orderBy: [{ field: 'createdAt', direction: 'desc' }],
      limit: 1
    });

    return plans.entities[0] || null;
  }

  /**
   * INTROSPECTION: Get learnings for domain
   */
  async getLearnings(domain: string, limit: number = 10): Promise<LearningEntry[]> {
    const learnings = await Commands.execute('data/list', {
      collection: COLLECTIONS.PERSONA_LEARNINGS,
      filter: {
        personaId: this.personaId,
        domain
      },
      orderBy: [
        { field: 'confidence', direction: 'desc' },
        { field: 'useCount', direction: 'desc' }
      ],
      limit
    });

    return learnings.entities as LearningEntry[];
  }
}
```

---

## Phase 6: PersonaUser Integration

**Goal**: Wire reasoning system into PersonaUser

### 6.1 Update PersonaUser Class

**File**: `system/user/server/PersonaUser.ts`

```typescript
import { PersonaReasoningSystem } from './modules/cognition/reasoning/PersonaReasoningSystem';

export class PersonaUser extends AIUser {
  private reasoning: PersonaReasoningSystem;

  async initialize(): Promise<void> {
    // ... existing initialization ...

    // Initialize reasoning system
    this.reasoning = new PersonaReasoningSystem(
      this.entity.id,
      this.entity.name,
      this.workingMemory,
      this.selfState,
      this.llm
    );
  }

  /**
   * NEW: Process domain event with reasoning
   *
   * This replaces the old reactive pattern with agent pattern
   */
  async processDomainEvent(domain: string, event: DomainEvent): Promise<void> {
    // 1. Parse event as task
    const task = this.parseEventAsTask(domain, event);

    // 2. Check if should engage (Phase 3)
    const decision = await this.shouldEngageWith(domain, event);
    if (!decision.shouldEngage) {
      console.log(`💤 [${this.entity.name}] Ignoring ${domain} event: ${decision.reasoning}`);
      return;
    }

    // 3. Formulate plan (NEW - Phase 3.5)
    const plan = await this.reasoning.formulatePlan(task);
    console.log(`📋 [${this.entity.name}] Plan: ${plan.goal}`);
    console.log(`   Steps: ${plan.steps.map(s => s.action).join(' → ')}`);

    // 4. Execute plan with adaptation
    let currentPlan = plan;
    let finalResult: ExecutionResult | null = null;

    for (let i = 0; i < currentPlan.steps.length; i++) {
      const step = currentPlan.steps[i];

      console.log(`⚙️ [${this.entity.name}] Executing step ${i + 1}: ${step.action}`);

      try {
        // Execute the step
        const result = await this.executeStep(step, domain);
        finalResult = result;

        // Check if we need to adjust plan
        const adjustment = await this.reasoning.adjustPlan(currentPlan, result);

        if (adjustment.action === 'REPLAN') {
          console.log(`🔄 [${this.entity.name}] ${adjustment.reasoning}`);
          currentPlan = adjustment.updatedPlan;
          i = -1;  // Restart from beginning with new plan
        } else if (adjustment.action === 'CONTINGENCY') {
          console.log(`⚠️ [${this.entity.name}] ${adjustment.reasoning}`);
          currentPlan = adjustment.updatedPlan;
        } else if (adjustment.action === 'ABORT') {
          console.error(`❌ [${this.entity.name}] ${adjustment.reasoning}`);
          break;
        }

      } catch (error) {
        console.error(`💥 [${this.entity.name}] Step failed:`, error);

        // Try to recover
        const adjustment = await this.reasoning.adjustPlan(currentPlan, {
          success: false,
          error: error as Error,
          duration: 0
        });

        if (adjustment.action === 'REPLAN') {
          currentPlan = adjustment.updatedPlan;
          i = -1;  // Restart
        } else {
          break;  // Can't recover
        }
      }
    }

    // 5. Evaluate outcome (NEW - Phase 3.5)
    if (finalResult) {
      const evaluation = await this.reasoning.evaluateOutcome(currentPlan, finalResult);

      console.log(`📊 [${this.entity.name}] Evaluation:`);
      console.log(`   Success: ${evaluation.meetsSuccessCriteria}`);
      console.log(`   Learned: ${evaluation.extractedPattern}`);

      // 6. Update self-state with learnings
      await this.updateSelfState({
        type: 'activity-completed',
        domain,
        outcome: evaluation.meetsSuccessCriteria ? 'success' : 'partial',
        learnings: evaluation.extractedPattern
      });
    }
  }

  private parseEventAsTask(domain: string, event: DomainEvent): Task {
    return {
      id: UUID.generate(),
      domain,
      contextId: event.contextId,
      description: this.describeEvent(event),
      priority: event.priority || 0.5,
      triggeredBy: event.id,
      createdAt: Date.now()
    };
  }

  private describeEvent(event: DomainEvent): string {
    // Convert event into human-readable task description
    // This is domain-specific
    if ('message' in event) {
      return `Respond to message: "${event.message.content}"`;
    }
    // ... other event types ...
    return 'Process event';
  }

  private async executeStep(step: PlanStep, domain: string): Promise<ExecutionResult> {
    const startTime = Date.now();

    try {
      // This is where domain-specific execution happens
      // For now, stub it out
      const output = await this.executeStepInDomain(step, domain);

      return {
        success: true,
        output,
        duration: Date.now() - startTime
      };
    } catch (error) {
      return {
        success: false,
        error: error as Error,
        duration: Date.now() - startTime
      };
    }
  }

  private async executeStepInDomain(step: PlanStep, domain: string): Promise<any> {
    // TODO: Implement domain-specific step execution
    // For chat: might be "retrieve context", "generate response", "post message"
    // For code: might be "read file", "analyze code", "suggest fix"
    throw new Error('Domain-specific execution not yet implemented');
  }
}
```

---

## Phase 7: Observable Commands

**Goal**: Make reasoning visible via CLI

### 7.1 ai/plan Command

**File**: `commands/ai-plan/shared/AIPlanTypes.ts`

```typescript
export interface AIPlanParams extends CommandParams {
  persona?: string;  // Persona ID or name
  planId?: UUID;  // Specific plan
}

export interface AIPlanResult extends CommandResult {
  plan: Plan | null;
  steps: {
    number: number;
    action: string;
    expectedOutcome: string;
    completed: boolean;
    completedAt?: number;
  }[];
  contingencies: Record<string, string[]>;
  successCriteria: string[];
}
```

**Server**: `commands/ai-plan/server/AIPlanServer.ts`

```typescript
export class AIPlanServer extends CommandServerBase<AIPlanParams, AIPlanResult> {
  async execute(params: AIPlanParams): Promise<AIPlanResult> {
    // Get plan from database
    const plan = await this.getPlan(params.persona, params.planId);

    if (!plan) {
      return {
        success: false,
        plan: null,
        steps: [],
        contingencies: {},
        successCriteria: []
      };
    }

    return {
      success: true,
      plan,
      steps: plan.steps.map(s => ({
        number: s.stepNumber,
        action: s.action,
        expectedOutcome: s.expectedOutcome,
        completed: s.completed,
        completedAt: s.completedAt
      })),
      contingencies: plan.contingencies,
      successCriteria: plan.successCriteria
    };
  }

  private async getPlan(personaIdOrName?: string, planId?: UUID): Promise<Plan | null> {
    if (planId) {
      const result = await Commands.execute('data/read', {
        collection: COLLECTIONS.PERSONA_PLANS,
        id: planId
      });
      return result.entity as Plan;
    }

    if (personaIdOrName) {
      const persona = await this.resolvePersona(personaIdOrName);
      const plans = await Commands.execute('data/list', {
        collection: COLLECTIONS.PERSONA_PLANS,
        filter: {
          personaId: persona.id,
          status: 'active'
        },
        orderBy: [{ field: 'createdAt', direction: 'desc' }],
        limit: 1
      });
      return plans.entities[0] || null;
    }

    return null;
  }
}
```

Usage:
```bash
# View current plan
./jtag ai/plan --persona=helper-ai

# View specific plan
./jtag ai/plan --planId=<uuid>
```

---

### 7.2 ai/learnings Command

**File**: `commands/ai-learnings/shared/AILearningsTypes.ts` + server

```bash
# View learnings
./jtag ai/learnings --persona=helper-ai --domain=chat

# View high-confidence learnings only
./jtag ai/learnings --persona=helper-ai --minConfidence=0.8
```

---

## Phase 8: Testing Strategy

### 8.1 Unit Tests (Isolated Components)

```bash
# Types and schemas
npx vitest tests/unit/reasoning-types.test.ts

# Plan formulation
npx vitest tests/unit/PlanFormulator.test.ts

# Plan adaptation
npx vitest tests/unit/PlanAdapter.test.ts

# Outcome evaluation
npx vitest tests/unit/OutcomeEvaluator.test.ts
```

### 8.2 Integration Tests (Full Flow)

```bash
# Full reasoning cycle
npx vitest tests/integration/reasoning-cycle.test.ts
# Test: Task → Plan → Execute → Adapt → Evaluate → Learn

# Error recovery
npx vitest tests/integration/error-recovery.test.ts
# Test: Planned failure → Contingency execution
# Test: Unexpected failure → Replan → Retry

# Learning persistence
npx vitest tests/integration/learning-accumulation.test.ts
# Test: Multiple tasks → Accumulated learnings → Used in future plans
```

### 8.3 System Tests (Real Scenarios)

```bash
npm start

# Scenario 1: Simple chat response (should create plan)
./jtag debug/chat-send --room="general" --message="What is React?"
# Wait 10 seconds
./jtag ai/plan --persona=helper-ai
# Verify: Plan exists with steps like "recall React knowledge", "compose response", "post message"

# Scenario 2: Error recovery (simulate rate limit)
# TODO: Inject rate limit error
./jtag ai/plan --persona=helper-ai
# Verify: Plan shows contingency step "wait and retry"

# Scenario 3: Learning accumulation
# Ask similar question 5 times, each with slight variation
./jtag debug/chat-send --room="general" --message="How do React hooks work?"
# ... repeat with variations ...
./jtag ai/learnings --persona=helper-ai --domain=chat
# Verify: Learning like "Check user's React experience before explaining hooks"
```

---

## Success Criteria

**Phase 3.5 is complete when:**

1. ✅ **Types defined**: All interfaces in reasoning/types.ts
2. ✅ **Database schemas**: Plans and learnings persistable
3. ✅ **PlanFormulator works**: Creates structured plans with CoT
4. ✅ **PlanAdapter works**: Adjusts plans on error
5. ✅ **OutcomeEvaluator works**: Extracts learnings
6. ✅ **PersonaReasoningSystem integrates**: All components work together
7. ✅ **PersonaUser uses it**: processDomainEvent() follows agent pattern
8. ✅ **Observable commands**: ./jtag ai/plan, ./jtag ai/learnings
9. ✅ **Tests pass**: Unit, integration, and system tests green
10. ✅ **Real-world validation**: AIs create plans, adapt, learn in actual use

---

## Migration Path

**Incremental rollout (don't break existing behavior):**

### Step 1: Add reasoning infrastructure (no behavior change)
- Create types, database schemas
- Build PlanFormulator, PlanAdapter, OutcomeEvaluator
- Add observable commands
- Test in isolation

### Step 2: Enable for ONE persona in ONE domain
- PersonaUser gets reasoning system (dormant)
- Add feature flag: `USE_REASONING_FOR_CHAT`
- Enable only for "Helper AI" in chat domain
- Monitor: Does it work? Are plans reasonable?

### Step 3: Expand gradually
- Enable for all personas in chat
- Then code domain
- Then game domain
- Monitor cost, latency, quality at each step

### Step 4: Deprecate old pattern
- Once reasoning is stable, remove old reactive handlers
- All domain events go through processDomainEvent()

---

## Risks and Mitigations

### Risk 1: Increased latency
**Problem**: Planning adds LLM calls before action
**Mitigation**:
- Cache plans for similar tasks
- Use faster models for planning (Haiku)
- Implement timeout limits

### Risk 2: Increased cost
**Problem**: More LLM calls = higher API costs
**Mitigation**:
- Use smaller models for adaptation/evaluation
- Batch evaluations (do them async)
- Learning reduces future costs (skip bad approaches)

### Risk 3: Planning failures
**Problem**: LLM might generate invalid plans
**Mitigation**:
- Strict JSON schema validation
- Fallback to simple reactive behavior if plan fails to generate
- Monitor plan quality metrics

### Risk 4: Learning pollution
**Problem**: Bad learnings could make AI worse
**Mitigation**:
- Confidence thresholds (only use high-confidence learnings)
- Manual review dashboard for learnings
- Ability to delete/override learnings

---

## Future Enhancements

**Once basic reasoning works:**

1. **Plan Templates**: Common task patterns pre-defined
2. **Multi-Agent Planning**: AIs collaborate on plans
3. **Hierarchical Planning**: Break complex goals into sub-plans
4. **Embedding-based Learning Retrieval**: Use RAG for learning lookup
5. **Plan Visualization**: UI showing plan graph with progress
6. **A/B Testing**: Compare reasoning vs reactive performance
7. **Plan Explanation**: "Why did you do that?" introspection

---

## Timeline Estimate

**Assuming full-time work:**

- Phase 1 (Types): 1 day
- Phase 2 (PlanFormulator): 2 days
- Phase 3 (PlanAdapter): 2 days
- Phase 4 (OutcomeEvaluator): 2 days
- Phase 5 (Integration): 1 day
- Phase 6 (PersonaUser): 2 days
- Phase 7 (Commands): 1 day
- Phase 8 (Testing): 3 days

**Total: ~14 days (2 weeks)**

**With part-time work or other priorities: 4-6 weeks**

---

## Related Documents

- `COGNITION-ARCHITECTURE.md` - Overall cognition vision (self-state + working memory)
- `PERSONA-CONVERGENCE-ROADMAP.md` - How reasoning fits into larger PersonaUser evolution
- Research paper: "Building Autonomous LLM Agents" (perception/reasoning/memory/action framework)

---

**Status**: Ready to implement
**Next Action**: Start Phase 1 (types and schemas)
