/**
 * ModelRegistry — Dynamic Model Metadata Discovery Cache
 * =======================================================
 *
 * Central registry for model metadata discovered from provider APIs at runtime.
 * Eliminates the need to hard-code every model's context window, output limits,
 * and capabilities in static maps.
 *
 * Architecture:
 *   Startup → static fallbacks available immediately (ModelContextWindows.ts)
 *   initializeDeferred() → adapters query provider APIs, push results here
 *   Lookups → ModelRegistry checked first, static map is fallback
 *
 * This is fully non-blocking. Discovery runs after the daemon is ready and
 * accepting requests. All I/O is async fetch() — no event loop blocking.
 *
 * Provider-scoped keys:
 *   Internal map key is `${provider}:${modelId}` to prevent last-writer-wins
 *   collisions when the same model exists on multiple providers (e.g.,
 *   meta-llama/Llama-3.1-8B-Instruct on Candle at 1400 tokens AND Together at 131072).
 *
 * Usage:
 *   const registry = ModelRegistry.sharedInstance();
 *   const ctx = registry.contextWindow('claude-sonnet-4-5-20250929');           // any provider
 *   const ctx = registry.contextWindow('meta-llama/Llama-3.1-8B-Instruct', 'candle');  // specific provider
 *
 * Future direction — Hardware-Matched Model Selection:
 *   ModelRegistry is designed to evolve into a queryable adapter catalog where
 *   models are matched to hardware, task design, and recipe requirements:
 *
 *   1. Each provider adapter reports hardware capabilities at discovery time:
 *      - GPU type/VRAM, quantization level, max batch size, measured TPS
 *      - This goes into ModelMetadata alongside contextWindow
 *
 *   2. Recipes/formulas declare model requirements:
 *      - minContextWindow, preferredSpeed, requiredCapabilities (vision, tools, etc.)
 *
 *   3. Selection query: "give me the best model for this recipe on this hardware"
 *      - Filters by capability, ranks by speed/quality/cost tradeoff
 *      - Works across local (Candle/Ollama) and cloud (REST APIs) uniformly
 *
 *   4. Users with varied hardware (M1 vs RTX 4090 vs cloud-only) get automatically
 *      matched to the best available model without manual configuration.
 *
 *   The provider-scoped key design (provider:modelId) already supports this —
 *   each adapter registers its models with hardware-specific metadata, and
 *   queries filter/sort across all registered providers.
 */

import type { ModelAdapterProfile } from './ModelCapabilities';

/**
 * Metadata for a discovered model
 */
export interface ModelMetadata {
  readonly modelId: string;
  readonly contextWindow: number;
  readonly maxOutputTokens?: number;
  readonly provider: string;
  readonly capabilities?: string[];
  readonly costPer1kTokens?: { input: number; output: number };
  readonly discoveredAt: number;

  /**
   * Fine-tuning, quantization, and adapter capability profile.
   * Populated by adapters that report detailed model capabilities.
   * Undefined for cloud APIs or adapters that haven't reported yet.
   */
  readonly adapterProfile?: ModelAdapterProfile;
}

/**
 * ModelRegistry — Singleton
 *
 * Provides fast lookup of model metadata discovered from provider APIs.
 * All normalization (date-suffix stripping, prefix matching) is built-in
 * so callers don't need to handle naming variations.
 *
 * Keys are provider-scoped: `${provider}:${modelId}` internally.
 * When provider is omitted from lookups, resolution strategy:
 *   - If only one provider has the model → return it
 *   - If multiple providers → return largest context window (cloud wins for backward compat)
 */
export class ModelRegistry {
  private static _instance: ModelRegistry;

  /** Primary index: `${provider}:${modelId}` → ModelMetadata */
  private _models: Map<string, ModelMetadata> = new Map();

  /** Secondary index: `modelId` → Set<provider> for fast unscoped lookups */
  private _modelProviders: Map<string, Set<string>> = new Map();

  private constructor() {}

  static sharedInstance(): ModelRegistry {
    if (!ModelRegistry._instance) {
      ModelRegistry._instance = new ModelRegistry();
    }
    return ModelRegistry._instance;
  }

  /**
   * Compose provider-scoped key
   */
  private static scopedKey(provider: string, modelId: string): string {
    return `${provider}:${modelId}`;
  }

  /**
   * Register a single model's metadata
   * Uses provider from metadata.provider to scope the key.
   */
  register(metadata: ModelMetadata): void {
    const key = ModelRegistry.scopedKey(metadata.provider, metadata.modelId);
    this._models.set(key, metadata);

    // Update secondary index
    let providers = this._modelProviders.get(metadata.modelId);
    if (!providers) {
      providers = new Set();
      this._modelProviders.set(metadata.modelId, providers);
    }
    providers.add(metadata.provider);
  }

  /**
   * Register a batch of models from adapter discovery
   */
  registerBatch(models: ModelMetadata[]): void {
    for (const model of models) {
      this.register(model);
    }
  }

  /**
   * Lookup context window for a model.
   * Returns undefined if the model is not in the registry (caller should fall back to static map).
   *
   * When provider is specified, only that provider's entry is checked.
   * When provider is omitted:
   *   - Single provider → return it
   *   - Multiple providers → return largest context window
   *
   * Normalization chain:
   * 1. Direct lookup by exact modelId (+ provider if given)
   * 2. Date-suffix stripped (e.g. 'claude-sonnet-4-5-20250929' → 'claude-sonnet-4-5')
   * 3. Prefix matching (e.g. 'claude-sonnet-4' matches 'claude-sonnet-4-5-20250929')
   */
  contextWindow(modelId: string, provider?: string): number | undefined {
    const metadata = this.get(modelId, provider);
    return metadata?.contextWindow;
  }

  /**
   * Lookup full metadata for a model.
   * Same normalization chain as contextWindow().
   *
   * When provider is specified, only returns that provider's entry.
   * When provider is omitted, resolves ambiguity by returning largest context window.
   */
  get(modelId: string, provider?: string): ModelMetadata | undefined {
    if (provider) {
      return this.getScopedWithNormalization(modelId, provider);
    }
    return this.getUnscopedWithNormalization(modelId);
  }

  /**
   * Get all registered entries for a model across all providers.
   * Useful for debugging provider collisions.
   */
  getAll(modelId: string): ModelMetadata[] {
    const results: ModelMetadata[] = [];

    // Direct match
    const providers = this._modelProviders.get(modelId);
    if (providers) {
      for (const p of providers) {
        const entry = this._models.get(ModelRegistry.scopedKey(p, modelId));
        if (entry) results.push(entry);
      }
    }

    if (results.length > 0) return results;

    // Date-suffix normalization
    const dateStripped = modelId.replace(/-\d{8}$/, '');
    if (dateStripped !== modelId) {
      const strippedProviders = this._modelProviders.get(dateStripped);
      if (strippedProviders) {
        for (const p of strippedProviders) {
          const entry = this._models.get(ModelRegistry.scopedKey(p, dateStripped));
          if (entry) results.push(entry);
        }
      }
      if (results.length > 0) return results;
    }

    // Prefix matching
    for (const [registeredId, registeredProviders] of this._modelProviders) {
      if (modelId.startsWith(registeredId) || registeredId.startsWith(modelId)) {
        for (const p of registeredProviders) {
          const entry = this._models.get(ModelRegistry.scopedKey(p, registeredId));
          if (entry) results.push(entry);
        }
      }
    }

    return results;
  }

  /**
   * Provider-scoped lookup with normalization chain.
   */
  private getScopedWithNormalization(modelId: string, provider: string): ModelMetadata | undefined {
    // 1. Direct
    const direct = this._models.get(ModelRegistry.scopedKey(provider, modelId));
    if (direct) return direct;

    // 2. Date-suffix
    const dateStripped = modelId.replace(/-\d{8}$/, '');
    if (dateStripped !== modelId) {
      const stripped = this._models.get(ModelRegistry.scopedKey(provider, dateStripped));
      if (stripped) return stripped;
    }

    // 3. Prefix matching (only within same provider)
    for (const [key, metadata] of this._models) {
      if (metadata.provider !== provider) continue;
      if (modelId.startsWith(metadata.modelId) || metadata.modelId.startsWith(modelId)) {
        return metadata;
      }
    }

    return undefined;
  }

  /**
   * Unscoped lookup with normalization chain.
   * When multiple providers have the same model, returns the one with largest context window.
   */
  private getUnscopedWithNormalization(modelId: string): ModelMetadata | undefined {
    const all = this.getAll(modelId);
    if (all.length === 0) return undefined;
    if (all.length === 1) return all[0];

    // Multiple providers — return largest context window (cloud models win for backward compat)
    console.log(`[ModelRegistry] Ambiguous lookup for "${modelId}": ${all.length} providers (${all.map(m => `${m.provider}:${m.contextWindow}`).join(', ')}). Returning largest context window.`);
    return all.reduce((best, current) =>
      current.contextWindow > best.contextWindow ? current : best
    );
  }

  /**
   * All registered models (read-only view)
   */
  get all(): ReadonlyMap<string, ModelMetadata> {
    return this._models;
  }

  /**
   * Number of models in the registry
   */
  get discoveredCount(): number {
    return this._models.size;
  }

  /**
   * Clear all discovered models (mainly for testing)
   */
  clear(): void {
    this._models.clear();
    this._modelProviders.clear();
  }
}
