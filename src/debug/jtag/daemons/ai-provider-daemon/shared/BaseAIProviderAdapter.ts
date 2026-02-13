import type {
  AIProviderAdapter,
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
  ModelCapability,
  ModelInfo
} from './AIProviderTypesV2';
import { Logger, FileMode, type ComponentLogger } from '../../../system/core/logging/Logger';
import { LoggingConfig } from '../../../system/core/logging/LoggingConfig';
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

  // Circuit breaker state - DISABLED for now
  // The shared circuit breaker caused memory-synthesis failures to block chat responses.
  // TODO: Implement per-operation-type circuit breakers if needed
  private consecutiveFailures: number = 0;
  private readonly maxConsecutiveFailures: number = 999999; // Effectively disabled
  private circuitBreakerOpen: boolean = false;
  private circuitBreakerResetTime: number = 0;
  private readonly circuitBreakerCooldown: number = 5000; // Reduced from 30s

  // Base layer timeout - adapters get this for FREE (can override in subclass)
  protected baseTimeout: number = 30000; // 30s default, Candle overrides to 60s

  // Logger cache for persona-specific adapters logs
  private personaLoggers: Map<string, ComponentLogger> = new Map();

  /**
   * Helper to log with persona context
   * Writes to persona-specific log directory if personaContext provided
   * Otherwise writes to shared adapter log file
   *
   * Uses Logger.ts for all logging (respects CLEAN mode for fresh logs per session)
   * Respects LoggingConfig for per-persona log filtering
   */
  protected log(request: TextGenerationRequest | null, level: 'info' | 'debug' | 'warn' | 'error', message: string, ...args: unknown[]): void {
    if (request?.personaContext) {
      // Extract uniqueId from logDir path (e.g., ".continuum/personas/helper/..." -> "helper")
      const logDir = request.personaContext.logDir;
      const uniqueIdMatch = logDir.match(/personas\/([^/]+)/);
      const uniqueId = uniqueIdMatch ? uniqueIdMatch[1] : request.personaContext.displayName;

      // Check if logging is enabled for this persona + adapters category
      if (!LoggingConfig.isEnabled(uniqueId, 'adapters')) {
        return; // Early exit - logging disabled for this persona
      }

      // Get or create logger for this persona's adapters.log (works like daemon logs)
      // Convert path to category: strip everything up to .continuum/
      const category = logDir.replace(/^.*\.continuum\//, '') + '/logs/adapters';

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
      // No persona context - check if system-level adapter logging is enabled
      if (!LoggingConfig.isSystemEnabled('adapters')) {
        return; // Early exit - system adapter logging disabled
      }
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
   * For Candle: restart gRPC server
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

      // Enhance error messages with troubleshooting context
      const enhancedError = this.enhanceApiError(error);
      const errorMessage = enhancedError instanceof Error ? enhancedError.message : String(enhancedError);
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

      throw enhancedError;
    }
  }

  /**
   * Enhance API errors with troubleshooting context
   * Makes cryptic provider errors more understandable
   */
  protected enhanceApiError(error: unknown): Error {
    if (!(error instanceof Error)) {
      return new Error(String(error));
    }

    const msg = error.message.toLowerCase();

    // Invalid prompt errors
    if (msg.includes('invalid') && msg.includes('prompt')) {
      const enhanced = new Error(
        `Invalid prompt: ${error.message}\n\n` +
        `Common causes:\n` +
        `• Empty messages array\n` +
        `• Missing system/user message\n` +
        `• Message content is empty or null\n` +
        `• Special characters not escaped\n\n` +
        `Fix: Ensure messages array has at least one user message with non-empty content`
      );
      enhanced.name = error.name;
      return enhanced;
    }

    // Rate limit errors
    if (msg.includes('rate') && (msg.includes('limit') || msg.includes('exceeded'))) {
      const enhanced = new Error(
        `Rate limited: ${error.message}\n\n` +
        `Too many requests to this provider. Try:\n` +
        `• Wait a few seconds and retry\n` +
        `• Use a different AI provider\n` +
        `• Reduce request frequency`
      );
      enhanced.name = error.name;
      return enhanced;
    }

    // Authentication errors
    if (msg.includes('auth') || msg.includes('api key') || msg.includes('unauthorized') || msg.includes('401')) {
      const enhanced = new Error(
        `Authentication failed: ${error.message}\n\n` +
        `Check:\n` +
        `• Is the API key correct?\n` +
        `• Is the API key expired?\n` +
        `• Does the key have required permissions?`
      );
      enhanced.name = error.name;
      return enhanced;
    }

    // Model not found
    if (msg.includes('model') && (msg.includes('not found') || msg.includes('does not exist'))) {
      const enhanced = new Error(
        `Model not found: ${error.message}\n\n` +
        `The requested model is unavailable. Check:\n` +
        `• Is the model name spelled correctly?\n` +
        `• Is this model available for your API tier?\n` +
        `• Try a different model`
      );
      enhanced.name = error.name;
      return enhanced;
    }

    // Context length exceeded
    if (msg.includes('context') || msg.includes('token') && msg.includes('exceed')) {
      const enhanced = new Error(
        `Context length exceeded: ${error.message}\n\n` +
        `The input is too long. Try:\n` +
        `• Shorter prompt\n` +
        `• Summarize conversation history\n` +
        `• Use a model with larger context window`
      );
      enhanced.name = error.name;
      return enhanced;
    }

    // Return original if no enhancement applies
    return error;
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
