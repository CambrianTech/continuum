/**
 * Genome Entity - Complete persona genome (LoRA layer stack)
 *
 * A genome is a stack of GenomeLayer references that define a persona's capabilities.
 * Genomes are:
 * - Composed of 0..N layers (stackable)
 * - Deterministically applied (order matters)
 * - Reusable via cosine similarity matching
 * - Evolvable through breeding/recombination
 * - Shareable across P2P mesh
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  JsonField,
  ForeignKeyField,
  TEXT_LENGTH
} from '../../data/decorators/FieldDecorators';
import { BaseEntity } from '../../data/entities/BaseEntity';
import type { TraitType } from './GenomeLayerEntity';

/**
 * Reference to a genome layer with stack position and weight
 */
export interface GenomeLayerReference {
  layerId: UUID;                // Reference to GenomeLayerEntity
  traitType: TraitType;         // Cached for filtering/searching
  orderIndex: number;           // Position in stack (0 = base, higher = override)
  weight: number;               // Layer influence (0-1, default 1.0)
  enabled: boolean;             // Runtime toggle (for A/B testing)
}

/**
 * Genome metadata for tracking evolution
 */
export interface GenomeMetadata {
  generation: number;           // Evolutionary generation (0 = original)
  parentGenomeIds?: UUID[];     // If created via breeding
  createdVia: 'manual' | 'trained' | 'bred' | 'refined';
  trainingDuration?: number;    // Total training time in milliseconds
  lastModified: Date;
}

/**
 * Genome fitness metrics (aggregate of all layers)
 */
export interface GenomeFitness {
  overallAccuracy: number;      // Average layer accuracy
  totalParameters: number;      // Sum of all layer parameters
  totalSizeMB: number;          // Total memory footprint
  averageLatency: number;       // Average inference time
  usageCount: number;           // Times this genome was used
  successRate: number;          // Overall success rate
}

/**
 * GenomeEntity - Complete persona genome
 */
export class GenomeEntity extends BaseEntity {
  static readonly collection = 'genomes';

  @TextField({ index: true })
  name: string;

  @TextField({ maxLength: TEXT_LENGTH.MEDIUM })
  description: string;

  @ForeignKeyField({ references: 'users.id', index: true })
  personaId: UUID;  // Which PersonaUser owns this genome

  @TextField()
  baseModel: string;  // Base model name (e.g., "llama-3.1-8B")

  // Layer stack - the core of the genome
  @JsonField()
  layers: GenomeLayerReference[];

  // Composite embedding (average of all layer embeddings)
  // Used for genome-level similarity matching
  @JsonField()
  compositeEmbedding: number[];

  // Metadata & Provenance
  @JsonField()
  metadata: GenomeMetadata;

  // Fitness & Performance
  @JsonField()
  fitness: GenomeFitness;

  // Tags for searching
  @JsonField()
  tags: string[];

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super();

    this.name = '';
    this.description = '';
    this.personaId = '' as UUID;
    this.baseModel = '';
    this.layers = [];
    this.compositeEmbedding = Array(768).fill(0);
    this.metadata = {
      generation: 0,
      createdVia: 'manual',
      lastModified: new Date()
    };
    this.fitness = {
      overallAccuracy: 0,
      totalParameters: 0,
      totalSizeMB: 0,
      averageLatency: 0,
      usageCount: 0,
      successRate: 0
    };
    this.tags = [];
  }

  get collection(): string {
    return GenomeEntity.collection;
  }

  /**
   * Validate genome data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.name?.trim()) {
      return { success: false, error: 'Genome name is required' };
    }

    if (this.name.length > 200) {
      return { success: false, error: 'Genome name must be 200 characters or less' };
    }

    if (!this.personaId?.trim()) {
      return { success: false, error: 'Genome personaId is required' };
    }

    if (!this.baseModel?.trim()) {
      return { success: false, error: 'Genome baseModel is required' };
    }

    if (!Array.isArray(this.layers)) {
      return { success: false, error: 'Genome layers must be an array' };
    }

    // Validate each layer reference
    for (let i = 0; i < this.layers.length; i++) {
      const layer = this.layers[i];
      if (!layer.layerId) {
        return { success: false, error: `Layer ${i} missing layerId` };
      }
      if (typeof layer.orderIndex !== 'number') {
        return { success: false, error: `Layer ${i} missing orderIndex` };
      }
      if (typeof layer.weight !== 'number' || layer.weight < 0 || layer.weight > 1) {
        return { success: false, error: `Layer ${i} weight must be between 0 and 1` };
      }
    }

    if (!Array.isArray(this.compositeEmbedding) || this.compositeEmbedding.length !== 768) {
      return { success: false, error: 'Genome compositeEmbedding must be 768-dimensional array' };
    }

    return { success: true };
  }

  /**
   * Add layer to genome stack
   */
  addLayer(layerRef: Omit<GenomeLayerReference, 'orderIndex'>): void {
    const orderIndex = this.layers.length;
    this.layers.push({
      ...layerRef,
      orderIndex
    });
  }

  /**
   * Remove layer from genome stack
   */
  removeLayer(layerId: UUID): boolean {
    const index = this.layers.findIndex(l => l.layerId === layerId);
    if (index === -1) return false;

    this.layers.splice(index, 1);

    // Reindex remaining layers
    this.layers.forEach((layer, i) => {
      layer.orderIndex = i;
    });

    return true;
  }

  /**
   * Reorder layers in stack
   */
  reorderLayers(layerIds: UUID[]): void {
    const newLayers: GenomeLayerReference[] = [];

    // Build new order based on provided IDs
    for (let i = 0; i < layerIds.length; i++) {
      const layer = this.layers.find(l => l.layerId === layerIds[i]);
      if (layer) {
        newLayers.push({
          ...layer,
          orderIndex: i
        });
      }
    }

    this.layers = newLayers;
  }

  /**
   * Get layers by trait type
   */
  getLayersByTrait(traitType: TraitType): GenomeLayerReference[] {
    return this.layers.filter(l => l.traitType === traitType);
  }

  /**
   * Get enabled layers only (for runtime execution)
   */
  getEnabledLayers(): GenomeLayerReference[] {
    return this.layers.filter(l => l.enabled);
  }

  /**
   * Calculate composite embedding from layer embeddings
   * (Requires loading actual GenomeLayerEntity instances)
   */
  static calculateCompositeEmbedding(layerEmbeddings: number[][]): number[] {
    if (layerEmbeddings.length === 0) {
      return Array(768).fill(0);
    }

    const composite = Array(768).fill(0);

    // Average all layer embeddings
    for (const embedding of layerEmbeddings) {
      for (let i = 0; i < 768; i++) {
        composite[i] += embedding[i];
      }
    }

    for (let i = 0; i < 768; i++) {
      composite[i] /= layerEmbeddings.length;
    }

    return composite;
  }

  /**
   * Update fitness metrics after inference
   */
  updateFitness(result: {
    success: boolean;
    latency: number;
  }): void {
    this.fitness.usageCount++;

    if (result.success) {
      const alpha = 0.1;
      this.fitness.successRate = (
        alpha * 1 + (1 - alpha) * this.fitness.successRate
      );
    } else {
      this.fitness.successRate = (
        0.1 * 0 + 0.9 * this.fitness.successRate
      );
    }

    const n = this.fitness.usageCount;
    this.fitness.averageLatency = (
      (this.fitness.averageLatency * (n - 1) + result.latency) / n
    );
  }
}
