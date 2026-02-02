/**
 * Code Task Command - Server Implementation
 *
 * Entry point for the full coding agent pipeline:
 * 1. Validates parameters
 * 2. Builds a CodingTask
 * 3. Invokes CodeAgentOrchestrator.execute()
 * 4. Maps CodingResult → CodeTaskResult
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeTaskParams, CodeTaskResult } from '../shared/CodeTaskTypes';
import { createCodeTaskResultFromParams } from '../shared/CodeTaskTypes';
import { CodeAgentOrchestrator } from '@system/code/server/CodeAgentOrchestrator';
import type { CodingTask, CodingTaskType, SecurityTierLevel, ExecutionOptions } from '@system/code/shared/CodingTypes';
import { v4 as uuidv4 } from 'uuid';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

const VALID_TASK_TYPES = new Set<string>(['planning', 'generation', 'editing', 'review', 'quick-fix', 'discovery']);
const VALID_TIERS = new Set<string>(['discovery', 'read', 'write', 'system']);

export class CodeTaskServerCommand extends CommandBase<CodeTaskParams, CodeTaskResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/task', context, subpath, commander);
  }

  async execute(params: CodeTaskParams): Promise<CodeTaskResult> {
    // Validate required parameters
    if (!params.description || params.description.trim() === '') {
      throw new ValidationError(
        'description',
        `Missing required parameter 'description'. Provide a natural language description of the coding task. See the code/task README for usage.`
      );
    }

    if (!params.userId) {
      throw new ValidationError(
        'userId',
        'Workspace operations require a userId (auto-injected for persona tool calls).'
      );
    }

    // Validate optional enum parameters
    const taskType: CodingTaskType = this.resolveTaskType(params.taskType);
    const securityTierOverride = this.resolveSecurityTier(params.securityTier);

    // Validate workspace mode
    const validModes = new Set(['sandbox', 'worktree']);
    const workspaceMode = params.workspaceMode && validModes.has(params.workspaceMode)
      ? params.workspaceMode as 'sandbox' | 'worktree'
      : undefined;

    if (workspaceMode === 'worktree' && (!params.sparsePaths || params.sparsePaths.length === 0)) {
      throw new ValidationError(
        'sparsePaths',
        `Worktree mode requires sparsePaths — specify which directories to checkout (e.g., ["src/system/code/", "docs/"])`
      );
    }

    // Build CodingTask
    const task: CodingTask = {
      id: uuidv4() as UUID,
      personaId: params.userId as UUID,
      description: params.description.trim(),
      taskType,
      contextId: params.sessionId as UUID | undefined,
      relevantFiles: params.relevantFiles,
      maxDurationMs: params.maxDurationMs || undefined,
      maxToolCalls: params.maxToolCalls || undefined,
      workspaceMode,
      sparsePaths: params.sparsePaths,
      createdAt: Date.now(),
    };

    // Build execution options
    const options: ExecutionOptions = {
      dryRun: params.dryRun ?? false,
      securityTier: securityTierOverride,
      delegationEnabled: params.delegationEnabled ?? false,
    };

    // Execute via orchestrator
    const orchestrator = new CodeAgentOrchestrator();
    const result = await orchestrator.execute(task, options);

    // Map CodingResult → CodeTaskResult
    return createCodeTaskResultFromParams(params, {
      success: result.status === 'completed',
      status: result.status,
      summary: result.summary,
      planSummary: result.planMetadata?.planSummary ?? result.summary,
      riskLevel: result.planMetadata?.riskLevel ?? '',
      securityTier: result.planMetadata?.requiredTier ?? securityTierOverride ?? '',
      stepsTotal: result.stepResults.length,
      stepsCompleted: result.stepResults.filter(s => s.status === 'completed').length,
      filesModified: result.filesModified,
      filesCreated: result.filesCreated,
      totalToolCalls: result.totalToolCalls,
      totalDurationMs: result.totalDurationMs,
      changeIds: result.changeIds,
      errors: result.errors,
      proposalId: result.proposalId ?? '',
    });
  }

  private resolveTaskType(raw?: string): CodingTaskType {
    if (!raw || raw.trim() === '') return 'generation';
    if (!VALID_TASK_TYPES.has(raw)) {
      throw new ValidationError(
        'taskType',
        `Invalid taskType '${raw}'. Must be one of: ${Array.from(VALID_TASK_TYPES).join(', ')}`
      );
    }
    return raw as CodingTaskType;
  }

  private resolveSecurityTier(raw?: string): SecurityTierLevel | undefined {
    if (!raw || raw.trim() === '') return undefined;
    if (!VALID_TIERS.has(raw)) {
      throw new ValidationError(
        'securityTier',
        `Invalid securityTier '${raw}'. Must be one of: ${Array.from(VALID_TIERS).join(', ')}`
      );
    }
    return raw as SecurityTierLevel;
  }
}
