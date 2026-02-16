/**
 * AI Agent Command Types
 * ======================
 *
 * Universal agentic loop: generate -> parse tool calls -> execute tools ->
 * feed results -> re-generate. Model decides when to stop.
 *
 * Used by:
 * - Sentinel pipelines (LLM step with agentMode=true)
 * - Future autonomous agents
 * - Direct invocation via ./jtag ai/agent
 */

import type { CommandParams, JTAGPayload, CommandInput } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { ChatMessage } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { Commands } from '../../../../system/core/shared/Commands';

// ─── Params ──────────────────────────────────────────────────────────

export interface AiAgentParams extends CommandParams {
  /** Simple text prompt (converted to single user message) */
  prompt?: string;

  /** Full message array (overrides prompt) */
  messages?: ChatMessage[];

  /** System prompt injected as first message */
  systemPrompt?: string;

  // ─── Model config ──────────────────────────────────────────────

  /** Model ID (e.g., 'claude-sonnet-4-5-20250929', 'llama-3.1-8b') */
  model?: string;

  /** Provider (e.g., 'anthropic', 'openai', 'together', 'ollama') */
  provider?: string;

  /** Sampling temperature */
  temperature?: number;

  /** Max tokens for generation */
  maxTokens?: number;

  // ─── Tool config ───────────────────────────────────────────────

  /** Tool subset: undefined = all public, [] = none, ['code/tree', 'code/read'] = specific */
  tools?: string[];

  /** Override safety cap for tool iterations */
  maxIterations?: number;

  // ─── Attribution ───────────────────────────────────────────────

  /** Sentinel handle for log correlation */
  sentinelHandle?: string;
}

// ─── Result ──────────────────────────────────────────────────────────

/** Record of a single tool call made during execution */
export interface ToolCallRecord {
  toolName: string;
  params: Record<string, string>;
  success: boolean;
  content?: string;
  error?: string;
  durationMs: number;
}

export interface AiAgentResult extends JTAGPayload {
  readonly success: boolean;

  /** Final response text from the LLM */
  readonly text: string;

  /** All tool calls made during execution */
  readonly toolCalls: ToolCallRecord[];

  /** Number of agent loop iterations */
  readonly iterations: number;

  /** Token usage if available */
  readonly tokenUsage?: { input: number; output: number };

  /** Actual model used */
  readonly model?: string;

  /** Actual provider used */
  readonly provider?: string;

  /** Total execution time in milliseconds */
  readonly durationMs: number;

  readonly error?: string;
}

// ─── Helpers ─────────────────────────────────────────────────────────

export const createAiAgentParams = (
  context: import('../../../../system/core/types/JTAGTypes').JTAGContext,
  sessionId: UUID,
  data: Omit<AiAgentParams, 'context' | 'sessionId'>
): AiAgentParams => createPayload(context, sessionId, data);

export const createAiAgentResult = (
  params: AiAgentParams,
  overrides: Omit<Partial<AiAgentResult>, 'context' | 'sessionId'>
): AiAgentResult => transformPayload(params, {
  success: false,
  text: '',
  toolCalls: [],
  iterations: 0,
  durationMs: 0,
  ...overrides,
});

/**
 * AiAgent — Type-safe command executor
 *
 * Usage:
 *   import { AiAgent } from '...shared/AiAgentTypes';
 *   const result = await AiAgent.execute({ prompt: 'List files', tools: ['code/tree'] });
 */
export const AiAgent = {
  execute(params: CommandInput<AiAgentParams>): Promise<AiAgentResult> {
    return Commands.execute<AiAgentParams, AiAgentResult>('ai/agent', params as Partial<AiAgentParams>);
  },
  commandName: 'ai/agent' as const,
} as const;
