/**
 * Sentinel Coding Agent Command — Types
 *
 * Execute external coding agents (Claude Code, Codex, etc.) as sentinel pipeline steps.
 * Provider architecture: each agent SDK is wrapped as a CodingAgentProvider.
 */

import type { CommandParams, CommandResult, CommandInput } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface SentinelCodingAgentParams extends CommandParams {
  /** Task prompt — what the agent should do */
  prompt: string;

  /** Which provider: "claude-code" (default), future: "codex", "aider" */
  provider?: string;

  /** Working directory for the agent */
  cwd?: string;

  /** System prompt override */
  systemPrompt?: string;

  /** Model override (e.g., "sonnet", "opus") */
  model?: string;

  /** Allowed tools (provider-specific names) */
  allowedTools?: string[];

  /** Max conversation turns */
  maxTurns?: number;

  /** Max budget in USD */
  maxBudgetUsd?: number;

  /** Permission mode: "default", "acceptEdits", "bypassPermissions" */
  permissionMode?: string;

  /** Resume a prior session */
  resumeSessionId?: string;

  /** Sentinel handle for progress events */
  sentinelHandle?: string;

  /** Capture interactions for LoRA training (default: true if personaId set) */
  captureTraining?: boolean;

  /** Persona ID for training attribution */
  personaId?: string;

  /** Path to an external git repo — triggers project worktree workspace (proper git isolation) */
  repoPath?: string;

  /** Branch slug for workspace: ai/sentinel-{handle}/{slug} (default: "work") */
  taskSlug?: string;

  /** Persona display name for git identity in workspace */
  personaName?: string;
}

export interface CodingAgentToolCallResult {
  toolName: string;
  input: Record<string, unknown>;
  output: string;
  isError: boolean;
  durationMs: number;
}

export interface CodingAgentInteractionResult {
  role: 'user' | 'assistant';
  content: string;
  toolCalls?: CodingAgentToolCallResult[];
  timestamp: number;
}

export interface SentinelCodingAgentResult extends CommandResult {
  success: boolean;

  /** Agent's final text output */
  text?: string;

  /** Agent session ID for resume capability (distinct from JTAGPayload.sessionId) */
  agentSessionId?: string;

  /** All tool calls made during the session */
  toolCalls?: CodingAgentToolCallResult[];

  /** Full interaction history (for training capture) */
  interactions?: CodingAgentInteractionResult[];

  /** Total cost in USD */
  totalCostUsd?: number;

  /** Number of conversation turns */
  numTurns?: number;

  /** Total execution time in ms */
  durationMs?: number;

  /** Model used */
  model?: string;

  /** Error message if failed */
  error?: string;

  /** Resolved workspace directory (for downstream step interpolation via {{steps.N.data.workspaceDir}}) */
  workspaceDir?: string;

  /** Git branch created for the workspace (for downstream step interpolation) */
  branch?: string;
}

/** Static executor for type-safe command invocation */
export const SentinelCodingAgent = {
  execute(params: CommandInput<SentinelCodingAgentParams>): Promise<SentinelCodingAgentResult> {
    return Commands.execute<SentinelCodingAgentParams, SentinelCodingAgentResult>('sentinel/coding-agent', params);
  },
  commandName: 'sentinel/coding-agent' as const,
} as const;

/**
 * Factory function for creating SentinelCodingAgentParams
 */
export const createSentinelCodingAgentParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelCodingAgentParams, 'context' | 'sessionId' | 'userId'>
): SentinelCodingAgentParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SentinelCodingAgentResult with defaults
 */
export const createSentinelCodingAgentResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelCodingAgentResult, 'context' | 'sessionId' | 'userId'>
): SentinelCodingAgentResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart sentinel/coding-agent-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSentinelCodingAgentResultFromParams = (
  params: SentinelCodingAgentParams,
  differences: Omit<SentinelCodingAgentResult, 'context' | 'sessionId' | 'userId'>
): SentinelCodingAgentResult => transformPayload(params, differences);

