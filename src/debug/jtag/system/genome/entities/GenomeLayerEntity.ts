/**
 * Genome Layer Entity - Individual LoRA adaptation layer
 *
 * Each layer represents a specific trait or capability that can be:
 * - Inherited from parent personas
 * - Trained via Academy
 * - Refined through additional training
 * - Shared across P2P mesh
 * - Reused via cosine similarity matching
 *
 * Layers are stackable - a PersonaUser's genome is a stack of these layers
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  EnumField,
  JsonField,
  ForeignKeyField,
  TEXT_LENGTH
} from '../../data/decorators/FieldDecorators';
import { BaseEntity } from '../../data/entities/BaseEntity';

/**
 * Trait type - free-entry string for evolutionary flexibility
 *
 * Common predefined traits (for convenience, not enforced):
 * - 'tone_and_voice' - Communication style (friendly, formal, etc.)
 * - 'ethical_reasoning' - Moral framework and safety constraints
 * - 'domain_expertise' - Specialized knowledge (biomech, legal, etc.)
 * - 'cultural_knowledge' - Cultural context and awareness
 * - 'memory_integration' - How persona uses RAG/memories
 * - 'reasoning_style' - Problem-solving approach
 * - 'creative_expression' - Artistic/creative capabilities
 * - 'social_dynamics' - Interpersonal interaction patterns
 *
 * But ANY string is valid - the ecosystem defines useful traits through natural selection
 */
export type TraitType = string;

/**
 * Layer source - how was this layer created?
 */
export type LayerSource =
  | 'inherited'    // Copied from parent persona (via breeding)
  | 'trained'      // Created via Academy training from scratch
  | 'refined'      // Existing layer improved via Academy
  | 'downloaded'   // Retrieved from P2P mesh
  | 'system';      // Pre-bundled foundational layer

/**
 * Training metadata for provenance tracking
 */
export interface TrainingMetadata {
  curriculumId?: UUID;         // Academy curriculum used
  datasetHash?: string;        // Training data fingerprint
  epochs: number;              // Training iterations
  loss: number;                // Final training loss
  performance: number;         // Benchmark score (0-1)
  trainingDuration: number;    // Milliseconds
  checkpoints?: string[];      // Saved checkpoint paths
}

/**
 * Fitness metrics for natural selection
 */
export interface LayerFitness {
  accuracy: number;            // Performance on benchmarks (0-1)
  efficiency: number;          // Parameters vs performance ratio
  usageCount: number;          // How many times used
  successRate: number;         // Success rate in inference (0-1)
  averageLatency: number;      // Milliseconds
  cacheHitRate: number;        // How often cached vs loaded (0-1)
}

/**
 * GenomeLayer Entity - Individual LoRA adaptation layer
 */
export class GenomeLayerEntity extends BaseEntity {
  static readonly collection = 'genome_layers';

  @TextField({ index: true })
  name: string;

  @TextField({ maxLength: TEXT_LENGTH.MEDIUM })
  description: string;

  @EnumField({ index: true })
  traitType: TraitType;

  @EnumField({ index: true })
  source: LayerSource;

  // Storage & Size
  @TextField()
  modelPath: string;  // Path to LoRA weights file

  @NumberField()
  sizeMB: number;  // File size for caching decisions

  @NumberField()
  rank: number;  // LoRA rank (8, 16, 32, etc.)

  // Searchability - 768-dim embedding for cosine similarity
  @JsonField()
  embedding: number[];

  @JsonField()
  tags: string[];  // Searchable keywords

  // Provenance & Training
  @ForeignKeyField({ references: 'users.id', nullable: true })
  creatorId?: UUID;  // Who created this layer

  @JsonField()
  trainingMetadata?: TrainingMetadata;

  // Fitness & Performance
  @JsonField()
  fitness: LayerFitness;

  // Versioning for layer evolution
  @ForeignKeyField({ references: 'genome_layers.id', nullable: true })
  parentLayerId?: UUID;  // If refined from existing layer

  @NumberField()
  generation: number;  // Evolutionary generation (0 = original)

  // P2P Distribution
  @TextField({ nullable: true })
  contentHash?: string;  // SHA-256 for integrity verification

  @TextField({ nullable: true })
  signature?: string;  // Ed25519 signature for trust

  @TextField({ nullable: true })
  license?: string;  // License type (MIT, GPL, proprietary)

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super();

    this.name = '';
    this.description = '';
    this.traitType = 'domain_expertise';
    this.source = 'trained';
    this.modelPath = '';
    this.sizeMB = 0;
    this.rank = 16;  // Default LoRA rank
    this.embedding = Array(768).fill(0);  // 768-dim zero vector
    this.tags = [];
    this.fitness = {
      accuracy: 0,
      efficiency: 0,
      usageCount: 0,
      successRate: 0,
      averageLatency: 0,
      cacheHitRate: 0
    };
    this.generation = 0;
  }

  get collection(): string {
    return GenomeLayerEntity.collection;
  }

  /**
   * Validate layer data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.name?.trim()) {
      return { success: false, error: 'Layer name is required' };
    }

    if (this.name.length > 200) {
      return { success: false, error: 'Layer name must be 200 characters or less' };
    }

    if (!this.modelPath?.trim()) {
      return { success: false, error: 'Layer modelPath is required' };
    }

    if (this.sizeMB <= 0) {
      return { success: false, error: 'Layer sizeMB must be positive' };
    }

    if (this.rank <= 0 || this.rank > 128) {
      return { success: false, error: 'Layer rank must be between 1 and 128' };
    }

    if (!Array.isArray(this.embedding) || this.embedding.length !== 768) {
      return { success: false, error: 'Layer embedding must be 768-dimensional array' };
    }

    // TraitType is free-entry (any string) - just validate it exists
    if (!this.traitType?.trim()) {
      return { success: false, error: 'Layer traitType is required (can be any string)' };
    }

    const validSources: LayerSource[] = ['inherited', 'trained', 'refined', 'downloaded', 'system'];
    if (!validSources.includes(this.source)) {
      return { success: false, error: `Layer source must be one of: ${validSources.join(', ')}` };
    }

    return { success: true };
  }

  /**
   * Calculate cosine similarity with another layer's embedding
   */
  calculateSimilarity(other: GenomeLayerEntity): number {
    if (this.embedding.length !== other.embedding.length) {
      throw new Error('Embedding dimensions must match for similarity calculation');
    }

    let dotProduct = 0;
    let normA = 0;
    let normB = 0;

    for (let i = 0; i < this.embedding.length; i++) {
      dotProduct += this.embedding[i] * other.embedding[i];
      normA += this.embedding[i] * this.embedding[i];
      normB += other.embedding[i] * other.embedding[i];
    }

    const magnitude = Math.sqrt(normA) * Math.sqrt(normB);
    if (magnitude === 0) return 0;

    return dotProduct / magnitude;
  }

  /**
   * Update fitness metrics after inference
   */
  updateFitness(result: {
    success: boolean;
    latency: number;
    cacheHit: boolean;
  }): void {
    this.fitness.usageCount++;

    if (result.success) {
      // Update success rate with exponential moving average
      const alpha = 0.1;  // Weight for new observation
      this.fitness.successRate = (
        alpha * 1 + (1 - alpha) * this.fitness.successRate
      );
    } else {
      this.fitness.successRate = (
        0.1 * 0 + 0.9 * this.fitness.successRate
      );
    }

    // Update average latency
    const n = this.fitness.usageCount;
    this.fitness.averageLatency = (
      (this.fitness.averageLatency * (n - 1) + result.latency) / n
    );

    // Update cache hit rate
    this.fitness.cacheHitRate = (
      0.1 * (result.cacheHit ? 1 : 0) + 0.9 * this.fitness.cacheHitRate
    );

    // Update efficiency (inverse of latency, normalized)
    this.fitness.efficiency = 1 / (1 + this.fitness.averageLatency / 1000);
  }
}
