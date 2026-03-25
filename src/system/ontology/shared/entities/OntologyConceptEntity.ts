/**
 * OntologyConceptEntity — Persisted canonical concept node
 *
 * Each row is a single concept (e.g. "low-rank-matrix-decomposition") with
 * per-model expressions and embedding vectors stored as JSON blobs.
 * The slug is stable and used as the business key across all joins.
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  BooleanField,
  JsonField,
  EnumField,
} from '../../../data/decorators/FieldDecorators';
import { BaseEntity } from '../../../data/entities/BaseEntity';
import type { OntologyDomain, ModelKey } from '../OntologyTypes';
import { ONTOLOGY_DOMAINS } from '../OntologyTypes';

export class OntologyConceptEntity extends BaseEntity {
  static readonly collection = 'ontology_concepts';

  /** URL-safe unique identifier for this concept — never changes */
  @TextField({ unique: true, index: true })
  slug: string;

  /** Human-readable display name */
  @TextField()
  displayName: string;

  /** Ontology domain for grouping and retrieval */
  @EnumField({ index: true })
  domain: OntologyDomain;

  /** Free-text description of the concept */
  @TextField({ maxLength: 0 }) // 0 = UNLIMITED
  description: string;

  /** Canonical expression used to bootstrap per-model alignment */
  @TextField({ maxLength: 0 })
  canonicalExpression: string;

  /** Related concept slugs */
  @JsonField({ nullable: true })
  relatedConcepts: string[];

  /**
   * Per-model natural-language expressions.
   * Serialised as { [ModelKey]: string }
   */
  @JsonField()
  expressions: Record<ModelKey, string>;

  /**
   * Per-model embedding vectors.
   * Serialised as { [ModelKey]: number[] }
   * Stored as blob when large (typical 768-dim vectors for many models exceed 4 KB).
   */
  @JsonField({ nullable: true, blobThreshold: 4096, blobRefField: 'embeddingsRef' })
  embeddings: Record<ModelKey, number[]>;

  /** Blob reference companion field for large embedding payloads */
  @TextField({ nullable: true })
  embeddingsRef?: string;

  /**
   * ISO timestamps of last embedding computation per model.
   * Serialised as { [ModelKey]: string }
   */
  @JsonField({ nullable: true })
  embeddingTimestamps: Record<ModelKey, string>;

  /** Number of models that have an expression for this concept */
  @NumberField()
  modelCount: number;

  /** Set when any model's embedding for this concept is stale */
  @BooleanField({ default: false })
  hasStaleEmbeddings: boolean;

  // Index signature for entity compatibility
  [key: string]: unknown;

  constructor() {
    super();
    this.slug = '';
    this.displayName = '';
    this.domain = 'domain-general';
    this.description = '';
    this.canonicalExpression = '';
    this.relatedConcepts = [];
    this.expressions = {} as Record<ModelKey, string>;
    this.embeddings = {} as Record<ModelKey, number[]>;
    this.embeddingTimestamps = {} as Record<ModelKey, string>;
    this.modelCount = 0;
    this.hasStaleEmbeddings = false;
  }

  get collection(): string {
    return OntologyConceptEntity.collection;
  }

  /** Models that currently have an expression for this concept */
  get registeredModels(): ModelKey[] {
    return Object.keys(this.expressions) as ModelKey[];
  }

  /** Whether a specific model has an embedding for this concept */
  hasEmbedding(modelKey: ModelKey): boolean {
    return Array.isArray(this.embeddings[modelKey]) && this.embeddings[modelKey].length > 0;
  }

  /** Record a new per-model expression and mark embeddings stale */
  registerExpression(modelKey: ModelKey, expression: string): void {
    const isNew = !(modelKey in this.expressions);
    this.expressions[modelKey] = expression;
    if (isNew) this.modelCount++;
    // Invalidate the embedding — will be recomputed by OntologyEvolutionService
    delete this.embeddings[modelKey];
    delete this.embeddingTimestamps[modelKey];
    this.hasStaleEmbeddings = true;
    this.updatedAt = new Date();
  }

  /** Record a freshly computed embedding vector */
  recordEmbedding(modelKey: ModelKey, vector: number[]): void {
    this.embeddings[modelKey] = vector;
    this.embeddingTimestamps[modelKey] = new Date().toISOString();
    // Recalculate staleness flag
    const modelKeys = Object.keys(this.expressions) as ModelKey[];
    this.hasStaleEmbeddings = modelKeys.some(k => !this.hasEmbedding(k));
    this.updatedAt = new Date();
  }

  validate(): { success: boolean; error?: string } {
    if (!this.slug || !/^[a-z0-9-]+$/.test(this.slug)) {
      return { success: false, error: 'slug must be lowercase alphanumeric with hyphens' };
    }
    if (!this.displayName) {
      return { success: false, error: 'displayName is required' };
    }
    if (!this.description) {
      return { success: false, error: 'description is required' };
    }
    if (!ONTOLOGY_DOMAINS.includes(this.domain)) {
      return { success: false, error: `domain must be one of: ${ONTOLOGY_DOMAINS.join(', ')}` };
    }
    return { success: true };
  }

  static override getPaginationConfig() {
    return {
      defaultSortField: 'slug',
      defaultSortDirection: 'asc' as const,
      defaultPageSize: 100,
      cursorField: 'slug',
    };
  }
}
