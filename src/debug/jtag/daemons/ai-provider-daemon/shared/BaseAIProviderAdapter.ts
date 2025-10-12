import type {
  AIProviderAdapter,
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus
} from './AIProviderTypes';

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

  // Health monitoring state
  private healthMonitorInterval?: ReturnType<typeof setInterval>;
  private consecutiveFailures: number = 0;
  private readonly MAX_CONSECUTIVE_FAILURES = 3;
  private readonly HEALTH_CHECK_INTERVAL = 30000; // 30 seconds
  private isRestarting: boolean = false;

  // Abstract methods subclasses MUST implement
  abstract generateText(request: TextGenerationRequest): Promise<TextGenerationResponse>;
  abstract healthCheck(): Promise<HealthStatus>;
  abstract getAvailableModels(): Promise<any>; // string[] or ModelInfo[] depending on adapter

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
    console.log(`🔧 ${this.providerName}: Initializing with health monitoring...`);
    await this.initializeProvider();
    this.startHealthMonitoring();
  }

  async shutdown(): Promise<void> {
    console.log(`🛑 ${this.providerName}: Shutting down...`);
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
    console.log(`💚 ${this.providerName}: Starting health monitoring (every ${this.HEALTH_CHECK_INTERVAL}ms)`);

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
          console.log(`✅ ${this.providerName}: Recovered after ${this.consecutiveFailures} failures`);
        }
        this.consecutiveFailures = 0;
        return;
      }

      // Health check failed
      this.consecutiveFailures++;
      console.log(`⚠️  ${this.providerName}: Health check failed (${this.consecutiveFailures}/${this.MAX_CONSECUTIVE_FAILURES})`);

      if (this.consecutiveFailures >= this.MAX_CONSECUTIVE_FAILURES && !this.isRestarting) {
        await this.attemptRestart();
      }
    } catch (error) {
      this.consecutiveFailures++;
      console.log(`❌ ${this.providerName}: Health check error (${this.consecutiveFailures}/${this.MAX_CONSECUTIVE_FAILURES}):`, error);

      if (this.consecutiveFailures >= this.MAX_CONSECUTIVE_FAILURES && !this.isRestarting) {
        await this.attemptRestart();
      }
    }
  }

  private async attemptRestart(): Promise<void> {
    this.isRestarting = true;

    try {
      console.log(`🔄 ${this.providerName}: Attempting restart after ${this.consecutiveFailures} consecutive failures...`);

      await this.restartProvider();

      // Wait for provider to stabilize
      await new Promise(resolve => setTimeout(resolve, 3000));

      // Verify health after restart
      const health = await this.healthCheck();

      if (health.status === 'healthy') {
        console.log(`✅ ${this.providerName}: Restart successful`);
        this.consecutiveFailures = 0;
      } else {
        console.log(`⚠️  ${this.providerName}: Restart completed but health check still failing`);
      }
    } catch (error) {
      console.log(`❌ ${this.providerName}: Restart failed:`, error);
    } finally {
      this.isRestarting = false;
    }
  }
}
