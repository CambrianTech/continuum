/**
 * LayerComposer - Merge multiple LoRA layers
 * ===========================================
 *
 * Composes multiple LoRA adapter layers into a single merged adapter.
 * This allows stacking multiple fine-tuned behaviors on a base model.
 *
 * Composition strategies:
 * - weighted-merge: Linear combination with weights (default)
 * - sequential: Apply layers in sequence
 * - max-pooling: Take maximum values across layers
 *
 * Phase 2.2: Simple placeholder implementation
 * Phase 2.3: Actual LoRA merging with ML library
 */

import type { UUID } from '../../../system/core/types/JTAGTypes';
import type {
  LoadedLayer,
  WeightedLayer,
  CompositionOptions,
  CompositionResult,
  CompatibilityResult,
} from '../shared/GenomeAssemblyTypes';
import { GenomeAssemblyError, AssemblyErrorCode } from '../shared/GenomeAssemblyTypes';

/**
 * LayerComposer - Merge LoRA layers with various strategies
 */
export class LayerComposer {
  private compositionsPerformed: number = 0;
  private totalCompositionTimeMs: number = 0;

  constructor() {
    console.log(`🔀 LayerComposer initialized`);
  }

  /**
   * Compose multiple weighted layers into a single adapter
   *
   * Phase 2.2: Simple placeholder - returns first layer
   * Phase 2.3: Actual LoRA merging implementation
   *
   * @param layers Array of weighted layers to compose
   * @param options Composition options
   * @returns Composition result with merged layer
   */
  async compose(
    layers: WeightedLayer[],
    options?: CompositionOptions
  ): Promise<CompositionResult> {
    const startTime = Date.now();

    try {
      console.log(`🔀 Composing ${layers.length} layers...`);

      // Validate inputs
      if (layers.length === 0) {
        throw new GenomeAssemblyError(
          'Cannot compose empty layer array',
          AssemblyErrorCode.COMPOSITION_FAILED
        );
      }

      // Sort layers by ordering
      const sortedLayers = [...layers].sort((a, b) => a.ordering - b.ordering);

      // Log composition plan
      for (const weighted of sortedLayers) {
        console.log(
          `   Layer: ${weighted.layer.layerId} (weight=${weighted.weight}, order=${weighted.ordering})`
        );
      }

      // Validate compatibility if requested
      if (options?.validateCompatibility) {
        const compat = this.checkCompatibility(sortedLayers.map((w) => w.layer));
        if (!compat.compatible) {
          throw new GenomeAssemblyError(
            `Layers incompatible: ${compat.reason}`,
            AssemblyErrorCode.INCOMPATIBLE_LAYERS,
            undefined,
            { reason: compat.reason }
          );
        }
      }

      // Normalize weights if requested
      let normalizedLayers = sortedLayers;
      if (options?.normalizeWeights) {
        normalizedLayers = this.normalizeWeights(sortedLayers);
      }

      // Perform composition based on strategy
      const strategy = options?.strategy || 'weighted-merge';
      let composedLayer: LoadedLayer;

      switch (strategy) {
        case 'weighted-merge':
          composedLayer = await this.weightedMerge(normalizedLayers);
          break;
        case 'sequential':
          composedLayer = await this.sequentialMerge(normalizedLayers);
          break;
        default:
          throw new GenomeAssemblyError(
            `Unsupported composition strategy: ${strategy}`,
            AssemblyErrorCode.COMPOSITION_FAILED,
            undefined,
            { strategy }
          );
      }

      const compositionTimeMs = Date.now() - startTime;

      // Update statistics
      this.compositionsPerformed++;
      this.totalCompositionTimeMs += compositionTimeMs;

      console.log(`✅ Composition complete: ${compositionTimeMs}ms`);

      return {
        success: true,
        composedLayer,
        metrics: {
          layersComposed: layers.length,
          compositionTimeMs,
          resultSizeBytes: composedLayer.sizeBytes,
        },
      };
    } catch (error) {
      const compositionTimeMs = Date.now() - startTime;

      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        metrics: {
          layersComposed: 0,
          compositionTimeMs,
          resultSizeBytes: 0,
        },
      };
    }
  }

  /**
   * Check if layers are compatible for composition
   * @param layers Layers to check
   * @returns Compatibility result
   */
  checkCompatibility(layers: LoadedLayer[]): CompatibilityResult {
    if (layers.length === 0) {
      return {
        compatible: false,
        reason: 'Empty layer array',
      };
    }

    // Check all layers have same base model
    const baseModel = layers[0].metadata.baseModel;
    for (const layer of layers) {
      if (layer.metadata.baseModel !== baseModel) {
        return {
          compatible: false,
          reason: `Base model mismatch: ${layer.metadata.baseModel} != ${baseModel}`,
          suggestions: ['Use layers trained for the same base model'],
        };
      }
    }

    // Check all layers have same LoRA rank
    const rank = layers[0].config.r;
    for (const layer of layers) {
      if (layer.config.r !== rank) {
        return {
          compatible: false,
          reason: `LoRA rank mismatch: ${layer.config.r} != ${rank}`,
          suggestions: ['Use layers with matching LoRA rank (r parameter)'],
        };
      }
    }

    // Check all layers target same modules
    const targetModules = new Set(layers[0].config.target_modules);
    for (const layer of layers) {
      const layerModules = new Set(layer.config.target_modules);
      if (layerModules.size !== targetModules.size) {
        return {
          compatible: false,
          reason: 'Layers target different numbers of modules',
          suggestions: ['Use layers with matching target_modules'],
        };
      }
    }

    return { compatible: true };
  }

  /**
   * Get composition statistics
   */
  getStats(): {
    compositionsPerformed: number;
    avgCompositionTimeMs: number;
  } {
    return {
      compositionsPerformed: this.compositionsPerformed,
      avgCompositionTimeMs:
        this.compositionsPerformed > 0
          ? this.totalCompositionTimeMs / this.compositionsPerformed
          : 0,
    };
  }

  // ============================================================================
  // Private Composition Strategies
  // ============================================================================

  /**
   * Weighted merge strategy (linear combination)
   *
   * Phase 2.2: Placeholder - returns first layer with metadata update
   * Phase 2.3: Actual tensor merging
   */
  private async weightedMerge(layers: WeightedLayer[]): Promise<LoadedLayer> {
    console.log(`   Strategy: weighted-merge`);

    // Phase 2.2: Placeholder implementation
    // Just return first layer with updated metadata
    const firstLayer = layers[0].layer;

    // Create composed layer with combined metadata
    const composedLayer: LoadedLayer = {
      ...firstLayer,
      layerId: `composed-${Date.now()}` as UUID,
      metadata: {
        ...firstLayer.metadata,
        layerId: `composed-${Date.now()}` as UUID,
        name: `Composed Layer (${layers.length} layers)`,
        description: `Weighted merge of: ${layers.map((l) => l.layer.metadata.name).join(', ')}`,
      },
      loadedAt: Date.now(),
    };

    console.log(`   [PLACEHOLDER] Using first layer as composed result`);
    console.log(`   TODO Phase 2.3: Implement actual tensor merging`);

    return composedLayer;
  }

  /**
   * Sequential merge strategy (apply in order)
   *
   * Phase 2.2: Placeholder
   * Phase 2.3: Sequential adapter application
   */
  private async sequentialMerge(layers: WeightedLayer[]): Promise<LoadedLayer> {
    console.log(`   Strategy: sequential`);

    // Phase 2.2: Same placeholder as weighted merge
    return this.weightedMerge(layers);
  }

  /**
   * Normalize layer weights to sum to 1.0
   */
  private normalizeWeights(layers: WeightedLayer[]): WeightedLayer[] {
    const totalWeight = layers.reduce((sum, l) => sum + l.weight, 0);

    if (totalWeight === 0) {
      // All weights are zero, distribute evenly
      const evenWeight = 1.0 / layers.length;
      return layers.map((l) => ({ ...l, weight: evenWeight }));
    }

    // Normalize to sum to 1.0
    return layers.map((l) => ({
      ...l,
      weight: l.weight / totalWeight,
    }));
  }
}
