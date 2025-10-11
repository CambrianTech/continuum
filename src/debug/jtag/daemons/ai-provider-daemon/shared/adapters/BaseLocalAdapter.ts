/**
 * BaseLocalAdapter - Shared logic for local inference providers
 *
 * Base class for local model providers that run on the same machine:
 * - Ollama
 * - llama.cpp server
 * - MLX server (future)
 * - LM Studio (future)
 *
 * Handles:
 * - Local HTTP requests with proper error handling
 * - Health checks via localhost endpoints
 * - Model listing from local servers
 * - Token estimation (no API-based counting needed)
 */

import type {
  AIProviderAdapter,
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
} from '../AIProviderTypes';

export interface LocalServerConfig {
  baseUrl: string;
  healthEndpoint: string;
  generateEndpoint: string;
  modelsEndpoint: string;
  timeout: number;
}

export abstract class BaseLocalAdapter implements AIProviderAdapter {
  abstract readonly providerId: string;
  abstract readonly providerName: string;

  protected config: LocalServerConfig;
  protected isInitialized = false;

  constructor(config: LocalServerConfig) {
    this.config = config;
  }

  async initialize(): Promise<void> {
    console.log(`🔌 ${this.providerName}: Initializing local adapter...`);

    // Check if local server is running
    const health = await this.healthCheck();
    if (health.status === 'unhealthy') {
      throw new Error(`${this.providerName} server is not running at ${this.config.baseUrl}`);
    }

    this.isInitialized = true;
    console.log(`✅ ${this.providerName}: Initialized successfully`);
  }

  async healthCheck(): Promise<HealthStatus> {
    const startTime = Date.now();

    try {
      const response = await fetch(`${this.config.baseUrl}${this.config.healthEndpoint}`, {
        method: 'GET',
        signal: AbortSignal.timeout(this.config.timeout),
      });

      const responseTime = Date.now() - startTime;

      if (response.ok) {
        return {
          status: 'healthy',
          apiAvailable: true,
          responseTime,
          errorRate: 0,
          lastChecked: Date.now(),
          message: `${this.providerName} server is running`,
        };
      } else {
        return {
          status: 'unhealthy',
          apiAvailable: false,
          responseTime,
          errorRate: 1,
          lastChecked: Date.now(),
          message: `${this.providerName} server returned ${response.status}`,
        };
      }
    } catch (error) {
      return {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime: Date.now() - startTime,
        errorRate: 1,
        lastChecked: Date.now(),
        message: `${this.providerName} server is not reachable: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
  }

  async getAvailableModels(): Promise<string[]> {
    try {
      const response = await fetch(`${this.config.baseUrl}${this.config.modelsEndpoint}`, {
        method: 'GET',
        signal: AbortSignal.timeout(this.config.timeout),
      });

      if (!response.ok) {
        throw new Error(`Failed to fetch models: ${response.status}`);
      }

      const data = await response.json();
      return this.parseModelsResponse(data);
    } catch (error) {
      console.error(`❌ ${this.providerName}: Failed to get models:`, error);
      return [];
    }
  }

  /**
   * Parse models response - override in subclass for provider-specific format
   */
  protected abstract parseModelsResponse(data: any): string[];

  /**
   * Generate text - override in subclass for provider-specific API
   */
  abstract generateText(request: TextGenerationRequest): Promise<TextGenerationResponse>;

  async shutdown(): Promise<void> {
    console.log(`🔄 ${this.providerName}: Shutting down (local adapter, no cleanup needed)`);
    this.isInitialized = false;
  }

  /**
   * Estimate tokens (simple approximation for local models)
   * More accurate than nothing, less accurate than API-based counting
   */
  protected estimateTokens(text: string): number {
    // Rough approximation: 1 token ≈ 4 characters
    return Math.ceil(text.length / 4);
  }

  /**
   * Make HTTP request with retry logic
   */
  protected async makeRequest<T>(
    endpoint: string,
    options: RequestInit,
    retries = 3
  ): Promise<T> {
    let lastError: Error | null = null;

    for (let attempt = 0; attempt < retries; attempt++) {
      try {
        const response = await fetch(`${this.config.baseUrl}${endpoint}`, {
          ...options,
          signal: AbortSignal.timeout(this.config.timeout),
        });

        if (!response.ok) {
          throw new Error(`HTTP ${response.status}: ${await response.text()}`);
        }

        return await response.json();
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        console.warn(`⚠️  ${this.providerName}: Request attempt ${attempt + 1} failed:`, lastError.message);

        if (attempt < retries - 1) {
          await new Promise(resolve => setTimeout(resolve, 1000 * (attempt + 1)));
        }
      }
    }

    throw lastError || new Error('Request failed after retries');
  }
}
