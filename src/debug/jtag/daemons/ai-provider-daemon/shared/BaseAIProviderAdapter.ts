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

  // Health monitoring state (managed by AdapterHealthMonitor, not local setInterval)
  private isRestarting: boolean = false;
  private isPermanentlyDisabled: boolean = false;
  private permanentDisableReason: string = '';

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
