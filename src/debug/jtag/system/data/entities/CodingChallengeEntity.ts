/**
 * CodingChallengeEntity - Progressive coding challenges for AI training
 *
 * Defines challenge specifications and tracks attempt results.
 * Challenges are progressive: beginner → intermediate → advanced → expert.
 * Each challenge has:
 * - Setup files (initial codebase state)
 * - Expected outcome description
 * - Evaluation criteria (rubric for AI judge)
 * - Resource limits (time, tool calls)
 * - Attempt history with scores
 *
 * Used by CodingChallengeRunner to execute and CodingJudge to evaluate.
 * Failed attempts feed into LoRA training data capture.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  JsonField,
  EnumField,
  CompositeIndex,
} from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

// ────────────────────────────────────────────────────────────
// Challenge difficulty
// ────────────────────────────────────────────────────────────

export type ChallengeDifficulty = 'beginner' | 'intermediate' | 'advanced' | 'expert';

// ────────────────────────────────────────────────────────────
// Challenge category
// ────────────────────────────────────────────────────────────

export type ChallengeCategory =
  | 'single-file'     // Operations on one file
  | 'multi-file'      // Cross-file coordination
  | 'refactoring'     // Extract, rename, restructure
  | 'bug-fix'         // Find and fix defects
  | 'feature'         // Add new functionality
  | 'architecture'    // Large-scale structural changes
  | 'discovery';      // Codebase exploration and analysis

// ────────────────────────────────────────────────────────────
// Challenge attempt result
// ────────────────────────────────────────────────────────────

export type AttemptStatus = 'passed' | 'failed' | 'partial' | 'timeout' | 'error';

export interface ChallengeAttempt {
  /** Which AI attempted this */
  personaId: UUID;
  /** CodingPlan that was executed */
  planId?: UUID;
  /** When the attempt started */
  startedAt: number;
  /** When the attempt finished */
  completedAt: number;
  /** Outcome */
  status: AttemptStatus;
  /** AI judge score (0-100) */
  score: number;
  /** AI judge feedback */
  feedback: string;
  /** Files modified during the attempt */
  filesModified: string[];
  /** Files created during the attempt */
  filesCreated: string[];
  /** Errors encountered */
  errors: string[];
  /** Tool calls consumed */
  toolCallsUsed: number;
  /** Total duration in milliseconds */
  durationMs: number;
  /** File contents after execution (for judge evaluation) */
  resultFiles?: Record<string, string>;
}

// ────────────────────────────────────────────────────────────
// Entity
// ────────────────────────────────────────────────────────────

@CompositeIndex({
  name: 'idx_coding_challenges_difficulty',
  fields: ['difficulty', 'category'],
  direction: 'ASC',
})
@CompositeIndex({
  name: 'idx_coding_challenges_order',
  fields: ['sequenceNumber'],
  direction: 'ASC',
})
export class CodingChallengeEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.CODING_CHALLENGES;

  // ── Identity ──────────────────────────────────────────────

  /** Human-readable challenge name */
  @TextField({ index: true })
  name!: string;

  /** Challenge description — what the AI needs to accomplish */
  @TextField()
  description!: string;

  /** Ordering for progressive difficulty */
  @NumberField()
  sequenceNumber!: number;

  // ── Classification ────────────────────────────────────────

  @EnumField()
  difficulty!: ChallengeDifficulty;

  @EnumField()
  category!: ChallengeCategory;

  // ── Challenge specification ───────────────────────────────

  /** Initial file contents that define the challenge workspace */
  @JsonField()
  setupFiles!: Record<string, string>;

  /** What success looks like (natural language for AI judge) */
  @TextField()
  expectedOutcome!: string;

  /** Rubric criteria for the AI judge to evaluate */
  @JsonField()
  evaluationCriteria!: string[];

  /** Optional: expected file contents after successful completion */
  @JsonField()
  expectedFiles?: Record<string, string>;

  // ── Resource limits ───────────────────────────────────────

  /** Maximum execution time in milliseconds */
  @NumberField()
  timeLimitMs!: number;

  /** Maximum tool calls allowed */
  @NumberField()
  toolCallLimit!: number;

  // ── Attempt history ───────────────────────────────────────

  /** All attempts made against this challenge */
  @JsonField()
  attempts!: ChallengeAttempt[];

  // ── Statistics ────────────────────────────────────────────

  /** Number of times this challenge has been attempted */
  @NumberField()
  totalAttempts!: number;

  /** Number of times this challenge has been passed */
  @NumberField()
  totalPasses!: number;

  /** Highest score achieved */
  @NumberField()
  highScore!: number;

  // ── Index signature ───────────────────────────────────────

  [key: string]: unknown;

  // ── Constructor ───────────────────────────────────────────

  constructor() {
    super();

    this.name = '';
    this.description = '';
    this.sequenceNumber = 0;
    this.difficulty = 'beginner';
    this.category = 'single-file';
    this.setupFiles = {};
    this.expectedOutcome = '';
    this.evaluationCriteria = [];
    this.timeLimitMs = 60_000;
    this.toolCallLimit = 10;
    this.attempts = [];
    this.totalAttempts = 0;
    this.totalPasses = 0;
    this.highScore = 0;
  }

  // ── BaseEntity implementation ─────────────────────────────

  get collection(): string {
    return CodingChallengeEntity.collection;
  }

  static override getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'sequenceNumber',
      defaultSortDirection: 'asc',
      defaultPageSize: 20,
      cursorField: 'sequenceNumber',
    };
  }

  validate(): { success: boolean; error?: string } {
    if (!this.name?.trim()) {
      return { success: false, error: 'Challenge name is required' };
    }
    if (!this.description?.trim()) {
      return { success: false, error: 'Challenge description is required' };
    }
    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 1) {
      return { success: false, error: 'Challenge sequenceNumber must be a positive integer' };
    }
    if (!this.expectedOutcome?.trim()) {
      return { success: false, error: 'Challenge expectedOutcome is required' };
    }
    if (!Array.isArray(this.evaluationCriteria) || this.evaluationCriteria.length === 0) {
      return { success: false, error: 'Challenge must have at least one evaluation criterion' };
    }
    if (Object.keys(this.setupFiles).length === 0) {
      return { success: false, error: 'Challenge must have at least one setup file' };
    }
    if (this.timeLimitMs < 5000) {
      return { success: false, error: 'Challenge time limit must be at least 5 seconds' };
    }
    if (this.toolCallLimit < 2) {
      return { success: false, error: 'Challenge tool call limit must be at least 2' };
    }

    return { success: true };
  }

  // ── Convenience methods ───────────────────────────────────

  /** Pass rate as a percentage (0-100) */
  get passRate(): number {
    if (this.totalAttempts === 0) return 0;
    return Math.round((this.totalPasses / this.totalAttempts) * 100);
  }

  /** Average score across all attempts */
  get averageScore(): number {
    if (this.attempts.length === 0) return 0;
    const total = this.attempts.reduce((sum, a) => sum + a.score, 0);
    return Math.round(total / this.attempts.length);
  }

  /** Best attempt for a specific persona */
  bestAttemptFor(personaId: UUID): ChallengeAttempt | undefined {
    return this.attempts
      .filter(a => a.personaId === personaId)
      .sort((a, b) => b.score - a.score)[0];
  }

  /** Record a new attempt and update statistics */
  recordAttempt(attempt: ChallengeAttempt): void {
    this.attempts.push(attempt);
    this.totalAttempts++;
    if (attempt.status === 'passed') {
      this.totalPasses++;
    }
    if (attempt.score > this.highScore) {
      this.highScore = attempt.score;
    }
  }
}
