/**
 * PersonaGenome - Virtual memory-style LoRA adapter paging system
 *
 * Manages PersonaUser's LoRA adapters with LRU eviction (slingshot approach).
 * Architecture inspired by virtual memory management:
 * - Adapters are "pages" loaded from disk into GPU memory
 * - LRU eviction when memory budget exceeded
 * - Task-based adapter activation (domain-specific)
 * - Memory pressure tracking and graceful degradation
 *
 * Philosophy: "Treat adapters like pages in virtual memory"
 * - Load on demand
 * - Evict LRU when full
 * - Track fitness metrics
 * - Swap expensive, optimize for cache hits
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { GenomeEntity, GenomeLayerReference } from '../../../genome/entities/GenomeEntity';
import type { GenomeLayerEntity, TraitType } from '../../../genome/entities/GenomeLayerEntity';

/**
 * Loaded adapter - in-memory representation
 */
export interface LoadedAdapter {
  layerId: UUID;
  traitType: TraitType;
  layer: GenomeLayerEntity;
  loadedAt: number;  // Timestamp for LRU
  lastUsedAt: number;  // Timestamp for LRU
  memoryMB: number;  // Memory footprint
}

/**
 * Memory budget configuration
 */
export interface GenomeConfig {
  maxMemoryMB: number;  // Total GPU memory budget for adapters
  enableLogging: boolean;  // Debug logging
}

/**
 * Memory statistics
 */
export interface GenomeStats {
  totalAdaptersLoaded: number;
  currentMemoryUsageMB: number;
  memoryBudgetMB: number;
  cacheHits: number;
  cacheMisses: number;
  evictions: number;
  loadLatencyMs: number;  // Average
}

/**
 * PersonaGenome - LoRA adapter paging system
 */
export class PersonaGenome {
  private readonly personaId: UUID;
  private readonly personaName: string;
  private readonly config: GenomeConfig;

  // Active adapters (LRU cache)
  private activeAdapters: Map<UUID, LoadedAdapter> = new Map();

  // Genome reference (stack of layer IDs)
  private genome: GenomeEntity | null = null;

  // Current adapter (most recently activated)
  private currentAdapter: LoadedAdapter | null = null;

  // Stats tracking
  private stats: GenomeStats = {
    totalAdaptersLoaded: 0,
    currentMemoryUsageMB: 0,
    memoryBudgetMB: 0,
    cacheHits: 0,
    cacheMisses: 0,
    evictions: 0,
    loadLatencyMs: 0
  };

  private totalLoadLatency: number = 0;
  private loadCount: number = 0;

  constructor(
    personaId: UUID,
    personaName: string,
    config: Partial<GenomeConfig> = {}
  ) {
    this.personaId = personaId;
    this.personaName = personaName;
    this.config = {
      maxMemoryMB: config.maxMemoryMB ?? 512,  // Default 512MB GPU memory
      enableLogging: config.enableLogging ?? false
    };

    this.stats.memoryBudgetMB = this.config.maxMemoryMB;

    if (this.config.enableLogging) {
      console.log(`🧬 PersonaGenome: Initialized for ${this.personaName} (budget: ${this.config.maxMemoryMB}MB)`);
    }
  }

  /**
   * Set genome reference (stack of layers)
   */
  setGenome(genome: GenomeEntity): void {
    this.genome = genome;

    if (this.config.enableLogging) {
      console.log(`🧬 PersonaGenome: Loaded genome "${genome.name}" with ${genome.layers.length} layers`);
    }
  }

  /**
   * Activate adapter for specific trait/task
   * Uses LRU eviction if memory budget exceeded
   */
  async activateSkill(traitType: TraitType): Promise<void> {
    if (!this.genome) {
      throw new Error('PersonaGenome: No genome loaded, call setGenome() first');
    }

    // Find layer reference in genome stack
    const layerRef = this.findLayerByTrait(traitType);
    if (!layerRef) {
      throw new Error(`PersonaGenome: No layer found for trait "${traitType}"`);
    }

    // Check if already loaded (cache hit)
    if (this.activeAdapters.has(layerRef.layerId)) {
      const adapter = this.activeAdapters.get(layerRef.layerId)!;
      adapter.lastUsedAt = Date.now();
      this.currentAdapter = adapter;
      this.stats.cacheHits++;

      if (this.config.enableLogging) {
        console.log(`🧬 PersonaGenome: Cache hit for ${traitType} (layer ${layerRef.layerId.slice(0, 8)}...)`);
      }

      return;
    }

    // Cache miss - load from disk
    this.stats.cacheMisses++;

    // Evict LRU adapters if memory budget exceeded
    await this.ensureMemoryBudget(layerRef);

    // Load adapter from disk (simulated)
    const startTime = Date.now();
    const layer = await this.loadLayer(layerRef.layerId);
    const loadLatency = Date.now() - startTime;

    this.totalLoadLatency += loadLatency;
    this.loadCount++;
    this.stats.loadLatencyMs = this.totalLoadLatency / this.loadCount;

    const loaded: LoadedAdapter = {
      layerId: layerRef.layerId,
      traitType: layerRef.traitType,
      layer,
      loadedAt: Date.now(),
      lastUsedAt: Date.now(),
      memoryMB: layer.sizeMB
    };

    this.activeAdapters.set(layerRef.layerId, loaded);
    this.currentAdapter = loaded;
    this.stats.currentMemoryUsageMB += layer.sizeMB;
    this.stats.totalAdaptersLoaded++;

    if (this.config.enableLogging) {
      console.log(`🧬 PersonaGenome: Loaded ${traitType} (${layer.sizeMB}MB, ${loadLatency}ms)`);
    }
  }

  /**
   * Get current active adapter
   */
  getCurrentAdapter(): LoadedAdapter | null {
    return this.currentAdapter;
  }

  /**
   * Get all active adapters
   */
  getActiveAdapters(): LoadedAdapter[] {
    return Array.from(this.activeAdapters.values());
  }

  /**
   * Get memory statistics
   */
  getStats(): GenomeStats {
    return { ...this.stats };
  }

  /**
   * Clear all loaded adapters (reset)
   */
  async unloadAll(): Promise<void> {
    this.activeAdapters.clear();
    this.currentAdapter = null;
    this.stats.currentMemoryUsageMB = 0;

    if (this.config.enableLogging) {
      console.log(`🧬 PersonaGenome: Unloaded all adapters`);
    }
  }

  /**
   * Check if memory budget allows loading new adapter
   * Evict LRU adapters if necessary
   */
  private async ensureMemoryBudget(layerRef: GenomeLayerReference): Promise<void> {
    // Estimate size (we don't have actual layer yet, use average or genome metadata)
    const estimatedSizeMB = 50;  // TODO: Get from genome metadata or layer reference

    while (
      this.stats.currentMemoryUsageMB + estimatedSizeMB > this.config.maxMemoryMB &&
      this.activeAdapters.size > 0
    ) {
      await this.evictLRU();
    }
  }

  /**
   * Evict least recently used adapter
   */
  private async evictLRU(): Promise<void> {
    if (this.activeAdapters.size === 0) {
      return;
    }

    // Find LRU adapter
    let lruAdapter: LoadedAdapter | null = null;
    let oldestTime = Date.now();

    for (const adapter of this.activeAdapters.values()) {
      if (adapter.lastUsedAt < oldestTime) {
        oldestTime = adapter.lastUsedAt;
        lruAdapter = adapter;
      }
    }

    if (!lruAdapter) {
      return;
    }

    // Evict
    this.activeAdapters.delete(lruAdapter.layerId);
    this.stats.currentMemoryUsageMB -= lruAdapter.memoryMB;
    this.stats.evictions++;

    // Clear current adapter if it was evicted
    if (this.currentAdapter?.layerId === lruAdapter.layerId) {
      this.currentAdapter = null;
    }

    if (this.config.enableLogging) {
      console.log(`🧬 PersonaGenome: Evicted LRU adapter ${lruAdapter.traitType} (freed ${lruAdapter.memoryMB}MB)`);
    }
  }

  /**
   * Find layer reference by trait type in genome stack
   * Returns highest priority (last in stack) layer for trait
   */
  private findLayerByTrait(traitType: TraitType): GenomeLayerReference | null {
    if (!this.genome) {
      return null;
    }

    // Search from end of stack (higher priority)
    for (let i = this.genome.layers.length - 1; i >= 0; i--) {
      const layer = this.genome.layers[i];
      if (layer.traitType === traitType && layer.enabled) {
        return layer;
      }
    }

    return null;
  }

  /**
   * Load layer from disk (simulated)
   * In production: Load actual LoRA weights from filesystem
   */
  private async loadLayer(layerId: UUID): Promise<GenomeLayerEntity> {
    // TODO: Actual implementation would load from DataDaemon
    // For now, create mock layer
    const { GenomeLayerEntity } = await import('../../../genome/entities/GenomeLayerEntity');
    const layer = new GenomeLayerEntity();
    layer.id = layerId;
    layer.name = `Mock Layer ${layerId.slice(0, 8)}`;
    layer.traitType = 'domain_expertise';
    layer.source = 'trained';
    layer.modelPath = `/path/to/adapters/${layerId}.safetensors`;
    layer.sizeMB = 50;  // Mock size
    layer.rank = 16;

    // Simulate disk I/O latency (10-50ms)
    await new Promise(resolve => setTimeout(resolve, Math.random() * 40 + 10));

    return layer;
  }
}
