/**
 * AI Provider Daemon Server - Server-specific AI Integration
 * ===========================================================
 *
 * Server implementation of AIProviderDaemon with full access to:
 * - HTTP requests (fetch API)
 * - File system (for config/cache)
 * - Environment variables (for API keys)
 * - ProcessPool for genome inference workers
 *
 * All AI provider logic is in the shared AIProviderDaemon base class.
 * This server version provides daemon registration and ProcessPool initialization.
 */

import { AIProviderDaemon } from '../shared/AIProviderDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { AIProviderAdapter } from '../shared/AIProviderTypesV2';
import { ProcessPool } from '../../../system/genome/server/ProcessPool';
import { initializeSecrets, getSecret } from '../../../system/secrets/SecretManager';
import { Logger } from '../../../system/core/logging/Logger';
import { RateLimiter, AsyncQueue, Semaphore, DaemonMetrics } from '../../../generator/DaemonConcurrency';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import * as path from 'path';

export class AIProviderDaemonServer extends AIProviderDaemon {
  private processPool?: ProcessPool;

  // ServerDaemonBase features: Concurrency primitives for metrics + performance
  private rateLimiter: RateLimiter;
  private requestQueue: AsyncQueue<BaseResponsePayload>;
  private semaphore: Semaphore;
  private metrics: DaemonMetrics;
  private healthState: {
    isHealthy: boolean;
    consecutiveFailures: number;
    lastSuccessTime: number;
    lastHeartbeat: number;
  };

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // DEBUG: Verify constructor runs
    console.log('🚀🚀🚀 AIProviderDaemonServer CONSTRUCTOR CALLED 🚀🚀🚀');

    // Set up file-based logging using class name automatically
    // Logs go to .continuum/.../logs/daemons/{ClassName}.log
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);

    // Opt-in to aggressive concurrency control for external API calls
    // Rate limit: 50 requests/sec, max 20 concurrent (handles multiple AI personas + external APIs)
    this.rateLimiter = new RateLimiter(50, 50);
    this.requestQueue = new AsyncQueue<BaseResponsePayload>();
    this.semaphore = new Semaphore(20);
    this.metrics = new DaemonMetrics();

    // Initialize health state
    this.healthState = {
      isHealthy: true,
      consecutiveFailures: 0,
      lastSuccessTime: Date.now(),
      lastHeartbeat: Date.now()
    };
  }

  /**
   * Override to return typed ProcessPool instance
   */
  protected getProcessPoolInstance(): ProcessPool | undefined {
    return this.processPool;
  }

  /**
   * PHASE 1: Core initialization (BLOCKING)
   * Registers adapters - the minimum needed to process AI requests.
   * Health monitoring is deferred to initializeDeferred().
   */
  protected async initialize(): Promise<void> {
    console.log('🚀 AIProviderDaemonServer: CORE init starting...');
    const coreStart = Date.now();

    // Initialize SecretManager FIRST (adapters depend on it)
    this.log.info('🔐 AIProviderDaemonServer: Initializing SecretManager...');
    await initializeSecrets();
    this.log.info('✅ AIProviderDaemonServer: SecretManager initialized');

    // Register adapters CONCURRENTLY for faster startup
    // Each adapter registration is independent - no need to wait for others
    this.log.info('🤖 AIProviderDaemonServer: Registering AI provider adapters (parallel)...');

    // STEP 1: Load all secrets in parallel (fast)
    const [sentinelPath, deepseekKey, groqKey, xaiKey, openaiKey, anthropicKey, togetherKey, fireworksKey] = await Promise.all([
      getSecret('SENTINEL_PATH'),
      getSecret('DEEPSEEK_API_KEY'),
      getSecret('GROQ_API_KEY'),
      getSecret('XAI_API_KEY'),
      getSecret('OPENAI_API_KEY'),
      getSecret('ANTHROPIC_API_KEY'),
      getSecret('TOGETHER_API_KEY'),
      getSecret('FIREWORKS_API_KEY'),
    ]);

    // STEP 2: Register LOCAL adapters first (Candle, Ollama, Sentinel) - these are critical
    // Candle gRPC adapter (native Rust inference)
    const candlePromise = (async () => {
      try {
        const { CandleGrpcAdapter } = await import('../adapters/candle-grpc/shared/CandleGrpcAdapter');
        const adapter = new CandleGrpcAdapter();
        await adapter.initialize();
        await this.registerAdapter(adapter, { priority: 105, enabled: true });
        this.log.info('✅ Candle gRPC adapter registered');
      } catch (error) {
        this.log.warn(`Candle gRPC not available: ${error instanceof Error ? error.message : 'unknown'}`);
      }
    })();

    // Ollama adapter (local, free, private)
    const ollamaPromise = (async () => {
      const { OllamaAdapter } = await import('../adapters/ollama/shared/OllamaAdapter');
      await this.registerAdapter(new OllamaAdapter(), { priority: 100, enabled: true });
      this.log.info('✅ Ollama adapter registered');
    })();

    // Sentinel adapter (if configured)
    const sentinelPromise = sentinelPath ? (async () => {
      const { SentinelAdapter } = await import('../adapters/sentinel/shared/SentinelAdapter');
      await this.registerAdapter(new SentinelAdapter(), { priority: 95, enabled: true });
      this.log.info('✅ Sentinel adapter registered');
    })() : Promise.resolve();

    // STEP 3: Register CLOUD adapters in parallel (independent of each other)
    const cloudAdapters = [
      deepseekKey && (async () => {
        const { DeepSeekAdapter } = await import('../adapters/deepseek/shared/DeepSeekAdapter');
        await this.registerAdapter(new DeepSeekAdapter(deepseekKey), { priority: 90, enabled: true });
        this.log.info('✅ DeepSeek adapter registered');
      })(),
      groqKey && (async () => {
        const { GroqAdapter } = await import('../adapters/groq/shared/GroqAdapter');
        await this.registerAdapter(new GroqAdapter(groqKey), { priority: 85, enabled: true });
        this.log.info('✅ Groq adapter registered');
      })(),
      xaiKey && (async () => {
        const { XAIAdapter } = await import('../adapters/xai/shared/XAIAdapter');
        await this.registerAdapter(new XAIAdapter(xaiKey), { priority: 83, enabled: true });
        this.log.info('✅ X.AI (Grok) adapter registered');
      })(),
      openaiKey && (async () => {
        const { OpenAIAdapter } = await import('../adapters/openai/shared/OpenAIAdapter');
        await this.registerAdapter(new OpenAIAdapter(openaiKey), { priority: 80, enabled: true });
        this.log.info('✅ OpenAI adapter registered');
      })(),
      anthropicKey && (async () => {
        const { AnthropicAdapter } = await import('../adapters/anthropic/shared/AnthropicAdapter');
        await this.registerAdapter(new AnthropicAdapter(anthropicKey), { priority: 80, enabled: true });
        this.log.info('✅ Anthropic adapter registered');
      })(),
      togetherKey && (async () => {
        const { TogetherAIAdapter } = await import('../adapters/together/shared/TogetherAIAdapter');
        await this.registerAdapter(new TogetherAIAdapter(togetherKey), { priority: 70, enabled: true });
        this.log.info('✅ Together.ai adapter registered');
      })(),
      fireworksKey && (async () => {
        const { FireworksAdapter } = await import('../adapters/fireworks/shared/FireworksAdapter');
        await this.registerAdapter(new FireworksAdapter(fireworksKey), { priority: 70, enabled: true });
        this.log.info('✅ Fireworks adapter registered');
      })(),
    ].filter(Boolean) as Promise<void>[];

    // Wait for ALL adapters to register (local + cloud in parallel)
    await Promise.allSettled([candlePromise, ollamaPromise, sentinelPromise, ...cloudAdapters]);

    // Call base initialization
    await super['initialize']();

    // Initialize static AIProviderDaemon interface (critical for PersonaUsers)
    AIProviderDaemon.initialize(this);

    const coreMs = Date.now() - coreStart;
    this.log.info(`✅ AIProviderDaemonServer: CORE init complete (${coreMs}ms) - READY to process requests`);
    this.log.info(`   Health monitoring will start in background via initializeDeferred()`);
  }

  /**
   * PHASE 2: Deferred initialization (NON-BLOCKING)
   * Starts health monitoring - runs AFTER daemon is READY and accepting messages.
   */
  protected async initializeDeferred(): Promise<void> {
    this.log.info('🔄 AIProviderDaemonServer: DEFERRED init starting (health monitoring)...');
    const deferredStart = Date.now();

    // Enable health monitoring with timing metrics (for performance optimization)
    // Heartbeat every 30 seconds checks for stuck operations
    this.registerInterval('health-monitoring', () => {
      this.healthState.lastHeartbeat = Date.now();

      // Check if daemon is stuck (no successful operations in 60s)
      const timeSinceSuccess = Date.now() - this.healthState.lastSuccessTime;
      if (timeSinceSuccess > 60000) {
        this.log.warn(`⚠️  AIProviderDaemon: Appears stuck (${Math.round(timeSinceSuccess / 1000)}s since last success)`);
        this.healthState.isHealthy = false;
      }
    }, 30000);

    // Initialize adapter health monitoring
    const { AdapterHealthMonitor } = await import('./AdapterHealthMonitor');
    const { SystemHealthTicker } = await import('../../system-daemon/server/SystemHealthTicker');

    // Register all adapters with health monitor
    const healthMonitor = AdapterHealthMonitor.getInstance();
    for (const [providerId, registration] of this.adapters) {
      healthMonitor.registerAdapter(registration.adapter);
      this.log.debug(`💚 Registered ${providerId} with health monitor`);
    }

    // Initialize health monitor (subscribes to system:health-check:tick events)
    await healthMonitor.initialize();

    // Start health ticker (emits system:health-check:tick events)
    const healthTicker = SystemHealthTicker.getInstance();
    await healthTicker.start();

    const deferredMs = Date.now() - deferredStart;
    this.log.info(`✅ AIProviderDaemonServer: DEFERRED init complete (${deferredMs}ms) - health monitoring active`);
  }

  /**
   * Server-specific shutdown
   * Shuts down health monitoring, ProcessPool, then delegates to base class
   */
  async shutdown(): Promise<void> {
    this.log.info('🔄 AIProviderDaemonServer: Shutting down health monitoring...');

    // Stop health ticker
    const { SystemHealthTicker } = await import('../../system-daemon/server/SystemHealthTicker');
    const healthTicker = SystemHealthTicker.getInstance();
    await healthTicker.stop();

    // Shutdown health monitor
    const { AdapterHealthMonitor } = await import('./AdapterHealthMonitor');
    const healthMonitor = AdapterHealthMonitor.getInstance();
    await healthMonitor.shutdown();

    this.log.info('✅ AIProviderDaemonServer: Health monitoring shutdown complete');

    this.log.info('🔄 AIProviderDaemonServer: Shutting down ProcessPool...');

    if (this.processPool) {
      await this.processPool.shutdown();
      this.log.info('✅ AIProviderDaemonServer: ProcessPool shutdown complete');
    }

    await super.shutdown();
  }
}
