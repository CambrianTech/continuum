import type {
  AIProviderAdapter,
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
  ModelCapability,
  ModelInfo
} from './AIProviderTypesV2';
import { Logger } from '../../../system/core/logging/Logger';
import * as fs from 'fs';
import * as path from 'path';
import { inspect } from 'util';

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

  /**
   * Helper to log with persona context
   * Writes to persona-specific log directory if personaContext provided
   * Otherwise writes to shared adapter log file
   */
  protected log(request: TextGenerationRequest | null, level: 'info' | 'debug' | 'warn' | 'error', message: string, ...args: any[]): void {
    if (request?.personaContext) {
      // Write to persona-specific log directory
      const logDir = request.personaContext.logDir;
      const logFile = path.join(logDir, 'logs', 'adapters.log');

      // Ensure log directory exists
      const logsDir = path.join(logDir, 'logs');
      if (!fs.existsSync(logsDir)) {
        fs.mkdirSync(logsDir, { recursive: true });
      }

      // Format message with timestamp and any additional args
      const timestamp = new Date().toISOString();
      const formattedArgs = args.length > 0
        ? ' ' + args.map(a =>
            typeof a === 'object' ? inspect(a, { depth: 2, colors: false, compact: true }) : String(a)
          ).join(' ')
        : '';
      const formattedMessage = `[${timestamp}] [${level.toUpperCase()}] ${message}${formattedArgs}\n`;

      // Append to file
      fs.appendFileSync(logFile, formattedMessage);
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
