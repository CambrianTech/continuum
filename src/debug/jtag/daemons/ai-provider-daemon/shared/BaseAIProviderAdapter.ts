import type {
  AIProviderAdapter,
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
  ModelCapability,
  ModelInfo
} from './AIProviderTypesV2';
import { Logger, FileMode, type ComponentLogger } from '../../../system/core/logging/Logger';
import { Events } from '../../../system/core/shared/Events';
import * as path from 'path';

/**
 * Abstract base class for all AI provider adapters
 *
 * Provides automatic health monitoring, recovery, and CONCURRENCY ISOLATION for ALL AI providers.
 * All providers can freeze, timeout, or fail - this class handles that universally.
 *
 * CONCURRENCY MODEL:
 * - Each adapter request runs with setImmediate() yielding to prevent event loop starvation
 * - Hard timeout enforced at base layer (adapter authors don't need to think about it)
 * - Circuit breaker marks adapter unhealthy after consecutive failures
 *
 * Subclasses must implement:
 * - generateTextImpl(): Provider-specific text generation (NO timeout handling needed!)
 * - healthCheck(): Provider-specific health verification
 * - getAvailableModels(): Provider-specific model listing
 * - restartProvider(): Provider-specific restart logic
 */
export abstract class BaseAIProviderAdapter implements AIProviderAdapter {
  abstract readonly providerId: string;
  abstract readonly providerName: string;
  abstract readonly supportedCapabilities: ModelCapability[];

  // Health monitoring state (managed by AdapterHealthMonitor, not local setInterval)
  private isRestarting: boolean = false;
  private isPermanentlyDisabled: boolean = false;
  private permanentDisableReason: string = '';

  // Circuit breaker state - mark adapter unhealthy after consecutive failures
  private consecutiveFailures: number = 0;
  private readonly maxConsecutiveFailures: number = 3;
  private circuitBreakerOpen: boolean = false;
  private circuitBreakerResetTime: number = 0;
  private readonly circuitBreakerCooldown: number = 30000; // 30s cooldown before retry

  // Base layer timeout - adapters get this for FREE
  protected readonly baseTimeout: number = 30000; // 30s hard timeout for any adapter

  // Logger cache for persona-specific adapters logs
  private personaLoggers: Map<string, ComponentLogger> = new Map();

  /**
   * Helper to log with persona context
   * Writes to persona-specific log directory if personaContext provided
   * Otherwise writes to shared adapter log file
   *
   * Uses Logger.ts for all logging (respects CLEAN mode for fresh logs per session)
   */
  protected log(request: TextGenerationRequest | null, level: 'info' | 'debug' | 'warn' | 'error', message: string, ...args: any[]): void {
    if (request?.personaContext) {
      // Get or create logger for this persona's adapters.log (works like daemon logs)
      const logDir = request.personaContext.logDir;
      // Convert path to category: logDir is '.continuum/personas/{uniqueId}'
      const category = logDir.replace(/^\.continuum\//, '') + '/logs/adapters';

      if (!this.personaLoggers.has(category)) {
        const componentName = `${this.providerName}:${request.personaContext.displayName}`;
        this.personaLoggers.set(
          category,
          Logger.create(componentName, category)
        );
      }

      // Use Logger.ts (handles queuing, async writes)
      const logger = this.personaLoggers.get(category)!;
      logger[level](message, ...args);
    } else {
      // No persona context - write to shared adapter log
      const systemLogger = Logger.create('AIProviderAdapter', 'adapters');
      systemLogger[level](message, ...args);
    }
  }

  // Abstract methods subclasses MUST implement
  // NOTE: Subclasses implement generateTextImpl(), NOT generateText()!
  // The base class generateText() wraps it with timeout and circuit breaker
  protected abstract generateTextImpl(request: TextGenerationRequest): Promise<TextGenerationResponse>;
  abstract healthCheck(): Promise<HealthStatus>;
  abstract getAvailableModels(): Promise<ModelInfo[]>;

  /**
   * Provider-specific restart logic
   *
   * For Ollama: killall ollama && ollama serve
   * For OpenAI: reconnect to API with backoff
   * For Anthropic: reconnect to API with backoff
   * etc.
   */
  protected abstract restartProvider(): Promise<void>;

  /**
   * PUBLIC generateText() - called by AIProviderDaemon
   *
   * Wraps subclass generateTextImpl() with:
   * 1. Circuit breaker - fail fast if adapter is known to be broken
   * 2. Timeout enforcement - hard 30s timeout at base layer
   * 3. Failure tracking - marks adapter unhealthy after consecutive failures
   *
   * Adapter authors just implement generateTextImpl() - NO timeout handling needed!
   * Concurrency isolation is FREE at this layer.
   */
  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    // Check circuit breaker FIRST - fail fast if adapter is known broken
    if (this.circuitBreakerOpen) {
      const now = Date.now();
      if (now < this.circuitBreakerResetTime) {
        const remainingMs = this.circuitBreakerResetTime - now;
        throw new Error(`${this.providerName} circuit breaker OPEN - ${this.consecutiveFailures} consecutive failures. Retry in ${Math.ceil(remainingMs / 1000)}s`);
      }
      // Cooldown expired - try again (half-open state)
      this.log(request, 'info', `⚡ ${this.providerName}: Circuit breaker half-open, attempting recovery...`);
    }

    const startTime = Date.now();

    try {
      // Wrap the subclass implementation with timeout
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => {
          reject(new Error(`${this.providerName} request timed out after ${this.baseTimeout}ms`));
        }, this.baseTimeout);
      });

      // Race between actual implementation and timeout
      const result = await Promise.race([
        this.generateTextImpl(request),
        timeoutPromise
      ]);

      // SUCCESS - reset circuit breaker
      if (this.consecutiveFailures > 0) {
        this.log(request, 'info', `✅ ${this.providerName}: Recovered! Resetting circuit breaker.`);
      }
      this.consecutiveFailures = 0;
      this.circuitBreakerOpen = false;

      return result;

    } catch (error) {
      const elapsed = Date.now() - startTime;
      this.consecutiveFailures++;

      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(request, 'error', `❌ ${this.providerName}: Failed (${this.consecutiveFailures}/${this.maxConsecutiveFailures}) after ${elapsed}ms: ${errorMessage}`);

      // Check if we should open circuit breaker
      if (this.consecutiveFailures >= this.maxConsecutiveFailures && !this.circuitBreakerOpen) {
        this.circuitBreakerOpen = true;
        this.circuitBreakerResetTime = Date.now() + this.circuitBreakerCooldown;
        this.log(request, 'warn', `🔥 ${this.providerName}: CIRCUIT BREAKER OPENED - ${this.consecutiveFailures} failures. Cooldown: ${this.circuitBreakerCooldown / 1000}s`);

        // Emit unhealthy event for AdapterHealthMonitor
        await Events.emit('system:adapter:unhealthy', {
          providerId: this.providerId,
          consecutiveFailures: this.consecutiveFailures,
          lastStatus: {
            status: 'unhealthy' as const,
            apiAvailable: false,
            responseTime: elapsed,
            errorRate: 1.0,
            lastChecked: Date.now(),
            message: `Circuit breaker opened: ${errorMessage}`,
          },
        });
      }

      throw error;
    }
  }

  /**
   * Check if adapter is currently healthy (circuit breaker closed)
   */
  isHealthy(): boolean {
    if (this.circuitBreakerOpen) {
      const now = Date.now();
      if (now >= this.circuitBreakerResetTime) {
        // Cooldown expired - allow retry
        return true;
      }
      return false;
    }
    return true;
  }

  // Lifecycle methods
  async initialize(): Promise<void> {
    this.log(null, 'info', `🔧 ${this.providerName}: Initializing...`);
    await this.initializeProvider();
    // Health monitoring handled by AdapterHealthMonitor (no local setInterval)
  }

  async shutdown(): Promise<void> {
    this.log(null, 'info', `🛑 ${this.providerName}: Shutting down...`);
    await this.shutdownProvider();
  }

  // Optional lifecycle hooks for subclasses
  protected async initializeProvider(): Promise<void> {
    // Default: no-op (subclasses can override)
  }

  protected async shutdownProvider(): Promise<void> {
    // Default: no-op (subclasses can override)
  }

  /**
   * Handle restart request from AdapterHealthMonitor
   * Called when adapter is unhealthy and needs restart
   */
  async handleRestartRequest(): Promise<void> {
    if (this.isRestarting) {
      this.log(null, 'warn', `⏭️  ${this.providerName}: Restart already in progress`);
      return;
    }

    this.isRestarting = true;

    try {
      this.log(null, 'warn', `🔄 ${this.providerName}: Attempting restart...`);

      await this.restartProvider();

      // Emit event that restart is complete (AdapterHealthMonitor will verify health)
      await Events.emit('system:adapter:restarted', {
        providerId: this.providerId,
        timestamp: Date.now(),
      });

      this.log(null, 'info', `✅ ${this.providerName}: Restart completed`);
    } catch (error) {
      this.log(null, 'error', `❌ ${this.providerName}: Restart failed:`, error);
    } finally {
      this.isRestarting = false;
    }
  }
}
