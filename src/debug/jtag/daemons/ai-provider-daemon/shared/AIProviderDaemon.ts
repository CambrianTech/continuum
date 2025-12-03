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
import type { JTAGContext, JTAGMessage, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';

import type {
  AIProviderAdapter,
  TextGenerationRequest,
  TextGenerationResponse,
  EmbeddingRequest,
  EmbeddingResponse,
  HealthStatus,
  ProviderRegistration,
} from './AIProviderTypesV2';
import { AIProviderError, chatMessagesToPrompt } from './AIProviderTypesV2';
import { AIGenerationEntity } from '../../../system/data/entities/AIGenerationEntity';
import { Commands } from '../../../system/core/shared/Commands';
import { DATA_COMMANDS } from '../../../commands/data/shared/DataCommandConstants';
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

  private adapters: Map<string, ProviderRegistration> = new Map();
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
   * Handle incoming messages
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
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
   */
  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {

    if (!this.initialized) {
      throw new AIProviderError(
        'AIProviderDaemon is not initialized',
        'daemon',
        'NOT_INITIALIZED'
      );
    }

    // Select provider
    const adapter = this.selectAdapter(request.preferredProvider);
    if (!adapter) {
      throw new AIProviderError(
        'No suitable AI provider available',
        'daemon',
        'NO_PROVIDER_AVAILABLE',
        { preferredProvider: request.preferredProvider }
      );
    }

    // Check if ProcessPool is available (server-side only)
    const processPool = this.getProcessPoolInstance() as any;
    if (processPool && typeof processPool.executeInference === 'function') {
      this.log.info(`🏊 AIProviderDaemon: Routing ${adapter.providerId} inference through ProcessPool`);

      try {
        // Convert chat messages to prompt
        const { prompt } = chatMessagesToPrompt(request.messages);

        // Route through ProcessPool
        const startTime = Date.now();
        const output = await processPool.executeInference({
          prompt,
          provider: adapter.providerId,
          model: request.model || 'phi3:mini',
          temperature: request.temperature,
          maxTokens: request.maxTokens,
          config: {}, // Adapter will use defaults
        });

        const responseTime = Date.now() - startTime;

        // Return formatted response
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
          responseTime,
          requestId: request.requestId || `req-${Date.now()}`,
        };
      } catch (error) {
        this.log.error(`❌ AIProviderDaemon: ProcessPool inference failed, falling back to direct adapter call`);
        // Fall through to direct adapter call
      }
    }

    // Direct adapter call (browser-side or fallback)
    this.log.info(`🤖 AIProviderDaemon: Using direct ${adapter.providerId} adapter call (no ProcessPool)`);

    if (!adapter.generateText) {
      throw new AIProviderError(
        `Adapter ${adapter.providerId} does not support text generation`,
        'adapter',
        'UNSUPPORTED_OPERATION'
      );
    }

    try {
      const response = await adapter.generateText(request);

      // Log successful generation to database for cost tracking
      // This is the SINGLE source of truth - only daemon logs, not individual adapters
      await this.logGeneration(response, request);

      return response;
    } catch (error) {
      this.log.error(`❌ AIProviderDaemon: Text generation failed with ${adapter.providerId}`);

      // Log failed generation to database
      await this.logFailedGeneration(
        request.requestId || `req-${Date.now()}`,
        request.model || 'unknown',
        error,
        request,
        adapter.providerId
      );

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
      await Commands.execute<DataCreateParams<AIGenerationEntity>, DataCreateResult<AIGenerationEntity>>(
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
      await Commands.execute<DataCreateParams<AIGenerationEntity>, DataCreateResult<AIGenerationEntity>>(
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
    const adapter = this.selectAdapter(request.preferredProvider);
    if (!adapter) {
      throw new AIProviderError(
        'No suitable AI provider available',
        'daemon',
        'NO_PROVIDER_AVAILABLE',
        { preferredProvider: request.preferredProvider }
      );
    }

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
   */
  private selectAdapter(preferredProvider?: string): AIProviderAdapter | null {
    // If preferred provider specified, try to use it
    if (preferredProvider) {
      const registration = this.adapters.get(preferredProvider);
      if (registration && registration.enabled) {
        return registration.adapter;
      }
    }

    // Otherwise, select highest priority enabled adapter
    const registrations = Array.from(this.adapters.values())
      .filter(reg => reg.enabled)
      .sort((a, b) => b.priority - a.priority);

    return registrations.length > 0 ? registrations[0].adapter : null;
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
