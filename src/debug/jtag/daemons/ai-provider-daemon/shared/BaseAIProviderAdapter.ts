import type {
  AIProviderAdapter,
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
  ModelCapability,
  ModelInfo
} from './AIProviderTypesV2';
import { Logger, FileMode, type ComponentLogger } from '../../../system/core/logging/Logger';
import * as path from 'path';

/**
 * Abstract base class for all AI provider adapters
 *
 * Provides automatic health monitoring and recovery for ALL AI providers.
 * All providers can freeze, timeout, or fail - this class handles that universally.
 *
 * Subclasses must implement:
 * - generateText(): Provider-specific text generation
 * - healthCheck(): Provider-specific health verification
 * - getAvailableModels(): Provider-specific model listing
 * - restartProvider(): Provider-specific restart logic
 */
export abstract class BaseAIProviderAdapter implements AIProviderAdapter {
  abstract readonly providerId: string;
  abstract readonly providerName: string;
  abstract readonly supportedCapabilities: ModelCapability[];

  // Health monitoring state
  private healthMonitorInterval?: ReturnType<typeof setInterval>;
  private consecutiveFailures: number = 0;
  private readonly MAX_CONSECUTIVE_FAILURES = 3;
  private readonly HEALTH_CHECK_INTERVAL = 30000; // 30 seconds
  private isRestarting: boolean = false;

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
      // Get or create logger for this persona's adapters.log
      const logDir = request.personaContext.logDir;
      const logFile = path.join(logDir, 'logs', 'adapters.log');

      if (!this.personaLoggers.has(logFile)) {
        const componentName = `${this.providerName}:${request.personaContext.displayName}`;
        this.personaLoggers.set(
          logFile,
          Logger.createWithFile(componentName, logFile, FileMode.CLEAN)
        );
      }

      // Use Logger.ts (handles queuing, async writes, and CLEAN mode)
      const logger = this.personaLoggers.get(logFile)!;
      logger[level](message, ...args);
    } else {
      // No persona context - write to shared adapter log
      const systemLogger = Logger.create('AIProviderAdapter', 'adapters');
      systemLogger[level](message, ...args);
    }
  }

  // Abstract methods subclasses MUST implement
  abstract generateText(request: TextGenerationRequest): Promise<TextGenerationResponse>;
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

  // Lifecycle methods
  async initialize(): Promise<void> {
    this.log(null, 'info', `🔧 ${this.providerName}: Initializing with health monitoring...`);
    await this.initializeProvider();
    this.startHealthMonitoring();
  }

  async shutdown(): Promise<void> {
    this.log(null, 'info', `🛑 ${this.providerName}: Shutting down...`);
    this.stopHealthMonitoring();
    await this.shutdownProvider();
  }

  // Optional lifecycle hooks for subclasses
  protected async initializeProvider(): Promise<void> {
    // Default: no-op (subclasses can override)
  }

  protected async shutdownProvider(): Promise<void> {
    // Default: no-op (subclasses can override)
  }

  // Health monitoring logic
  private startHealthMonitoring(): void {
    this.log(null, 'info', `💚 ${this.providerName}: Starting health monitoring (every ${this.HEALTH_CHECK_INTERVAL}ms)`);

    this.healthMonitorInterval = setInterval(async () => {
      await this.checkHealthAndRecover();
    }, this.HEALTH_CHECK_INTERVAL);
  }

  private stopHealthMonitoring(): void {
    if (this.healthMonitorInterval) {
      clearInterval(this.healthMonitorInterval);
      this.healthMonitorInterval = undefined;
    }
  }

  private async checkHealthAndRecover(): Promise<void> {
    try {
      const health = await this.healthCheck();

      if (health.status === 'healthy') {
        if (this.consecutiveFailures > 0) {
          this.log(null, 'info', `✅ ${this.providerName}: Recovered after ${this.consecutiveFailures} failures`);
        }
        this.consecutiveFailures = 0;
        return;
      }

      // Health check failed
      this.consecutiveFailures++;
      this.log(null, 'warn', `⚠️  ${this.providerName}: Health check failed (${this.consecutiveFailures}/${this.MAX_CONSECUTIVE_FAILURES})`);

      if (this.consecutiveFailures >= this.MAX_CONSECUTIVE_FAILURES && !this.isRestarting) {
        await this.attemptRestart();
      }
    } catch (error) {
      this.consecutiveFailures++;
      this.log(null, 'error', `❌ ${this.providerName}: Health check error (${this.consecutiveFailures}/${this.MAX_CONSECUTIVE_FAILURES}):`, error);

      if (this.consecutiveFailures >= this.MAX_CONSECUTIVE_FAILURES && !this.isRestarting) {
        await this.attemptRestart();
      }
    }
  }

  private async attemptRestart(): Promise<void> {
    this.isRestarting = true;

    try {
      this.log(null, 'warn', `🔄 ${this.providerName}: Attempting restart after ${this.consecutiveFailures} consecutive failures...`);

      await this.restartProvider();

      // Wait for provider to stabilize
      await new Promise(resolve => setTimeout(resolve, 3000));

      // Verify health after restart
      const health = await this.healthCheck();

      if (health.status === 'healthy') {
        this.log(null, 'info', `✅ ${this.providerName}: Restart successful`);
        this.consecutiveFailures = 0;
      } else {
        this.log(null, 'warn', `⚠️  ${this.providerName}: Restart completed but health check still failing`);
      }
    } catch (error) {
      this.log(null, 'error', `❌ ${this.providerName}: Restart failed:`, error);
    } finally {
      this.isRestarting = false;
    }
  }
}
