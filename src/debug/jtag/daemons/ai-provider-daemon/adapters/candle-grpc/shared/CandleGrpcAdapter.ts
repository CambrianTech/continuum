/**
 * CandleGrpcAdapter - Local inference via gRPC
 *
 * Simple adapter that replaces the broken Unix socket client with gRPC.
 * Currently returns hardcoded responses to validate end-to-end communication.
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
    this.baseTimeout = 120000; // 2 minutes
  }

  async initialize(): Promise<void> {
    // Test connection
    try {
      const pong = await this.client.ping();
      console.log(`[CandleGrpcAdapter] Connected: ${pong.message}`);
    } catch (err) {
      console.error(`[CandleGrpcAdapter] Failed to connect:`, err);
      throw err;
    }
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
    // Return fake models for now
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
    // Close and reconnect
    this.client.close();
    this.client = InferenceGrpcClient.sharedInstance();
    await this.initialize();
  }

  protected async generateTextImpl(
    request: TextGenerationRequest
  ): Promise<TextGenerationResponse> {
    const startTime = Date.now();
    const requestId = request.requestId || randomUUID();

    // Convert messages to prompt
    const prompt = this.formatMessagesAsPrompt(request);
    const maxTokens = request.maxTokens || 200;

    console.log(`[CandleGrpcAdapter] Generate: prompt=${prompt.length} chars, maxTokens=${maxTokens}`);

    try {
      const result = await this.client.generate(prompt, maxTokens, {
        timeoutMs: this.baseTimeout,
      });

      const responseTime = Date.now() - startTime;

      const usage: UsageMetrics = {
        inputTokens: Math.ceil(prompt.length / 4), // rough estimate
        outputTokens: result.tokens,
        totalTokens: Math.ceil(prompt.length / 4) + result.tokens,
        estimatedCost: 0, // local = free
      };

      const routing: RoutingInfo = {
        provider: this.providerId,
        isLocal: true,
        routingReason: 'explicit_provider',
        adaptersApplied: [],
        modelRequested: request.model || 'llama3.2:3b',
      };

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
    } catch (err) {
      console.error(`[CandleGrpcAdapter] Generate failed:`, err);
      throw err;
    }
  }

  /**
   * Format chat messages as a single prompt string
   */
  private formatMessagesAsPrompt(request: TextGenerationRequest): string {
    if (!request.messages || request.messages.length === 0) {
      return '';
    }

    // Simple format: role: content
    return request.messages
      .map((m) => `${m.role}: ${m.content}`)
      .join('\n');
  }
}
