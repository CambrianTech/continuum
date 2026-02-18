/**
 * Adapter Provider Registry
 *
 * Unified interface for searching and deploying adapters across all providers.
 * Like an automotive parts finder - "what's your model? here are compatible parts"
 *
 * Features:
 * - Federated search across all providers
 * - Automatic compatibility checking
 * - Smart provider selection (cost, availability, compatibility)
 * - Semantic search with provider filtering
 */

import type {
  IAdapterProvider,
  IAdapterProviderRegistry,
  AdapterSearchOptions,
  CompatibilityResult,
  CostEstimate,
} from './IAdapterProvider';
import type { AdapterSearchResultItem } from '../../commands/adapter/search/shared/AdapterSearchTypes';
import { LocalAdapterProvider } from './LocalAdapterProvider';
import { TogetherAdapterProvider } from './TogetherAdapterProvider';

/**
 * Search result with provider compatibility info
 */
export interface EnrichedSearchResult extends AdapterSearchResultItem {
  /** Providers that can run this adapter */
  compatibleProviders: string[];
  /** Best provider recommendation */
  recommendedProvider?: string;
  /** Cost estimates by provider */
  costByProvider?: Map<string, number>;
}

/**
 * Provider recommendation result
 */
export interface ProviderRecommendation {
  provider: IAdapterProvider;
  compatibility: CompatibilityResult;
  cost: CostEstimate;
  score: number;  // 0-100, higher = better
}

/**
 * Singleton registry managing all adapter providers
 */
export class AdapterProviderRegistry implements IAdapterProviderRegistry {
  private providers: Map<string, IAdapterProvider> = new Map();
  private static instance: AdapterProviderRegistry | null = null;

  private constructor() {
    // Register default providers
    this.register(new LocalAdapterProvider());
    this.register(new TogetherAdapterProvider());
    // Add more providers as they're implemented:
    // this.register(new FireworksAdapterProvider());
    // this.register(new ReplicateAdapterProvider());
  }

  /**
   * Get singleton instance
   */
  static getInstance(): AdapterProviderRegistry {
    if (!AdapterProviderRegistry.instance) {
      AdapterProviderRegistry.instance = new AdapterProviderRegistry();
    }
    return AdapterProviderRegistry.instance;
  }

  /**
   * Register a provider
   */
  register(provider: IAdapterProvider): void {
    this.providers.set(provider.name, provider);
    console.log(`📦 Registered adapter provider: ${provider.name} (${provider.type})`);
  }

  /**
   * Get provider by name
   */
  get(name: string): IAdapterProvider | undefined {
    return this.providers.get(name);
  }

  /**
   * Get all registered providers
   */
  all(): IAdapterProvider[] {
    return Array.from(this.providers.values());
  }

  /**
   * Search across all providers
   * Deduplicates results and adds compatibility info
   */
  async searchAll(options: AdapterSearchOptions): Promise<{
    results: AdapterSearchResultItem[];
    byProvider: Map<string, AdapterSearchResultItem[]>;
  }> {
    const byProvider = new Map<string, AdapterSearchResultItem[]>();
    const seenIds = new Set<string>();
    const allResults: AdapterSearchResultItem[] = [];

    // Search each provider in parallel
    const searchPromises = Array.from(this.providers.values()).map(async provider => {
      try {
        const results = await provider.search(options);
        byProvider.set(provider.name, results);
        return { provider: provider.name, results };
      } catch (error) {
        console.warn(`Search failed for provider ${provider.name}:`, error);
        byProvider.set(provider.name, []);
        return { provider: provider.name, results: [] };
      }
    });

    const providerResults = await Promise.all(searchPromises);

    // Merge and deduplicate
    for (const { results } of providerResults) {
      for (const result of results) {
        if (!seenIds.has(result.id)) {
          seenIds.add(result.id);
          allResults.push(result);
        }
      }
    }

    // Sort by downloads (default)
    allResults.sort((a, b) => (b.downloads || 0) - (a.downloads || 0));

    return { results: allResults.slice(0, options.limit || 10), byProvider };
  }

  /**
   * Search with compatibility enrichment
   * Returns results annotated with which providers can run each adapter
   */
  async searchWithCompatibility(
    options: AdapterSearchOptions,
    tokensPerMonth: number = 1_000_000
  ): Promise<EnrichedSearchResult[]> {
    const { results } = await this.searchAll(options);
    const enrichedResults: EnrichedSearchResult[] = [];

    for (const result of results) {
      const compatibleProviders: string[] = [];
      const costByProvider = new Map<string, number>();
      let recommendedProvider: string | undefined;
      let lowestCost = Infinity;

      // Check compatibility with each provider
      for (const provider of this.providers.values()) {
        const compat = await provider.checkCompatibility(result);
        if (compat.compatible) {
          compatibleProviders.push(provider.name);

          // Get cost estimate
          const cost = await provider.estimateCost(result.id, tokensPerMonth);
          costByProvider.set(provider.name, cost.monthlyEstimate);

          // Track cheapest compatible provider
          if (cost.monthlyEstimate < lowestCost) {
            lowestCost = cost.monthlyEstimate;
            recommendedProvider = provider.name;
          }
        }
      }

      enrichedResults.push({
        ...result,
        compatibleProviders,
        recommendedProvider,
        costByProvider,
      });
    }

    return enrichedResults;
  }

  /**
   * Find best provider for an adapter
   * Considers: compatibility, cost, availability, user preferences
   */
  async findBestProvider(
    adapter: AdapterSearchResultItem,
    preferences?: {
      preferLocal?: boolean;
      maxCostPerMillion?: number;
    }
  ): Promise<ProviderRecommendation | null> {
    const recommendations: ProviderRecommendation[] = [];

    for (const provider of this.providers.values()) {
      const compatibility = await provider.checkCompatibility(adapter);
      if (!compatibility.compatible) continue;

      const isAvailable = await provider.ping();
      if (!isAvailable) continue;

      const cost = await provider.estimateCost(adapter.id, 1_000_000);

      // Check cost constraint
      if (preferences?.maxCostPerMillion && cost.perMillionTokens > preferences.maxCostPerMillion) {
        continue;
      }

      // Calculate score (0-100)
      let score = 50;  // Base score

      // Local preference bonus
      if (preferences?.preferLocal && provider.type === 'local') {
        score += 30;
      }

      // Cost bonus (lower = better)
      if (cost.perMillionTokens === 0) {
        score += 20;  // Free (local)
      } else if (cost.perMillionTokens < 0.5) {
        score += 10;  // Cheap
      }

      // Cloud LoRA bonus (native adapter support)
      if (provider.type === 'cloud-lora') {
        score += 5;
      }

      recommendations.push({
        provider,
        compatibility,
        cost,
        score,
      });
    }

    if (recommendations.length === 0) return null;

    // Sort by score descending
    recommendations.sort((a, b) => b.score - a.score);
    return recommendations[0];
  }

  /**
   * Deploy adapter to best available provider
   */
  async deployToBest(
    adapterId: string,
    preferences?: {
      preferLocal?: boolean;
      maxCostPerMillion?: number;
      targetProvider?: string;
    }
  ): Promise<{
    provider: string;
    deployment: Awaited<ReturnType<IAdapterProvider['deploy']>>;
  }> {
    // If target provider specified, use it directly
    if (preferences?.targetProvider) {
      const provider = this.providers.get(preferences.targetProvider);
      if (!provider) {
        throw new Error(`Provider not found: ${preferences.targetProvider}`);
      }
      const deployment = await provider.deploy(adapterId);
      return { provider: provider.name, deployment };
    }

    // Otherwise find best provider
    // First we need the adapter info to check compatibility
    const searchResult = await this.searchAll({ query: adapterId, limit: 1 });
    const adapter = searchResult.results[0];

    if (!adapter) {
      throw new Error(`Adapter not found: ${adapterId}`);
    }

    const recommendation = await this.findBestProvider(adapter, preferences);
    if (!recommendation) {
      throw new Error(`No compatible provider found for: ${adapterId}`);
    }

    const deployment = await recommendation.provider.deploy(adapterId);
    return { provider: recommendation.provider.name, deployment };
  }
}

/**
 * Convenience function to get registry instance
 */
export function getAdapterRegistry(): AdapterProviderRegistry {
  return AdapterProviderRegistry.getInstance();
}
