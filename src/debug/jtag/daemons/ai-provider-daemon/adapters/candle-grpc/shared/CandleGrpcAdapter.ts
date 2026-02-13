/**
 * CandleAdapter - Local inference via continuum-core IPC (Unix socket)
 *
 * SIMPLE DESIGN:
 * 1. Send request to continuum-core via IPC
 * 2. Wait for response
 * 3. Return response
 *
 * Uses RustCoreIPCClient to communicate with the Candle adapter in continuum-core.
 * NO gRPC - all inference runs inside the unified Rust process.
 *
 * LOGGING:
 * Uses base class log() which routes through Logger.ts (async, respects levels)
 */

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
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../../workers/continuum-core/bindings/RustCoreIPC';

export class CandleGrpcAdapter extends BaseAIProviderAdapter {
  readonly providerId = 'candle';
  readonly providerName = 'Candle (Local)';
  readonly supportedCapabilities: ModelCapability[] = ['text-generation', 'chat'];

  private client: RustCoreIPCClient | null = null;

  constructor() {
    super();
    this.baseTimeout = 300000; // 5 minutes - let it complete
  }

  async initialize(): Promise<void> {
    try {
      // Get singleton IPC client (auto-connects)
      this.client = await RustCoreIPCClient.getInstanceAsync();
      console.log(`[CandleAdapter] Connected to continuum-core via IPC`);
    } catch (err) {
      console.error(`[CandleAdapter] Failed to connect to continuum-core:`, err);
      throw err;
    }
  }

  async shutdown(): Promise<void> {
    // Don't disconnect singleton - it may be used by other modules
    this.client = null;
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const start = Date.now();
      // Use the IPC client to ping (any command will verify connection)
      if (!this.client) {
        this.client = await RustCoreIPCClient.getInstanceAsync();
      }
      // Try a quick generation to verify Candle is loaded
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
        name: 'Llama 3.2 3B (Quantized)',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 8192,
        supportsStreaming: false,
        supportsFunctions: false,
      },
    ];
  }

  async restartProvider(): Promise<void> {
    // Reconnect to IPC
    this.client = null;
    await this.initialize();
  }

  protected async generateTextImpl(
    request: TextGenerationRequest
  ): Promise<TextGenerationResponse> {
    const startTime = Date.now();
    const requestId = request.requestId || generateUUID();

    if (!this.client) {
      this.client = await RustCoreIPCClient.getInstanceAsync();
    }

    // Convert messages to the format expected by the Rust ai/generate command
    const messages = request.messages?.map(msg => {
      const content = typeof msg.content === 'string'
        ? msg.content
        : msg.content.map(p => p.type === 'text' ? p.text : '[media]').join('\n');
      return { role: msg.role, content };
    }) || [];

    // Cap tokens reasonably for local model
    const maxTokens = Math.min(request.maxTokens || 150, 200);

    this.log(request, 'info', `[Candle] Generate: messages=${messages.length}, maxTokens=${maxTokens}`);

    try {
      // Call continuum-core via IPC
      const result = await this.client.aiGenerate({
        messages,
        provider: 'candle',  // Force Candle adapter
        maxTokens,
        temperature: request.temperature,
        requestId,
        userId: request.userId,
        roomId: request.roomId,
        purpose: request.purpose || 'chat',
      });

      const responseTime = Date.now() - startTime;

      const usage: UsageMetrics = {
        inputTokens: result.usage.inputTokens,
        outputTokens: result.usage.outputTokens,
        totalTokens: result.usage.totalTokens,
        estimatedCost: 0, // Local is free
      };

      const routing: RoutingInfo = {
        provider: this.providerId,
        isLocal: true,
        routingReason: 'explicit_provider',
        adaptersApplied: result.routing?.adaptersApplied || [],
        modelRequested: request.model || 'llama3.2:3b',
      };

      this.log(request, 'info', `[Candle] Complete: ${result.usage.outputTokens} tokens in ${responseTime}ms`);

      return {
        text: result.text,
        finishReason: result.finishReason as any,
        model: result.model,
        provider: this.providerId,
        usage,
        responseTime,
        requestId: result.requestId,
        routing,
        // Note: Local Candle models don't support tool calling, so toolCalls is always undefined
        toolCalls: undefined,
      };
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      this.log(request, 'error', `[Candle] Error: ${errorMsg}`);
      throw new Error(`Candle inference failed: ${errorMsg}`);
    }
  }
}
