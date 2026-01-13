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
import { LOCAL_MODELS } from '../../../../../system/shared/Constants';
import { existsSync } from 'fs';
import { resolve } from 'path';

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
  /**
   * Maximum input tokens before truncation (default: 4000)
   *
   * Small models (1.5B params) process ~8-10ms per token. With a 180s timeout:
   * - 4000 tokens ≈ 35s (safe)
   * - 10000 tokens ≈ 90s (risky)
   * - 20000 tokens ≈ 180s (timeout!)
   *
   * This limit prevents timeout errors from large RAG contexts.
   */
  maxInputTokens?: number;
}

/** Map of model ID to loaded adapters (for multi-adapter composition) */
interface LoadedAdapterInfo {
  modelId: string;
  adapterName: string;
  adapterPath: string;
}

// ============================================================================
// Model Name Mapping (Ollama → HuggingFace)
// ============================================================================
// Uses LOCAL_MODELS from Constants.ts as SINGLE SOURCE OF TRUTH
// See system/shared/Constants.ts for the canonical mapping

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

  private client: InferenceGrpcClient;
  private defaultModel: string;
  private loadedModels: Set<string> = new Set();
  private loadedAdapters: Map<string, LoadedAdapterInfo[]> = new Map(); // modelId -> adapters
  private maxInputTokens: number;

  constructor(config: CandleAdapterConfig = {}) {
    super();

    // Use gRPC client (replaces Unix socket)
    this.client = InferenceGrpcClient.sharedInstance();

    this.defaultModel = config.defaultModel || LOCAL_MODELS.DEFAULT;
    this.baseTimeout = config.timeout || 180000; // 180s to handle model download + generation
    this.maxInputTokens = config.maxInputTokens || 4000; // ~35s at 8ms/token

    // Note: Model is pre-loaded by gRPC server at startup
  }

  // Note: Model is pre-loaded by gRPC server at startup, not by TypeScript

  // ============================================================================
  // Core Text Generation
  // ============================================================================

  protected async generateTextImpl(
    request: TextGenerationRequest
  ): Promise<TextGenerationResponse> {
    const startTime = Date.now();
    const requestId = request.requestId || generateUUID();

    this.log(request, 'info', `🔧 TRACE-1: generateTextImpl START (requestId=${requestId.slice(0,8)})`);

    // Determine model to use - map Ollama names to HuggingFace via central config
    const requestedModel = request.model || this.defaultModel;
    const modelId = LOCAL_MODELS.mapToHuggingFace(requestedModel);

    // Log mapping if different
    if (modelId !== requestedModel) {
      this.log(request, 'info', `Model mapped: ${requestedModel} → ${modelId}`);
    }

    // Model is pre-loaded by gRPC server at startup
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const ensureModelLoaded = async (): Promise<void> => {
      // No-op: gRPC server preloads model
    };

    await ensureModelLoaded();
    this.log(request, 'info', `🔧 TRACE-2: Model loaded successfully`);

    // GENOME INTEGRATION: Load adapters from request if provided
    // This enables PersonaGenome to specify which LoRA skills should be active
    if (request.activeAdapters && request.activeAdapters.length > 0) {
      this.log(request, 'info', `🧬 TRACE-2.5: Loading ${request.activeAdapters.length} adapters from request`);

      // Check which adapters need to be loaded
      const currentAdapters = new Set(this.getActiveAdaptersForModel(modelId));
      const needsReapply = request.activeAdapters.some(a => !currentAdapters.has(a.name));

      if (needsReapply) {
        // Filter out adapters with non-existent files BEFORE loading
        // This prevents blocking the inference queue on missing adapter files
        const adaptersToLoad = request.activeAdapters
          .filter(a => {
            const absolutePath = resolve(a.path);
            if (!existsSync(absolutePath)) {
              this.log(request, 'warn', `🧬 Skipping adapter ${a.name}: file not found at ${absolutePath}`);
              return false;
            }
            return true;
          })
          .map(a => ({
            adapterPath: a.path,
            adapterName: a.name,
          }));

        if (adaptersToLoad.length > 0) {
          try {
            await this.applySkills(modelId, adaptersToLoad);
            this.log(request, 'info', `🧬 TRACE-2.6: Adapters applied: [${adaptersToLoad.map(a => a.adapterName).join(', ')}]`);
          } catch (error) {
            // Log but don't fail - generation can proceed without adapters
            this.log(request, 'warn', `🧬 TRACE-2.6: Failed to apply adapters: ${error instanceof Error ? error.message : error}`);
          }
        } else {
          this.log(request, 'info', `🧬 TRACE-2.5: No adapters to load (all files missing or already loaded)`);
        }
      } else {
        this.log(request, 'info', `🧬 TRACE-2.5: All requested adapters already loaded`);
      }
    }

    // Convert messages to prompt string
    // (Candle currently takes raw prompt, not chat format)
    let prompt = this.formatMessagesAsPrompt(request);
    this.log(request, 'info', `🔧 TRACE-3: Prompt formatted (${prompt.length} chars)`);

    // CRITICAL: Truncate prompt if too long for fast inference
    // Small models (1.5B) take ~8-10ms per token; 20k tokens = 180s timeout!
    const { truncated, estimatedTokens, wasTruncated } = this.truncatePromptIfNeeded(prompt);
    prompt = truncated;

    if (wasTruncated) {
      this.log(request, 'warn', `⚠️ Prompt truncated from ${estimatedTokens} to ~${this.maxInputTokens} tokens to prevent timeout`);
    }

    // Get active adapters for this model (includes any just loaded from request)
    const adapters = this.getActiveAdaptersForModel(modelId);
    const maxTokens = request.maxTokens || 2048;
    const temperature = request.temperature || 0.7;

    // Generate via gRPC
    let grpcResponse;
    this.log(request, 'info', `🔧 TRACE-4: Calling gRPC generate (prompt=${prompt.length} chars, maxTokens=${maxTokens}, adapters=[${adapters.join(',')}])`);
    try {
      // gRPC client handles all transport - Rust does the heavy ML logic
      grpcResponse = await this.client.generate(modelId, prompt, { maxTokens, temperature });
      this.log(request, 'info', `🔧 TRACE-5: gRPC generate returned: text=${grpcResponse.text?.length ?? 0} chars, tokens=${grpcResponse.tokens}`);
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      this.log(request, 'error', `🔧 TRACE-ERROR: gRPC generate threw: ${errorMsg}`);
      throw error;
    }

    this.log(request, 'info', `🔧 TRACE-6: Building response object`);
    const responseTime = Date.now() - startTime;

    // Estimate input tokens from prompt length (rough: 4 chars per token)
    const estimatedInputTokens = Math.ceil(prompt.length / 4);

    // Build usage metrics
    const usage: UsageMetrics = {
      inputTokens: estimatedInputTokens,
      outputTokens: grpcResponse.tokens,
      totalTokens: estimatedInputTokens + grpcResponse.tokens,
      // Local inference is free
      estimatedCost: 0,
    };

    // Build routing info for observability
    // This tells AIProviderDaemon (and ultimately the caller) exactly what happened
    const routing: RoutingInfo = {
      provider: this.providerId,
      isLocal: true,  // Candle is always local
      routingReason: 'explicit_provider', // Will be overridden by daemon if different
      adaptersApplied: adapters,  // Which LoRA adapters were active
      modelMapped: modelId !== requestedModel ? modelId : undefined,  // Show mapping if different
      modelRequested: requestedModel,
    };

    const finalResponse: TextGenerationResponse = {
      text: grpcResponse.text,
      finishReason: 'stop' as const, // TODO: Get actual finish reason from worker
      model: modelId,
      provider: this.providerId,
      usage,
      responseTime,
      requestId,
      routing,
    };

    this.log(request, 'info', `🔧 TRACE-7: Returning response (text=${finalResponse.text.length} chars, ${responseTime}ms, adapters=[${adapters.join(',')}])`);
    return finalResponse;
  }

  // ============================================================================
  // Model Management
  // ============================================================================

  async getAvailableModels(): Promise<ModelInfo[]> {
    // Return the preloaded model (gRPC server only supports one model currently)
    return [{
      id: LOCAL_MODELS.DEFAULT,
      name: LOCAL_MODELS.DEFAULT,
      provider: this.providerId,
      capabilities: ['text-generation', 'chat'] as ModelCapability[],
      contextWindow: 4096,
      maxOutputTokens: 2048,
      supportsStreaming: false,
      supportsFunctions: false,
    }];
  }

  async healthCheck(): Promise<HealthStatus> {
    const startTime = Date.now();

    try {
      const pingResult = await this.client.ping();

      return {
        status: 'healthy',
        apiAvailable: true,
        responseTime: Date.now() - startTime,
        errorRate: 0,
        lastChecked: Date.now(),
        message: pingResult.message,
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
    // Close connection - will reconnect on next request
    this.client.close();
    this.loadedModels.clear();
    this.loadedAdapters.clear();
  }

  // ============================================================================
  // Skill/Adapter Management (LoRA) - STUBBED
  // TODO: Re-implement when gRPC server supports LoRA
  // ============================================================================

  /**
   * Apply a LoRA skill/adapter to a model
   * STUBBED: gRPC server doesn't support LoRA yet
   */
  async applySkill(skillImplementation: {
    modelId: string;
    adapterPath: string;
    adapterName: string;
    applyImmediately?: boolean;
  }): Promise<void> {
    this.log(null, 'warn', `🧬 applySkill: LoRA not yet supported in gRPC server (adapter: ${skillImplementation.adapterName})`);
    // Track for future use
    const modelId = LOCAL_MODELS.mapToHuggingFace(skillImplementation.modelId);
    const adapters = this.loadedAdapters.get(modelId) || [];
    adapters.push({
      modelId,
      adapterName: skillImplementation.adapterName,
      adapterPath: skillImplementation.adapterPath
    });
    this.loadedAdapters.set(modelId, adapters);
  }

  /**
   * Load multiple adapters
   * STUBBED: gRPC server doesn't support LoRA yet
   */
  async applySkills(
    modelId: string,
    adapters: Array<{ adapterPath: string; adapterName: string }>
  ): Promise<void> {
    this.log(null, 'warn', `🧬 applySkills: LoRA not yet supported in gRPC server (${adapters.length} adapters)`);
    // Track for future use
    const tracked = this.loadedAdapters.get(modelId) || [];
    for (const adapter of adapters) {
      tracked.push({ modelId, ...adapter });
    }
    this.loadedAdapters.set(modelId, tracked);
  }

  /**
   * Remove a LoRA skill/adapter
   * STUBBED: gRPC server doesn't support LoRA yet
   */
  async removeSkill(skillId: string): Promise<void> {
    const [modelId, adapterName] = skillId.split(':');
    this.log(null, 'warn', `🧬 removeSkill: LoRA not yet supported in gRPC server (adapter: ${adapterName})`);
    // Update tracking
    const adapters = this.loadedAdapters.get(modelId) || [];
    const filtered = adapters.filter((a) => a.adapterName !== adapterName);
    this.loadedAdapters.set(modelId, filtered);
  }

  // ============================================================================
  // Helper Methods
  // ============================================================================

  /**
   * Truncate prompt if it exceeds maxInputTokens
   *
   * Uses a simple heuristic: ~4 characters per token (typical for English text).
   * If the prompt is too long, truncates from the MIDDLE to preserve:
   * - System prompt and early context (beginning)
   * - Most recent messages and the actual question (end)
   *
   * @returns Object with truncated prompt, estimated tokens, and truncation flag
   */
  private truncatePromptIfNeeded(prompt: string): {
    truncated: string;
    estimatedTokens: number;
    wasTruncated: boolean;
  } {
    // Estimate tokens: ~4 chars per token is typical for English text
    // This is a rough estimate - actual tokenization varies by model
    const charsPerToken = 4;
    const estimatedTokens = Math.ceil(prompt.length / charsPerToken);

    if (estimatedTokens <= this.maxInputTokens) {
      return { truncated: prompt, estimatedTokens, wasTruncated: false };
    }

    // Need to truncate - calculate target length
    const targetChars = this.maxInputTokens * charsPerToken;

    // Strategy: Keep first 30% (system prompt + context) and last 70% (recent messages + question)
    // This preserves the persona's instructions and the actual query
    const keepFromStart = Math.floor(targetChars * 0.3);
    const keepFromEnd = Math.floor(targetChars * 0.7);

    const beginning = prompt.slice(0, keepFromStart);
    const end = prompt.slice(-keepFromEnd);

    // Join with a clear marker that content was truncated
    const truncated = beginning + '\n\n[... earlier context truncated for inference speed ...]\n\n' + end;

    return {
      truncated,
      estimatedTokens,
      wasTruncated: true,
    };
  }

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
   * STUBBED: gRPC server preloads model at startup
   */
  async preloadModel(requestedModelId: string): Promise<void> {
    const modelId = LOCAL_MODELS.mapToHuggingFace(requestedModelId);
    this.log(null, 'info', `preloadModel: Model ${modelId} is preloaded by gRPC server`);
    this.loadedModels.add(modelId);
  }

  /**
   * Unload a model to free memory
   * STUBBED: gRPC server manages model lifecycle
   */
  async unloadModel(modelId: string): Promise<void> {
    this.log(null, 'warn', `unloadModel: Model lifecycle managed by gRPC server`);
    this.loadedModels.delete(modelId);
    this.loadedAdapters.delete(modelId);
  }
}
