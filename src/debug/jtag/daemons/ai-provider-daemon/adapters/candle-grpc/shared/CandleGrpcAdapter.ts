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
  // The Rust gRPC server is single-threaded, so we must wait for each request to complete
  private inFlight: boolean = false;
  private queueDepth: number = 0;
  // Increased timeout: local inference takes ~15s per request
  // With queue depth of 3, worst case wait is ~60s (acceptable for local inference)
  private static readonly MAX_WAIT_TIME_MS = 90000; // 90 seconds max wait

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

    // Track queue depth for logging
    this.queueDepth++;
    if (this.queueDepth > 1) {
      console.log(`[CandleGrpcAdapter] Queue depth: ${this.queueDepth} (waiting for in-flight request)`);
    }

    // Wait for current request to finish (simple serial lock)
    // Local inference takes ~15s per request, so we wait patiently
    while (this.inFlight) {
      await new Promise(resolve => setTimeout(resolve, 100));
      // Check if we've been waiting too long
      if (Date.now() - startTime > CandleGrpcAdapter.MAX_WAIT_TIME_MS) {
        this.queueDepth--;
        throw new Error(`CandleGrpcAdapter: Timeout waiting for slot (${CandleGrpcAdapter.MAX_WAIT_TIME_MS / 1000}s)`);
      }
    }

    this.inFlight = true;
    this.queueDepth--;

    try {
      // Convert messages to prompt and truncate to prevent OOM
      // Llama 3.2 3B has 128K context but we limit to 8K chars (~2K tokens) for memory safety
      const MAX_PROMPT_CHARS = 8000;
      let prompt = this.formatMessagesAsLlama32(request);
      if (prompt.length > MAX_PROMPT_CHARS) {
        console.log(`[CandleGrpcAdapter] Truncating prompt from ${prompt.length} to ${MAX_PROMPT_CHARS} chars`);
        prompt = prompt.slice(-MAX_PROMPT_CHARS); // Keep the most recent context
      }

      // Cap maxTokens to prevent queue starvation
      // Local inference ~14 tok/sec, so 150 tokens = ~10 seconds
      const requestedTokens = request.maxTokens || 200;
      const maxTokens = Math.min(requestedTokens, 150);

      // SINGLE MODEL ROUTING: All requests go to the loaded model
      // The gRPC server loads Llama-3.2-3B-Instruct
      const modelId = 'Llama-3.2-3B-Instruct';

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
   * Format chat messages using Llama 3.2 chat template
   *
   * Llama 3.2 uses special tokens:
   * - <|begin_of_text|> at the start
   * - <|start_header_id|>role<|end_header_id|> before each message
   * - <|eot_id|> at the end of each message
   *
   * Example:
   * <|begin_of_text|><|start_header_id|>system<|end_header_id|>
   * You are a helpful assistant.<|eot_id|><|start_header_id|>user<|end_header_id|>
   * What is 2+2?<|eot_id|><|start_header_id|>assistant<|end_header_id|>
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

    // Add the assistant header to prompt the model to generate
    parts.push('<|start_header_id|>assistant<|end_header_id|>\n\n');

    return parts.join('');
  }
}
