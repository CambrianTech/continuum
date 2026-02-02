/**
 * Code Task Command - Shared Types
 *
 * Execute a coding task end-to-end via the coding agent pipeline. Formulates a plan using LLM reasoning, enforces security tiers, and executes steps via code/* commands. Supports dry-run mode, governance approval for high-risk plans, and multi-agent delegation.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Code Task Command Parameters
 */
export interface CodeTaskParams extends CommandParams {
  // What the coding task should accomplish (natural language)
  description: string;
  // Task type for model selection: 'planning' | 'generation' | 'editing' | 'review' | 'quick-fix' | 'discovery'. Defaults to 'generation'
  taskType?: string;
  // File paths already known to be relevant (hints for discovery phase)
  relevantFiles?: string[];
  // Execute read-only commands normally but mock writes. Returns predicted changes without modifying files
  dryRun?: boolean;
  // Override security tier: 'discovery' | 'read' | 'write' | 'system'. Defaults to plan's assessed risk level
  securityTier?: string;
  // Enable multi-agent delegation for parallel execution across file clusters
  delegationEnabled?: boolean;
  // Maximum execution time in milliseconds (default: 120000)
  maxDurationMs?: number;
  // Maximum number of tool calls allowed (default: 15)
  maxToolCalls?: number;
  // Workspace mode: 'sandbox' (isolated directory, default) or 'worktree' (git worktree on real repo)
  workspaceMode?: string;
  // Paths to sparse-checkout when using worktree mode (e.g., ["src/system/code/", "docs/"])
  sparsePaths?: string[];
}

/**
 * Factory function for creating CodeTaskParams
 */
export const createCodeTaskParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // What the coding task should accomplish (natural language)
    description: string;
    // Task type for model selection: 'planning' | 'generation' | 'editing' | 'review' | 'quick-fix' | 'discovery'. Defaults to 'generation'
    taskType?: string;
    // File paths already known to be relevant (hints for discovery phase)
    relevantFiles?: string[];
    // Execute read-only commands normally but mock writes. Returns predicted changes without modifying files
    dryRun?: boolean;
    // Override security tier: 'discovery' | 'read' | 'write' | 'system'. Defaults to plan's assessed risk level
    securityTier?: string;
    // Enable multi-agent delegation for parallel execution across file clusters
    delegationEnabled?: boolean;
    // Maximum execution time in milliseconds (default: 120000)
    maxDurationMs?: number;
    // Maximum number of tool calls allowed (default: 15)
    maxToolCalls?: number;
    // Workspace mode: 'sandbox' (isolated directory, default) or 'worktree' (git worktree on real repo)
    workspaceMode?: string;
    // Paths to sparse-checkout when using worktree mode
    sparsePaths?: string[];
  }
): CodeTaskParams => createPayload(context, sessionId, {
  taskType: data.taskType ?? '',
  relevantFiles: data.relevantFiles ?? undefined,
  dryRun: data.dryRun ?? false,
  securityTier: data.securityTier ?? '',
  delegationEnabled: data.delegationEnabled ?? false,
  maxDurationMs: data.maxDurationMs ?? 0,
  maxToolCalls: data.maxToolCalls ?? 0,
  workspaceMode: data.workspaceMode ?? '',
  sparsePaths: data.sparsePaths ?? [],
  ...data
});

/**
 * Code Task Command Result
 */
export interface CodeTaskResult extends CommandResult {
  success: boolean;
  // Overall status: 'completed' | 'partial' | 'failed' | 'budget_exceeded' | 'pending_approval'
  status: string;
  // Human-readable summary of what was accomplished
  summary: string;
  // The LLM-generated plan summary
  planSummary: string;
  // Assessed risk level: 'low' | 'medium' | 'high' | 'critical'
  riskLevel: string;
  // Security tier used for execution
  securityTier: string;
  // Total number of steps in the plan
  stepsTotal: number;
  // Number of steps that completed successfully
  stepsCompleted: number;
  // Files that were modified during execution
  filesModified: string[];
  // Files that were created during execution
  filesCreated: string[];
  // Total tool calls used
  totalToolCalls: number;
  // Total execution time in milliseconds
  totalDurationMs: number;
  // Change IDs from file operations (for potential undo)
  changeIds: string[];
  // Errors encountered during execution
  errors: string[];
  // Governance proposal ID if plan requires approval (status='pending_approval')
  proposalId: string;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeTaskResult with defaults
 */
export const createCodeTaskResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Overall status: 'completed' | 'partial' | 'failed' | 'budget_exceeded' | 'pending_approval'
    status?: string;
    // Human-readable summary of what was accomplished
    summary?: string;
    // The LLM-generated plan summary
    planSummary?: string;
    // Assessed risk level: 'low' | 'medium' | 'high' | 'critical'
    riskLevel?: string;
    // Security tier used for execution
    securityTier?: string;
    // Total number of steps in the plan
    stepsTotal?: number;
    // Number of steps that completed successfully
    stepsCompleted?: number;
    // Files that were modified during execution
    filesModified?: string[];
    // Files that were created during execution
    filesCreated?: string[];
    // Total tool calls used
    totalToolCalls?: number;
    // Total execution time in milliseconds
    totalDurationMs?: number;
    // Change IDs from file operations (for potential undo)
    changeIds?: string[];
    // Errors encountered during execution
    errors?: string[];
    // Governance proposal ID if plan requires approval (status='pending_approval')
    proposalId?: string;
    error?: JTAGError;
  }
): CodeTaskResult => createPayload(context, sessionId, {
  status: data.status ?? '',
  summary: data.summary ?? '',
  planSummary: data.planSummary ?? '',
  riskLevel: data.riskLevel ?? '',
  securityTier: data.securityTier ?? '',
  stepsTotal: data.stepsTotal ?? 0,
  stepsCompleted: data.stepsCompleted ?? 0,
  filesModified: data.filesModified ?? [],
  filesCreated: data.filesCreated ?? [],
  totalToolCalls: data.totalToolCalls ?? 0,
  totalDurationMs: data.totalDurationMs ?? 0,
  changeIds: data.changeIds ?? [],
  errors: data.errors ?? [],
  proposalId: data.proposalId ?? '',
  ...data
});

/**
 * Smart Code Task-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeTaskResultFromParams = (
  params: CodeTaskParams,
  differences: Omit<CodeTaskResult, 'context' | 'sessionId'>
): CodeTaskResult => transformPayload(params, differences);

/**
 * Code Task — Type-safe command executor
 *
 * Usage:
 *   import { CodeTask } from '...shared/CodeTaskTypes';
 *   const result = await CodeTask.execute({ ... });
 */
export const CodeTask = {
  execute(params: CommandInput<CodeTaskParams>): Promise<CodeTaskResult> {
    return Commands.execute<CodeTaskParams, CodeTaskResult>('code/task', params as Partial<CodeTaskParams>);
  },
  commandName: 'code/task' as const,
} as const;
