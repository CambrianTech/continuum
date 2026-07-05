/**
 * BaseLocalAdapter - Shared logic for local inference providers
 *
 * Base class for local model providers that run on the same machine:
 * - Candle (primary)
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
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
} from '../AIProviderTypesV2';
import { BaseAIProviderAdapter } from '../BaseAIProviderAdapter';
import { spawn } from 'child_process';

export interface LocalServerConfig {
  baseUrl: string;
  healthEndpoint: string;
  generateEndpoint: string;
  modelsEndpoint: string;
  timeout: number;
}

export abstract class BaseLocalAdapter extends BaseAIProviderAdapter {
  abstract readonly providerId: string;
  abstract readonly providerName: string;

  protected config: LocalServerConfig;
  protected isInitialized = false;

  constructor(config: LocalServerConfig) {
    super();
    this.config = config;
  }

  protected async initializeProvider(): Promise<void> {
    this.log(null, 'info', `🔌 ${this.providerName}: Initializing local adapter...`);

    // Check if local server is running
    const health = await this.healthCheck();
    if (health.status === 'unhealthy') {
      throw new Error(`${this.providerName} server is not running at ${this.config.baseUrl}`);
    }

    this.isInitialized = true;
    this.log(null, 'info', `✅ ${this.providerName}: Initialized successfully`);
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
          responseTimeMs: responseTime,
          errorRate: 0,
          lastChecked: Date.now(),
          message: `${this.providerName} server is running`,
        };
      } else {
        return {
          status: 'unhealthy',
          apiAvailable: false,
          responseTimeMs: responseTime,
          errorRate: 1,
          lastChecked: Date.now(),
          message: `${this.providerName} server returned ${response.status}`,
        };
      }
    } catch (error) {
      return {
        status: 'unhealthy',
        apiAvailable: false,
        responseTimeMs: Date.now() - startTime,
        errorRate: 1,
        lastChecked: Date.now(),
        message: `${this.providerName} server is not reachable: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
  }

  async getAvailableModels(): Promise<import('../AIProviderTypesV2').ModelInfo[]> {
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
      this.log(null, 'error', `❌ ${this.providerName}: Failed to get models:`, error);
      return [];
    }
  }

  /**
   * Parse models response - override in subclass for provider-specific format
   */
  protected abstract parseModelsResponse(data: unknown): import('../AIProviderTypesV2').ModelInfo[];

  /**
   * Generate text - override in subclass for provider-specific API
   */
  abstract generateText(request: TextGenerationRequest): Promise<TextGenerationResponse>;

  protected async shutdownProvider(): Promise<void> {
    this.log(null, 'info', `🔄 ${this.providerName}: Shutting down (local adapter, no cleanup needed)`);
    this.isInitialized = false;
  }

  /**
   * Restart local server (override in subclass for provider-specific restart)
   */
  protected async restartProvider(): Promise<void> {
    this.log(null, 'warn', `⚠️  ${this.providerName}: No restart logic implemented for this local provider`);
    this.log(null, 'warn', `   Please manually restart the server at ${this.config.baseUrl}`);
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
        this.log(null, 'warn', `⚠️  ${this.providerName}: Request attempt ${attempt + 1} failed:`, lastError.message);

        if (attempt < retries - 1) {
          await new Promise(resolve => setTimeout(resolve, 1000 * (attempt + 1)));
        }
      }
    }

    throw lastError || new Error('Request failed after retries');
  }
}
