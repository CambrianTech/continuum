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
  HealthStatus,
  ProviderRegistration,
} from './AIProviderTypes';
import { AIProviderError, chatMessagesToPrompt } from './AIProviderTypes';
import { OllamaAdapter } from './OllamaAdapter';

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
    console.log('🤖 AIProviderDaemon: Initializing...');

    // Register Ollama adapter (local, free, private)
    await this.registerAdapter(new OllamaAdapter(), {
      priority: 100, // Highest priority - free and local
      enabled: true,
    });

    // TODO: Register OpenAI adapter (if API key available)
    // TODO: Register Anthropic adapter (if API key available)

    this.initialized = true;
    console.log('✅ AIProviderDaemon: Initialized successfully');
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
      console.log(`🏊 AIProviderDaemon: Routing ${adapter.providerId} inference through ProcessPool`);

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
        console.error(`❌ AIProviderDaemon: ProcessPool inference failed, falling back to direct adapter call`);
        // Fall through to direct adapter call
      }
    }

    // Direct adapter call (browser-side or fallback)
    console.log(`🤖 AIProviderDaemon: Using direct ${adapter.providerId} adapter call (no ProcessPool)`);
    try {
      const response = await adapter.generateText(request);
      return response;
    } catch (error) {
      console.error(`❌ AIProviderDaemon: Text generation failed with ${adapter.providerId}`);

      // TODO: Implement failover to alternative providers
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
   * Register a new AI provider adapter
   */
  private async registerAdapter(
    adapter: AIProviderAdapter,
    options: { priority: number; enabled: boolean }
  ): Promise<void> {
    console.log(`🔌 AIProviderDaemon: Registering ${adapter.providerName} (priority ${options.priority})...`);

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

      console.log(`✅ AIProviderDaemon: ${adapter.providerName} registered successfully`);
    } catch (error) {
      console.error(`❌ AIProviderDaemon: Failed to register ${adapter.providerName}`);
      console.error(`   Error: ${error instanceof Error ? error.message : String(error)}`);

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
    console.log('🔄 AIProviderDaemon: Shutting down...');

    for (const [providerId, registration] of this.adapters) {
      try {
        console.log(`   Shutting down ${providerId}...`);
        await registration.adapter.shutdown();
      } catch (error) {
        console.error(`   Failed to shutdown ${providerId}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }

    this.adapters.clear();
    this.initialized = false;

    await super.shutdown();
    console.log('✅ AIProviderDaemon: Shutdown complete');
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
}
