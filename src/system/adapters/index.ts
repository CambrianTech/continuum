/**
 * Adapter System
 *
 * Unified interface for LoRA adapter management across providers:
 * - Local (Candle)
 * - Together.ai
 * - Fireworks.ai
 * - (more to come)
 *
 * @example
 * ```typescript
 * import { getAdapterRegistry } from '@system/adapters';
 *
 * const registry = getAdapterRegistry();
 *
 * // Search across all providers
 * const results = await registry.searchWithCompatibility({
 *   query: 'code review',
 *   baseModel: 'llama'
 * });
 *
 * // Each result shows compatible providers
 * for (const adapter of results) {
 *   console.log(`${adapter.name}: ${adapter.compatibleProviders.join(', ')}`);
 * }
 *
 * // Deploy to best provider
 * const { provider, deployment } = await registry.deployToBest(
 *   'codelion/Llama-3.2-1B-Instruct-tool-calling-lora',
 *   { preferLocal: true }
 * );
 * ```
 */

// Core interfaces
export type {
  IAdapterProvider,
  IAdapterProviderRegistry,
  ProviderType,
  SupportedModel,
  AdapterSearchOptions,
  CompatibilityResult,
  DeployedAdapter,
  CostEstimate,
} from './IAdapterProvider';

// Provider implementations
export { LocalAdapterProvider } from './LocalAdapterProvider';
export { TogetherAdapterProvider } from './TogetherAdapterProvider';

// Registry
export {
  AdapterProviderRegistry,
  getAdapterRegistry,
  type EnrichedSearchResult,
  type ProviderRecommendation,
} from './AdapterProviderRegistry';
