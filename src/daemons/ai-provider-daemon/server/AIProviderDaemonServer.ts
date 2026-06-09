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
import type { TextGenerationResponse, TextGenerationRequest } from '../shared/AIProviderTypesV2';
import { AIGenerationEntity } from '../../../system/data/entities/AIGenerationEntity';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { ProcessPool } from '../../../system/genome/server/ProcessPool';
import { initializeSecrets, getSecret } from '../../../system/secrets/SecretManager';
import { Logger } from '../../../system/core/logging/Logger';
import { RateLimiter, AsyncQueue, Semaphore, DaemonMetrics } from '../../../generator/DaemonConcurrency';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../core/continuum-core/bindings/RustCoreIPC';
import type { CollectionName } from '../../../shared/generated-collection-constants';
import { MetricsCollector } from '../../../system/metrics/server/MetricsCollector';

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
   * Server override: persist AI generation records to the metrics SQLite database.
   * Uses the same database as system metrics (GPU/CPU/memory) — designed for
   * high-frequency fire-and-forget telemetry writes.
   *
   * Previous approach (Postgres via 'default' handle) silently dropped writes
   * despite reporting success — the Postgres adapter returns success but doesn't
   * actually persist records. SQLite via MetricsCollector's handle works reliably.
   */
  protected override async logGeneration(response: TextGenerationResponse, request: TextGenerationRequest): Promise<void> {
    try {
      const usage = response.usage;
      if (!usage) return;

      // Get the metrics database handle (opened by MetricsCollector on startup)
      const metricsHandle = MetricsCollector.instance.handle;
      if (!metricsHandle) {
        // MetricsCollector not started yet — skip silently
        return;
      }

      const entity = await AIGenerationEntity.createFromResponse(response, {
        userId: request.userId,
        roomId: request.roomId,
        purpose: request.purpose || 'chat',
      });

      await ORM.store(
        AIGenerationEntity.collection as CollectionName,
        entity,
        true,  // suppressEvents — cost-tracking writes don't need broadcast
        metricsHandle
      );

      this.log.info(`💾 Logged generation (${response.provider}/${response.model}, ${usage.totalTokens} tokens, $${(usage.estimatedCost || 0).toFixed(4)})`);
    } catch (error) {
      this.log.error(`❌ logGeneration failed: ${error instanceof Error ? error.message : error}`);
    }
  }

  /**
   * PHASE 1: Core initialization (BLOCKING)
   * Registers adapters - the minimum needed to process AI requests.
   * Health monitoring is deferred to initializeDeferred().
   */
  protected async initialize(): Promise<void> {
    const coreStart = Date.now();

    // Initialize SecretManager FIRST (adapters depend on it)
    this.log.info('🔐 AIProviderDaemonServer: Initializing SecretManager...');
    await initializeSecrets();
    this.log.info('✅ AIProviderDaemonServer: SecretManager initialized');

    // Register adapters CONCURRENTLY for faster startup
    // Each adapter registration is independent - no need to wait for others
    this.log.info('🤖 AIProviderDaemonServer: Registering AI provider adapters...');

    // Per task #219 + #229 (headless Rust doctrine, no-fallbacks): the
    // 9 cloud-inference TS adapter classes (DeepSeek/Groq/XAI/OpenAI/
    // Anthropic/Together/Fireworks/Google/Mistral) have been deleted.
    // Cloud inference is owned by continuum-core's Rust catalog
    // (model_registry/catalog.rs); the substrate dispatches via
    // ai_provider.rs which the inference routing campaign (#112/#113/
    // #114) is finishing to route every cognition call through.
    // This daemon now ONLY registers the Sentinel adapter (separate
    // provider, awaiting its own migration card).
    //
    // Background: see docs/architecture/SUBSTRATE-DOCTRINE-ORGANIC-FLOW.md
    // forbidden-moves clause 7 ("no new TS daemon owning runtime behavior")
    // and docs/planning/CANARY-ALPHA-EXECUTION-ROADMAP.md Group A.

    const sentinelPath = await getSecret('SENTINEL_PATH');

    // Sentinel adapter (if configured)
    const sentinelPromise = sentinelPath ? (async () => {
      const { SentinelAdapter } = await import('../adapters/sentinel/shared/SentinelAdapter');
      await this.registerAdapter(new SentinelAdapter(), { priority: 95, enabled: true });
      this.log.info('✅ Sentinel adapter registered');
    })() : Promise.resolve();

    await Promise.allSettled([sentinelPromise]);

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

    // Register local models (Candle adapter) — the adapter is the source of truth
    // for its own context window and capabilities (not the static map).
    this.registerLocalModels();

    // Bootstrap the shared ontology — load concepts, seed missing ones, start drift detection.
    // Runs non-blocking after adapters are ready (embeddings require the AI provider to be up).
    this.initOntologyRegistry().catch(err => {
      this.log.warn(`⚠️  OntologyRegistry bootstrap failed (non-fatal): ${err}`);
    });

    const deferredMs = Date.now() - deferredStart;
    this.log.info(`✅ AIProviderDaemonServer: DEFERRED init complete (${deferredMs}ms) - health monitoring active`);
  }

  /**
   * Initialise the shared ontology registry.
   *
   * Embedder and generator are thin closures over Commands.execute — the
   * AIProviderDaemon is already up at this point so the calls will resolve.
   * OntologyRegistry seeds missing concepts and bootstraps embeddings async.
   */
  private async initOntologyRegistry(): Promise<void> {
    const { OntologyRegistry } = await import('../../../system/ontology/server/OntologyRegistry');
    const { EmbeddingGenerate } = await import('../../../commands/ai/embedding/generate/shared/EmbeddingGenerateTypes');
    const { AIGenerate } = await import('../../../commands/ai/generate/shared/AIGenerateTypes');

    const embedder = async (text: string, modelKey: string): Promise<number[]> => {
      const [providerId, ...modelParts] = modelKey.split('/');
      const result = await EmbeddingGenerate.execute({
        input: text,
        provider: providerId,
        model: modelParts.join('/'),
      });
      return result?.embeddings?.[0] ?? [];
    };

    const generator = async (userPrompt: string, systemPrompt: string, modelKey: string): Promise<string> => {
      const [providerId, ...modelParts] = modelKey.split('/');
      const result = await AIGenerate.execute({
        messages: [{ role: 'user', content: userPrompt }],
        systemPrompt,
        provider: providerId as 'openai' | 'anthropic' | 'local' | 'candle' | 'groq' | 'deepseek',
        model: modelParts.join('/'),
        maxTokens: 512,
        temperature: 0.3,
      });
      return result?.text ?? '';
    };

    await OntologyRegistry.sharedInstance().init({
      embedder,
      generator,
      log: (msg, level = 'info') => this.log[level](`[OntologyRegistry] ${msg}`),
    });

    this.log.info('✅ OntologyRegistry initialised');
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
      const adapterRecord = adapter as unknown as Record<string, unknown>;
      const config = adapterRecord.config as { apiKey?: string; baseUrl?: string; models?: Array<{ id: string; contextWindow: number; maxOutputTokens?: number; capabilities?: string[]; costPer1kTokens?: { input: number; output: number } }> } | undefined;
      if (config?.apiKey && config?.baseUrl) {
        const staticModels = config.models?.map((m) => ({
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
      const apiKey = adapterRecord.apiKey as string | undefined;
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
      const googleConfig = adapterRecord.googleConfig as { apiKey?: string } | undefined;
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
   * Register local model capabilities in the ModelRegistry.
   *
   * The Candle adapter is the single source of truth for its own context window
   * and capabilities. This queries the Rust adapter and registers the result
   * so ModelContextWindows.ts static entries are never used for local models.
   */
  private registerLocalModels(): void {
    const client = new RustCoreIPCClient(getContinuumCoreSocketPath());
    client.connect()
      .then(() => client.execute<{ models: Array<{ id: string; context_window: number; max_output_tokens?: number; provider: string }> }>('ai/models/list', {}))
      .then(async (result) => {
        if (!result.success || !result.data?.models) return;

        const { ModelRegistry } = await import('../../../system/shared/ModelRegistry');
        const registry = ModelRegistry.sharedInstance();
        let count = 0;

        for (const model of result.data.models) {
          registry.register({
            modelId: model.id,
            contextWindow: model.context_window,
            maxOutputTokens: model.max_output_tokens,
            provider: model.provider,
            discoveredAt: Date.now(),
          });
          count++;
        }

        if (count > 0) {
          this.log.info(`ModelRegistry: ${count} local models registered from Rust adapters`);
        }
        client.disconnect();
      })
      .catch((err) => {
        this.log.debug(`Local model registration skipped: ${err.message}`);
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
