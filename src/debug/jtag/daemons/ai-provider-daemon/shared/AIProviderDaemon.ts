/**
 * AI Provider Daemon - Universal AI Integration Layer
 * ====================================================
 *
 * Central daemon for all AI provider integrations (Ollama, OpenAI, Anthropic, etc.)
 * Provides unified interface for PersonaUsers and other AI-powered features.
 *
 * Architecture:
 * - Adapter registry for pluggable AI providers
 * - Automatic provider selection based on preferences
 * - Health monitoring and failover
 * - Request routing and error handling
 *
 * Usage:
 * const response = await router.postMessage({
 *   endpoint: '/ai-provider',
 *   payload: {
 *     type: 'generate-text',
 *     request: { messages: [...], preferredProvider: 'ollama' }
 *   }
 * });
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext, JTAGMessage, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { TimingHarness } from '../../../system/core/shared/TimingHarness';

import type {
  AIProviderAdapter,
  TextGenerationRequest,
  TextGenerationResponse,
  EmbeddingRequest,
  EmbeddingResponse,
  HealthStatus,
  ProviderRegistration,
  RoutingInfo,
} from './AIProviderTypesV2';
import { AIProviderError, chatMessagesToPrompt } from './AIProviderTypesV2';

/**
 * Internal type for adapter selection result - carries routing metadata
 */
interface AdapterSelection {
  adapter: AIProviderAdapter;
  routingReason: RoutingInfo['routingReason'];
  isLocal: boolean;
}
import { AIGenerationEntity } from '../../../system/data/entities/AIGenerationEntity';
import { Commands } from '../../../system/core/shared/Commands';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';

// AI Provider Payloads
export interface AIProviderPayload extends JTAGPayload {
  readonly type: 'generate-text' | 'health-check' | 'list-providers';
  readonly request?: TextGenerationRequest;
}

// AI Provider Responses
export interface AIProviderSuccessResponse extends BaseResponsePayload {
  data: TextGenerationResponse | HealthStatus[] | string[];
}

export interface AIProviderErrorResponse extends BaseResponsePayload {
  error: string;
  errorDetails?: Record<string, unknown>;
}

export type AIProviderResponse = AIProviderSuccessResponse | AIProviderErrorResponse;

export class AIProviderDaemon extends DaemonBase {
  public readonly subpath = '/ai-provider';

  protected adapters: Map<string, ProviderRegistration> = new Map();
  private initialized = false;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('AIProviderDaemon', context, router);
  }

  /**
   * Get ProcessPool instance (server-side only, overridden in server subclass)
   * Returns undefined in shared/browser contexts
   */
  protected getProcessPoolInstance(): unknown | undefined {
    return undefined;
  }

  protected async initialize(): Promise<void> {
    this.log.info('🤖 AIProviderDaemon: Base initialization (adapters registered by subclass)');
    this.initialized = true;
  }

  /**
   * Process incoming messages
   */
  protected async processMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as AIProviderPayload;

    try {
      switch (payload.type) {
        case 'generate-text':
          return await this.handleGenerateText(payload);

        case 'health-check':
          return await this.handleHealthCheck(payload);

        case 'list-providers':
          return await this.handleListProviders(payload);

        default:
          return createPayload(payload.context, payload.sessionId, {
            success: false,
            timestamp: new Date().toISOString(),
            error: `Unknown AI provider operation: ${(payload as any).type}`,
          });
      }
    } catch (error) {
      return createPayload(payload.context, payload.sessionId, {
        success: false,
        timestamp: new Date().toISOString(),
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Generate text using AI provider
   * Routes through ProcessPool if available (server-side), otherwise uses adapter directly (browser/fallback)
   *
   * OBSERVABILITY: Response always includes `routing` field showing:
   * - Which provider was used and why
   * - Whether local or cloud inference
   * - Which LoRA adapters were applied (if any)
   */
  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const timer = TimingHarness.start('ai/generate-text', 'ai');
    timer.setMeta('preferredProvider', request.preferredProvider || 'auto');
    timer.setMeta('model', request.model || 'default');
    timer.setMeta('userId', request.userId || 'unknown');

    if (!this.initialized) {
      timer.setError('NOT_INITIALIZED');
      timer.finish();
      throw new AIProviderError(
        'AIProviderDaemon is not initialized',
        'daemon',
        'NOT_INITIALIZED'
      );
    }

    // Select provider (considers both preferredProvider AND model name)
    const selection = this.selectAdapter(request.preferredProvider, request.model);
    timer.mark('select_adapter');

    if (!selection) {
      timer.setError('NO_PROVIDER_AVAILABLE');
      timer.finish();
      throw new AIProviderError(
        'No suitable AI provider available',
        'daemon',
        'NO_PROVIDER_AVAILABLE',
        { preferredProvider: request.preferredProvider }
      );
    }

    const { adapter, routingReason, isLocal } = selection;
    timer.setMeta('provider', adapter.providerId);
    timer.setMeta('isLocal', isLocal);

    // Build base routing info (will be enhanced by adapter response)
    const baseRouting: RoutingInfo = {
      provider: adapter.providerId,
      isLocal,
      routingReason,
      adaptersApplied: [],  // Will be populated by CandleAdapter
      modelRequested: request.model,
    };

    // Check if ProcessPool is available (server-side only)
    const processPool = this.getProcessPoolInstance() as any;
    if (processPool && typeof processPool.executeInference === 'function') {
      this.log.info(`🏊 AIProviderDaemon: Routing ${adapter.providerId} inference through ProcessPool`);
      timer.setMeta('route', 'ProcessPool');

      try {
        // Convert chat messages to prompt
        const { prompt } = chatMessagesToPrompt(request.messages);
        timer.mark('build_prompt');

        // Route through ProcessPool
        const output = await processPool.executeInference({
          prompt,
          provider: adapter.providerId,
          model: request.model || 'phi3:mini',
          temperature: request.temperature,
          maxTokens: request.maxTokens,
          config: {}, // Adapter will use defaults
        });
        timer.mark('inference');

        const record = timer.finish();

        // Return formatted response with routing info
        return {
          text: output,
          finishReason: 'stop',
          model: request.model || 'phi3:mini',
          provider: adapter.providerId,
          usage: {
            inputTokens: 0, // Worker should track this
            outputTokens: 0,
            totalTokens: 0,
            estimatedCost: 0,
          },
          responseTime: record.totalMs,
          requestId: request.requestId || `req-${Date.now()}`,
          routing: baseRouting,
        };
      } catch (error) {
        this.log.error(`❌ AIProviderDaemon: ProcessPool inference failed, falling back to direct adapter call`);
        timer.mark('processpool_failed');
        // Fall through to direct adapter call
      }
    }

    // Direct adapter call (browser-side or fallback)
    this.log.info(`🤖 AIProviderDaemon: Using direct ${adapter.providerId} adapter call (no ProcessPool)`);
    timer.setMeta('route', 'DirectAdapter');

    if (!adapter.generateText) {
      timer.setError('UNSUPPORTED_OPERATION');
      timer.finish();
      throw new AIProviderError(
        `Adapter ${adapter.providerId} does not support text generation`,
        'adapter',
        'UNSUPPORTED_OPERATION'
      );
    }

    try {
      const response = await adapter.generateText(request);
      timer.mark('inference');

      // Merge adapter's routing info with our base routing
      // Adapter may have additional info (e.g., CandleAdapter has adaptersApplied, modelMapped)
      // Safe spread handles adapters that don't return routing yet
      const adapterRouting = response.routing || {};
      const finalResponse: TextGenerationResponse = {
        ...response,
        routing: {
          ...baseRouting,
          ...adapterRouting,  // Adapter can override/add fields (e.g., adaptersApplied, modelMapped)
          // But we always preserve our routing reason (daemon knows best why it selected this adapter)
          routingReason,
          isLocal,
        },
      };

      // Log successful generation to database for cost tracking
      // This is the SINGLE source of truth - only daemon logs, not individual adapters
      await this.logGeneration(finalResponse, request);
      timer.mark('log_generation');

      // Log routing info for observability (routing is guaranteed to exist since we just built it)
      const r = finalResponse.routing!;
      this.log.info(`✅ AIProviderDaemon: Generation complete. Routing: provider=${r.provider}, isLocal=${r.isLocal}, reason=${r.routingReason}, adapters=[${r.adaptersApplied.join(',')}]`);

      timer.setMeta('outputTokens', response.usage?.outputTokens || 0);
      timer.finish();
      return finalResponse;
    } catch (error) {
      this.log.error(`❌ AIProviderDaemon: Text generation failed with ${adapter.providerId}`);
      timer.setError(error instanceof Error ? error.message : String(error));

      // Log failed generation to database
      await this.logFailedGeneration(
        request.requestId || `req-${Date.now()}`,
        request.model || 'unknown',
        error,
        request,
        adapter.providerId
      );

      timer.finish();
      // TODO: Implement failover to alternative providers
      throw error;
    }
  }

  /**
   * Log successful AI generation to database for cost tracking
   * SINGLE source of truth - called only by AIProviderDaemon, not adapters
   */
  private async logGeneration(response: TextGenerationResponse, request: TextGenerationRequest): Promise<void> {
    try {
      const result = AIGenerationEntity.create({
        timestamp: Date.now(),
        requestId: response.requestId,
        provider: response.provider,
        model: response.model,
        inputTokens: response.usage.inputTokens,
        outputTokens: response.usage.outputTokens,
        totalTokens: response.usage.totalTokens,
        estimatedCost: response.usage.estimatedCost || 0,
        responseTime: response.responseTime,
        userId: request.userId,
        roomId: request.roomId,
        purpose: request.purpose || 'chat',
        finishReason: response.finishReason,
        success: true
      });

      if (!result.success || !result.entity) {
        this.log.error(`❌ AIProviderDaemon: Failed to create AIGenerationEntity: ${result.error}`);
        return;
      }

      // Persist to database using data/create command
      await Commands.execute<DataCreateParams, DataCreateResult<AIGenerationEntity>>(
        DATA_COMMANDS.CREATE,
        {
          collection: 'ai_generations',
          backend: 'server',
          data: result.entity
        }
      );

      this.log.info(`💾 AIProviderDaemon: Logged generation (${response.provider}/${response.model}, ${response.usage.totalTokens} tokens, $${(response.usage.estimatedCost || 0).toFixed(4)})`);
    } catch (error) {
      // Don't fail generation if logging fails - just warn
      this.log.error(`❌ AIProviderDaemon: Failed to log generation:`, error);
    }
  }

  /**
   * Log failed AI generation to database
   */
  private async logFailedGeneration(
    requestId: string,
    model: string,
    error: unknown,
    request: TextGenerationRequest,
    providerId: string
  ): Promise<void> {
    try {
      const result = AIGenerationEntity.create({
        timestamp: Date.now(),
        requestId,
        provider: providerId,
        model,
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        estimatedCost: 0,
        responseTime: 0,
        userId: request.userId,
        roomId: request.roomId,
        purpose: request.purpose || 'chat',
        finishReason: 'error' as const,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });

      if (!result.success || !result.entity) {
        this.log.error(`❌ AIProviderDaemon: Failed to create AIGenerationEntity for error: ${result.error}`);
        return;
      }

      // Persist to database
      await Commands.execute<DataCreateParams, DataCreateResult<AIGenerationEntity>>(
        DATA_COMMANDS.CREATE,
        {
          collection: 'ai_generations',
          backend: 'server',
          data: result.entity
        }
      );

      this.log.info(`💾 AIProviderDaemon: Logged failed generation (${providerId}/${model})`);
    } catch (logError) {
      this.log.error(`❌ AIProviderDaemon: Failed to log error:`, logError);
    }
  }

  /**
   * Create embeddings using AI provider
   */
  async createEmbedding(request: EmbeddingRequest): Promise<EmbeddingResponse> {
    if (!this.initialized) {
      throw new AIProviderError(
        'AIProviderDaemon is not initialized',
        'daemon',
        'NOT_INITIALIZED'
      );
    }

    // Validate input (Bug #4 fix - catch undefined/empty text values early)
    if (!request.input) {
      throw new AIProviderError(
        'Embedding request missing input text',
        'daemon',
        'INVALID_REQUEST'
      );
    }

    // Normalize to array for validation
    const inputArray = Array.isArray(request.input) ? request.input : [request.input];
    if (inputArray.length === 0) {
      throw new AIProviderError(
        'Embedding request has empty input array',
        'daemon',
        'INVALID_REQUEST'
      );
    }

    const invalidInputs = inputArray.filter((text: string) => !text || text.trim().length === 0);
    if (invalidInputs.length > 0) {
      throw new AIProviderError(
        `Embedding request contains ${invalidInputs.length} empty or undefined text values`,
        'daemon',
        'INVALID_REQUEST'
      );
    }

    // Select provider
    const selection = this.selectAdapter(request.preferredProvider);
    if (!selection) {
      throw new AIProviderError(
        'No suitable AI provider available',
        'daemon',
        'NO_PROVIDER_AVAILABLE',
        { preferredProvider: request.preferredProvider }
      );
    }

    const { adapter } = selection;

    // Check if adapter supports embeddings
    if (!adapter.createEmbedding) {
      throw new AIProviderError(
        `Adapter ${adapter.providerId} does not support embeddings`,
        'adapter',
        'UNSUPPORTED_OPERATION'
      );
    }

    try {
      const response = await adapter.createEmbedding(request);
      return response;
    } catch (error) {
      this.log.error(`❌ AIProviderDaemon: Embedding generation failed with ${adapter.providerId}`);
      throw error;
    }
  }

  /**
   * Check health of all providers
   */
  async checkProviderHealth(): Promise<Map<string, HealthStatus>> {
    const healthMap = new Map<string, HealthStatus>();

    for (const [providerId, registration] of this.adapters) {
      if (registration.enabled) {
        try {
          const health = await registration.adapter.healthCheck();
          healthMap.set(providerId, health);
        } catch (error) {
          healthMap.set(providerId, {
            status: 'unhealthy',
            apiAvailable: false,
            responseTime: 0,
            errorRate: 1.0,
            lastChecked: Date.now(),
            message: `Health check failed: ${error instanceof Error ? error.message : String(error)}`,
          });
        }
      }
    }

    return healthMap;
  }

  /**
   * Get list of available providers
   */
  getAvailableProviders(): string[] {
    return Array.from(this.adapters.entries())
      .filter(([_, reg]) => reg.enabled)
      .sort((a, b) => b[1].priority - a[1].priority) // Sort by priority
      .map(([providerId]) => providerId);
  }

  /**
   * Get adapter by provider ID
   * Used by diagnostic commands to test adapters directly
   */
  getAdapter(providerId: string): AIProviderAdapter | null {
    const registration = this.adapters.get(providerId);
    return registration ? registration.adapter : null;
  }

  /**
   * Get all registered adapters (including disabled ones)
   */
  getAllAdapters(): Map<string, AIProviderAdapter> {
    const result = new Map<string, AIProviderAdapter>();
    for (const [providerId, registration] of this.adapters) {
      result.set(providerId, registration.adapter);
    }
    return result;
  }

  /**
   * Register a new AI provider adapter
   * Protected so server subclass can register adapters
   */
  protected async registerAdapter(
    adapter: AIProviderAdapter,
    options: { priority: number; enabled: boolean }
  ): Promise<void> {
    this.log.info(`🔌 AIProviderDaemon: Registering ${adapter.providerName} (priority ${options.priority})...`);

    try {
      // Initialize adapter
      await adapter.initialize();

      // Register adapter
      this.adapters.set(adapter.providerId, {
        providerId: adapter.providerId,
        adapter: adapter,
        configuration: {
          timeout: 60000, // 60s - increased from 30s to handle large prompts with llama3.2:3b
          retryAttempts: 3,
          retryDelay: 1000,
          defaultModel: '',
          defaultTemperature: 0.7,
          logRequests: true,
        },
        priority: options.priority,
        enabled: options.enabled,
      });

      this.log.info(`✅ AIProviderDaemon: ${adapter.providerName} registered successfully`);
    } catch (error) {
      this.log.error(`❌ AIProviderDaemon: Failed to register ${adapter.providerName}`);
      this.log.error(`   Error: ${error instanceof Error ? error.message : String(error)}`);

      // Register anyway but mark as disabled
      this.adapters.set(adapter.providerId, {
        providerId: adapter.providerId,
        adapter: adapter,
        configuration: {
          timeout: 60000, // 60s - increased from 30s to handle large prompts with llama3.2:3b
          retryAttempts: 3,
          retryDelay: 1000,
          defaultModel: '',
          defaultTemperature: 0.7,
          logRequests: true,
        },
        priority: options.priority,
        enabled: false, // Disabled due to initialization failure
      });
    }
  }

  /**
   * Select best adapter based on preferences and availability
   *
   * Implements LOCAL MODEL ROUTING for the genome vision:
   * - When preferredProvider is 'local' etc., route to Candle (native Rust)
   * - Candle enables LoRA adapter composition for the genome vision
   *
   * OLLAMA IS REMOVED: Candle is the ONLY local inference path.
   * Legacy 'ollama' provider requests are aliased to Candle for backward compat.
   *
   * IMPORTANT: Candle is ONLY used for local inference.
   * Cloud providers use their own adapters. This prevents queue bottlenecks.
   *
   * ROUTING PRIORITY (in order):
   * 1. Explicit preferredProvider (if specified and available)
   * 2. Local provider aliasing (legacy 'ollama'/local → candle)
   * 3. Default by priority (highest priority enabled adapter)
   *
   * @returns AdapterSelection with routing metadata for observability
   */
  private selectAdapter(preferredProvider?: string, model?: string): AdapterSelection | null {
    // 1. EXPLICIT PROVIDER: Honor preferredProvider first (most specific)
    // This MUST be checked BEFORE model detection to avoid routing Groq's
    // 'llama-3.1-8b-instant' to Candle just because it starts with 'llama'
    if (preferredProvider) {
      // LOCAL PROVIDER ALIASING: Route local providers to Candle
      // Candle is the ONLY local inference path - 'ollama' kept for backward compat only
      const localProviders = ['local', 'llamacpp', 'ollama']; // 'ollama' DEPRECATED - aliased to candle
      if (localProviders.includes(preferredProvider)) {
        const candleReg = this.adapters.get('candle');
        if (candleReg && candleReg.enabled) {
          this.log.info(`🔄 AIProviderDaemon: Routing '${preferredProvider}' → 'candle' (provider_aliasing)`);
          return {
            adapter: candleReg.adapter,
            routingReason: 'provider_aliasing',
            isLocal: true,
          };
        }
        // NO FALLBACK: If candle not available, FAIL - don't silently use something else
        throw new AIProviderError(
          `Local provider '${preferredProvider}' requested but Candle adapter not available`,
          'daemon',
          'CANDLE_NOT_AVAILABLE'
        );
      }

      // Try to use the explicit provider
      const registration = this.adapters.get(preferredProvider);
      if (registration && registration.enabled) {
        const isLocal = ['candle', 'local', 'llamacpp'].includes(preferredProvider);
        this.log.info(`🎯 AIProviderDaemon: Using explicit provider '${preferredProvider}' (explicit_provider)`);
        return {
          adapter: registration.adapter,
          routingReason: 'explicit_provider',
          isLocal,
        };
      }

      // preferredProvider specified but not available - FAIL, don't silently use something else
      throw new AIProviderError(
        `Preferred provider '${preferredProvider}' not available`,
        'daemon',
        'PROVIDER_NOT_AVAILABLE',
        { preferredProvider }
      );
    }

    // 2. LOCAL MODEL DETECTION: Route local models to Candle when NO preferredProvider
    // This catches cases like SignalDetector using 'llama3.2:1b' without specifying provider
    // ONLY runs when preferredProvider is NOT specified to avoid misrouting API models
    if (!preferredProvider && model && this.isLocalModel(model)) {
      const candleReg = this.adapters.get('candle');
      if (candleReg && candleReg.enabled) {
        this.log.info(`🔄 AIProviderDaemon: Routing model '${model}' → 'candle' (model_detection, no preferredProvider)`);
        return {
          adapter: candleReg.adapter,
          routingReason: 'model_detection',
          isLocal: true,
        };
      }
    }

    // 3. DEFAULT: Select highest priority enabled adapter EXCLUDING Candle
    // Candle is only for local inference - don't use as catch-all default
    // This prevents queue bottlenecks when many personas share one local model
    const registrations = Array.from(this.adapters.values())
      .filter(reg => reg.enabled && reg.providerId !== 'candle')
      .sort((a, b) => b.priority - a.priority);

    if (registrations.length > 0) {
      const selected = registrations[0];
      this.log.info(`📊 AIProviderDaemon: Using default provider '${selected.providerId}' (default_priority, priority=${selected.priority})`);
      return {
        adapter: selected.adapter,
        routingReason: 'default_priority',
        isLocal: false,  // Cloud providers are default
      };
    }

    return null;
  }

  /**
   * Check if a model name indicates a local model that should use Candle
   * Examples: llama3.2:1b, qwen2:1.5b, phi3:mini, mistral:7b
   */
  private isLocalModel(model: string): boolean {
    const localModelPrefixes = [
      'llama',      // Meta's LLaMA models
      'qwen',       // Alibaba's Qwen models
      'phi',        // Microsoft's Phi models
      'mistral',    // Mistral AI models
      'codellama',  // Code-focused LLaMA
      'gemma',      // Google's Gemma models
      'tinyllama',  // TinyLlama
      'orca',       // Orca models
      'vicuna',     // Vicuna models
      'wizardlm',   // WizardLM
      'neural-chat',// Intel Neural Chat
      'stablelm',   // Stability AI LM
      'yi',         // 01.AI Yi models
      'deepseek-coder', // DeepSeek local coder (not the API)
    ];

    const modelLower = model.toLowerCase();
    return localModelPrefixes.some(prefix => modelLower.startsWith(prefix));
  }

  /**
   * Handle generate-text message
   */
  private async handleGenerateText(payload: AIProviderPayload): Promise<BaseResponsePayload> {
    if (!payload.request) {
      return createPayload(payload.context, payload.sessionId, {
        success: false,
        timestamp: new Date().toISOString(),
        error: 'Missing request in payload',
      });
    }

    try {
      const response = await this.generateText(payload.request);
      return createPayload(payload.context, payload.sessionId, {
        success: true,
        timestamp: new Date().toISOString(),
        data: response,
      });
    } catch (error) {
      return createPayload(payload.context, payload.sessionId, {
        success: false,
        timestamp: new Date().toISOString(),
        error: error instanceof Error ? error.message : String(error),
        errorDetails: error instanceof AIProviderError ? error.details : undefined,
      });
    }
  }

  /**
   * Handle health-check message
   */
  private async handleHealthCheck(payload: AIProviderPayload): Promise<BaseResponsePayload> {
    try {
      const healthMap = await this.checkProviderHealth();
      const healthArray = Array.from(healthMap.entries()).map(([providerId, health]) => ({
        providerId,
        ...health,
      }));

      return createPayload(payload.context, payload.sessionId, {
        success: true,
        timestamp: new Date().toISOString(),
        data: {
          providers: healthArray,
          totalProviders: this.adapters.size,
          healthyProviders: healthArray.filter(h => h.status === 'healthy').length,
        },
      });
    } catch (error) {
      return createPayload(payload.context, payload.sessionId, {
        success: false,
        timestamp: new Date().toISOString(),
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Handle list-providers message
   */
  private async handleListProviders(payload: AIProviderPayload): Promise<BaseResponsePayload> {
    const providers = this.getAvailableProviders();
    return createPayload(payload.context, payload.sessionId, {
      success: true,
      timestamp: new Date().toISOString(),
      data: {
        providers,
        count: providers.length,
      },
    });
  }

  /**
   * Shutdown daemon and all adapters
   */
  async shutdown(): Promise<void> {
    this.log.info('🔄 AIProviderDaemon: Shutting down...');

    for (const [providerId, registration] of this.adapters) {
      try {
        this.log.info(`   Shutting down ${providerId}...`);
        await registration.adapter.shutdown();
      } catch (error) {
        this.log.error(`   Failed to shutdown ${providerId}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }

    this.adapters.clear();
    this.initialized = false;

    await super.shutdown();
    this.log.info('✅ AIProviderDaemon: Shutdown complete');
  }

  // =============================================
  // CLEAN DOMAIN-OWNED STATIC INTERFACE
  // =============================================

  private static sharedInstance: AIProviderDaemon | undefined;

  /**
   * Initialize static AIProviderDaemon context (called by system)
   */
  static initialize(instance: AIProviderDaemon): void {
    AIProviderDaemon.sharedInstance = instance;
  }

  /**
   * Check if AIProviderDaemon has been initialized
   * Useful for components that need to wait for daemon readiness
   */
  static isInitialized(): boolean {
    return AIProviderDaemon.sharedInstance !== undefined;
  }

  /**
   * Generate text with automatic instance injection - CLEAN INTERFACE
   *
   * @example
   * const response = await AIProviderDaemon.generateText({
   *   messages: [{ role: 'user', content: 'Hello!' }],
   *   model: 'llama3.2:1b',
   *   preferredProvider: 'ollama'
   * });
   */
  static async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    if (!AIProviderDaemon.sharedInstance) {
      throw new Error('AIProviderDaemon not initialized - system must call AIProviderDaemon.initialize() first');
    }

    return await AIProviderDaemon.sharedInstance.generateText(request);
  }

  /**
   * Create embeddings with automatic instance injection - CLEAN INTERFACE
   *
   * @example
   * const response = await AIProviderDaemon.createEmbedding({
   *   input: 'Hello, world!',
   *   model: 'nomic-embed-text',
   *   preferredProvider: 'ollama'
   * });
   */
  static async createEmbedding(request: EmbeddingRequest): Promise<EmbeddingResponse> {
    if (!AIProviderDaemon.sharedInstance) {
      throw new Error('AIProviderDaemon not initialized - system must call AIProviderDaemon.initialize() first');
    }

    return await AIProviderDaemon.sharedInstance.createEmbedding(request);
  }

  /**
   * Check provider health with automatic instance injection - CLEAN INTERFACE
   *
   * @example
   * const healthMap = await AIProviderDaemon.checkHealth();
   */
  static async checkHealth(): Promise<Map<string, HealthStatus>> {
    if (!AIProviderDaemon.sharedInstance) {
      throw new Error('AIProviderDaemon not initialized - system must call AIProviderDaemon.initialize() first');
    }

    return await AIProviderDaemon.sharedInstance.checkProviderHealth();
  }

  /**
   * Get available providers with automatic instance injection - CLEAN INTERFACE
   *
   * @example
   * const providers = AIProviderDaemon.getProviders();
   */
  static getProviders(): string[] {
    if (!AIProviderDaemon.sharedInstance) {
      throw new Error('AIProviderDaemon not initialized - system must call AIProviderDaemon.initialize() first');
    }

    return AIProviderDaemon.sharedInstance.getAvailableProviders();
  }

  /**
   * Get genome ProcessPool with automatic instance injection - CLEAN INTERFACE
   *
   * @example
   * const pool = AIProviderDaemon.getProcessPool();
   * const stats = pool.getStats();
   */
  static getProcessPool(): unknown {
    if (!AIProviderDaemon.sharedInstance) {
      throw new Error('AIProviderDaemon not initialized - system must call AIProviderDaemon.initialize() first');
    }

    const pool = AIProviderDaemon.sharedInstance.getProcessPoolInstance();

    if (!pool) {
      throw new Error('ProcessPool not initialized - only available on server side');
    }

    return pool;
  }

  /**
   * Get adapter by provider ID with automatic instance injection - CLEAN INTERFACE
   *
   * @example
   * const adapter = AIProviderDaemon.getAdapter('ollama');
   * if (adapter) {
   *   const models = await adapter.getAvailableModels();
   * }
   */
  static getAdapter(providerId: string): AIProviderAdapter | null {
    if (!AIProviderDaemon.sharedInstance) {
      throw new Error('AIProviderDaemon not initialized - system must call AIProviderDaemon.initialize() first');
    }

    return AIProviderDaemon.sharedInstance.getAdapter(providerId);
  }

  /**
   * Get all registered adapters with automatic instance injection - CLEAN INTERFACE
   *
   * @example
   * const adapters = AIProviderDaemon.getAllAdapters();
   * for (const [providerId, adapter] of adapters) {
   *   this.log.info(`Provider: ${providerId}`);
   * }
   */
  static getAllAdapters(): Map<string, AIProviderAdapter> {
    if (!AIProviderDaemon.sharedInstance) {
      throw new Error('AIProviderDaemon not initialized - system must call AIProviderDaemon.initialize() first');
    }

    return AIProviderDaemon.sharedInstance.getAllAdapters();
  }
}
