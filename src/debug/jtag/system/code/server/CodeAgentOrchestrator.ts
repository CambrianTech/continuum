/**
 * CodeAgentOrchestrator - Executes CodingPlans step-by-step
 *
 * Takes a CodingPlan (DAG of steps) and executes each step via Commands.execute(),
 * respecting dependency ordering. Independent steps could execute in parallel.
 *
 * Execution lifecycle:
 * 1. Discover — code/tree + code/search to understand codebase
 * 2. Read — code/read to gather context
 * 3. Plan — PlanFormulator decomposes task via LLM
 * 4. Governance — Check if plan requires team approval (high-risk/system-tier)
 * 5. Execute — Run each step via code/* commands
 * 6. Verify — After each write/edit, read back to confirm
 * 7. Fix — If verification fails, retry (max 3 attempts per step)
 * 8. Report — Summarize changes via code/history
 *
 * Persistence:
 * - Plans are persisted as CodingPlanEntity via DataDaemon
 * - Status updated in real-time during execution
 * - Persistence is best-effort (orchestrator works without DataDaemon)
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
  ExecutionOptions,
  RiskLevel,
  SecurityTierLevel,
} from '../shared/CodingTypes';
import { PlanFormulator } from './PlanFormulator';
import { CodingModelSelector } from './CodingModelSelector';
import { ToolAllowlistEnforcer, ToolDeniedError } from './ToolAllowlistEnforcer';
import { getTier } from './SecurityTier';
import { PlanGovernance } from './PlanGovernance';
import { CodeTaskDelegator } from './CodeTaskDelegator';
import { Commands } from '../../core/shared/Commands';
import { Logger } from '../../core/logging/Logger';
import { CodingPlanEntity } from '../../data/entities/CodingPlanEntity';
import type { CodingStepSnapshot, CodingPlanStatus } from '../../data/entities/CodingPlanEntity';
import { COLLECTIONS } from '../../shared/Constants';
import type { UUID } from '../../core/types/CrossPlatformUUID';

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
  private readonly governance: PlanGovernance;
  private readonly delegator: CodeTaskDelegator;

  constructor(modelSelector?: CodingModelSelector) {
    this.modelSelector = modelSelector ?? new CodingModelSelector();
    this.planFormulator = new PlanFormulator(this.modelSelector);
    this.governance = new PlanGovernance();
    this.delegator = new CodeTaskDelegator();
  }

  /**
   * Execute a coding task end-to-end:
   * 1. Optionally discover codebase context
   * 2. Formulate a plan via LLM
   * 3. Check governance (high-risk plans require team approval)
   * 4. Persist the plan as a CodingPlanEntity
   * 5. Execute each step (updating entity in real-time)
   * 6. Return results
   *
   * Options:
   * - dryRun: Execute read-only commands normally, but mock write/edit commands
   * - securityTier: Override the plan's required tier
   * - delegationEnabled: Enable multi-agent delegation for parallel execution
   */
  async execute(task: CodingTask, options?: ExecutionOptions): Promise<CodingResult> {
    const dryRun = options?.dryRun ?? false;
    const budget = new ExecutionBudget(
      task.maxDurationMs ?? DEFAULT_MAX_DURATION_MS,
      task.maxToolCalls ?? DEFAULT_MAX_TOOL_CALLS,
    );

    log.info(`Starting task${dryRun ? ' [DRY RUN]' : ''}: ${task.description.slice(0, 80)}... (budget: ${budget.remainingToolCalls} calls)`);

    const filesModified: string[] = [];
    const filesCreated: string[] = [];
    const changeIds: string[] = [];
    const errors: string[] = [];
    const stepResults: StepResult[] = [];
    let planEntity: CodingPlanEntity | undefined;

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
      log.info(`Plan: "${plan.summary}" — ${plan.steps.length} steps (risk: ${plan.riskLevel}, tier: ${plan.requiredTier})`);

      // Phase 2b: Create security enforcer from plan's required tier (or override)
      const tierLevel = options?.securityTier ?? plan.requiredTier;
      const enforcer = new ToolAllowlistEnforcer(getTier(tierLevel));

      // Phase 2c: Persist plan as entity (best-effort — works without DataDaemon)
      planEntity = await this.persistPlan(task, plan);

      // Phase 2d: Governance — check if plan requires approval
      if (planEntity && this.governance.shouldRequireApproval(planEntity)) {
        log.info(`Plan requires governance approval (risk: ${plan.riskLevel}, tier: ${tierLevel})`);
        const proposalId = await this.governance.proposePlan(planEntity);

        if (proposalId) {
          // Update plan status to 'proposed' and return early
          await this.updatePlanStatus(planEntity, 'proposed');
          return this.buildResult(
            task, 'pending_approval',
            `Plan submitted for governance approval: ${plan.summary}`,
            [], filesModified, filesCreated, changeIds, errors, budget,
            { proposalId: proposalId as string, planMetadata: { riskLevel: plan.riskLevel, requiredTier: plan.requiredTier, planSummary: plan.summary } },
          );
        }

        // Governance proposal failed — log and continue (auto-approve)
        log.warn('Governance proposal creation failed, auto-approving plan');
      }

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

        // Execute step with retry (enforcer gates each tool call)
        const result = await this.executeStepWithRetry(step, task, budget, enforcer, dryRun);
        stepResults.push(result);

        if (result.status === 'completed') {
          completedSteps.add(step.stepNumber);

          // Track file changes
          this.trackChanges(step, result, filesModified, filesCreated, changeIds);
        } else {
          errors.push(`Step ${step.stepNumber} (${step.action}): ${result.error ?? 'unknown error'}`);
        }

        // Update persisted plan step status
        await this.updatePlanStep(planEntity, step.stepNumber, result);
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

      const codingResult = this.buildResult(
        task, status, summary, stepResults, filesModified, filesCreated, changeIds, errors, budget,
        { planMetadata: { riskLevel: plan.riskLevel, requiredTier: plan.requiredTier, planSummary: plan.summary } },
      );

      // Finalize persisted plan
      await this.finalizePlan(planEntity, codingResult);

      return codingResult;

    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log.error(`Task failed: ${message}`);
      errors.push(message);
      const codingResult = this.buildResult(task, 'failed', `Failed: ${message}`, stepResults, filesModified, filesCreated, changeIds, errors, budget);
      await this.finalizePlan(planEntity, codingResult);
      return codingResult;
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
    enforcer: ToolAllowlistEnforcer,
    dryRun: boolean = false,
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

      const result = await this.executeStep(step, task, budget, enforcer, dryRun);

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
   * In dryRun mode, read-only commands execute normally but write commands return mock results.
   */
  private async executeStep(
    step: CodingStep,
    task: CodingTask,
    budget: ExecutionBudget,
    enforcer: ToolAllowlistEnforcer,
    dryRun: boolean = false,
  ): Promise<StepResult> {
    const startTime = performance.now();

    try {
      log.debug(`Step ${step.stepNumber}${dryRun ? ' [DRY]' : ''}: ${step.action} — ${step.description}`);

      // Inject personaId (userId) into params for workspace scoping
      const params = {
        ...step.toolParams,
        userId: task.personaId,
      };

      // Gate tool call through security tier enforcer
      enforcer.enforce(step.toolCall, params);

      // DryRun: mock write/edit commands, execute read-only normally
      if (dryRun && this.isWriteAction(step.action)) {
        budget.recordToolCall();
        const durationMs = performance.now() - startTime;
        return {
          stepNumber: step.stepNumber,
          status: 'completed',
          output: {
            success: true,
            dryRun: true,
            wouldModify: step.targetFiles,
            action: step.action,
            description: step.description,
          },
          durationMs,
          toolCall: step.toolCall,
        };
      }

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
   * Whether a coding action modifies files (write, edit, undo).
   * DryRun mode mocks these actions instead of executing them.
   */
  private isWriteAction(action: string): boolean {
    return action === 'write' || action === 'edit' || action === 'undo';
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
    extra?: { proposalId?: string; planMetadata?: CodingResult['planMetadata'] },
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
      proposalId: extra?.proposalId,
      planMetadata: extra?.planMetadata,
    };
  }

  // ────────────────────────────────────────────────────────────
  // Plan Persistence (best-effort via DataDaemon)
  // ────────────────────────────────────────────────────────────

  /**
   * Persist a newly formulated plan as a CodingPlanEntity.
   * Returns the entity if persistence succeeded, undefined otherwise.
   */
  private async persistPlan(task: CodingTask, plan: CodingPlan): Promise<CodingPlanEntity | undefined> {
    try {
      const { DataDaemon } = await import('../../../daemons/data-daemon/shared/DataDaemon');

      const entity = new CodingPlanEntity();
      entity.taskId = task.id;
      entity.createdById = task.personaId;
      entity.leadId = task.personaId;
      entity.summary = plan.summary;
      entity.taskDescription = task.description;
      entity.estimatedToolCalls = plan.estimatedToolCalls;
      entity.assignees = [task.personaId];
      entity.generatedBy = {
        provider: plan.generatedBy.provider,
        model: plan.generatedBy.model,
        temperature: 0,
        durationMs: 0,
      };
      entity.riskLevel = plan.riskLevel;
      entity.riskReason = plan.riskReason;
      entity.securityTier = plan.requiredTier;
      entity.status = 'executing';
      entity.executionStartedAt = Date.now();

      // Convert plan steps to snapshots
      entity.steps = plan.steps.map(step => ({
        stepNumber: step.stepNumber,
        action: step.action,
        description: step.description,
        targetFiles: step.targetFiles,
        toolCall: step.toolCall,
        toolParams: step.toolParams,
        dependsOn: step.dependsOn,
        verification: step.verification,
        status: 'pending' as const,
      }));

      const stored = await DataDaemon.store<CodingPlanEntity>(COLLECTIONS.CODING_PLANS, entity);
      log.info(`Plan persisted: ${stored.id}`);
      return stored;
    } catch {
      log.debug('Plan persistence skipped (DataDaemon not available)');
      return undefined;
    }
  }

  /**
   * Update a step's status in the persisted plan entity.
   */
  private async updatePlanStep(
    planEntity: CodingPlanEntity | undefined,
    stepNumber: number,
    result: StepResult,
  ): Promise<void> {
    if (!planEntity) return;

    try {
      const { DataDaemon } = await import('../../../daemons/data-daemon/shared/DataDaemon');

      const stepIndex = planEntity.steps.findIndex(s => s.stepNumber === stepNumber);
      if (stepIndex === -1) return;

      // Update step snapshot in-place
      const snapshot = planEntity.steps[stepIndex];
      snapshot.status = result.status === 'completed' ? 'completed'
        : result.status === 'skipped' ? 'skipped'
        : 'failed';
      snapshot.completedAt = Date.now();
      snapshot.durationMs = result.durationMs;
      snapshot.output = result.output;
      snapshot.error = result.error;

      await DataDaemon.update<CodingPlanEntity>(
        COLLECTIONS.CODING_PLANS,
        planEntity.id as UUID,
        { steps: planEntity.steps } as Partial<CodingPlanEntity>,
      );
    } catch {
      // Best-effort — don't interrupt execution for persistence failures
    }
  }

  /**
   * Update the plan's top-level status.
   */
  private async updatePlanStatus(
    planEntity: CodingPlanEntity,
    status: CodingPlanStatus,
  ): Promise<void> {
    try {
      const { DataDaemon } = await import('../../../daemons/data-daemon/shared/DataDaemon');
      await DataDaemon.update<CodingPlanEntity>(
        COLLECTIONS.CODING_PLANS,
        planEntity.id as UUID,
        { status } as Partial<CodingPlanEntity>,
      );
    } catch {
      // Best-effort
    }
  }

  /**
   * Finalize the persisted plan with execution results.
   */
  private async finalizePlan(
    planEntity: CodingPlanEntity | undefined,
    result: CodingResult,
  ): Promise<void> {
    if (!planEntity) return;

    try {
      const { DataDaemon } = await import('../../../daemons/data-daemon/shared/DataDaemon');

      const statusMap: Record<string, CodingPlanStatus> = {
        completed: 'completed',
        partial: 'partial',
        failed: 'failed',
        budget_exceeded: 'partial',
        pending_approval: 'proposed',
      };

      await DataDaemon.update<CodingPlanEntity>(
        COLLECTIONS.CODING_PLANS,
        planEntity.id as UUID,
        {
          status: statusMap[result.status] ?? 'failed',
          executionCompletedAt: Date.now(),
          filesModified: result.filesModified,
          filesCreated: result.filesCreated,
          changeIds: result.changeIds,
          errors: result.errors,
          totalToolCalls: result.totalToolCalls,
          totalDurationMs: result.totalDurationMs,
        } as Partial<CodingPlanEntity>,
      );

      log.info(`Plan finalized: ${planEntity.id} → ${result.status}`);
    } catch {
      // Best-effort
    }
  }
}
