/**
 * CodingAgentProvider — interface for external coding agent providers.
 *
 * Each provider (Claude Code, Codex, aider, etc.) implements this interface.
 * Providers self-register with CodingAgentRegistry. No switch statements,
 * no central command list — pure dynamic discovery.
 */

export interface CodingAgentToolCall {
  toolName: string;
  input: Record<string, unknown>;
  output: string;
  isError: boolean;
  durationMs: number;
}

export interface CodingAgentInteraction {
  role: 'user' | 'assistant';
  content: string;
  toolCalls?: CodingAgentToolCall[];
  timestamp: number;
}

export interface CodingAgentConfig {
  prompt: string;
  cwd: string;
  systemPrompt?: string;
  model?: string;
  allowedTools?: string[];
  maxTurns?: number;
  maxBudgetUsd?: number;
  permissionMode?: string;
  resumeSessionId?: string;
  sentinelHandle?: string;
  captureTraining?: boolean;
  personaId?: string;
  /** Explicit API key for billing. If omitted, Claude Code uses its own auth (Max subscription). */
  apiKey?: string;
}

export interface CodingAgentResult {
  success: boolean;
  text: string;
  sessionId: string;
  toolCalls: CodingAgentToolCall[];
  interactions: CodingAgentInteraction[];
  totalCostUsd: number;
  numTurns: number;
  durationMs: number;
  model: string;
  error?: string;
}

export interface CodingAgentProgressEvent {
  type: 'tool_start' | 'tool_end' | 'assistant_message' | 'status';
  toolName?: string;
  message?: string;
  timestamp: number;
}

export interface CodingAgentProvider {
  readonly providerId: string;
  readonly providerName: string;
  execute(
    config: CodingAgentConfig,
    onProgress?: (event: CodingAgentProgressEvent) => void,
  ): Promise<CodingAgentResult>;
  isAvailable(): Promise<boolean>;
}
