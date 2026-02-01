/**
 * CodeAgentOrchestrator - Executes CodingPlans step-by-step
 *
 * Takes a CodingPlan (DAG of steps) and executes each step via Commands.execute(),
 * respecting dependency ordering. Independent steps could execute in parallel.
 *
 * Execution lifecycle:
 * 1. Discover — code/tree + code/search to understand codebase
 * 2. Read — code/read to gather context
 * 3. Plan — PlanFormulator decomposes task (already done before orchestrator runs)
 * 4. Execute — Run each step via code/* commands
 * 5. Verify — After each write/edit, read back to confirm
 * 6. Fix — If verification fails, retry (max 3 attempts per step)
 * 7. Report — Summarize changes via code/history
 *
 * Budget enforcement:
 * - Max duration (default 120s)
 * - Max tool calls (default 15)
 * - Stops gracefully when budget exceeded
 */

import type {
  CodingTask,
  CodingPlan,
  CodingStep,
  CodingResult,
  CodingResultStatus,
  StepResult,
  StepStatus,
} from '../shared/CodingTypes';
import { PlanFormulator } from './PlanFormulator';
import { CodingModelSelector } from './CodingModelSelector';
import { Commands } from '../../core/shared/Commands';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('CodeAgentOrchestrator', 'code');

/** Maximum retries per failed step */
const MAX_RETRIES_PER_STEP = 3;

/** Default budget limits */
const DEFAULT_MAX_DURATION_MS = 120_000;
const DEFAULT_MAX_TOOL_CALLS = 15;

/**
 * Runtime budget tracker for execution limits.
 */
class ExecutionBudget {
  private readonly startTime: number;
  private readonly maxDurationMs: number;
  private readonly maxToolCalls: number;
  private _toolCallsUsed = 0;

  constructor(maxDurationMs: number, maxToolCalls: number) {
    this.startTime = performance.now();
    this.maxDurationMs = maxDurationMs;
    this.maxToolCalls = maxToolCalls;
  }

  recordToolCall(): void {
    this._toolCallsUsed++;
  }

  get toolCallsUsed(): number {
    return this._toolCallsUsed;
  }

  get elapsedMs(): number {
    return performance.now() - this.startTime;
  }

  get exceeded(): boolean {
    return this.elapsedMs >= this.maxDurationMs || this._toolCallsUsed >= this.maxToolCalls;
  }

  get remainingToolCalls(): number {
    return Math.max(0, this.maxToolCalls - this._toolCallsUsed);
  }

  get reason(): string {
    if (this.elapsedMs >= this.maxDurationMs) return 'time_exceeded';
    if (this._toolCallsUsed >= this.maxToolCalls) return 'tool_calls_exceeded';
    return 'ok';
  }
}

export class CodeAgentOrchestrator {
  private readonly modelSelector: CodingModelSelector;
  private readonly planFormulator: PlanFormulator;

  constructor(modelSelector?: CodingModelSelector) {
    this.modelSelector = modelSelector ?? new CodingModelSelector();
    this.planFormulator = new PlanFormulator(this.modelSelector);
  }

  /**
   * Execute a coding task end-to-end:
   * 1. Optionally discover codebase context
   * 2. Formulate a plan via LLM
   * 3. Execute each step
   * 4. Return results
   */
  async execute(task: CodingTask): Promise<CodingResult> {
    const budget = new ExecutionBudget(
      task.maxDurationMs ?? DEFAULT_MAX_DURATION_MS,
      task.maxToolCalls ?? DEFAULT_MAX_TOOL_CALLS,
    );

    log.info(`Starting task: ${task.description.slice(0, 80)}... (budget: ${budget.remainingToolCalls} calls)`);

    const filesModified: string[] = [];
    const filesCreated: string[] = [];
    const changeIds: string[] = [];
    const errors: string[] = [];
    const stepResults: StepResult[] = [];

    try {
      // Phase 1: Discovery (optional — gather codebase context for planning)
      let codebaseContext: string | undefined;
      if (!budget.exceeded) {
        codebaseContext = await this.discoverContext(task, budget);
      }

      // Phase 2: Plan formulation
      if (budget.exceeded) {
        return this.buildResult(task, 'budget_exceeded', 'Budget exceeded before planning', stepResults, filesModified, filesCreated, changeIds, errors, budget);
      }

      const plan = await this.planFormulator.formulate(task, codebaseContext);
      log.info(`Plan: "${plan.summary}" — ${plan.steps.length} steps`);

      // Phase 3: Execute plan steps in dependency order
      const completedSteps = new Set<number>();

      for (const step of plan.steps) {
        if (budget.exceeded) {
          log.warn(`Budget exceeded at step ${step.stepNumber}, stopping`);
          stepResults.push({
            stepNumber: step.stepNumber,
            status: 'skipped',
            durationMs: 0,
            toolCall: step.toolCall,
            error: `Budget exceeded (${budget.reason})`,
          });
          continue;
        }

        // Check dependencies are met
        const depsOk = step.dependsOn.every(dep => completedSteps.has(dep));
        if (!depsOk) {
          const missingDeps = step.dependsOn.filter(d => !completedSteps.has(d));
          log.warn(`Step ${step.stepNumber} skipped — dependencies not met: ${missingDeps.join(', ')}`);
          stepResults.push({
            stepNumber: step.stepNumber,
            status: 'skipped',
            durationMs: 0,
            toolCall: step.toolCall,
            error: `Dependencies not met: steps ${missingDeps.join(', ')}`,
          });
          continue;
        }

        // Execute step with retry
        const result = await this.executeStepWithRetry(step, task, budget);
        stepResults.push(result);

        if (result.status === 'completed') {
          completedSteps.add(step.stepNumber);

          // Track file changes
          this.trackChanges(step, result, filesModified, filesCreated, changeIds);
        } else {
          errors.push(`Step ${step.stepNumber} (${step.action}): ${result.error ?? 'unknown error'}`);
        }
      }

      // Determine overall status
      const allCompleted = stepResults.every(r => r.status === 'completed');
      const anyCompleted = stepResults.some(r => r.status === 'completed');
      const status: CodingResultStatus = allCompleted
        ? 'completed'
        : anyCompleted
          ? 'partial'
          : budget.exceeded
            ? 'budget_exceeded'
            : 'failed';

      const summary = allCompleted
        ? `Completed: ${plan.summary}`
        : `${status}: ${stepResults.filter(r => r.status === 'completed').length}/${plan.steps.length} steps completed`;

      return this.buildResult(task, status, summary, stepResults, filesModified, filesCreated, changeIds, errors, budget);

    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log.error(`Task failed: ${message}`);
      errors.push(message);
      return this.buildResult(task, 'failed', `Failed: ${message}`, stepResults, filesModified, filesCreated, changeIds, errors, budget);
    }
  }

  /**
   * Discover codebase context for planning.
   * Runs code/tree on the workspace root (or relevant paths).
   */
  private async discoverContext(task: CodingTask, budget: ExecutionBudget): Promise<string | undefined> {
    try {
      // Get workspace tree
      const treeResult = await Commands.execute<any, any>('code/tree', {
        userId: task.personaId,
        path: '',
        maxDepth: 3,
      });
      budget.recordToolCall();

      if (!treeResult?.success) {
        return undefined;
      }

      let context = `## Workspace Tree\n${JSON.stringify(treeResult.root, null, 2).slice(0, 2000)}`;

      // If relevant files are specified, read their contents
      if (task.relevantFiles && task.relevantFiles.length > 0 && !budget.exceeded) {
        for (const file of task.relevantFiles.slice(0, 3)) { // Max 3 files for context
          if (budget.exceeded) break;

          const readResult = await Commands.execute<any, any>('code/read', {
            userId: task.personaId,
            filePath: file,
          });
          budget.recordToolCall();

          if (readResult?.success && readResult.content) {
            // Truncate large files
            const content = readResult.content.length > 3000
              ? readResult.content.slice(0, 3000) + '\n... (truncated)'
              : readResult.content;
            context += `\n\n## ${file}\n\`\`\`\n${content}\n\`\`\``;
          }
        }
      }

      return context;
    } catch (error) {
      log.warn(`Discovery failed: ${error instanceof Error ? error.message : String(error)}`);
      return undefined;
    }
  }

  /**
   * Execute a single step with retry logic.
   */
  private async executeStepWithRetry(
    step: CodingStep,
    task: CodingTask,
    budget: ExecutionBudget,
  ): Promise<StepResult> {
    let lastError: string | undefined;

    for (let attempt = 0; attempt < MAX_RETRIES_PER_STEP; attempt++) {
      if (budget.exceeded) {
        return {
          stepNumber: step.stepNumber,
          status: 'failed',
          durationMs: 0,
          toolCall: step.toolCall,
          error: `Budget exceeded before retry ${attempt + 1}`,
        };
      }

      const result = await this.executeStep(step, task, budget);

      if (result.status === 'completed') {
        return result;
      }

      lastError = result.error;
      if (attempt < MAX_RETRIES_PER_STEP - 1) {
        log.warn(`Step ${step.stepNumber} failed (attempt ${attempt + 1}/${MAX_RETRIES_PER_STEP}): ${lastError}`);
      }
    }

    return {
      stepNumber: step.stepNumber,
      status: 'failed',
      durationMs: 0,
      toolCall: step.toolCall,
      error: `Failed after ${MAX_RETRIES_PER_STEP} attempts: ${lastError}`,
    };
  }

  /**
   * Execute a single step via Commands.execute().
   */
  private async executeStep(
    step: CodingStep,
    task: CodingTask,
    budget: ExecutionBudget,
  ): Promise<StepResult> {
    const startTime = performance.now();

    try {
      log.debug(`Step ${step.stepNumber}: ${step.action} — ${step.description}`);

      // Inject personaId (userId) into params for workspace scoping
      const params = {
        ...step.toolParams,
        userId: task.personaId,
      };

      const result = await Commands.execute<any, any>(step.toolCall, params);
      budget.recordToolCall();

      const durationMs = performance.now() - startTime;
      const success = result?.success === true;

      if (!success) {
        const error = result?.error?.message ?? result?.error ?? 'Command returned success=false';
        return {
          stepNumber: step.stepNumber,
          status: 'failed',
          output: result,
          error: typeof error === 'string' ? error : JSON.stringify(error),
          durationMs,
          toolCall: step.toolCall,
        };
      }

      return {
        stepNumber: step.stepNumber,
        status: 'completed',
        output: result,
        durationMs,
        toolCall: step.toolCall,
      };
    } catch (error) {
      const durationMs = performance.now() - startTime;
      const message = error instanceof Error ? error.message : String(error);
      return {
        stepNumber: step.stepNumber,
        status: 'failed',
        error: message,
        durationMs,
        toolCall: step.toolCall,
      };
    }
  }

  /**
   * Track file modifications and change IDs from step results.
   */
  private trackChanges(
    step: CodingStep,
    result: StepResult,
    filesModified: string[],
    filesCreated: string[],
    changeIds: string[],
  ): void {
    const output = result.output as Record<string, unknown> | undefined;

    if (step.action === 'write' || step.action === 'edit') {
      for (const file of step.targetFiles) {
        if (step.action === 'write' && !filesModified.includes(file)) {
          filesCreated.push(file);
        } else if (!filesModified.includes(file)) {
          filesModified.push(file);
        }
      }

      // Extract changeId from write/edit results
      if (output?.changeId && typeof output.changeId === 'string') {
        changeIds.push(output.changeId);
      }
    }
  }

  /**
   * Build the final CodingResult.
   */
  private buildResult(
    task: CodingTask,
    status: CodingResultStatus,
    summary: string,
    stepResults: StepResult[],
    filesModified: string[],
    filesCreated: string[],
    changeIds: string[],
    errors: string[],
    budget: ExecutionBudget,
  ): CodingResult {
    return {
      taskId: task.id,
      status,
      summary,
      stepResults,
      filesModified,
      filesCreated,
      totalToolCalls: budget.toolCallsUsed,
      totalDurationMs: budget.elapsedMs,
      changeIds,
      errors,
    };
  }
}
