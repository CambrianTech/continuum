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

  // Serial execution: only one request at a time
  // The Rust gRPC server is single-threaded, so queuing requests just causes timeouts
  private inFlight: boolean = false;
  private queueDepth: number = 0;
  private static readonly MAX_QUEUE_DEPTH = 3; // Reject if more than 3 waiting

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

    // SERIAL EXECUTION GUARD: Reject if queue is too deep
    // The gRPC server is single-threaded, so deep queues just cause timeouts
    if (this.queueDepth >= CandleGrpcAdapter.MAX_QUEUE_DEPTH) {
      throw new Error(`CandleGrpcAdapter: Queue full (${this.queueDepth} waiting). Try again later.`);
    }

    // Track queue depth for rejection decisions
    this.queueDepth++;

    // Wait for current request to finish (simple serial lock)
    while (this.inFlight) {
      await new Promise(resolve => setTimeout(resolve, 100));
      // Check if we've been waiting too long
      if (Date.now() - startTime > 30000) {
        this.queueDepth--;
        throw new Error('CandleGrpcAdapter: Timeout waiting for slot (30s)');
      }
    }

    this.inFlight = true;
    this.queueDepth--;

    try {
      // Convert messages to prompt and truncate to prevent OOM
      // Qwen2-1.5B has 32K context but we limit to 8K chars (~2K tokens) for memory safety
      const MAX_PROMPT_CHARS = 8000;
      let prompt = this.formatMessagesAsPrompt(request);
      if (prompt.length > MAX_PROMPT_CHARS) {
        console.log(`[CandleGrpcAdapter] Truncating prompt from ${prompt.length} to ${MAX_PROMPT_CHARS} chars`);
        prompt = prompt.slice(-MAX_PROMPT_CHARS); // Keep the most recent context
      }

      // Cap maxTokens to prevent queue starvation
      // Local inference ~14 tok/sec, so 150 tokens = ~10 seconds
      const requestedTokens = request.maxTokens || 200;
      const maxTokens = Math.min(requestedTokens, 150);

      // SINGLE MODEL ROUTING: All requests go to the loaded model
      // The gRPC server only has one model loaded (Qwen2-1.5B-Instruct)
      // So we route everything to it regardless of what model was requested
      const modelId = 'Qwen/Qwen2-1.5B-Instruct';

      console.log(`[CandleGrpcAdapter] Generate: model=${modelId}, prompt=${prompt.length} chars, maxTokens=${maxTokens}, queue=${this.queueDepth}`);

      const result = await this.client.generate(modelId, prompt, {
        maxTokens,
        temperature: request.temperature,
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
    } finally {
      // Always release the lock
      this.inFlight = false;
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
