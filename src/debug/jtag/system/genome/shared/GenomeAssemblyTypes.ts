/**
 * Genome Assembly Types
 * ======================
 *
 * Type definitions for LoRA layer loading, caching, and composition.
 * These types support the dynamic genome assembly system that allows
 * PersonaUsers to have customized, fine-tuned AI models.
 *
 * Key concepts:
 * - LoadedLayer: Raw LoRA adapter loaded from disk
 * - WeightedLayer: Layer with importance weight for composition
 * - AssembledGenome: Final composed adapter ready for inference
 * - LayerCache: LRU cache for performance
 */

import type { UUID } from '../../../system/core/types/JTAGTypes';

/** Unix timestamp in milliseconds */
type Timestamp = number;

// ============================================================================
// LoRA Layer Types
// ============================================================================

/**
 * LoRA configuration from adapter_config.json
 * Standard format used by HuggingFace PEFT library
 */
export interface LoRAConfig {
  /** Rank - dimensionality of adapter matrices (lower = smaller/faster) */
  r: number;

  /** Scaling factor for adapter weights */
  lora_alpha: number;

  /** Model layers to adapt (e.g., ["q_proj", "v_proj", "k_proj"]) */
  target_modules: string[];

  /** Dropout probability during training */
  lora_dropout: number;

  /** Bias handling strategy */
  bias: 'none' | 'all' | 'lora_only';

  /** Task type (optional) */
  task_type?: string;

  /** Inference mode flag */
  inference_mode?: boolean;
}

/**
 * Layer metadata from metadata.json
 * Additional info not in LoRA config
 */
export interface LayerMetadata {
  /** Unique layer identifier */
  layerId: UUID;

  /** Human-readable name */
  name: string;

  /** Description of layer's purpose/training */
  description: string;

  /** Base model this layer was trained for */
  baseModel: string;

  /** Training date */
  trainedOn: string;

  /** File size in bytes */
  sizeBytes: number;

  /** SHA-256 checksum for integrity */
  checksum: string;

  /** File format */
  format: 'safetensors' | 'pytorch' | 'gguf';

  /** Training dataset (optional) */
  dataset?: string;

  /** Training hyperparameters (optional) */
  trainingParams?: {
    epochs: number;
    learning_rate: number;
    batch_size: number;
  };
}

/**
 * Raw LoRA layer loaded from disk
 * Contains both configuration and actual adapter weights
 */
export interface LoadedLayer {
  /** Unique layer identifier */
  layerId: UUID;

  /** LoRA configuration */
  config: LoRAConfig;

  /** Layer metadata */
  metadata: LayerMetadata;

  /** Actual adapter weights (binary data) */
  weights: Buffer;

  /** Size in bytes (for cache management) */
  sizeBytes: number;

  /** File format */
  format: 'safetensors' | 'pytorch' | 'gguf';

  /** When layer was loaded into memory */
  loadedAt: Timestamp;
}

/**
 * Layer with weight for composition
 * Used when stacking multiple LoRA adapters
 */
export interface WeightedLayer {
  /** The loaded layer */
  layer: LoadedLayer;

  /** Importance weight (0.0 - 1.0) */
  weight: number;

  /** Stack ordering (lower = applied first) */
  ordering: number;

  /** Optional label for debugging */
  label?: string;
}

/**
 * Composed genome ready for inference
 * Multiple LoRA layers merged into single adapter
 */
export interface AssembledGenome {
  /** Unique genome identifier */
  genomeId: UUID;

  /** Base model name (e.g., "meta-llama/Llama-3-8B") */
  baseModelId: string;

  /** Merged adapter layer */
  composedLayer: LoadedLayer;

  /** Number of layers that were composed */
  layerCount: number;

  /** Total size of all layers in bytes */
  totalSizeBytes: number;

  /** Time taken to assemble (ms) */
  assemblyTimeMs: number;

  /** Number of cache hits during assembly */
  cacheHits: number;

  /** Number of cache misses (had to load from disk) */
  cacheMisses: number;

  /** Individual layers that were composed */
  sourceLayers: WeightedLayer[];

  /** When genome was assembled */
  assembledAt: Timestamp;
}

// ============================================================================
// Cache Types
// ============================================================================

/**
 * Cache entry with metadata
 * Tracks usage for LRU eviction
 */
export interface CacheEntry {
  /** Layer identifier */
  layerId: UUID;

  /** The cached layer */
  layer: LoadedLayer;

  /** Last access timestamp */
  lastAccessed: Timestamp;

  /** Total access count */
  accessCount: number;

  /** Size in bytes (for cache limits) */
  sizeBytes: number;

  /** When entry was added to cache */
  cachedAt: Timestamp;
}

/**
 * Cache statistics
 * For monitoring and optimization
 */
export interface CacheStats {
  /** Number of entries in cache */
  entries: number;

  /** Total size of cached data (bytes) */
  totalSizeBytes: number;

  /** Cache hit rate (0.0 - 1.0) */
  hitRate: number;

  /** Total cache hits */
  hits: number;

  /** Total cache misses */
  misses: number;

  /** Number of evictions performed */
  evictionCount: number;

  /** Maximum cache size (bytes) */
  maxSizeBytes: number;

  /** Current utilization (0.0 - 1.0) */
  utilization: number;
}

/**
 * Cache configuration
 */
export interface CacheConfig {
  /** Maximum cache size in bytes (default: 4GB) */
  maxSizeBytes: number;

  /** Maximum number of entries (optional) */
  maxEntries?: number;

  /** Enable statistics tracking */
  trackStats: boolean;

  /** Eviction strategy */
  evictionStrategy: 'lru' | 'lfu' | 'size-weighted';
}

// ============================================================================
// Assembly Types
// ============================================================================

/**
 * Assembly statistics
 * Tracks overall genome assembly performance
 */
export interface AssemblyStats {
  /** Total genomes assembled */
  genomesAssembled: number;

  /** Total layers loaded */
  layersLoaded: number;

  /** Cache hit rate across all assemblies */
  cacheHitRate: number;

  /** Average assembly time (ms) */
  avgAssemblyTimeMs: number;

  /** Fastest assembly (ms) */
  fastestAssemblyMs: number;

  /** Slowest assembly (ms) */
  slowestAssemblyMs: number;

  /** Total bytes loaded from disk */
  totalBytesLoaded: number;

  /** Uptime since assembler initialized */
  uptimeMs: number;
}

/**
 * Assembly request options
 */
export interface AssemblyOptions {
  /** Skip cache and force reload from disk */
  forceReload?: boolean;

  /** Preload into cache without assembling */
  preloadOnly?: boolean;

  /** Validate layer checksums */
  validateChecksums?: boolean;

  /** Timeout for assembly (ms) */
  timeoutMs?: number;
}

/**
 * Assembly result with detailed metrics
 */
export interface AssemblyResult {
  /** Success flag */
  success: boolean;

  /** Assembled genome (if successful) */
  genome?: AssembledGenome;

  /** Error message (if failed) */
  error?: string;

  /** Detailed metrics */
  metrics: {
    layersRequested: number;
    layersLoaded: number;
    cacheHits: number;
    cacheMisses: number;
    diskReadTimeMs: number;
    compositionTimeMs: number;
    totalTimeMs: number;
  };
}

// ============================================================================
// Loader Types
// ============================================================================

/**
 * Layer loading options
 */
export interface LoaderOptions {
  /** Validate file checksum */
  validateChecksum?: boolean;

  /** Skip weights, load only metadata */
  metadataOnly?: boolean;

  /** Timeout for load operation (ms) */
  timeoutMs?: number;
}

/**
 * Loader statistics
 */
export interface LoaderStats {
  /** Total layers loaded */
  layersLoaded: number;

  /** Total bytes read from disk */
  bytesRead: number;

  /** Average load time (ms) */
  avgLoadTimeMs: number;

  /** Failed load attempts */
  failures: number;

  /** Checksum validation failures */
  checksumFailures: number;
}

// ============================================================================
// Composer Types
// ============================================================================

/**
 * Composition strategy
 */
export type CompositionStrategy =
  | 'weighted-merge'      // Linear combination with weights
  | 'sequential'          // Apply layers in sequence
  | 'max-pooling'         // Take maximum values across layers
  | 'learned-merge';      // Use learned combination weights

/**
 * Composition options
 */
export interface CompositionOptions {
  /** Composition strategy */
  strategy: CompositionStrategy;

  /** Normalize weights to sum to 1.0 */
  normalizeWeights?: boolean;

  /** Validate layer compatibility */
  validateCompatibility?: boolean;

  /** Timeout for composition (ms) */
  timeoutMs?: number;
}

/**
 * Composition result
 */
export interface CompositionResult {
  /** Success flag */
  success: boolean;

  /** Composed layer (if successful) */
  composedLayer?: LoadedLayer;

  /** Error message (if failed) */
  error?: string;

  /** Composition metrics */
  metrics: {
    layersComposed: number;
    compositionTimeMs: number;
    resultSizeBytes: number;
  };
}

// ============================================================================
// Validation Types
// ============================================================================

/**
 * Layer validation result
 */
export interface ValidationResult {
  /** Is layer valid? */
  valid: boolean;

  /** Validation errors */
  errors: string[];

  /** Validation warnings */
  warnings: string[];
}

/**
 * Compatibility check result
 */
export interface CompatibilityResult {
  /** Are layers compatible? */
  compatible: boolean;

  /** Reason for incompatibility */
  reason?: string;

  /** Suggested fixes */
  suggestions?: string[];
}

// ============================================================================
// Error Types
// ============================================================================

/**
 * Genome assembly error codes
 */
export enum AssemblyErrorCode {
  LAYER_NOT_FOUND = 'LAYER_NOT_FOUND',
  CHECKSUM_MISMATCH = 'CHECKSUM_MISMATCH',
  INCOMPATIBLE_LAYERS = 'INCOMPATIBLE_LAYERS',
  LOAD_TIMEOUT = 'LOAD_TIMEOUT',
  COMPOSITION_FAILED = 'COMPOSITION_FAILED',
  CACHE_FULL = 'CACHE_FULL',
  INVALID_CONFIG = 'INVALID_CONFIG',
}

/**
 * Custom error for genome assembly failures
 */
export class GenomeAssemblyError extends Error {
  constructor(
    message: string,
    public readonly code: AssemblyErrorCode,
    public readonly layerId?: UUID,
    public readonly details?: Record<string, unknown>
  ) {
    super(message);
    this.name = 'GenomeAssemblyError';
  }
}
