/**
 * GenomeAssembler - High-level orchestrator for genome loading
 * =============================================================
 *
 * The GenomeAssembler coordinates the entire process of loading and
 * assembling a PersonaUser's genome (stack of LoRA layers).
 *
 * Flow:
 * 1. Query database for GenomeEntity by ID
 * 2. Load each layer (using LayerLoader + LayerCache)
 * 3. Compose layers (using LayerComposer)
 * 4. Return AssembledGenome ready for inference worker
 *
 * This is the main entry point for genome loading in the system.
 */

import type { UUID } from '../../../system/core/types/JTAGTypes';
import type {
  AssembledGenome,
  WeightedLayer,
  AssemblyOptions,
  AssemblyResult,
  AssemblyStats,
  LoadedLayer,
} from '../shared/GenomeAssemblyTypes';
import { GenomeAssemblyError, AssemblyErrorCode } from '../shared/GenomeAssemblyTypes';
import { LayerLoader } from './LayerLoader';
import { LayerCache } from './LayerCache';
import { LayerComposer } from './LayerComposer';

// Placeholder for GenomeEntity type (will come from database)
interface GenomeEntity {
  id: UUID;
  name: string;
  baseModel: string;
  layers: {
    layerId: UUID;
    weight: number;
    ordering: number;
  }[];
}

/**
 * GenomeAssembler - Orchestrate genome loading and assembly
 */
export class GenomeAssembler {
  private loader: LayerLoader;
  private cache: LayerCache;
  private composer: LayerComposer;

  // Statistics
  private genomesAssembled: number = 0;
  private layersLoaded: number = 0;
  private totalAssemblyTimeMs: number = 0;
  private fastestAssemblyMs: number = Infinity;
  private slowestAssemblyMs: number = 0;
  private initTime: number;

  constructor(
    loader?: LayerLoader,
    cache?: LayerCache,
    composer?: LayerComposer
  ) {
    this.loader = loader || new LayerLoader();
    this.cache = cache || new LayerCache();
    this.composer = composer || new LayerComposer();
    this.initTime = Date.now();

    console.log(`🧬 GenomeAssembler initialized`);
  }

  /**
   * Load and assemble a complete genome
   *
   * This is the main entry point for genome loading.
   *
   * @param genomeId UUID of genome entity in database
   * @param options Assembly options
   * @returns Assembled genome ready for inference
   */
  async assembleGenome(
    genomeId: UUID,
    options?: AssemblyOptions
  ): Promise<AssembledGenome> {
    const startTime = Date.now();

    console.log(`\n🧬 Assembling genome: ${genomeId}`);

    try {
      // Step 1: Load genome entity from database
      const genomeEntity = await this.loadGenomeEntity(genomeId);
      console.log(`   Genome: ${genomeEntity.name}`);
      console.log(`   Base model: ${genomeEntity.baseModel}`);
      console.log(`   Layers: ${genomeEntity.layers.length}`);

      if (genomeEntity.layers.length === 0) {
        throw new GenomeAssemblyError(
          `Genome has no layers: ${genomeId}`,
          AssemblyErrorCode.INVALID_CONFIG,
          genomeId
        );
      }

      // Step 2: Load each layer (with caching)
      const weightedLayers: WeightedLayer[] = [];
      let cacheHits = 0;
      let cacheMisses = 0;

      for (const layerRef of genomeEntity.layers) {
        const { layerId, weight, ordering } = layerRef;

        // Check cache first
        let layer = this.cache.get(layerId);

        if (layer === null) {
          // Cache miss - load from disk
          cacheMisses++;
          layer = await this.loader.loadLayer(layerId, {
            validateChecksum: options?.validateChecksums,
          });

          // Store in cache (unless forceReload)
          if (!options?.forceReload) {
            this.cache.set(layerId, layer);
          }
        } else {
          // Cache hit
          cacheHits++;
        }

        weightedLayers.push({ layer, weight, ordering });
      }

      this.layersLoaded += weightedLayers.length;

      console.log(`   Cache performance: ${cacheHits} hits, ${cacheMisses} misses`);

      // Step 3: Compose layers
      const compositionResult = await this.composer.compose(weightedLayers, {
        strategy: 'weighted-merge',
        normalizeWeights: true,
        validateCompatibility: true,
      });

      if (!compositionResult.success || !compositionResult.composedLayer) {
        throw new GenomeAssemblyError(
          `Composition failed: ${compositionResult.error}`,
          AssemblyErrorCode.COMPOSITION_FAILED,
          genomeId
        );
      }

      // Step 4: Create assembled genome result
      const assemblyTimeMs = Date.now() - startTime;
      const totalSizeBytes = weightedLayers.reduce(
        (sum, wl) => sum + wl.layer.sizeBytes,
        0
      );

      const assembledGenome: AssembledGenome = {
        genomeId,
        baseModelId: genomeEntity.baseModel,
        composedLayer: compositionResult.composedLayer,
        layerCount: weightedLayers.length,
        totalSizeBytes,
        assemblyTimeMs,
        cacheHits,
        cacheMisses,
        sourceLayers: weightedLayers,
        assembledAt: Date.now(),
      };

      // Update statistics
      this.genomesAssembled++;
      this.totalAssemblyTimeMs += assemblyTimeMs;
      this.fastestAssemblyMs = Math.min(this.fastestAssemblyMs, assemblyTimeMs);
      this.slowestAssemblyMs = Math.max(this.slowestAssemblyMs, assemblyTimeMs);

      console.log(`✅ Genome assembled in ${assemblyTimeMs}ms`);
      console.log(`   Total size: ${this.formatBytes(totalSizeBytes)}`);
      console.log(`   Cache hit rate: ${((cacheHits / (cacheHits + cacheMisses)) * 100).toFixed(1)}%`);

      return assembledGenome;
    } catch (error) {
      const assemblyTimeMs = Date.now() - startTime;

      if (error instanceof GenomeAssemblyError) {
        throw error;
      }

      throw new GenomeAssemblyError(
        `Assembly failed: ${error instanceof Error ? error.message : String(error)}`,
        AssemblyErrorCode.COMPOSITION_FAILED,
        genomeId,
        { assemblyTimeMs }
      );
    }
  }

  /**
   * Preload a genome into cache (warm-up)
   *
   * Loads all layers into cache without assembling.
   * Useful for reducing latency on first use.
   *
   * @param genomeId UUID of genome to preload
   */
  async preloadGenome(genomeId: UUID): Promise<void> {
    console.log(`🔥 Preloading genome: ${genomeId}`);

    const genomeEntity = await this.loadGenomeEntity(genomeId);

    for (const layerRef of genomeEntity.layers) {
      const { layerId } = layerRef;

      // Check if already cached
      if (this.cache.has(layerId)) {
        console.log(`   ✓ ${layerId} (already cached)`);
        continue;
      }

      // Load and cache
      const layer = await this.loader.loadLayer(layerId);
      this.cache.set(layerId, layer);
      console.log(`   ✓ ${layerId} (loaded)`);
    }

    console.log(`✅ Preloaded ${genomeEntity.layers.length} layers`);
  }

  /**
   * Unload a genome from cache
   *
   * Evicts all layers belonging to this genome from cache.
   *
   * @param genomeId UUID of genome to unload
   */
  async unloadGenome(genomeId: UUID): Promise<void> {
    console.log(`🗑️  Unloading genome: ${genomeId}`);

    const genomeEntity = await this.loadGenomeEntity(genomeId);
    let evicted = 0;

    for (const layerRef of genomeEntity.layers) {
      const { layerId } = layerRef;

      if (this.cache.evict(layerId)) {
        evicted++;
      }
    }

    console.log(`✅ Unloaded ${evicted} layers from cache`);
  }

  /**
   * Get assembly statistics
   */
  getStats(): AssemblyStats {
    const cacheStats = this.cache.getStats();
    const loaderStats = this.loader.getStats();

    return {
      genomesAssembled: this.genomesAssembled,
      layersLoaded: this.layersLoaded,
      cacheHitRate: cacheStats.hitRate,
      avgAssemblyTimeMs:
        this.genomesAssembled > 0
          ? this.totalAssemblyTimeMs / this.genomesAssembled
          : 0,
      fastestAssemblyMs:
        this.fastestAssemblyMs === Infinity ? 0 : this.fastestAssemblyMs,
      slowestAssemblyMs: this.slowestAssemblyMs,
      totalBytesLoaded: loaderStats.bytesRead,
      uptimeMs: Date.now() - this.initTime,
    };
  }

  /**
   * Get cache statistics
   */
  getCacheStats() {
    return this.cache.getStats();
  }

  /**
   * Get loader statistics
   */
  getLoaderStats() {
    return this.loader.getStats();
  }

  /**
   * Get composer statistics
   */
  getComposerStats() {
    return this.composer.getStats();
  }

  // ============================================================================
  // Private Helper Methods
  // ============================================================================

  /**
   * Load genome entity from database
   *
   * Phase 2.2: Mock implementation
   * Phase 2.3: Actual database query
   */
  private async loadGenomeEntity(genomeId: UUID): Promise<GenomeEntity> {
    // Phase 2.2: Return mock genome entity
    // Phase 2.3: Query database
    console.log(`   [PLACEHOLDER] Loading genome entity from database`);

    // Mock genome with 3 layers
    const mockGenome: GenomeEntity = {
      id: genomeId,
      name: 'Mock Python Expert Genome',
      baseModel: 'meta-llama/Llama-3-8B',
      layers: [
        {
          layerId: 'test-layer-python-expert' as UUID,
          weight: 1.0,
          ordering: 0,
        },
        {
          layerId: 'test-layer-friendly-assistant' as UUID,
          weight: 0.8,
          ordering: 1,
        },
        {
          layerId: 'test-layer-code-reviewer' as UUID,
          weight: 0.5,
          ordering: 2,
        },
      ],
    };

    return mockGenome;
  }

  /**
   * Format bytes as human-readable string
   */
  private formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';

    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const k = 1024;
    const i = Math.floor(Math.log(bytes) / Math.log(k));

    return `${(bytes / Math.pow(k, i)).toFixed(2)} ${units[i]}`;
  }
}
