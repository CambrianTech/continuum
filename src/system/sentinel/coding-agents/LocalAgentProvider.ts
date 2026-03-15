/**
 * LocalAgentProvider — CodingAgentProvider backed by the universal ai/agent command.
 *
 * Wraps the existing agentic tool loop (any provider × any model × all code tools)
 * so sentinel CodingAgent steps can run entirely locally or with any cloud provider,
 * not just Claude Code.
 *
 * Provider routing:
 * - 'local-agent'            → Candle (local Llama) — fully offline
 * - 'local-agent:deepseek'   → DeepSeek via cloud
 * - 'local-agent:anthropic'  → Anthropic via API
 * - 'local-agent:groq'       → Groq cloud
 * - etc.
 *
 * The model and provider can be overridden per CodingAgentConfig.
 */

import type {
  CodingAgentConfig,
  CodingAgentInteraction,
  CodingAgentProgressEvent,
  CodingAgentProvider,
  CodingAgentResult,
  CodingAgentToolCall,
} from './CodingAgentProvider';
import { Commands } from '@system/core/shared/Commands';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';

/** Code tools available to the agent — mirrors what Claude Code can do */
const CODE_TOOLS = [
  'code/read',
  'code/write',
  'code/edit',
  'code/search',
  'code/tree',
  'code/diff',
  'code/undo',
  'code/history',
  'code/verify',
  'code/git',
  'code/shell/execute',
  'code/shell/watch',
  'code/shell/status',
  'code/shell/kill',
];

export class LocalAgentProvider implements CodingAgentProvider {
  readonly providerId = 'local-agent';
  readonly providerName = 'Local Agent (ai/agent)';

  private readonly _defaultProvider: string;
  private readonly _defaultModel?: string;

  constructor(defaultProvider = 'candle', defaultModel?: string) {
    this._defaultProvider = defaultProvider;
    this._defaultModel = defaultModel;
  }

  async isAvailable(): Promise<boolean> {
    // Always available — ai/agent works with any registered provider
    return true;
  }

  async execute(
    config: CodingAgentConfig,
    onProgress?: (event: CodingAgentProgressEvent) => void,
  ): Promise<CodingAgentResult> {
    const startTime = Date.now();
    const sessionId = generateUUID();

    onProgress?.({
      type: 'status',
      message: `Local agent session: ${sessionId}`,
      timestamp: Date.now(),
    });

    // Resolve provider (can be overridden in config.model as 'provider:model')
    let provider = this._defaultProvider;
    let model = this._defaultModel;

    if (config.model) {
      if (config.model.includes(':')) {
        // Format: 'provider:model' e.g. 'deepseek:deepseek-chat'
        const [p, m] = config.model.split(':', 2);
        provider = p;
        model = m;
      } else {
        model = config.model;
      }
    }

    // Build system prompt — coding-focused with workspace context
    const systemPrompt = this.buildSystemPrompt(config);

    // Determine tools — use config.allowedTools or default code tools
    const tools = config.allowedTools && config.allowedTools.length > 0
      ? config.allowedTools
      : CODE_TOOLS;

    try {
      // Call ai/agent — the universal agentic loop
      const result = await Commands.execute('ai/agent', {
        prompt: config.prompt,
        systemPrompt,
        provider,
        model,
        tools,
        maxIterations: config.maxTurns || 10,
        sentinelHandle: config.sentinelHandle,
        personaId: config.personaId,
        temperature: 0.3,
        maxTokens: 4096,
      } as Record<string, unknown>) as unknown as {
        success: boolean;
        text: string;
        toolCalls: Array<{
          toolName: string;
          params: Record<string, string>;
          success: boolean;
          content?: string;
          error?: string;
          durationMs: number;
        }>;
        iterations: number;
        model?: string;
        provider?: string;
        durationMs: number;
        error?: string;
      };

      const durationMs = Date.now() - startTime;

      // Map ai/agent tool calls to CodingAgentToolCall format
      const toolCalls: CodingAgentToolCall[] = (result.toolCalls || []).map(tc => ({
        toolName: tc.toolName,
        input: tc.params as Record<string, unknown>,
        output: tc.content || tc.error || '',
        isError: !tc.success,
        durationMs: tc.durationMs,
      }));

      // Build interactions from the result
      const interactions: CodingAgentInteraction[] = [
        {
          role: 'user',
          content: config.prompt,
          timestamp: startTime,
        },
        {
          role: 'assistant',
          content: result.text || '',
          toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
          timestamp: Date.now(),
        },
      ];

      // Report progress for each tool call
      for (const tc of toolCalls) {
        onProgress?.({
          type: 'tool_end',
          toolName: tc.toolName,
          message: tc.isError ? `${tc.toolName} failed` : `${tc.toolName} completed`,
          timestamp: Date.now(),
        });
      }

      onProgress?.({
        type: 'status',
        message: result.success
          ? `Completed in ${result.iterations} iterations`
          : `Failed: ${result.error}`,
        timestamp: Date.now(),
      });

      return {
        success: result.success,
        text: result.text || '',
        sessionId,
        toolCalls,
        interactions,
        totalCostUsd: 0, // Local models are free; cloud costs not tracked at this layer
        numTurns: result.iterations,
        durationMs,
        model: result.model || model || provider,
        error: result.error,
      };
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        text: '',
        sessionId,
        toolCalls: [],
        interactions: [{
          role: 'user',
          content: config.prompt,
          timestamp: startTime,
        }],
        totalCostUsd: 0,
        numTurns: 0,
        durationMs: Date.now() - startTime,
        model: model || provider,
        error: errorMsg,
      };
    }
  }

  private buildSystemPrompt(config: CodingAgentConfig): string {
    const parts: string[] = [
      `You are a coding agent working in: ${config.cwd}`,
      '',
      'You have access to code tools for reading, writing, editing, searching, and executing shell commands.',
      'Use them to accomplish the task. Be methodical:',
      '1. Read relevant files first to understand the codebase',
      '2. Plan your changes',
      '3. Make edits using code/edit (preferred) or code/write',
      '4. Verify your work with code/verify or code/shell/execute',
      '5. Fix any errors before reporting completion',
      '',
      'Important:',
      '- Always read a file before editing it',
      '- Use code/edit for modifications (not code/write which overwrites)',
      '- Run code/verify after changes to check compilation',
      '- Be concise in your responses — show what you did, not what you plan to do',
    ];

    if (config.systemPrompt) {
      parts.push('', '--- Additional Context ---', '', config.systemPrompt);
    }

    return parts.join('\n');
  }
}
