/**
 * Competition Entity — Tracks a multi-persona competition session
 *
 * A competition pits N personas against a shared curriculum from one teacher.
 * Each persona gets a student sentinel; all share the teacher's exam questions.
 * The entity tracks competitor entries, rankings, and tournament rounds.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  EnumField,
  JsonField,
  BooleanField,
  TEXT_LENGTH,
} from '../../data/decorators/FieldDecorators';
import { BaseEntity } from '../../data/entities/BaseEntity';
import type {
  CompetitionStatus,
  CompetitorEntry,
  CompetitionConfig,
  TournamentRound,
} from '../shared/CompetitionTypes';
import {
  VALID_COMPETITION_STATUSES,
  DEFAULT_COMPETITION_CONFIG,
} from '../shared/CompetitionTypes';

export class CompetitionEntity extends BaseEntity {
  static readonly collection = 'academy_competitions';

  /** Skill being competed on (e.g., "typescript-generics") */
  @TextField({ index: true })
  skill: string;

  /** Base model used for all competitors */
  @TextField()
  baseModel: string;

  /** Current competition lifecycle status */
  @EnumField({ index: true })
  status: CompetitionStatus;

  /** Sentinel handle for the shared teacher pipeline */
  @TextField({ nullable: true })
  teacherHandle?: string;

  /** Reference to the shared curriculum */
  @TextField({ nullable: true })
  curriculumId?: string;

  /** All competitors and their state */
  @JsonField()
  competitors: CompetitorEntry[];

  /** Competition configuration */
  @JsonField()
  config: CompetitionConfig;

  /** Current tournament round (1-based) */
  @NumberField()
  currentRound: number;

  /** Tournament round history */
  @JsonField()
  rounds: TournamentRound[];

  /** Total number of topics in the curriculum */
  @NumberField()
  totalTopics: number;

  /** When the competition started */
  @TextField({ nullable: true })
  startedAt?: string;

  /** When the competition completed */
  @TextField({ nullable: true })
  completedAt?: string;

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super();
    this.skill = '';
    this.baseModel = 'smollm2:135m';
    this.status = 'pending';
    this.competitors = [];
    this.config = { ...DEFAULT_COMPETITION_CONFIG };
    this.currentRound = 0;
    this.rounds = [];
    this.totalTopics = 0;
  }

  get collection(): string {
    return CompetitionEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.skill?.trim()) {
      return { success: false, error: 'skill is required' };
    }

    if (!this.baseModel?.trim()) {
      return { success: false, error: 'baseModel is required' };
    }

    if (!VALID_COMPETITION_STATUSES.includes(this.status)) {
      return { success: false, error: `status must be one of: ${VALID_COMPETITION_STATUSES.join(', ')}` };
    }

    if (this.competitors.length < 2) {
      return { success: false, error: 'competition requires at least 2 competitors' };
    }

    for (const c of this.competitors) {
      if (!c.personaId?.trim()) {
        return { success: false, error: 'each competitor must have a personaId' };
      }
      if (!c.personaName?.trim()) {
        return { success: false, error: 'each competitor must have a personaName' };
      }
    }

    if (this.config.passingScore < 0 || this.config.passingScore > 100) {
      return { success: false, error: 'config.passingScore must be between 0 and 100' };
    }

    if (this.config.maxTopicAttempts < 1) {
      return { success: false, error: 'config.maxTopicAttempts must be >= 1' };
    }

    if (this.config.tournamentRounds < 1) {
      return { success: false, error: 'config.tournamentRounds must be >= 1' };
    }

    return { success: true };
  }
}
