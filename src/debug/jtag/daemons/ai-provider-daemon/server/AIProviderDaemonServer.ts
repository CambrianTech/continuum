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
  private heartbeatTimer?: NodeJS.Timeout;

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
   * Server-specific initialization
   * Initializes base daemon, dynamically loads adapters, and sets up static interface
   */
  protected async initialize(): Promise<void> {
    console.log('🚀🚀🚀 AIProviderDaemonServer INITIALIZE CALLED 🚀🚀🚀');

    // Enable health monitoring with timing metrics (for performance optimization)
    // Heartbeat every 30 seconds checks for stuck operations
    this.heartbeatTimer = setInterval(() => {
      this.healthState.lastHeartbeat = Date.now();

      // Check if daemon is stuck (no successful operations in 60s)
      const timeSinceSuccess = Date.now() - this.healthState.lastSuccessTime;
      if (timeSinceSuccess > 60000) {
        this.log.warn(`⚠️  AIProviderDaemon: Appears stuck (${Math.round(timeSinceSuccess / 1000)}s since last success)`);
        this.healthState.isHealthy = false;
      }
    }, 30000);
    this.log.info('📊 AIProviderDaemonServer: Health monitoring + metrics enabled');

    // Initialize SecretManager FIRST (adapters depend on it)
    this.log.info('🔐 AIProviderDaemonServer: Initializing SecretManager...');
    await initializeSecrets();
    this.log.info('✅ AIProviderDaemonServer: SecretManager initialized');

    // Register adapters dynamically (server-only code)
    this.log.info('🤖 AIProviderDaemonServer: Registering AI provider adapters...');

    // Register Ollama adapter (local, free, private)
    // maxConcurrent=4 allows multiple AI personas (Helper, Teacher, CodeReview) to generate simultaneously
    const { OllamaAdapter } = await import('../adapters/ollama/shared/OllamaAdapter');
    await this.registerAdapter(new OllamaAdapter({ maxConcurrent: 4 }), {
      priority: 100, // Highest priority - free and local
      enabled: true,
    });

    // Register Sentinel adapter (local, free, private, pre-trained models)
    // Provides TinyLlama, Phi-2, CodeLlama, DistilGPT2 for PersonaUsers

    const sentinelPath = await getSecret('SENTINEL_PATH'); //Enabled if SENTINEL_PATH is set
    
    if (sentinelPath) {
      const { SentinelAdapter } = await import('../adapters/sentinel/shared/SentinelAdapter');
      await this.registerAdapter(new SentinelAdapter(), {
        priority: 95, // High priority - local and free, but slower than Ollama
        enabled: true,
      });
      this.log.info('✅ AIProviderDaemonServer: Sentinel adapter registered');
    }

    // Register cloud adapters if API keys are available
    // Priority order: Ollama (100) > DeepSeek (90) > Groq (85) > OpenAI/Anthropic (80) > Together/Fireworks (70)

    // DeepSeek: Cheapest SOTA model ($0.27/M tokens vs GPT-4's $3.50/M)
    const deepseekKey = await getSecret('DEEPSEEK_API_KEY');
    if (deepseekKey) {
      const { DeepSeekAdapter } = await import('../adapters/deepseek/shared/DeepSeekAdapter');
      await this.registerAdapter(new DeepSeekAdapter(deepseekKey), {
        priority: 90,
        enabled: true,
      });
      this.log.info('✅ AIProviderDaemonServer: DeepSeek adapter registered');
    }

    // Groq: Fastest inference (LPU hardware, <100ms latency)
    const groqKey = await getSecret('GROQ_API_KEY');
    if (groqKey) {
      const { GroqAdapter } = await import('../adapters/groq/shared/GroqAdapter');
      await this.registerAdapter(new GroqAdapter(groqKey), {
        priority: 85,
        enabled: true,
      });
      this.log.info('✅ AIProviderDaemonServer: Groq adapter registered');
    }

    // X.AI: Grok models with advanced reasoning
    const xaiKey = await getSecret('XAI_API_KEY');
    if (xaiKey) {
      const { XAIAdapter } = await import('../adapters/xai/shared/XAIAdapter');
      await this.registerAdapter(new XAIAdapter(xaiKey), {
        priority: 83,
        enabled: true,
      });
      this.log.info('✅ AIProviderDaemonServer: X.AI (Grok) adapter registered');
    }

    // OpenAI: Premium quality (GPT-4, expensive)
    const openaiKey = await getSecret('OPENAI_API_KEY');
    if (openaiKey) {
      const { OpenAIAdapter } = await import('../adapters/openai/shared/OpenAIAdapter');
      await this.registerAdapter(new OpenAIAdapter(openaiKey), {
        priority: 80,
        enabled: true,
      });
      this.log.info('✅ AIProviderDaemonServer: OpenAI adapter registered');
    }

    // Anthropic: Best reasoning (Claude 3)
    const anthropicKey = await getSecret('ANTHROPIC_API_KEY');
    if (anthropicKey) {
      const { AnthropicAdapter } = await import('../adapters/anthropic/shared/AnthropicAdapter');
      await this.registerAdapter(new AnthropicAdapter(anthropicKey), {
        priority: 80,
        enabled: true,
      });
      this.log.info('✅ AIProviderDaemonServer: Anthropic adapter registered');
    }

    // Together.ai: Cheap + diverse models
    const togetherKey = await getSecret('TOGETHER_API_KEY');
    if (togetherKey) {
      const { TogetherAIAdapter } = await import('../adapters/together/shared/TogetherAIAdapter');
      await this.registerAdapter(new TogetherAIAdapter(togetherKey), {
        priority: 70,
        enabled: true,
      });
      this.log.info('✅ AIProviderDaemonServer: Together.ai adapter registered');
    }

    // Fireworks: Fast inference + coding models
    const fireworksKey = await getSecret('FIREWORKS_API_KEY');
    if (fireworksKey) {
      const { FireworksAdapter } = await import('../adapters/fireworks/shared/FireworksAdapter');
      await this.registerAdapter(new FireworksAdapter(fireworksKey), {
        priority: 70,
        enabled: true,
      });
      this.log.info('✅ AIProviderDaemonServer: Fireworks adapter registered');
    }

    // Call base initialization
    await super['initialize']();

    // DISABLED: ProcessPool adds 40s overhead - direct Ollama adapter is 132x faster
    // ProcessPool with HTTP workers → 41s per request
    // Direct Ollama adapter → 310ms per request
    this.log.info('🤖 AIProviderDaemonServer: Using direct Ollama adapter (no ProcessPool)');

    // Initialize static AIProviderDaemon interface FIRST (critical for PersonaUsers)
    // This must happen before health monitoring to prevent race conditions where
    // PersonaUsers try to call AIProviderDaemon.generateText() before sharedInstance is set
    AIProviderDaemon.initialize(this);
    this.log.info('✅ AIProviderDaemonServer: Static daemon interface initialized');

    // Initialize adapter health monitoring
    this.log.info('💓 AIProviderDaemonServer: Initializing adapter health monitoring...');
    const { AdapterHealthMonitor } = await import('./AdapterHealthMonitor');
    const { SystemHealthTicker } = await import('../../system-daemon/server/SystemHealthTicker');

    // Register all adapters with health monitor
    const healthMonitor = AdapterHealthMonitor.getInstance();
    for (const [providerId, registration] of this.adapters) {
      healthMonitor.registerAdapter(registration.adapter);
      this.log.info(`💚 Registered ${providerId} with health monitor`);
    }

    // Initialize health monitor (subscribes to system:health-check:tick events)
    await healthMonitor.initialize();

    // Start health ticker (emits system:health-check:tick events)
    const healthTicker = SystemHealthTicker.getInstance();
    await healthTicker.start();

    this.log.info('✅ AIProviderDaemonServer: Adapter health monitoring active');

    this.log.info(`🤖 ${this.toString()}: AI provider daemon server initialized with direct adapters (no ProcessPool)`);
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
