/**
 * CandleGrpcAdapter - Local inference via gRPC
 *
 * SIMPLE DESIGN:
 * 1. Send request to gRPC server
 * 2. Wait for response
 * 3. Return response
 *
 * NO circuit breakers. NO safe mode. NO garbage detection.
 * If it fails, it fails loudly. No "protective" nonsense.
 *
 * LOGGING:
 * - Per-persona: writes to persona's adapters.log when personaContext available
 * - Fallback: console.log for requests without personaContext
 */

import * as fs from 'fs';
import * as path from 'path';
import { generateUUID } from '../../../../../system/core/types/CrossPlatformUUID';
import { BaseAIProviderAdapter } from '../../../shared/BaseAIProviderAdapter';
import type {
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
  ModelCapability,
  ModelInfo,
  UsageMetrics,
  RoutingInfo,
} from '../../../shared/AIProviderTypesV2';
import { InferenceGrpcClient } from '../../../../../system/core/services/InferenceGrpcClient';

export class CandleGrpcAdapter extends BaseAIProviderAdapter {
  readonly providerId = 'candle';
  readonly providerName = 'Candle (gRPC)';
  readonly supportedCapabilities: ModelCapability[] = ['text-generation', 'chat'];

  private client: InferenceGrpcClient;

  constructor() {
    super();
    this.client = InferenceGrpcClient.sharedInstance();
    this.baseTimeout = 300000; // 5 minutes - let it complete
  }

  /**
   * Log to persona's adapters.log if personaContext available, else console
   */
  private logToPersona(logDir: string | undefined, message: string): void {
    const timestamp = new Date().toISOString();
    const logLine = `[${timestamp}] ${message}\n`;

    if (logDir) {
      try {
        const logFile = path.join(logDir, 'adapters.log');
        fs.appendFileSync(logFile, logLine);
      } catch {
        // Fallback to console if file write fails
        console.log(`[CandleGrpcAdapter] ${message}`);
      }
    } else {
      console.log(`[CandleGrpcAdapter] ${message}`);
    }
  }

  async initialize(): Promise<void> {
    const pong = await this.client.ping();
    console.log(`[CandleGrpcAdapter] Connected: ${pong.message}`);
  }

  async shutdown(): Promise<void> {
    this.client.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const start = Date.now();
      await this.client.ping();
      return {
        status: 'healthy',
        apiAvailable: true,
        responseTime: Date.now() - start,
        errorRate: 0,
        lastChecked: Date.now(),
      };
    } catch (err) {
      return {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime: 0,
        errorRate: 1,
        lastChecked: Date.now(),
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  async getAvailableModels(): Promise<ModelInfo[]> {
    return [
      {
        id: 'llama3.2:3b',
        name: 'Llama 3.2 3B',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 8192,
        supportsStreaming: true,
        supportsFunctions: false,
      },
    ];
  }

  async restartProvider(): Promise<void> {
    this.client.close();
    this.client = InferenceGrpcClient.sharedInstance();
    await this.initialize();
  }

  protected async generateTextImpl(
    request: TextGenerationRequest
  ): Promise<TextGenerationResponse> {
    const startTime = Date.now();
    const requestId = request.requestId || generateUUID();

    // Format prompt
    const prompt = this.formatMessagesAsLlama32(request);

    // Cap tokens reasonably
    const maxTokens = Math.min(request.maxTokens || 150, 200);

    // Extract persona context for per-persona logging
    const personaId = request.personaContext?.uniqueId || '';
    const personaName = request.personaContext?.displayName || 'unknown';
    const logDir = request.personaContext?.logDir;

    this.logToPersona(logDir, `[Candle] Generate: prompt=${prompt.length} chars, maxTokens=${maxTokens}`);

    // Just call the gRPC server and wait - includes persona info for Rust logging
    const result = await this.client.generate('Llama-3.2-3B-Instruct', prompt, {
      maxTokens,
      temperature: request.temperature,
      timeoutMs: this.baseTimeout,
      personaId,
      personaName,
    });

    const responseTime = Date.now() - startTime;

    const usage: UsageMetrics = {
      inputTokens: Math.ceil(prompt.length / 4),
      outputTokens: result.tokens,
      totalTokens: Math.ceil(prompt.length / 4) + result.tokens,
      estimatedCost: 0,
    };

    const routing: RoutingInfo = {
      provider: this.providerId,
      isLocal: true,
      routingReason: 'explicit_provider',
      adaptersApplied: [],
      modelRequested: request.model || 'llama3.2:3b',
    };

    this.logToPersona(logDir, `[Candle] Complete: ${result.tokens} tokens in ${responseTime}ms`);

    return {
      text: result.text,
      finishReason: 'stop',
      model: request.model || 'llama3.2:3b',
      provider: this.providerId,
      usage,
      responseTime,
      requestId,
      routing,
    };
  }

  /**
   * Format chat messages using Llama 3.2 chat template
   *
   * Includes automatic truncation to fit within model context window:
   * - Llama 3.2 3B has 8K token context (~32K chars)
   * - We target 24K chars max to leave room for response
   * - Truncation preserves system prompt + recent messages
   */
  private formatMessagesAsLlama32(request: TextGenerationRequest): string {
    if (!request.messages || request.messages.length === 0) {
      return '';
    }

    // Target 24K chars max (leaves ~8K chars / 2K tokens for response)
    const MAX_PROMPT_CHARS = 24000;
    const parts: string[] = ['<|begin_of_text|>'];

    // Format all messages first
    const formattedMessages: { role: string; content: string; formatted: string }[] = [];
    for (const msg of request.messages) {
      const role = msg.role === 'assistant' ? 'assistant' : msg.role === 'system' ? 'system' : 'user';
      // Handle both string and ContentPart[] message content
      const contentStr = typeof msg.content === 'string'
        ? msg.content
        : msg.content.map(p => p.type === 'text' ? p.text : '[media]').join('\n');
      const formatted = `<|start_header_id|>${role}<|end_header_id|>\n\n${contentStr}<|eot_id|>`;
      formattedMessages.push({ role, content: contentStr, formatted });
    }

    // Calculate current size
    let totalChars = formattedMessages.reduce((sum, m) => sum + m.formatted.length, 0);
    totalChars += 50; // Overhead for begin/end tokens

    // If under limit, use all messages
    if (totalChars <= MAX_PROMPT_CHARS) {
      for (const msg of formattedMessages) {
        parts.push(msg.formatted);
      }
    } else {
      // Truncation strategy: keep system prompt + last N messages
      const systemMsgs = formattedMessages.filter(m => m.role === 'system');
      const nonSystemMsgs = formattedMessages.filter(m => m.role !== 'system');

      // Add system messages first (usually small)
      let charBudget = MAX_PROMPT_CHARS - 50; // Reserve for tokens
      for (const msg of systemMsgs) {
        if (msg.formatted.length <= charBudget) {
          parts.push(msg.formatted);
          charBudget -= msg.formatted.length;
        }
      }

      // Add recent non-system messages from the end (most relevant)
      const recentMsgs: string[] = [];
      for (let i = nonSystemMsgs.length - 1; i >= 0 && charBudget > 0; i--) {
        const msg = nonSystemMsgs[i];
        if (msg.formatted.length <= charBudget) {
          recentMsgs.unshift(msg.formatted);
          charBudget -= msg.formatted.length;
        } else {
          // Truncate the oldest message we're including if needed
          const truncated = msg.formatted.slice(0, charBudget);
          recentMsgs.unshift(truncated + '...[truncated]<|eot_id|>');
          break;
        }
      }
      parts.push(...recentMsgs);

      console.log(`[CandleGrpcAdapter] Truncated prompt from ${totalChars} to ${parts.join('').length} chars`);
    }

    parts.push('<|start_header_id|>assistant<|end_header_id|>\n\n');

    return parts.join('');
  }
}
