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
  AIProviderAdapter,
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

export class OllamaAdapter implements AIProviderAdapter {
  readonly providerId = 'ollama';
  readonly providerName = 'Ollama';

  private config: ProviderConfiguration;
  private healthCache: { status: HealthStatus; timestamp: number } | null = null;
  private readonly healthCacheTTL = 30000; // 30 seconds

  constructor(config?: Partial<ProviderConfiguration>) {
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

  async initialize(): Promise<void> {
    console.log(`🤖 ${this.providerName}: Initializing...`);

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

    // Log available models
    const models = await this.getAvailableModels();
    console.log(`✅ ${this.providerName}: Initialized with ${models.length} models available`);
    console.log(`   Models: ${models.join(', ')}`);
  }

  async shutdown(): Promise<void> {
    console.log(`🔄 ${this.providerName}: Shutting down...`);
    this.healthCache = null;
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
   * Make HTTP request to Ollama API with retry logic
   */
  private async makeRequest<T>(
    endpoint: string,
    body?: unknown,
    attempt = 1
  ): Promise<T> {
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

        return this.makeRequest<T>(endpoint, body, attempt + 1);
      }

      // All retries exhausted
      throw error;
    }
  }
}
