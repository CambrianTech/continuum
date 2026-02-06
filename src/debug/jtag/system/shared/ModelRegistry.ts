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
 * Usage:
 *   const registry = ModelRegistry.sharedInstance();
 *   const ctx = registry.contextWindow('claude-sonnet-4-5-20250929');
 *   // Returns 200000 if discovered, undefined if not (caller falls back to static)
 */

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
}

/**
 * ModelRegistry — Singleton
 *
 * Provides fast lookup of model metadata discovered from provider APIs.
 * All normalization (date-suffix stripping, prefix matching) is built-in
 * so callers don't need to handle naming variations.
 */
export class ModelRegistry {
  private static _instance: ModelRegistry;
  private _models: Map<string, ModelMetadata> = new Map();

  private constructor() {}

  static sharedInstance(): ModelRegistry {
    if (!ModelRegistry._instance) {
      ModelRegistry._instance = new ModelRegistry();
    }
    return ModelRegistry._instance;
  }

  /**
   * Register a single model's metadata (overwrites if already present)
   */
  register(metadata: ModelMetadata): void {
    this._models.set(metadata.modelId, metadata);
  }

  /**
   * Register a batch of models from adapter discovery
   */
  registerBatch(models: ModelMetadata[]): void {
    for (const model of models) {
      this._models.set(model.modelId, model);
    }
  }

  /**
   * Lookup context window for a model.
   * Returns undefined if the model is not in the registry (caller should fall back to static map).
   *
   * Normalization chain:
   * 1. Direct lookup by exact modelId
   * 2. Date-suffix stripped (e.g. 'claude-sonnet-4-5-20250929' → 'claude-sonnet-4-5')
   * 3. Prefix matching (e.g. 'claude-sonnet-4' matches 'claude-sonnet-4-5-20250929')
   */
  contextWindow(modelId: string): number | undefined {
    // 1. Direct lookup
    const direct = this._models.get(modelId);
    if (direct) return direct.contextWindow;

    // 2. Date-suffix normalization
    const dateStripped = modelId.replace(/-\d{8}$/, '');
    if (dateStripped !== modelId) {
      const stripped = this._models.get(dateStripped);
      if (stripped) return stripped.contextWindow;
    }

    // 3. Prefix matching — check if any registered model starts with or is started by this ID
    for (const [registeredId, metadata] of this._models) {
      if (modelId.startsWith(registeredId) || registeredId.startsWith(modelId)) {
        return metadata.contextWindow;
      }
    }

    return undefined;
  }

  /**
   * Lookup full metadata for a model.
   * Same normalization chain as contextWindow().
   */
  get(modelId: string): ModelMetadata | undefined {
    // Direct
    const direct = this._models.get(modelId);
    if (direct) return direct;

    // Date-suffix
    const dateStripped = modelId.replace(/-\d{8}$/, '');
    if (dateStripped !== modelId) {
      const stripped = this._models.get(dateStripped);
      if (stripped) return stripped;
    }

    // Prefix matching
    for (const [registeredId, metadata] of this._models) {
      if (modelId.startsWith(registeredId) || registeredId.startsWith(modelId)) {
        return metadata;
      }
    }

    return undefined;
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
  }
}
