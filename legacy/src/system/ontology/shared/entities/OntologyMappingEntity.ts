/**
 * OntologyMappingEntity — Persisted cross-model semantic mapping
 *
 * Captures how a single canonical concept translates FROM one model's semantic
 * dialect TO another's. Directionality is explicit: a row for A→B is distinct
 * from B→A because asymmetric confidence is common (smaller models often
 * understand large-model expressions better than the reverse).
 *
 * Similarity and confidence are updated by OntologyEvolutionService whenever
 * a model is fine-tuned or new embeddings are computed.
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  BooleanField,
  JsonField,
  ForeignKeyField,
} from '../../../data/decorators/FieldDecorators';
import { BaseEntity } from '../../../data/entities/BaseEntity';
import type { ModelIdentifier, ModelKey } from '../OntologyTypes';

export class OntologyMappingEntity extends BaseEntity {
  static readonly collection = 'ontology_mappings';

  /** Canonical concept slug this mapping belongs to */
  @TextField({ index: true })
  conceptSlug: string;

  /** Concept entity ID — foreign key for join operations */
  @ForeignKeyField({ references: 'ontology_concepts.id', index: true })
  conceptId: UUID;

  /** Source model (serialised ModelIdentifier) */
  @JsonField()
  sourceModel: ModelIdentifier;

  /** Denormalised source model key for indexed lookup */
  @TextField({ index: true })
  sourceModelKey: ModelKey;

  /** Target model (serialised ModelIdentifier) */
  @JsonField()
  targetModel: ModelIdentifier;

  /** Denormalised target model key for indexed lookup */
  @TextField({ index: true })
  targetModelKey: ModelKey;

  /** How the source model expresses this concept */
  @TextField({ maxLength: 0 })
  sourceExpression: string;

  /** How the target model expresses this concept */
  @TextField({ maxLength: 0 })
  targetExpression: string;

  /**
   * Cosine similarity between source and target embedding vectors for this concept.
   * Range [0, 1]. Recomputed by OntologyEvolutionService when embeddings update.
   */
  @NumberField()
  similarity: number;

  /**
   * Translation confidence score.
   * Range [0, 1]. Starts at 0.5. Rises when translation is validated
   * (model receiving the translated content does not ask for clarification).
   */
  @NumberField()
  confidence: number;

  /** ISO timestamp of last verification */
  @TextField({ nullable: true })
  verifiedAt?: string;

  /**
   * Set when similarity has drifted beyond ONTOLOGY_CONSTANTS.DRIFT_THRESHOLD
   * since the last alignment run. OntologyEvolutionService will re-align.
   */
  @BooleanField({ default: false })
  needsReview: boolean;

  /** Model version tag at source-side alignment time */
  @TextField({ nullable: true })
  sourceVersion?: string;

  /** Model version tag at target-side alignment time */
  @TextField({ nullable: true })
  targetVersion?: string;

  /**
   * Number of times this mapping has been used in a translation.
   * Higher use → more validation data → higher confidence.
   */
  @NumberField()
  useCount: number;

  /**
   * Number of times the translated content was validated as correct
   * (target model understood without asking for re-clarification).
   */
  @NumberField()
  validatedCount: number;

  // Index signature for entity compatibility
  [key: string]: unknown;

  constructor() {
    super();
    this.conceptSlug = '';
    this.conceptId = '' as UUID;
    this.sourceModel = { providerId: '', modelId: '' };
    this.sourceModelKey = '' as ModelKey;
    this.targetModel = { providerId: '', modelId: '' };
    this.targetModelKey = '' as ModelKey;
    this.sourceExpression = '';
    this.targetExpression = '';
    this.similarity = 0;
    this.confidence = 0.5;
    this.needsReview = false;
    this.useCount = 0;
    this.validatedCount = 0;
  }

  get collection(): string {
    return OntologyMappingEntity.collection;
  }

  /** Record a successful translation use; updates confidence incrementally */
  recordUse(validated: boolean): void {
    this.useCount++;
    if (validated) {
      this.validatedCount++;
      // Bayesian-style confidence update: weighted average with prior
      this.confidence = Math.min(0.99, (this.validatedCount / this.useCount) * 0.8 + this.confidence * 0.2);
    }
    this.updatedAt = new Date();
  }

  /** Update similarity after embedding recomputation */
  updateSimilarity(newSimilarity: number, driftThreshold: number): void {
    const drift = Math.abs(newSimilarity - this.similarity);
    this.similarity = newSimilarity;
    this.needsReview = drift > driftThreshold;
    this.verifiedAt = new Date().toISOString();
    this.updatedAt = new Date();
  }

  validate(): { success: boolean; error?: string } {
    if (!this.conceptSlug) {
      return { success: false, error: 'conceptSlug is required' };
    }
    if (!this.sourceModelKey || !this.targetModelKey) {
      return { success: false, error: 'sourceModelKey and targetModelKey are required' };
    }
    if (this.sourceModelKey === this.targetModelKey) {
      return { success: false, error: 'source and target models must be different' };
    }
    if (!this.sourceExpression || !this.targetExpression) {
      return { success: false, error: 'sourceExpression and targetExpression are required' };
    }
    if (this.similarity < 0 || this.similarity > 1) {
      return { success: false, error: 'similarity must be in range [0, 1]' };
    }
    return { success: true };
  }

  static override getPaginationConfig() {
    return {
      defaultSortField: 'conceptSlug',
      defaultSortDirection: 'asc' as const,
      defaultPageSize: 200,
      cursorField: 'conceptSlug',
    };
  }
}
