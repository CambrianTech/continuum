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
import { LocalContextBuilder } from './LocalContextBuilder';
import { LocalModelRouter } from './LocalModelRouter';
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { AiAgentResult } from '../../../commands/ai/agent/shared/AiAgentTypes';

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
    const modelExplicitlySet = !!config.model;

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

    const isLocal = provider === 'candle' || provider === 'candle-q';

    // For local providers without an explicit model override: route based on VRAM.
    // This determines whether we get BF16 batch prefill (800 token budget) or
    // GGUF token-by-token (350 token budget). The Rust side does the actual
    // model selection — the router just informs budget math here.
    let maxSystemTokens = 350;
    let usesBatchPrefill = false;
    if (isLocal && !modelExplicitlySet) {
      const gpuStats = await this.fetchGpuStats();
      const routing = LocalModelRouter.sharedInstance().route(gpuStats.totalVramMb);
      model = routing.model;
      maxSystemTokens = routing.maxSystemTokens;
      usesBatchPrefill = routing.usesBatchPrefill;
    }

    // Build dynamic system prompt using colon-shorthand tool format.
    // LocalContextBuilder selects task-relevant tools and formats them compactly
    // with colon-shorthand examples — the native output format of the 14B coder model.
    // tools=[] prevents AiAgentServerCommand from injecting its own verbose tool docs.
    // Cloud providers use full ToolGroupRegistry (undefined = all public tools).
    let systemPrompt: string;
    let tools: string[] | undefined;
    if (isLocal) {
      const ctxResult = await LocalContextBuilder.sharedInstance().build({
        cwd: config.cwd,
        taskPrompt: config.prompt,
        maxSystemTokens,
        customSystemPrompt: config.systemPrompt,
      });
      systemPrompt = ctxResult.systemPrompt;
      tools = []; // Compact format is in systemPrompt; don't let ai/agent add verbose docs
    } else {
      systemPrompt = config.systemPrompt ?? `You are a coding agent working in: ${config.cwd}`;
      tools = config.allowedTools ?? undefined;
    }

    try {
      // Start agent — local providers return a handle immediately and run in background.
      // Cloud providers block until complete (fast enough for request/response).
      const agentResponse = await Commands.execute('ai/agent', {
        prompt: config.prompt,
        systemPrompt,
        provider,
        model,
        tools,
        maxIterations: config.maxTurns || 10,
        sentinelHandle: config.sentinelHandle,
        personaId: config.personaId,
        // GGUF: greedy decoding (temperature=0) — quantized models get stuck in repetition
      // loops under multinomial sampling at any temperature. BF16 batch prefill path
      // can handle temperature > 0 since the model is more coherent.
      temperature: isLocal && !usesBatchPrefill ? 0 : 0.3,
        maxTokens: 4096,
      } as Record<string, unknown>) as unknown as AiAgentResult;

      // Local providers return { handleId, status: 'started' } immediately.
      // Subscribe to the completion event and wait — no IPC timeout, no 300s cliff.
      const result = agentResponse.handleId
        ? await this.waitForCompletion(agentResponse.handleId, onProgress)
        : agentResponse;

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

  /**
   * Fetch GPU stats for model routing decisions.
   * Returns zero VRAM on failure (routes to GGUF path, safe default).
   */
  private async fetchGpuStats(): Promise<{ totalVramMb: number }> {
    try {
      const stats = await Commands.execute('gpu/stats') as { totalVramMb?: number };
      return { totalVramMb: stats.totalVramMb ?? 0 };
    } catch {
      return { totalVramMb: 0 };
    }
  }

  /**
   * Wait for a background ai/agent session to complete via events.
   * No request timeout — the caller's context (e.g. Sentinel step timeout) controls lifetime.
   */
  private waitForCompletion(
    handleId: string,
    onProgress?: (event: CodingAgentProgressEvent) => void,
  ): Promise<AiAgentResult> {
    return new Promise((resolve, reject) => {
      const unsubComplete = Events.subscribe(`ai:agent:${handleId}:complete`, (data) => {
        unsubComplete();
        unsubError();
        resolve(data as AiAgentResult);
      });

      const unsubError = Events.subscribe(`ai:agent:${handleId}:error`, (data) => {
        unsubComplete();
        unsubError();
        const msg = (data as { error?: string }).error ?? 'Agent failed';
        reject(new Error(msg));
      });

      onProgress?.({
        type: 'status',
        message: `Running (handle: ${handleId})`,
        timestamp: Date.now(),
      });
    });
  }
}
