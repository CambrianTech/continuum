/**
 * Academy Session Entity — Tracks a dual-sentinel teaching/learning session
 *
 * Each session represents one skill being taught by a Teacher Sentinel
 * to a Student Sentinel (a specific persona). The session tracks the
 * lifecycle from curriculum design through training and examination.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  EnumField,
  JsonField,
  ForeignKeyField,
} from '../../data/decorators/FieldDecorators';
import { BaseEntity } from '../../data/entities/BaseEntity';
import type {
  AcademySessionStatus,
  AcademyConfig,
} from '../shared/AcademyTypes';
import {
  VALID_SESSION_STATUSES,
  DEFAULT_ACADEMY_CONFIG,
} from '../shared/AcademyTypes';
import { LOCAL_MODELS } from '@system/shared/Constants';

export class AcademySessionEntity extends BaseEntity {
  static readonly collection = 'academy_sessions';

  /** The student persona being trained */
  @ForeignKeyField({ references: 'users.id', index: true })
  personaId: UUID;

  /** Student persona display name */
  @TextField()
  personaName: string;

  /** Skill being taught (e.g., "typescript-generics", "ethical-reasoning") */
  @TextField({ index: true })
  skill: string;

  /** Base model used for training (defaults to LOCAL_MODELS.DEFAULT) */
  @TextField()
  baseModel: string;

  /** Current session lifecycle status */
  @EnumField({ index: true })
  status: AcademySessionStatus;

  /** Sentinel handle for the teacher pipeline */
  @TextField({ nullable: true })
  teacherHandle?: string;

  /** Sentinel handle for the student pipeline */
  @TextField({ nullable: true })
  studentHandle?: string;

  /** Reference to the generated curriculum */
  @ForeignKeyField({ references: 'academy_curricula.id', nullable: true })
  curriculumId?: UUID;

  /** Current topic index in the curriculum (0-based) */
  @NumberField()
  currentTopic: number;

  /** Total exam rounds completed across all topics */
  @NumberField()
  examRounds: number;

  /** Session configuration */
  @JsonField()
  config: AcademyConfig;

  /** Training metrics summary (populated as training progresses) */
  @JsonField({ nullable: true })
  metrics?: {
    topicsPassed: number;
    topicsFailed: number;
    totalTrainingTime: number;
    averageExamScore: number;
    layerIds: UUID[];
  };

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super();
    this.personaId = '' as UUID;
    this.personaName = '';
    this.skill = '';
    this.baseModel = LOCAL_MODELS.DEFAULT;
    this.status = 'pending';
    this.currentTopic = 0;
    this.examRounds = 0;
    this.config = { ...DEFAULT_ACADEMY_CONFIG };
  }

  get collection(): string {
    return AcademySessionEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.personaId?.trim()) {
      return { success: false, error: 'personaId is required' };
    }

    if (!this.personaName?.trim()) {
      return { success: false, error: 'personaName is required' };
    }

    if (!this.skill?.trim()) {
      return { success: false, error: 'skill is required' };
    }

    if (!this.baseModel?.trim()) {
      return { success: false, error: 'baseModel is required' };
    }

    if (!VALID_SESSION_STATUSES.includes(this.status)) {
      return { success: false, error: `status must be one of: ${VALID_SESSION_STATUSES.join(', ')}` };
    }

    if (this.currentTopic < 0) {
      return { success: false, error: 'currentTopic must be >= 0' };
    }

    if (this.examRounds < 0) {
      return { success: false, error: 'examRounds must be >= 0' };
    }

    if (this.config.passingScore < 0 || this.config.passingScore > 100) {
      return { success: false, error: 'config.passingScore must be between 0 and 100' };
    }

    if (this.config.maxTopicAttempts < 1) {
      return { success: false, error: 'config.maxTopicAttempts must be >= 1' };
    }

    return { success: true };
  }
}
