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
import { ProcessPool } from '../../../system/genome/server/ProcessPool';
import { initializeSecrets, getSecret } from '../../../system/secrets/SecretManager';
import { Logger } from '../../../system/core/logging/Logger';
import { RateLimiter, AsyncQueue, Semaphore, DaemonMetrics } from '../../../generator/DaemonConcurrency';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../workers/continuum-core/bindings/RustCoreIPC';

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
    const [sentinelPath, deepseekKey, groqKey, xaiKey, openaiKey, anthropicKey, togetherKey, fireworksKey, googleKey] = await Promise.all([
      getSecret('SENTINEL_PATH'),
      getSecret('DEEPSEEK_API_KEY'),
      getSecret('GROQ_API_KEY'),
      getSecret('XAI_API_KEY'),
      getSecret('OPENAI_API_KEY'),
      getSecret('ANTHROPIC_API_KEY'),
      getSecret('TOGETHER_API_KEY'),
      getSecret('FIREWORKS_API_KEY'),
      getSecret('GOOGLE_API_KEY'),
    ]);

    // STEP 2: Register LOCAL adapter - Candle gRPC (native Rust inference)
    // NO FALLBACK: If Candle fails, we FAIL. No silent degradation.
    // Candle is the ONLY local inference path.
    const candlePromise = (async () => {
      const { CandleGrpcAdapter } = await import('../adapters/candle-grpc/shared/CandleGrpcAdapter');
      const adapter = new CandleGrpcAdapter();
      await adapter.initialize();
      await this.registerAdapter(adapter, { priority: 105, enabled: true });
      this.log.info('✅ Candle gRPC adapter registered (Candle is the only local path)');
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
      googleKey && (async () => {
        const { GoogleAdapter } = await import('../adapters/google/shared/GoogleAdapter');
        await this.registerAdapter(new GoogleAdapter(googleKey), { priority: 75, enabled: true });
        this.log.info('✅ Google Gemini adapter registered');
      })(),
    ].filter(Boolean) as Promise<void>[];

    // Wait for ALL adapters to register (Candle + Sentinel + cloud in parallel)
    // Candle is the only local inference path
    await Promise.allSettled([candlePromise, sentinelPromise, ...cloudAdapters]);

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

    // Discover model metadata from provider APIs — OFF the main thread.
    // ALL HTTP I/O runs in the Rust process (continuum-core) via IPC.
    // Node.js main thread only does Map.set() registration with results.
    this.discoverModelsViaRust();

    const deferredMs = Date.now() - deferredStart;
    this.log.info(`✅ AIProviderDaemonServer: DEFERRED init complete (${deferredMs}ms) - health monitoring active`);
  }

  /**
   * Discover model metadata via Rust IPC (continuum-core process).
   *
   * ALL HTTP I/O runs in the Rust process — completely off the Node.js main thread.
   * Node.js only sends provider configs and receives discovered models via IPC.
   */
  private discoverModelsViaRust(): void {
    // Build provider configs from registered adapters
    const providers: Array<{
      provider_id: string;
      api_key: string;
      base_url: string;
      static_models?: Array<{
        id: string;
        context_window: number;
        max_output_tokens?: number;
        capabilities?: string[];
        cost_per_1k_tokens?: { input: number; output: number };
      }>;
    }> = [];

    for (const [providerId, registration] of this.adapters) {
      const adapter = registration.adapter;

      // OpenAI-compatible adapters have config with apiKey and baseUrl
      const config = (adapter as any).config;
      if (config?.apiKey && config?.baseUrl) {
        const staticModels = config.models?.map((m: any) => ({
          id: m.id,
          context_window: m.contextWindow,
          max_output_tokens: m.maxOutputTokens,
          capabilities: m.capabilities,
          cost_per_1k_tokens: m.costPer1kTokens,
        }));

        providers.push({
          provider_id: providerId,
          api_key: config.apiKey,
          base_url: config.baseUrl,
          static_models: staticModels || undefined,
        });
        continue;
      }

      // Anthropic adapter has apiKey directly (not OpenAI-compatible)
      const apiKey = (adapter as any).apiKey;
      if (apiKey && providerId === 'anthropic') {
        providers.push({
          provider_id: providerId,
          api_key: apiKey,
          base_url: 'https://api.anthropic.com',
          static_models: [
            { id: 'claude-sonnet-4-5-20250929', context_window: 200000, max_output_tokens: 8192 },
            { id: 'claude-opus-4-20250514', context_window: 200000, max_output_tokens: 4096 },
            { id: 'claude-3-5-haiku-20241022', context_window: 200000, max_output_tokens: 4096 },
          ],
        });
      }

      // Google adapter has apiKey in googleConfig
      const googleConfig = (adapter as any).googleConfig;
      if (googleConfig?.apiKey && providerId === 'google') {
        providers.push({
          provider_id: providerId,
          api_key: googleConfig.apiKey,
          base_url: 'https://generativelanguage.googleapis.com',
        });
      }
    }

    if (providers.length === 0) {
      this.log.info('No provider configs for model discovery');
      return;
    }

    this.log.info(`Sending ${providers.length} provider configs to Rust for model discovery...`);

    // Fire-and-forget IPC call to Rust — all HTTP runs in the Rust process
    const client = new RustCoreIPCClient(getContinuumCoreSocketPath());
    client.connect()
      .then(() => client.modelsDiscover(providers))
      .then(async (result) => {
        const { ModelRegistry } = await import('../../../system/shared/ModelRegistry');
        const registry = ModelRegistry.sharedInstance();
        for (const model of result.models) {
          registry.register(model);
        }
        this.log.info(`ModelRegistry: ${result.count} models discovered from ${result.providers} providers (Rust IPC)`);
        client.disconnect();
      })
      .catch((err) => {
        this.log.warn(`Model discovery via Rust failed: ${err.message}`);
        client.disconnect();
      });
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
