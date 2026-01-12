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
 */

import { randomUUID } from 'crypto';
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
    const requestId = request.requestId || randomUUID();

    // Format prompt
    const prompt = this.formatMessagesAsLlama32(request);

    // Cap tokens reasonably
    const maxTokens = Math.min(request.maxTokens || 150, 200);

    // Extract persona context for per-persona logging in Rust
    const personaId = request.personaContext?.uniqueId || '';
    const personaName = request.personaContext?.displayName || 'unknown';

    console.log(`[CandleGrpcAdapter] [${personaName}] Generate: prompt=${prompt.length} chars, maxTokens=${maxTokens}`);

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

    console.log(`[CandleGrpcAdapter] [${personaName}] Complete: ${result.tokens} tokens in ${responseTime}ms`);

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
   */
  private formatMessagesAsLlama32(request: TextGenerationRequest): string {
    if (!request.messages || request.messages.length === 0) {
      return '';
    }

    const parts: string[] = ['<|begin_of_text|>'];

    for (const msg of request.messages) {
      const role = msg.role === 'assistant' ? 'assistant' : msg.role === 'system' ? 'system' : 'user';
      parts.push(`<|start_header_id|>${role}<|end_header_id|>\n\n${msg.content}<|eot_id|>`);
    }

    parts.push('<|start_header_id|>assistant<|end_header_id|>\n\n');

    return parts.join('');
  }
}
