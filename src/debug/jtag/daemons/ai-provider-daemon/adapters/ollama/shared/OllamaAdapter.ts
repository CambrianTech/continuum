/**
 * Ollama Adapter - Local LLM Integration
 * =======================================
 *
 * Adapter for Ollama local models (llama3.2:1b, phi3:mini, etc.)
 * Provides free, private, offline AI inference for PersonaUsers.
 *
 * Features:
 * - Text generation with local models
 * - No API keys required
 * - Privacy-first (data never leaves machine)
 * - Fast inference (~200-500ms)
 *
 * Ollama API: http://localhost:11434
 */

import type {
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
  ProviderConfiguration,
  UsageMetrics,
  EmbeddingRequest,
  EmbeddingResponse,
} from '../../../shared/AIProviderTypesV2';
import {
  chatMessagesToPrompt,
  AIProviderError,
} from '../../../shared/AIProviderTypesV2';

// Helper function previously imported from old AIProviderTypes
function createRequestId(): string {
  return `ollama-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

// Helper function previously imported from old AIProviderTypes
function estimateTokenCount(text: string): number {
  return Math.ceil(text.length / 4); // Rough approximation: 1 token ≈ 4 characters
}

// Ollama-specific types
interface OllamaGenerateRequest {
  model: string;
  prompt: string;
  system?: string;
  temperature?: number;
  num_predict?: number;
  stream?: boolean;
}

interface OllamaGenerateResponse {
  model: string;
  response: string;
  done: boolean;
  context?: number[];
  total_duration?: number;
  load_duration?: number;
  prompt_eval_duration?: number;
  eval_duration?: number;
}

interface OllamaListResponse {
  models: Array<{ name: string; modified_at: string; size: number }>;
}

interface OllamaEmbeddingRequest {
  model: string;
  prompt: string;
}

interface OllamaEmbeddingResponse {
  embedding: number[];
}
import { BaseAIProviderAdapter } from '../../../shared/BaseAIProviderAdapter';
import { spawn } from 'child_process';

/**
 * Request queue for Ollama API
 * Prevents overload by limiting concurrent requests and supports cancellation
 */
interface QueuedRequest {
  executor: () => Promise<unknown>;
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  requestId: string;
  abortController?: AbortController;  // For timeout cancellation
  enqueuedAt: number;  // Track when request was added to queue
  timeoutHandle?: ReturnType<typeof setTimeout>;  // Timeout timer
}

class OllamaRequestQueue {
  private queue: Array<QueuedRequest> = [];
  private activeRequests = 0;
  private readonly maxConcurrent: number;
  private activeRequestIds: Set<string> = new Set();
  private readonly REQUEST_TIMEOUT = 90000; // 90 seconds max wait time in queue
  private log: (message: string) => void;

  constructor(maxConcurrent: number = 4, logger?: (message: string) => void) {
    this.maxConcurrent = maxConcurrent;
    this.log = logger || console.log.bind(console);
    this.log(`🔧 Ollama Queue: Initialized with maxConcurrent=${maxConcurrent}, timeout=${this.REQUEST_TIMEOUT}ms`);
  }

  async enqueue<T>(executor: () => Promise<T>, requestId: string, abortController?: AbortController): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const queuedRequest: QueuedRequest = {
        executor: executor as () => Promise<unknown>,
        resolve: resolve as (value: unknown) => void,
        reject,
        requestId,
        abortController,
        enqueuedAt: Date.now()
      };

      // Setup queue timeout - reject if request waits too long
      queuedRequest.timeoutHandle = setTimeout(() => {
        const queueIndex = this.queue.findIndex(req => req.requestId === requestId);
        if (queueIndex !== -1) {
          // Still in queue after timeout - reject it
          this.queue.splice(queueIndex, 1);
          const waitTime = Date.now() - queuedRequest.enqueuedAt;
          this.log(`⏰ Ollama Queue: Request ${requestId} timed out after ${waitTime}ms in queue (max: ${this.REQUEST_TIMEOUT}ms)`);
          reject(new Error(`Request timed out in queue after ${waitTime}ms (max: ${this.REQUEST_TIMEOUT}ms)`));
        }
      }, this.REQUEST_TIMEOUT);

      this.queue.push(queuedRequest);
      this.log(`🔄 Ollama Queue: Enqueued request ${requestId} (queue size: ${this.queue.length}, active: ${this.activeRequests}/${this.maxConcurrent})`);

      // Setup abort handler
      if (abortController) {
        abortController.signal.addEventListener('abort', () => {
          this.cancelRequest(requestId);
        });
      }

      this.processQueue();
    });
  }

  /**
   * Cancel a specific request (removes from queue or aborts if active)
   */
  cancelRequest(requestId: string): void {
    // Remove from queue if not yet active
    const queueIndex = this.queue.findIndex(req => req.requestId === requestId);
    if (queueIndex !== -1) {
      const request = this.queue[queueIndex];
      this.queue.splice(queueIndex, 1);

      // Clear timeout if it exists
      if (request.timeoutHandle) {
        clearTimeout(request.timeoutHandle);
        request.timeoutHandle = undefined;
      }

      request.reject(new Error('Request cancelled while queued'));
      this.log(`❌ Ollama Queue: Cancelled queued request ${requestId} (queue size: ${this.queue.length})`);
      return;
    }

    // If active, AbortController already handles cancellation
    if (this.activeRequestIds.has(requestId)) {
      this.log(`⚠️  Ollama Queue: Request ${requestId} is active, aborting via AbortController`);
    }
  }

  private async processQueue(): Promise<void> {
    if (this.activeRequests >= this.maxConcurrent || this.queue.length === 0) {
      return;
    }

    this.activeRequests++;
    const request = this.queue.shift()!;
    this.activeRequestIds.add(request.requestId);

    // Clear queue timeout - request is now active
    if (request.timeoutHandle) {
      clearTimeout(request.timeoutHandle);
      request.timeoutHandle = undefined;
    }

    const queueWaitTime = Date.now() - request.enqueuedAt;
    this.log(`▶️  Ollama Queue: Starting request ${request.requestId} after ${queueWaitTime}ms wait (queue size: ${this.queue.length}, active: ${this.activeRequests}/${this.maxConcurrent})`);

    try {
      const result = await request.executor();
      request.resolve(result);
    } catch (error) {
      request.reject(error as Error);
    } finally {
      this.activeRequests--;
      this.activeRequestIds.delete(request.requestId);
      this.log(`✅ Ollama Queue: Completed request ${request.requestId} (queue size: ${this.queue.length}, active: ${this.activeRequests}/${this.maxConcurrent})`);
      this.processQueue();  // Process next request in queue
    }
  }

  getStats(): { queueSize: number; activeRequests: number; maxConcurrent: number } {
    return {
      queueSize: this.queue.length,
      activeRequests: this.activeRequests,
      maxConcurrent: this.maxConcurrent
    };
  }
}

export class OllamaAdapter extends BaseAIProviderAdapter {
  readonly providerId = 'ollama';
  readonly providerName = 'Ollama';
  readonly supportedCapabilities = ['text-generation' as const, 'chat' as const, 'embeddings' as const];

  private config: ProviderConfiguration;
  private healthCache: { status: HealthStatus; timestamp: number } | null = null;
  private readonly healthCacheTTL = 5000; // 5 seconds - reduced from 30s to detect degradation faster
  private readonly requestQueue: OllamaRequestQueue;

  constructor(config?: Partial<ProviderConfiguration>) {
    super();
    this.config = {
      apiEndpoint: 'http://localhost:11434',
      timeout: 60000, // 60s - increased from 30s to handle large prompts with llama3.2:3b
      retryAttempts: 3,
      retryDelay: 1000,
      defaultModel: 'phi3:mini',
      defaultTemperature: 0.7,
      logRequests: true,
      maxConcurrent: 12, // Increased from 4 to handle 13 AI personas responding simultaneously
      ...config,
    };

    // Initialize queue with configured maxConcurrent and logger
    this.requestQueue = new OllamaRequestQueue(
      this.config.maxConcurrent || 12,
      (msg: string) => this.log(null, 'info', msg)
    );
  }

  /**
   * Ollama-specific initialization
   */
  protected async initializeProvider(): Promise<void> {
    // Check Ollama is available
    const health = await this.healthCheck();
    if (!health.apiAvailable) {
      throw new AIProviderError(
        'Ollama is not available. Please ensure Ollama is installed and running.',
        'adapter',
        'OLLAMA_UNAVAILABLE',
        { endpoint: this.config.apiEndpoint }
      );
    }

    // Clear Ollama context for consistent results
    this.log(null, 'info', '🧹 Ollama: Clearing loaded models for fresh state...');
    await this.clearLoadedModels();

    // Log available models
    const models = await this.getAvailableModels();
    this.log(null, 'info', `   ${models.length} models available: ${models.join(', ')}`);
  }

  /**
   * Ollama-specific shutdown
   */
  protected async shutdownProvider(): Promise<void> {
    this.healthCache = null;
  }

  /**
   * Restart frozen Ollama service
   *
   * Non-blocking: Kills and restarts Ollama, returns immediately
   * Health monitoring will detect when it's back online
   */
  protected async restartProvider(): Promise<void> {
    this.log(null, 'info', '🔄 Ollama: Restarting service...');

    // Kill existing Ollama processes (non-blocking)
    spawn('killall', ['ollama']);

    // Start fresh Ollama server (non-blocking, detached from parent)
    // Health monitoring (AdapterHealthMonitor) will detect when server is ready
    spawn('ollama', ['serve'], { detached: true, stdio: 'ignore' }).unref();

    this.log(null, 'info', '🔄 Ollama: Restart initiated, health monitoring will verify recovery');
  }

  /**
   * Get current queue statistics for load-aware PersonaInbox consolidation
   * Exposes Ollama request queue state for feedback-driven load management
   */
  getQueueStats(): { queueSize: number; activeRequests: number; maxConcurrent: number; load: number } {
    const stats = this.requestQueue.getStats();
    const load = (stats.queueSize + stats.activeRequests) / stats.maxConcurrent;
    return {
      ...stats,
      load: Math.min(1.0, load) // Cap at 1.0
    };
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const startTime = Date.now();
    const requestId = request.requestId || createRequestId();

    try {
      // Convert chat messages to Ollama prompt format
      const { prompt, systemPrompt: system } = chatMessagesToPrompt(request.messages);

      // Build Ollama request
      const ollamaRequest: OllamaGenerateRequest = {
        model: request.model || this.config.defaultModel,
        prompt: prompt,
        system: system || request.systemPrompt,
        temperature: request.temperature ?? this.config.defaultTemperature,
        num_predict: request.maxTokens,
        stream: false,
      };

      if (this.config.logRequests) {
        this.log(request, 'info', `🤖 ${this.providerName}: Generating text with model ${ollamaRequest.model}`);
        this.log(request, 'debug', `   Request ID: ${requestId}`);
        this.log(request, 'debug', `   Prompt length: ${prompt.length} chars`);
      }

      // Make request to Ollama
      const response = await this.makeRequest<OllamaGenerateResponse>(
        '/api/generate',
        ollamaRequest
      );

      const responseTime = Date.now() - startTime;

      // Calculate usage metrics
      const usage: UsageMetrics = {
        inputTokens: estimateTokenCount(prompt + (system || '')),
        outputTokens: estimateTokenCount(response.response),
        totalTokens: 0,
        estimatedCost: 0, // Ollama is free
      };
      usage.totalTokens = usage.inputTokens + usage.outputTokens;

      if (this.config.logRequests) {
        this.log(request, 'info', `✅ ${this.providerName}: Generated response in ${responseTime}ms`);
        this.log(request, 'debug', `   Output length: ${response.response.length} chars`);
        this.log(request, 'debug', `   Tokens: ${usage.inputTokens} in, ${usage.outputTokens} out`);
      }

      return {
        text: response.response,
        finishReason: response.done ? 'stop' : 'length',
        model: response.model,
        provider: this.providerId,
        usage,
        responseTime,
        requestId,
      };
    } catch (error) {
      const responseTime = Date.now() - startTime;

      this.log(request, 'error', `❌ ${this.providerName}: Generation failed after ${responseTime}ms`);
      this.log(request, 'error', `   Error: ${error instanceof Error ? error.message : String(error)}`);

      throw new AIProviderError(
        `Text generation failed: ${error instanceof Error ? error.message : String(error)}`,
        'adapter',
        'GENERATION_FAILED',
        { requestId, responseTime, originalError: error }
      );
    }
  }

  async createEmbedding(request: EmbeddingRequest): Promise<EmbeddingResponse> {
    const startTime = Date.now();
    const requestId = request.requestId ?? createRequestId();

    try {
      // Determine which model to use based on request
      // Default to nomic-embed-text for general embeddings
      // For code-specific embeddings, use qwen3-embedding if specified
      const model = request.model ?? 'nomic-embed-text';

      // Handle both string and string[] inputs
      const inputs = Array.isArray(request.input) ? request.input : [request.input];

      if (this.config.logRequests) {
        this.log(null, 'info', `🔢 ${this.providerName}: Generating ${inputs.length} embedding(s) with model ${model}`);
        this.log(null, 'debug', `   Request ID: ${requestId}`);
      }

      // Generate embeddings for each input
      const embeddings: number[][] = [];

      for (const input of inputs) {
        const ollamaRequest: OllamaEmbeddingRequest = {
          model,
          prompt: input,
        };

        const response = await this.makeRequest<OllamaEmbeddingResponse>(
          '/api/embeddings',
          ollamaRequest
        );

        embeddings.push(response.embedding);
      }

      const responseTime = Date.now() - startTime;

      // Calculate usage metrics
      const totalInputTokens = inputs.reduce((sum, input) => sum + estimateTokenCount(input), 0);
      const usage: UsageMetrics = {
        inputTokens: totalInputTokens,
        outputTokens: 0, // Embeddings don't generate text
        totalTokens: totalInputTokens,
        estimatedCost: 0, // Ollama is free
      };

      if (this.config.logRequests) {
        this.log(null, 'info', `✅ ${this.providerName}: Generated ${embeddings.length} embedding(s) in ${responseTime}ms`);
        this.log(null, 'debug', `   Embedding dimensions: ${embeddings[0]?.length || 0}`);
        this.log(null, 'debug', `   Input tokens: ${totalInputTokens}`);
      }

      return {
        embeddings,
        model,
        provider: this.providerId,
        usage,
        responseTime,
        requestId,
      };
    } catch (error) {
      const responseTime = Date.now() - startTime;

      this.log(null, 'error', `❌ ${this.providerName}: Embedding generation failed after ${responseTime}ms`);
      this.log(null, 'error', `   Error: ${error instanceof Error ? error.message : String(error)}`);

      throw new AIProviderError(
        `Embedding generation failed: ${error instanceof Error ? error.message : String(error)}`,
        'adapter',
        'EMBEDDING_FAILED',
        { requestId, responseTime, originalError: error }
      );
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    // Return cached health if recent
    if (this.healthCache && Date.now() - this.healthCache.timestamp < this.healthCacheTTL) {
      const cacheAge = Math.floor((Date.now() - this.healthCache.timestamp) / 1000);
      this.log(null, 'debug', `🔍 Ollama Health: Returning cached ${this.healthCache.status.status} (${cacheAge}s old)`);
      return this.healthCache.status;
    }

    this.log(null, 'info', `🔍 Ollama Health: Running actual health check...`);
    const startTime = Date.now();

    try {
      // Step 1: Check API availability (lightweight check)
      const apiResponse = await fetch(`${this.config.apiEndpoint}/api/tags`, {
        method: 'GET',
        signal: AbortSignal.timeout(5000), // 5 second timeout for API check
      });

      if (!apiResponse.ok) {
        const message = `Ollama API returned ${apiResponse.status}`;
        const status: HealthStatus = {
          status: 'unhealthy',
          apiAvailable: false,
          responseTime: Date.now() - startTime,
          errorRate: 1.0,
          lastChecked: Date.now(),
          message,
        };
        this.healthCache = { status, timestamp: Date.now() };
        this.log(null, 'error', `❌ Ollama Health: ${message}`);
        return status;
      }

      // Step 2: Test actual generation performance (detect degraded state)
      const genStartTime = Date.now();
      try {
        const testRequest: OllamaGenerateRequest = {
          model: this.config.defaultModel,
          prompt: 'Hi', // Minimal prompt for health check
          stream: false,
        };

        const genResponse = await fetch(`${this.config.apiEndpoint}/api/generate`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(testRequest),
          signal: AbortSignal.timeout(10000), // 10 second timeout for generation test
        });

        const genTime = Date.now() - genStartTime;

        if (!genResponse.ok) {
          const message = `Ollama generation failed: ${genResponse.status}`;
          const status: HealthStatus = {
            status: 'unhealthy',
            apiAvailable: true,
            responseTime: genTime,
            errorRate: 1.0,
            lastChecked: Date.now(),
            message,
          };
          this.healthCache = { status, timestamp: Date.now() };
          this.log(null, 'error', `❌ Ollama Health: ${message}`);
          return status;
        }

        // Determine health based on generation time
        let healthStatus: 'healthy' | 'degraded' | 'unhealthy';
        let message: string;

        if (genTime < 3000) {
          healthStatus = 'healthy';
          message = `Ollama generating normally (${genTime}ms)`;
        } else if (genTime < 8000) {
          healthStatus = 'degraded';
          message = `Ollama is slow (${genTime}ms, expected <3s)`;
        } else {
          healthStatus = 'unhealthy';
          message = `Ollama severely degraded (${genTime}ms, expected <3s)`;
        }

        const status: HealthStatus = {
          status: healthStatus,
          apiAvailable: true,
          responseTime: genTime,
          errorRate: healthStatus === 'unhealthy' ? 1.0 : 0,
          lastChecked: Date.now(),
          message,
        };

        this.healthCache = { status, timestamp: Date.now() };

        // Log result based on status
        if (healthStatus === 'healthy') {
          this.log(null, 'info', `✅ Ollama Health: ${message}`);
        } else if (healthStatus === 'degraded') {
          this.log(null, 'warn', `⚠️  Ollama Health: ${message}`);
        } else {
          this.log(null, 'error', `❌ Ollama Health: ${message}`);
        }

        return status;

      } catch (genError) {
        // Generation test failed (timeout or error)
        const genTime = Date.now() - genStartTime;
        const message = `Ollama generation timeout/error after ${genTime}ms: ${genError instanceof Error ? genError.message : String(genError)}`;
        const status: HealthStatus = {
          status: 'unhealthy',
          apiAvailable: true,
          responseTime: genTime,
          errorRate: 1.0,
          lastChecked: Date.now(),
          message,
        };
        this.healthCache = { status, timestamp: Date.now() };
        this.log(null, 'error', `❌ Ollama Health: ${message}`);
        return status;
      }

    } catch (error) {
      const responseTime = Date.now() - startTime;
      const message = `Ollama is not available: ${error instanceof Error ? error.message : String(error)}`;

      const status: HealthStatus = {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime,
        errorRate: 1.0,
        lastChecked: Date.now(),
        message,
      };

      this.healthCache = { status, timestamp: Date.now() };
      this.log(null, 'error', `❌ Ollama Health: ${message}`);
      return status;
    }
  }

  /**
   * Clear all loaded models from Ollama memory
   * This ensures fresh, consistent state for each server restart
   */
  private async clearLoadedModels(): Promise<void> {
    try {
      const { execSync } = await import('child_process');
      // Get model names from ollama ps and stop each one
      const psOutput = execSync('ollama ps', { encoding: 'utf-8' });
      const lines = psOutput.split('\n').slice(1); // Skip header

      for (const line of lines) {
        const modelName = line.trim().split(/\s+/)[0];
        if (modelName && modelName !== 'NAME') {
          try {
            execSync(`ollama stop ${modelName}`, { stdio: 'ignore', timeout: 2000 });
            this.log(null, 'debug', `🧹 Ollama: Unloaded ${modelName}`);
          } catch (e) {
            // Individual model stop failed, continue
          }
        }
      }
      this.log(null, 'info', '✅ Ollama: All models unloaded for fresh state');
    } catch (error) {
      // Non-critical - log but continue
      this.log(null, 'warn', '⚠️  Ollama: Could not unload models (non-critical)');
    }
  }

  async getAvailableModels(): Promise<import('../../../shared/AIProviderTypesV2').ModelInfo[]> {
    try {
      const response = await this.makeRequest<OllamaListResponse>('/api/tags');
      return response.models.map(m => ({
        id: m.name,
        name: m.name,
        provider: 'ollama',
        capabilities: ['text-generation' as const, 'chat' as const],
        contextWindow: 4096, // Default, Ollama doesn't expose this
        supportsStreaming: true,
        supportsFunctions: false
      }));
    } catch (error) {
      this.log(null, 'error', `❌ ${this.providerName}: Failed to list models`);
      this.log(null, 'error', `   Error: ${error instanceof Error ? error.message : String(error)}`);
      return [];
    }
  }

  /**
   * Make HTTP request to Ollama API with retry logic and queue management
   */
  private async makeRequest<T>(
    endpoint: string,
    body?: unknown,
    attempt = 1,
    requestId?: string
  ): Promise<T> {
    const reqId = requestId || createRequestId();

    // Wrap the actual request in queue to prevent overload
    return this.requestQueue.enqueue(async () => {
      const url = `${this.config.apiEndpoint}${endpoint}`;

      try {
        const response = await fetch(url, {
          method: body ? 'POST' : 'GET',
          headers: body ? { 'Content-Type': 'application/json' } : undefined,
          body: body ? JSON.stringify(body) : undefined,
          signal: AbortSignal.timeout(this.config.timeout),
        });

        if (!response.ok) {
          throw new Error(`HTTP ${response.status}: ${response.statusText}`);
        }

        return (await response.json()) as T;
      } catch (error) {
        // Retry logic
        if (attempt < this.config.retryAttempts) {
          this.log(null, 'warn', `⚠️  ${this.providerName}: Request failed (attempt ${attempt}/${this.config.retryAttempts}), retrying...`);

          // Wait before retry
          await new Promise(resolve => setTimeout(resolve, this.config.retryDelay));

          // Note: Retry will re-enter the queue
          return this.makeRequest<T>(endpoint, body, attempt + 1, reqId);
        }

        // All retries exhausted
        throw error;
      }
    }, reqId);
  }
}
