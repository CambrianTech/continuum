/**
 * Inference Worker - Isolated process for genome inference
 *
 * Runs in child process spawned by ProcessPool.
 * Handles genome loading, LoRA layer assembly, and inference execution.
 *
 * Communication Protocol:
 * - Parent → Worker: { type: 'load-genome', genomeId, layers }
 * - Parent → Worker: { type: 'infer', prompt, genomeId }
 * - Parent → Worker: { type: 'shutdown' }
 * - Worker → Parent: { type: 'ready' }
 * - Worker → Parent: { type: 'loaded', genomeId }
 * - Worker → Parent: { type: 'result', output }
 * - Worker → Parent: { type: 'error', error }
 *
 * Phase 2.1: Basic process lifecycle (spawn/ready/shutdown)
 * Phase 2.2: Genome loading + LoRA assembly
 * Phase 2.3: Actual inference execution
 */

import type { AIProviderAdapter, TextGenerationRequest } from '../../../daemons/ai-provider-daemon/shared/AIProviderTypes';
import { OllamaAdapter } from '../../../daemons/ai-provider-daemon/shared/OllamaAdapter';
// TODO: Import additional adapters when implemented:
// import { ClaudeAdapter } from '../../../daemons/ai-provider-daemon/shared/ClaudeAdapter';
// import { OpenAIAdapter } from '../../../daemons/ai-provider-daemon/shared/OpenAIAdapter';

/**
 * Worker state
 */
interface WorkerState {
  processId: string;
  poolTier: 'hot' | 'warm' | 'cold';
  loadedGenomeId?: string;
  isReady: boolean;
  requestCount: number;
}

/**
 * Message types from parent
 */
type ParentMessage =
  | { type: 'load-genome'; genomeId: string; layers: string[] }
  | {
      type: 'infer';
      prompt: string;
      provider: string;
      model: string;
      temperature?: number;
      maxTokens?: number;
      config?: Record<string, any>;
      genomeId?: string;
    }
  | { type: 'shutdown' }
  | { type: 'health-check' };

/**
 * Message types to parent
 */
type WorkerMessage =
  | { type: 'ready' }
  | { type: 'loaded'; genomeId: string }
  | { type: 'result'; output: string }
  | { type: 'error'; error: string }
  | { type: 'health'; memoryMB: number; uptime: number };

/**
 * IPC message from parent (generic unknown type until parsed)
 */
interface IPCMessage {
  type: string;
  [key: string]: unknown;
}

// Initialize worker state
const state: WorkerState = {
  processId: process.env.PROCESS_ID || 'unknown',
  poolTier: (process.env.POOL_TIER as 'hot' | 'warm' | 'cold') || 'cold',
  isReady: false,
  requestCount: 0,
};

// Keep-alive interval to prevent process from exiting
// Declared at module level so shutdown handler can access it
let keepAliveInterval: NodeJS.Timeout;

console.log(
  `🔧 InferenceWorker: Starting (PID: ${process.pid}, ID: ${state.processId}, Tier: ${state.poolTier})`
);

/**
 * Send message to parent process
 */
function sendToParent(message: WorkerMessage): void {
  if (process.send) {
    process.send(message);
  } else {
    console.error('❌ InferenceWorker: No IPC channel to parent');
  }
}

/**
 * Handle messages from parent
 */
process.on('message', async (rawMessage: unknown) => {
  try {
    // Type guard: ensure message is an object with a type property
    if (!rawMessage || typeof rawMessage !== 'object' || !('type' in rawMessage)) {
      console.warn('⚠️  InferenceWorker: Invalid message received:', rawMessage);
      return;
    }

    const message = rawMessage as ParentMessage;

    switch (message.type) {
      case 'load-genome':
        await handleLoadGenome(message.genomeId, message.layers);
        break;

      case 'infer':
        await handleInference(message);
        break;

      case 'health-check':
        handleHealthCheck();
        break;

      case 'shutdown':
        await handleShutdown();
        break;

      default:
        console.warn(
          `⚠️  InferenceWorker: Unknown message type:`,
          (message as IPCMessage).type
        );
    }
  } catch (error) {
    const errorMessage =
      error instanceof Error ? error.message : 'Unknown error';
    console.error(`❌ InferenceWorker: Error handling message:`, error);
    sendToParent({ type: 'error', error: errorMessage });
  }
});

/**
 * Handle genome loading request
 */
async function handleLoadGenome(
  genomeId: string,
  layers: string[]
): Promise<void> {
  console.log(
    `🔄 InferenceWorker: Loading genome ${genomeId} with ${layers.length} layers...`
  );

  // Phase 2.1: Placeholder implementation
  // Phase 2.2: Actual LoRA layer loading
  // Phase 2.3: Layer assembly and model preparation

  // Simulate loading time
  await new Promise((resolve) => setTimeout(resolve, 100));

  state.loadedGenomeId = genomeId;
  console.log(`✅ InferenceWorker: Genome ${genomeId} loaded`);

  sendToParent({ type: 'loaded', genomeId });
}

/**
 * Handle inference request - generic adapter-agnostic implementation
 */
async function handleInference(message: {
  prompt: string;
  provider: string;
  model: string;
  temperature?: number;
  maxTokens?: number;
  config?: Record<string, unknown>;
  genomeId?: string;
}): Promise<void> {
  console.log(`🔧 CLAUDE-FIX-${Date.now()}: InferenceWorker handling generic ${message.provider} inference`);
  console.log(
    `🔄 InferenceWorker: Running inference (provider: ${message.provider}, model: ${message.model}, prompt length: ${message.prompt.length})`
  );

  try {
    // Load and initialize the appropriate adapter (imported at top)
    const adapter = await loadProviderAdapter(message.provider, message.config);

    // Build inference request
    const request: TextGenerationRequest = {
      messages: [{ role: 'user', content: message.prompt }],
      model: message.model,
      temperature: message.temperature,
      maxTokens: message.maxTokens,
    };

    // Execute inference through adapter
    const response = await adapter.generateText(request);

    state.requestCount++;
    console.log(
      `✅ InferenceWorker: Inference complete (total requests: ${state.requestCount})`
    );

    sendToParent({ type: 'result', output: response.text });
  } catch (error) {
    console.error(`❌ InferenceWorker: Inference failed:`, error);
    throw error;
  }
}

/**
 * Cache for initialized adapters (avoid re-initializing)
 */
const adapterCache = new Map<string, AIProviderAdapter>();

/**
 * Load and initialize AI provider adapter (adapters imported at top)
 * Supports: ollama, claude, openai, etc.
 */
async function loadProviderAdapter(
  provider: string,
  config?: Record<string, unknown>
): Promise<AIProviderAdapter> {
  const cacheKey = `${provider.toLowerCase()}-${JSON.stringify(config ?? {})}`;

  // Return cached adapter if available
  if (adapterCache.has(cacheKey)) {
    console.log(`♻️  InferenceWorker: Using cached ${provider} adapter`);
    return adapterCache.get(cacheKey)!;
  }

  console.log(`🔄 InferenceWorker: Loading ${provider} adapter...`);

  let adapter: AIProviderAdapter;

  switch (provider.toLowerCase()) {
    case 'ollama':
      adapter = new OllamaAdapter(config);
      break;

    // TODO: Add when adapters are implemented
    // case 'claude':
    //   adapter = new ClaudeAdapter(config);
    //   break;
    //
    // case 'openai':
    //   adapter = new OpenAIAdapter(config);
    //   break;

    default:
      throw new Error(`Unknown AI provider: ${provider} (only 'ollama' currently supported)`);
  }

  // Initialize adapter (lazy initialization happens here, not at worker startup)
  await adapter.initialize();

  // Cache for future requests
  adapterCache.set(cacheKey, adapter);

  return adapter;
}

/**
 * Handle health check request
 */
function handleHealthCheck(): void {
  const memUsage = process.memoryUsage();
  const memoryMB = Math.round(memUsage.heapUsed / 1024 / 1024);
  const uptime = process.uptime();

  sendToParent({ type: 'health', memoryMB, uptime });
}

/**
 * Handle shutdown request
 */
async function handleShutdown(): Promise<void> {
  console.log(`🔄 InferenceWorker: Shutting down...`);

  // Phase 2.2: Cleanup loaded genomes
  // Phase 2.3: Flush pending requests

  // Graceful cleanup
  if (state.loadedGenomeId) {
    console.log(
      `🧹 InferenceWorker: Unloading genome ${state.loadedGenomeId}`
    );
    state.loadedGenomeId = undefined;
  }

  console.log(
    `✅ InferenceWorker: Shutdown complete (processed ${state.requestCount} requests)`
  );

  // Clear keep-alive interval before exiting
  if (keepAliveInterval) {
    clearInterval(keepAliveInterval);
  }
  process.exit(0);
}

/**
 * Handle uncaught errors
 */
process.on('uncaughtException', (error) => {
  console.error(`❌ InferenceWorker: Uncaught exception:`, error);
  sendToParent({ type: 'error', error: error.message });
  process.exit(1);
});

process.on('unhandledRejection', (reason) => {
  console.error(`❌ InferenceWorker: Unhandled rejection:`, reason);
  sendToParent({
    type: 'error',
    error: reason instanceof Error ? reason.message : String(reason),
  });
  process.exit(1);
});

// Initialize and signal ready
(async () => {
  try {
    // Phase 2.2: Initialize inference engine
    // Phase 2.3: Warm up model

    // Start keep-alive interval to prevent process from exiting
    keepAliveInterval = setInterval(() => {
      // Do nothing - just keep event loop alive
    }, 30000); // Every 30 seconds

    state.isReady = true;
    console.log(`✅ InferenceWorker: Ready (PID: ${process.pid})`);
    sendToParent({ type: 'ready' });
  } catch (error) {
    console.error(`❌ InferenceWorker: Initialization failed:`, error);
    if (keepAliveInterval) {
      clearInterval(keepAliveInterval);
    }
    process.exit(1);
  }
})();

// Clean up keep-alive on shutdown
process.on('beforeExit', () => {
  if (keepAliveInterval) {
    clearInterval(keepAliveInterval);
  }
});
