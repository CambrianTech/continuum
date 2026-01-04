/**
 * PricingManager - Centralized pricing with adapter overrides
 *
 * Design:
 * - Base pricing in JSON (fallback)
 * - Adapters can override with API-fetched pricing
 * - Conservative rounding (always round UP)
 * - Cached pricing (don't hit APIs every generation)
 */

export interface ModelPricing {
  inputPer1M: number;   // USD per 1 million input tokens
  outputPer1M: number;  // USD per 1 million output tokens
  currency: string;
  effectiveDate: string;
  notes?: string;
}

export interface PricingCache {
  [provider: string]: {
    [model: string]: {
      pricing: ModelPricing;
      fetchedAt: number;
      source: 'static' | 'api' | 'adapter';
    };
  };
}

/**
 * Static pricing configuration structure loaded from JSON
 */
export interface StaticPricingConfig {
  providers: {
    [provider: string]: {
      models: {
        [model: string]: ModelPricing;
      };
    };
  };
}

export class PricingManager {
  private static instance: PricingManager;
  private cache: PricingCache = {};
  private staticPricing: StaticPricingConfig | null = null;
  private readonly CACHE_TTL = 24 * 60 * 60 * 1000; // 24 hours
  private log: (message: string) => void = () => {}; // No-op by default

  private constructor() {
    // Load static pricing from JSON on initialization
    this.loadStaticPricing();
  }

  static getInstance(): PricingManager {
    if (!PricingManager.instance) {
      PricingManager.instance = new PricingManager();
    }
    return PricingManager.instance;
  }

  /**
   * Set logger for pricing warnings (called by AIProviderDaemon)
   */
  setLogger(logger: (message: string) => void): void {
    this.log = logger;
  }

  /**
   * Load static pricing from JSON file (fallback)
   */
  private async loadStaticPricing(): Promise<void> {
    try {
      // TODO: Load pricing.json via file system
      // For now, use inline fallback
      this.staticPricing = {
        providers: {
          openai: {
            models: {
              'gpt-4o': { inputPer1M: 2.50, outputPer1M: 10.00, currency: 'USD', effectiveDate: '2024-05-13' },
              'gpt-4': { inputPer1M: 30.00, outputPer1M: 60.00, currency: 'USD', effectiveDate: '2023-03-14' },
              'gpt-3.5-turbo': { inputPer1M: 0.50, outputPer1M: 1.50, currency: 'USD', effectiveDate: '2023-11-06' }
            }
          },
          anthropic: {
            models: {
              'claude-3-opus-20240229': { inputPer1M: 15.00, outputPer1M: 75.00, currency: 'USD', effectiveDate: '2024-03-04' },
              'claude-3-sonnet-20240229': { inputPer1M: 3.00, outputPer1M: 15.00, currency: 'USD', effectiveDate: '2024-03-04' },
              'claude-3-5-sonnet-20241022': { inputPer1M: 3.00, outputPer1M: 15.00, currency: 'USD', effectiveDate: '2024-10-22' },
              'claude-3-haiku-20240307': { inputPer1M: 0.25, outputPer1M: 1.25, currency: 'USD', effectiveDate: '2024-03-07' }
            }
          },
          deepseek: {
            models: {
              'deepseek-chat': { inputPer1M: 0.27, outputPer1M: 1.10, currency: 'USD', effectiveDate: '2024-01-01' },
              'deepseek-reasoner': { inputPer1M: 0.55, outputPer1M: 2.19, currency: 'USD', effectiveDate: '2025-01-20' }
            }
          },
          ollama: {
            models: {
              '*': { inputPer1M: 0.00, outputPer1M: 0.00, currency: 'USD', effectiveDate: '2024-01-01' }
            }
          }
        }
      };
    } catch (error) {
      console.error('❌ PricingManager: Failed to load static pricing:', error);
    }
  }

  /**
   * Get pricing for a model (checks cache, then static)
   * Synchronous because cache and static pricing are in-memory
   */
  getModelPricing(provider: string, model: string): ModelPricing | null {
    // Check cache first (adapter-provided pricing takes precedence)
    const cached = this.cache[provider]?.[model];
    if (cached && Date.now() - cached.fetchedAt < this.CACHE_TTL) {
      return cached.pricing;
    }

    // Check static pricing
    const staticPrice = this.staticPricing?.providers?.[provider]?.models?.[model];
    if (staticPrice) {
      // Cache it
      if (!this.cache[provider]) this.cache[provider] = {};
      this.cache[provider][model] = {
        pricing: staticPrice,
        fetchedAt: Date.now(),
        source: 'static'
      };
      return staticPrice;
    }

    // Check wildcard pricing (e.g., ollama = free)
    const wildcardPrice = this.staticPricing?.providers?.[provider]?.models?.['*'];
    if (wildcardPrice) {
      return wildcardPrice;
    }

    // No pricing found
    this.log(`⚠️ PricingManager: No pricing for ${provider}/${model}`);
    return null;
  }

  /**
   * Register adapter-provided pricing (overrides static)
   * Adapters call this after fetching from API
   */
  registerAdapterPricing(provider: string, model: string, pricing: ModelPricing): void {
    if (!this.cache[provider]) this.cache[provider] = {};
    this.cache[provider][model] = {
      pricing,
      fetchedAt: Date.now(),
      source: 'adapter'
    };
    //console.log(`✅ PricingManager: Cached adapter pricing for ${provider}/${model}`);
  }

  /**
   * Calculate cost with conservative rounding (always round UP)
   *
   * @param inputTokens - Number of input tokens
   * @param outputTokens - Number of output tokens
   * @param pricing - Model pricing info
   * @returns Cost in USD, rounded UP to nearest 0.0001 (1/100th cent)
   */
  calculateCost(inputTokens: number, outputTokens: number, pricing: ModelPricing): number {
    const inputCost = (inputTokens / 1_000_000) * pricing.inputPer1M;
    const outputCost = (outputTokens / 1_000_000) * pricing.outputPer1M;
    const totalCost = inputCost + outputCost;

    // Round UP to nearest 0.0001 USD (conservative)
    return Math.ceil(totalCost * 10000) / 10000;
  }

  /**
   * Clear pricing cache (useful for testing or forcing refresh)
   */
  clearCache(provider?: string, model?: string): void {
    if (provider && model) {
      delete this.cache[provider]?.[model];
    } else if (provider) {
      delete this.cache[provider];
    } else {
      this.cache = {};
    }
  }
}
