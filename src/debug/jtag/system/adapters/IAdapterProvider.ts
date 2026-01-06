/**
 * Adapter Provider Interface
 *
 * Abstracts adapter operations across different backends:
 * - Local (Candle/Ollama) - direct LoRA weight merging
 * - Together.ai - cloud LoRA hosting
 * - Fireworks.ai - cloud LoRA hosting
 * - Replicate - custom model deployment
 *
 * Like an automotive parts finder - "what's your model? here are compatible parts"
 */

import type { AdapterSearchResultItem, AdapterSource, AdapterSortBy } from '../../commands/adapter/search/shared/AdapterSearchTypes';

/**
 * Provider types - determines capability set
 */
export type ProviderType = 'local' | 'cloud-lora' | 'cloud-finetune';

/**
 * Supported base models per provider
 */
export interface SupportedModel {
  id: string;           // e.g., "meta-llama/Llama-3.2-3B-Instruct"
  name: string;         // e.g., "Llama 3.2 3B"
  family: string;       // e.g., "llama"
  maxContext: number;   // e.g., 128000
  supportedRanks: number[];  // e.g., [8, 16, 32, 64]
}

/**
 * Adapter compatibility result
 */
export interface CompatibilityResult {
  compatible: boolean;
  reason?: string;
  suggestedProvider?: string;  // If incompatible, suggest alternative
}

/**
 * Deployment result
 */
export interface DeployedAdapter {
  adapterId: string;
  provider: string;
  endpoint?: string;      // API endpoint if cloud
  localPath?: string;     // Local path if local
  status: 'pending' | 'deploying' | 'ready' | 'failed';
  error?: string;
}

/**
 * Cost estimate for running adapter
 */
export interface CostEstimate {
  provider: string;
  perMillionTokens: number;  // USD
  monthlyEstimate: number;   // Based on provided token count
  notes?: string;
}

/**
 * Search options for adapter discovery
 */
export interface AdapterSearchOptions {
  query: string;
  baseModel?: string;
  limit?: number;
  sort?: AdapterSortBy;
  minDownloads?: number;
  tags?: string[];
}

/**
 * Unified Adapter Provider Interface
 *
 * Each provider implements this to enable:
 * 1. Searching for compatible adapters
 * 2. Checking compatibility before deployment
 * 3. Deploying adapters (upload or load)
 * 4. Estimating costs
 */
export interface IAdapterProvider {
  /** Provider identifier */
  readonly name: string;

  /** Provider type - determines capabilities */
  readonly type: ProviderType;

  /** Source identifier for search results */
  readonly source: AdapterSource;

  /** Human-readable description */
  readonly description: string;

  /**
   * Get models supported by this provider
   */
  getSupportedModels(): Promise<SupportedModel[]>;

  /**
   * Search for adapters available through this provider
   * @param options Search parameters
   */
  search(options: AdapterSearchOptions): Promise<AdapterSearchResultItem[]>;

  /**
   * Check if an adapter is compatible with this provider
   * @param adapter Adapter to check
   * @param targetModel Optional target model to check against
   */
  checkCompatibility(
    adapter: AdapterSearchResultItem,
    targetModel?: string
  ): Promise<CompatibilityResult>;

  /**
   * Deploy an adapter to this provider
   * - Local: Load into inference worker
   * - Cloud: Upload to provider's registry
   * @param adapterId HuggingFace repo ID or local adapter ID
   * @param options Deployment options
   */
  deploy(
    adapterId: string,
    options?: {
      baseModel?: string;
      scale?: number;
      alias?: string;
    }
  ): Promise<DeployedAdapter>;

  /**
   * Undeploy/unload an adapter
   * @param adapterId Adapter to remove
   */
  undeploy(adapterId: string): Promise<void>;

  /**
   * List currently deployed adapters
   */
  listDeployed(): Promise<DeployedAdapter[]>;

  /**
   * Estimate cost for running this adapter
   * @param adapterId Adapter to estimate
   * @param tokensPerMonth Expected monthly token usage
   */
  estimateCost(
    adapterId: string,
    tokensPerMonth: number
  ): Promise<CostEstimate>;

  /**
   * Health check - is provider available?
   */
  ping(): Promise<boolean>;
}

/**
 * Provider registry - singleton managing all providers
 */
export interface IAdapterProviderRegistry {
  /**
   * Register a provider
   */
  register(provider: IAdapterProvider): void;

  /**
   * Get provider by name
   */
  get(name: string): IAdapterProvider | undefined;

  /**
   * Get all registered providers
   */
  all(): IAdapterProvider[];

  /**
   * Search across all providers
   */
  searchAll(options: AdapterSearchOptions): Promise<{
    results: AdapterSearchResultItem[];
    byProvider: Map<string, AdapterSearchResultItem[]>;
  }>;

  /**
   * Find best provider for an adapter
   * Based on: compatibility, cost, availability
   */
  findBestProvider(
    adapter: AdapterSearchResultItem,
    preferences?: {
      preferLocal?: boolean;
      maxCostPerMillion?: number;
    }
  ): Promise<{
    provider: IAdapterProvider;
    compatibility: CompatibilityResult;
    cost: CostEstimate;
  } | null>;
}
