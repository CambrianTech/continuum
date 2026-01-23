/**
 * FeedbackEntity - Cross-AI Learning Feedback Loop
 *
 * Stores successful patterns discovered by personas that can be shared
 * and learned from by other AIs. This is the foundation for collective
 * intelligence and continuous improvement.
 *
 * Philosophy: "When one AI learns something valuable, all AIs can benefit"
 *
 * Use cases:
 * - Share successful debugging patterns
 * - Propagate tool usage discoveries
 * - Capture effective response strategies
 * - Build collective problem-solving knowledge
 */

import { BaseEntity } from './BaseEntity';
import { TextField, JsonField, NumberField, DateField, EnumField, BooleanField, CompositeIndex } from '../decorators/FieldDecorators';
import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Types of patterns that can be shared
 */
export enum FeedbackType {
  /** Successful debugging approach */
  DEBUGGING = 'debugging',
  /** Effective tool usage pattern */
  TOOL_USE = 'tool-use',
  /** Good response strategy */
  RESPONSE_STRATEGY = 'response-strategy',
  /** Performance optimization */
  OPTIMIZATION = 'optimization',
  /** Error handling approach */
  ERROR_HANDLING = 'error-handling',
  /** Collaboration pattern */
  COLLABORATION = 'collaboration',
  /** Domain-specific expertise */
  EXPERTISE = 'expertise'
}

/**
 * Domain where the pattern applies
 */
export enum FeedbackDomain {
  CHAT = 'chat',
  CODE = 'code',
  TOOLS = 'tools',
  COORDINATION = 'coordination',
  GENERAL = 'general'
}

/**
 * Status of the feedback pattern
 */
export enum FeedbackStatus {
  /** Newly discovered, not yet validated */
  PENDING = 'pending',
  /** Validated by multiple personas */
  VALIDATED = 'validated',
  /** Deprecated or superseded */
  DEPRECATED = 'deprecated',
  /** Actively being used */
  ACTIVE = 'active'
}

/**
 * FeedbackEntity - Stores shareable learning patterns
 *
 * Composite indexes optimize:
 * 1. Find patterns by type/domain
 * 2. Find validated patterns
 * 3. Find patterns by source persona
 * 4. Semantic search via embedding
 */
@CompositeIndex({
  name: 'idx_feedback_type_domain',
  fields: ['type', 'domain'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_feedback_status_confidence',
  fields: ['status', 'confidence'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_feedback_source_timestamp',
  fields: ['sourcePersonaId', 'discoveredAt'],
  direction: 'DESC'
})
export class FeedbackEntity extends BaseEntity {
  static readonly collection = 'feedback_patterns';

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return FeedbackEntity.collection;
  }

  /**
   * Implement BaseEntity abstract method - validate pattern data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.sourcePersonaId?.trim()) {
      return { success: false, error: 'Pattern sourcePersonaId is required' };
    }
    if (!this.name?.trim()) {
      return { success: false, error: 'Pattern name is required' };
    }
    if (!this.type) {
      return { success: false, error: 'Pattern type is required' };
    }
    if (!this.domain) {
      return { success: false, error: 'Pattern domain is required' };
    }
    if (!this.problem?.trim()) {
      return { success: false, error: 'Pattern problem is required' };
    }
    if (!this.solution?.trim()) {
      return { success: false, error: 'Pattern solution is required' };
    }
    return { success: true };
  }

  // =========================================================================
  // Pattern Identity
  // =========================================================================

  /** Persona who discovered this pattern */
  @TextField({ index: true })
  sourcePersonaId!: UUID;

  /** Human-readable name for the pattern */
  @TextField()
  name!: string;

  /** Detailed description of what this pattern does */
  @TextField()
  description!: string;

  // =========================================================================
  // Pattern Classification
  // =========================================================================

  /** Type of pattern */
  @EnumField({ index: true })
  type!: FeedbackType;

  /** Domain where pattern applies */
  @EnumField({ index: true })
  domain!: FeedbackDomain;

  /** Current status */
  @EnumField({ index: true })
  status!: FeedbackStatus;

  /** Tags for categorization and search */
  @JsonField()
  tags!: string[];

  // =========================================================================
  // Pattern Content
  // =========================================================================

  /** The problem this pattern solves */
  @TextField()
  problem!: string;

  /** The solution/approach that works */
  @TextField()
  solution!: string;

  /** Conditions when this pattern applies */
  @JsonField()
  applicableWhen!: {
    keywords?: string[];
    contexts?: string[];
    preconditions?: string[];
  };

  /** Example usage */
  @JsonField({ nullable: true })
  examples?: {
    input: string;
    output: string;
    explanation?: string;
  }[];

  // =========================================================================
  // Metrics
  // =========================================================================

  /** Confidence score (0.0-1.0) based on success rate */
  @NumberField()
  confidence!: number;

  /** Times this pattern was applied successfully */
  @NumberField()
  successCount!: number;

  /** Times this pattern failed */
  @NumberField()
  failureCount!: number;

  /** Computed success rate */
  get successRate(): number {
    const total = this.successCount + this.failureCount;
    return total > 0 ? this.successCount / total : 0;
  }

  // =========================================================================
  // Sharing & Validation
  // =========================================================================

  /** Personas who have received this pattern */
  @JsonField()
  sharedWith!: UUID[];

  /** Personas who have validated/endorsed this pattern */
  @JsonField()
  endorsedBy!: UUID[];

  /** Whether this pattern is publicly available to all personas */
  @BooleanField()
  isPublic!: boolean;

  // =========================================================================
  // Temporal
  // =========================================================================

  /** When the pattern was discovered */
  @DateField({ index: true })
  discoveredAt!: Date;

  /** When last successfully used */
  @DateField({ nullable: true })
  lastUsedAt?: Date;

  /** When last modified */
  @DateField({ nullable: true })
  lastModifiedAt?: Date;

  // =========================================================================
  // Semantic Search
  // =========================================================================

  /** Embedding vector for semantic similarity search */
  @JsonField({ nullable: true })
  embedding?: number[];

  // =========================================================================
  // Factory Methods
  // =========================================================================

  /**
   * Create a new feedback pattern
   * Named createPattern to avoid conflict with BaseEntity.create
   */
  static createPattern(params: {
    sourcePersonaId: UUID;
    name: string;
    description: string;
    type: FeedbackType;
    domain: FeedbackDomain;
    problem: string;
    solution: string;
    tags?: string[];
    applicableWhen?: FeedbackEntity['applicableWhen'];
    examples?: FeedbackEntity['examples'];
  }): FeedbackEntity {
    const entity = new FeedbackEntity();

    // Identity
    entity.sourcePersonaId = params.sourcePersonaId;
    entity.name = params.name;
    entity.description = params.description;

    // Classification
    entity.type = params.type;
    entity.domain = params.domain;
    entity.status = FeedbackStatus.PENDING;
    entity.tags = params.tags ?? [];

    // Content
    entity.problem = params.problem;
    entity.solution = params.solution;
    entity.applicableWhen = params.applicableWhen ?? {};
    entity.examples = params.examples;

    // Initial metrics
    entity.confidence = 0.5; // Start at neutral
    entity.successCount = 1; // Creator's first success
    entity.failureCount = 0;

    // Sharing
    entity.sharedWith = [];
    entity.endorsedBy = [params.sourcePersonaId]; // Self-endorse
    entity.isPublic = false; // Private until validated

    // Temporal
    entity.discoveredAt = new Date();

    return entity;
  }

  /**
   * Record a successful use of this pattern
   */
  recordSuccess(personaId: UUID): void {
    this.successCount++;
    this.lastUsedAt = new Date();

    // Update confidence based on success rate
    this.updateConfidence();

    // Auto-validate if enough successes
    if (this.successCount >= 3 && this.status === FeedbackStatus.PENDING) {
      this.status = FeedbackStatus.VALIDATED;
      this.isPublic = true;
    }

    // Track endorsement
    if (!this.endorsedBy.includes(personaId)) {
      this.endorsedBy.push(personaId);
    }
  }

  /**
   * Record a failed use of this pattern
   */
  recordFailure(): void {
    this.failureCount++;
    this.updateConfidence();

    // Deprecate if too many failures
    if (this.failureCount > this.successCount * 2) {
      this.status = FeedbackStatus.DEPRECATED;
    }
  }

  /**
   * Update confidence based on success rate
   */
  private updateConfidence(): void {
    const total = this.successCount + this.failureCount;
    if (total === 0) {
      this.confidence = 0.5;
      return;
    }

    // Wilson score lower bound for confidence
    // Provides more accurate confidence for small sample sizes
    const n = total;
    const p = this.successCount / n;
    const z = 1.96; // 95% confidence
    const denominator = 1 + (z * z) / n;
    const center = p + (z * z) / (2 * n);
    const spread = z * Math.sqrt((p * (1 - p) + (z * z) / (4 * n)) / n);

    this.confidence = Math.max(0, Math.min(1, (center - spread) / denominator));
  }

  /**
   * Share this pattern with another persona
   */
  shareWith(personaId: UUID): void {
    if (!this.sharedWith.includes(personaId)) {
      this.sharedWith.push(personaId);
    }
  }

  /**
   * Check if this pattern is applicable to a given context
   */
  isApplicableTo(context: {
    keywords?: string[];
    domain?: FeedbackDomain;
  }): boolean {
    // Domain mismatch
    if (context.domain && this.domain !== context.domain && this.domain !== FeedbackDomain.GENERAL) {
      return false;
    }

    // Check keyword overlap
    if (context.keywords && this.applicableWhen.keywords) {
      const overlap = context.keywords.some(k =>
        this.applicableWhen.keywords!.includes(k.toLowerCase())
      );
      if (!overlap) return false;
    }

    return true;
  }
}
