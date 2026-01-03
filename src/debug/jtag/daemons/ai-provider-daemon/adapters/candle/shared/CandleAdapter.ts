/**
 * Candle Adapter - Native Rust LLM Inference via Candle Framework
 *
 * Replaces Ollama for local inference with key advantages:
 * - Unix socket (no HTTP overhead)
 * - Multi-adapter LoRA composition (genome vision)
 * - Metal-accelerated on Apple Silicon
 * - In-process control (no external binary)
 *
 * Implements AIProviderAdapter interface for seamless integration with
 * AIProviderDaemon - higher-level code doesn't know if it's Candle, Ollama, or API.
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
} from '../../../shared/AIProviderTypesV2';
import {
  InferenceWorkerClient,
  type GenerateRequest as CandleGenerateRequest,
} from '../../../../../system/core/services/InferenceWorkerClient';

// ============================================================================
// Types
// ============================================================================

interface CandleAdapterConfig {
  /** Socket path for inference worker (default: /tmp/jtag-inference.sock) */
  socketPath?: string;
  /** Default model to use (HuggingFace model ID) */
  defaultModel?: string;
  /** Request timeout in ms (default: 60000) */
  timeout?: number;
  /** Max concurrent requests (not currently enforced at adapter level) */
  maxConcurrent?: number;
}

/** Map of model ID to loaded adapters (for multi-adapter composition) */
interface LoadedAdapterInfo {
  modelId: string;
  adapterName: string;
  adapterPath: string;
}

// ============================================================================
// Candle Adapter
// ============================================================================

export class CandleAdapter extends BaseAIProviderAdapter {
  readonly providerId = 'candle';
  readonly providerName = 'Candle (Native Rust)';
  readonly supportedCapabilities: ModelCapability[] = [
    'text-generation',
    'chat',
  ];

  private client: InferenceWorkerClient;
  private defaultModel: string;
  private loadedModels: Set<string> = new Set();
  private loadedAdapters: Map<string, LoadedAdapterInfo[]> = new Map(); // modelId -> adapters

  constructor(config: CandleAdapterConfig = {}) {
    super();

    this.client = config.socketPath
      ? InferenceWorkerClient.create(config.socketPath)
      : InferenceWorkerClient.instance;

    this.defaultModel = config.defaultModel || 'gpt2'; // Safe default
    this.baseTimeout = config.timeout || 60000; // 60s for model loading
  }

  // ============================================================================
  // Core Text Generation
  // ============================================================================

  protected async generateTextImpl(
    request: TextGenerationRequest
  ): Promise<TextGenerationResponse> {
    const startTime = Date.now();
    const requestId = request.requestId || randomUUID();

    // Determine model to use
    const modelId = request.model || this.defaultModel;

    // Ensure model is loaded
    if (!this.loadedModels.has(modelId)) {
      this.log(request, 'info', `Loading model: ${modelId}`);
      try {
        await this.client.loadModel(modelId);
        this.loadedModels.add(modelId);
      } catch (error) {
        throw new Error(
          `Failed to load model ${modelId}: ${error instanceof Error ? error.message : error}`
        );
      }
    }

    // Convert messages to prompt string
    // (Candle currently takes raw prompt, not chat format)
    const prompt = this.formatMessagesAsPrompt(request);

    // Get active adapters for this model (if any)
    const adapters = this.getActiveAdaptersForModel(modelId);

    // Build Candle request
    const candleRequest: CandleGenerateRequest = {
      modelId,
      prompt,
      maxTokens: request.maxTokens || 2048,
      temperature: request.temperature || 0.7,
      topP: request.topP || 0.9,
      adapters: adapters.length > 0 ? adapters : undefined,
    };

    // Generate
    const response = await this.client.generate(candleRequest);

    const responseTime = Date.now() - startTime;

    // Build usage metrics
    const usage: UsageMetrics = {
      inputTokens: response.promptTokens,
      outputTokens: response.generatedTokens,
      totalTokens: response.promptTokens + response.generatedTokens,
      // Local inference is free
      estimatedCost: 0,
    };

    return {
      text: response.text,
      finishReason: 'stop', // TODO: Get actual finish reason from worker
      model: response.modelId,
      provider: this.providerId,
      usage,
      responseTime,
      requestId,
    };
  }

  // ============================================================================
  // Model Management
  // ============================================================================

  async getAvailableModels(): Promise<ModelInfo[]> {
    try {
      const { models } = await this.client.listModels();

      return models.map((m) => ({
        id: m.modelId,
        name: m.modelId,
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'] as ModelCapability[],
        contextWindow: 4096, // TODO: Get from model config
        maxOutputTokens: 2048,
        supportsStreaming: false, // TODO: Add streaming support
        supportsFunctions: false, // TODO: Add function calling
      }));
    } catch (error) {
      // Worker not available - return empty list
      return [];
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    const startTime = Date.now();

    try {
      const available = await this.client.isAvailable();

      if (!available) {
        return {
          status: 'unhealthy',
          apiAvailable: false,
          responseTime: Date.now() - startTime,
          errorRate: 1.0,
          lastChecked: Date.now(),
          message: 'Inference worker not available',
        };
      }

      const pingResult = await this.client.ping();

      return {
        status: 'healthy',
        apiAvailable: true,
        responseTime: Date.now() - startTime,
        errorRate: 0,
        lastChecked: Date.now(),
        message: `Worker: ${pingResult.worker} v${pingResult.version}`,
      };
    } catch (error) {
      return {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime: Date.now() - startTime,
        errorRate: 1.0,
        lastChecked: Date.now(),
        message: error instanceof Error ? error.message : 'Health check failed',
      };
    }
  }

  protected async restartProvider(): Promise<void> {
    // Close connection and let it reconnect on next request
    this.client.close();
    this.loadedModels.clear();
    this.loadedAdapters.clear();

    // TODO: Could spawn the inference worker process if it's not running
    // For now, just reset state and hope the worker recovers
  }

  // ============================================================================
  // Skill/Adapter Management (LoRA)
  // ============================================================================

  /**
   * Apply a LoRA skill/adapter to a model
   *
   * This is THE genome vision - compose multiple adapters at inference time.
   * Example: Load tone_adapter + reasoning_adapter + expertise_adapter
   *
   * The flow is:
   * 1. Load adapter weights into memory (adapter/load)
   * 2. Merge weights into model (adapter/apply) - rebuilds model
   * 3. Generate uses merged weights
   *
   * NOTE: Once adapters are applied, you cannot change them without reloading the model.
   * For multi-adapter composition, load ALL adapters first, then call applySkill once.
   */
  async applySkill(skillImplementation: {
    modelId: string;
    adapterPath: string;
    adapterName: string;
    applyImmediately?: boolean; // Default true - merge weights right away
  }): Promise<void> {
    const { modelId, adapterPath, adapterName, applyImmediately = true } = skillImplementation;

    // Ensure model is loaded first
    if (!this.loadedModels.has(modelId)) {
      await this.client.loadModel(modelId);
      this.loadedModels.add(modelId);
    }

    // Load the adapter weights into memory
    await this.client.loadAdapter(modelId, adapterPath, adapterName);

    // Track loaded adapter
    const adapters = this.loadedAdapters.get(modelId) || [];
    adapters.push({ modelId, adapterName, adapterPath });
    this.loadedAdapters.set(modelId, adapters);

    this.log(null, 'info', `Loaded adapter ${adapterName} for model ${modelId}`);

    // Apply adapters (merge weights) if requested
    if (applyImmediately) {
      const result = await this.client.applyAdapters(modelId);
      this.log(null, 'info', `Applied adapters to ${modelId}: ${result.layersMerged} layers merged in ${result.applyTimeMs}ms`);
    }
  }

  /**
   * Load multiple adapters and apply them together
   *
   * For multi-adapter composition, use this method to load all adapters first,
   * then merge them in one step. This is more efficient than applying one at a time.
   */
  async applySkills(
    modelId: string,
    adapters: Array<{ adapterPath: string; adapterName: string }>
  ): Promise<void> {
    // Ensure model is loaded first
    if (!this.loadedModels.has(modelId)) {
      await this.client.loadModel(modelId);
      this.loadedModels.add(modelId);
    }

    // Load all adapters without applying
    for (const { adapterPath, adapterName } of adapters) {
      await this.client.loadAdapter(modelId, adapterPath, adapterName);

      const tracked = this.loadedAdapters.get(modelId) || [];
      tracked.push({ modelId, adapterName, adapterPath });
      this.loadedAdapters.set(modelId, tracked);

      this.log(null, 'info', `Loaded adapter ${adapterName} for model ${modelId}`);
    }

    // Apply all adapters at once (single weight merge)
    const result = await this.client.applyAdapters(modelId);
    this.log(null, 'info', `Applied ${adapters.length} adapters to ${modelId}: ${result.layersMerged} layers merged in ${result.applyTimeMs}ms`);
  }

  /**
   * Remove a LoRA skill/adapter from a model
   */
  async removeSkill(skillId: string): Promise<void> {
    // skillId format: "modelId:adapterName"
    const [modelId, adapterName] = skillId.split(':');

    if (!modelId || !adapterName) {
      throw new Error(`Invalid skillId format: ${skillId}. Expected "modelId:adapterName"`);
    }

    await this.client.unloadAdapter(modelId, adapterName);

    // Update tracking
    const adapters = this.loadedAdapters.get(modelId) || [];
    const filtered = adapters.filter((a) => a.adapterName !== adapterName);
    this.loadedAdapters.set(modelId, filtered);

    this.log(null, 'info', `Removed adapter ${adapterName} from model ${modelId}`);
  }

  // ============================================================================
  // Helper Methods
  // ============================================================================

  /**
   * Format chat messages as a prompt string for Candle
   *
   * TODO: Use proper chat templates based on model type (Llama, Mistral, etc.)
   */
  private formatMessagesAsPrompt(request: TextGenerationRequest): string {
    const parts: string[] = [];

    // Add system prompt if provided
    if (request.systemPrompt) {
      parts.push(`System: ${request.systemPrompt}\n`);
    }

    // Add messages
    for (const message of request.messages) {
      const content =
        typeof message.content === 'string'
          ? message.content
          : message.content
              .filter((p) => p.type === 'text')
              .map((p) => (p as { type: 'text'; text: string }).text)
              .join('\n');

      switch (message.role) {
        case 'system':
          parts.push(`System: ${content}\n`);
          break;
        case 'user':
          parts.push(`User: ${content}\n`);
          break;
        case 'assistant':
          parts.push(`Assistant: ${content}\n`);
          break;
      }
    }

    // Add assistant prefix for continuation
    parts.push('Assistant:');

    return parts.join('');
  }

  /**
   * Get list of active adapter names for a model
   */
  private getActiveAdaptersForModel(modelId: string): string[] {
    const adapters = this.loadedAdapters.get(modelId) || [];
    return adapters.map((a) => a.adapterName);
  }

  /**
   * Preload a model to avoid cold-start latency
   */
  async preloadModel(modelId: string): Promise<void> {
    if (!this.loadedModels.has(modelId)) {
      await this.client.loadModel(modelId);
      this.loadedModels.add(modelId);
    }
  }

  /**
   * Unload a model to free memory
   */
  async unloadModel(modelId: string): Promise<void> {
    if (this.loadedModels.has(modelId)) {
      await this.client.unloadModel(modelId);
      this.loadedModels.delete(modelId);
      this.loadedAdapters.delete(modelId);
    }
  }
}
