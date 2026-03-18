/**
 * LocalAgentProvider — CodingAgentProvider backed by the universal ai/agent command.
 *
 * Wraps the existing agentic tool loop (any provider x any model x all code tools)
 * so sentinel CodingAgent steps can run entirely locally or with any cloud provider,
 * not just Claude Code.
 *
 * Provider routing:
 * - 'local-agent'            -> Candle (local Llama) — fully offline
 * - 'local-agent:deepseek'   -> DeepSeek via cloud
 * - 'local-agent:anthropic'  -> Anthropic via API
 * - 'local-agent:groq'       -> Groq cloud
 * - etc.
 *
 * Tool resolution is fully delegated to ai/agent:
 * - config.allowedTools set   -> passes that subset to ai/agent
 * - config.allowedTools unset -> passes tools=undefined, ai/agent resolves all public tools
 *
 * System prompt is minimal — just identity + workspace. Tool formatting, group selection,
 * and budget management are handled by AiAgentServerCommand + ToolGroupRegistry.
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

    // Minimal system prompt: identity + workspace context.
    // Tool definitions, group selection, and budget management are handled
    // entirely by ai/agent (AiAgentServerCommand + ToolGroupRegistry).
    const systemParts: string[] = [
      `You are a coding agent working in: ${config.cwd}`,
    ];
    if (config.systemPrompt) {
      systemParts.push('', config.systemPrompt);
    }
    const systemPrompt = systemParts.join('\n');

    // Tool resolution:
    // - config.allowedTools defined -> pass that subset (ai/agent filters to these)
    // - config.allowedTools undefined -> pass undefined (ai/agent resolves all public tools)
    const tools = config.allowedTools ?? undefined;

    try {
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
}
