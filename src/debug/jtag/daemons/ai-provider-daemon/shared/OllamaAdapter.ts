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
  OllamaGenerateRequest,
  OllamaGenerateResponse,
  OllamaListResponse,
} from './AIProviderTypes';
import {
  chatMessagesToPrompt,
  estimateTokenCount,
  createRequestId,
  AIProviderError,
} from './AIProviderTypes';
import { BaseAIProviderAdapter } from './BaseAIProviderAdapter';
import { spawn } from 'child_process';

/**
 * Request queue for Ollama API
 * Prevents overload by limiting concurrent requests to Ollama's actual capacity (2)
 */
interface QueuedRequest {
  executor: () => Promise<unknown>;
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  requestId: string;
}

class OllamaRequestQueue {
  private queue: Array<QueuedRequest> = [];
  private activeRequests = 0;
  private readonly maxConcurrent = 2;  // Ollama's actual capacity

  async enqueue<T>(executor: () => Promise<T>, requestId: string): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      this.queue.push({
        executor: executor as () => Promise<unknown>,
        resolve: resolve as (value: unknown) => void,
        reject,
        requestId
      });
      console.log(`🔄 Ollama Queue: Enqueued request ${requestId} (queue size: ${this.queue.length}, active: ${this.activeRequests}/${this.maxConcurrent})`);
      this.processQueue();
    });
  }

  private async processQueue(): Promise<void> {
    if (this.activeRequests >= this.maxConcurrent || this.queue.length === 0) {
      return;
    }

    this.activeRequests++;
    const request = this.queue.shift()!;

    console.log(`▶️  Ollama Queue: Starting request ${request.requestId} (queue size: ${this.queue.length}, active: ${this.activeRequests}/${this.maxConcurrent})`);

    try {
      const result = await request.executor();
      request.resolve(result);
    } catch (error) {
      request.reject(error as Error);
    } finally {
      this.activeRequests--;
      console.log(`✅ Ollama Queue: Completed request ${request.requestId} (queue size: ${this.queue.length}, active: ${this.activeRequests}/${this.maxConcurrent})`);
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

  private config: ProviderConfiguration;
  private healthCache: { status: HealthStatus; timestamp: number } | null = null;
  private readonly healthCacheTTL = 30000; // 30 seconds
  private readonly requestQueue = new OllamaRequestQueue();

  constructor(config?: Partial<ProviderConfiguration>) {
    super();
    this.config = {
      apiEndpoint: 'http://localhost:11434',
      timeout: 30000,
      retryAttempts: 3,
      retryDelay: 1000,
      defaultModel: 'phi3:mini',
      defaultTemperature: 0.7,
      logRequests: true,
      ...config,
    };
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
        this.providerId,
        'OLLAMA_UNAVAILABLE',
        { endpoint: this.config.apiEndpoint }
      );
    }

    // Clear Ollama context for consistent results
    console.log('🧹 Ollama: Clearing loaded models for fresh state...');
    await this.clearLoadedModels();

    // Log available models
    const models = await this.getAvailableModels();
    console.log(`   ${models.length} models available: ${models.join(', ')}`);
  }

  /**
   * Ollama-specific shutdown
   */
  protected async shutdownProvider(): Promise<void> {
    this.healthCache = null;
  }

  /**
   * Restart frozen Ollama service
   */
  protected async restartProvider(): Promise<void> {
    spawn('killall', ['ollama']);
    await new Promise(resolve => setTimeout(resolve, 2000));
    spawn('ollama', ['serve'], { detached: true, stdio: 'ignore' }).unref();
    await new Promise(resolve => setTimeout(resolve, 3000));
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const startTime = Date.now();
    const requestId = request.requestId || createRequestId();

    try {
      // Convert chat messages to Ollama prompt format
      const { prompt, system } = chatMessagesToPrompt(request.messages);

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
        console.log(`🤖 ${this.providerName}: Generating text with model ${ollamaRequest.model}`);
        console.log(`   Request ID: ${requestId}`);
        console.log(`   Prompt length: ${prompt.length} chars`);
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
        console.log(`✅ ${this.providerName}: Generated response in ${responseTime}ms`);
        console.log(`   Output length: ${response.response.length} chars`);
        console.log(`   Tokens: ${usage.inputTokens} in, ${usage.outputTokens} out`);
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

      console.error(`❌ ${this.providerName}: Generation failed after ${responseTime}ms`);
      console.error(`   Error: ${error instanceof Error ? error.message : String(error)}`);

      throw new AIProviderError(
        `Text generation failed: ${error instanceof Error ? error.message : String(error)}`,
        this.providerId,
        'GENERATION_FAILED',
        { requestId, responseTime, originalError: error }
      );
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    // Return cached health if recent
    if (this.healthCache && Date.now() - this.healthCache.timestamp < this.healthCacheTTL) {
      return this.healthCache.status;
    }

    const startTime = Date.now();

    try {
      // Try to list models (lightweight health check)
      const response = await fetch(`${this.config.apiEndpoint}/api/tags`, {
        method: 'GET',
        signal: AbortSignal.timeout(5000), // 5 second timeout for health check
      });

      const responseTime = Date.now() - startTime;

      if (!response.ok) {
        const status: HealthStatus = {
          status: 'unhealthy',
          apiAvailable: false,
          responseTime,
          errorRate: 1.0,
          lastChecked: Date.now(),
          message: `Ollama API returned ${response.status}`,
        };
        this.healthCache = { status, timestamp: Date.now() };
        return status;
      }

      const status: HealthStatus = {
        status: responseTime < 1000 ? 'healthy' : 'degraded',
        apiAvailable: true,
        responseTime,
        errorRate: 0,
        lastChecked: Date.now(),
        message: responseTime < 1000 ? 'Ollama is responding normally' : 'Ollama is slow',
      };

      this.healthCache = { status, timestamp: Date.now() };
      return status;
    } catch (error) {
      const responseTime = Date.now() - startTime;

      const status: HealthStatus = {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime,
        errorRate: 1.0,
        lastChecked: Date.now(),
        message: `Ollama is not available: ${error instanceof Error ? error.message : String(error)}`,
      };

      this.healthCache = { status, timestamp: Date.now() };
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
            console.log(`🧹 Ollama: Unloaded ${modelName}`);
          } catch (e) {
            // Individual model stop failed, continue
          }
        }
      }
      console.log('✅ Ollama: All models unloaded for fresh state');
    } catch (error) {
      // Non-critical - log but continue
      console.log('⚠️  Ollama: Could not unload models (non-critical)');
    }
  }

  async getAvailableModels(): Promise<string[]> {
    try {
      const response = await this.makeRequest<OllamaListResponse>('/api/tags');
      return response.models.map(m => m.name);
    } catch (error) {
      console.error(`❌ ${this.providerName}: Failed to list models`);
      console.error(`   Error: ${error instanceof Error ? error.message : String(error)}`);
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
          console.log(`⚠️  ${this.providerName}: Request failed (attempt ${attempt}/${this.config.retryAttempts}), retrying...`);

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
