/**
 * Academy Examination Entity — A teacher-generated exam and student responses
 *
 * Each examination covers one topic within a curriculum. The teacher generates
 * questions, the student answers them, and the teacher grades the responses.
 * Multiple rounds are possible per topic if the student fails.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  JsonField,
  ForeignKeyField,
  BooleanField,
} from '../../data/decorators/FieldDecorators';
import { BaseEntity } from '../../data/entities/BaseEntity';
import type { ExamQuestion, ExamResponse } from '../shared/AcademyTypes';

export class AcademyExaminationEntity extends BaseEntity {
  static readonly collection = 'academy_examinations';

  /** Owning Academy session */
  @ForeignKeyField({ references: 'academy_sessions.id', index: true })
  sessionId: UUID;

  /** Topic index within the curriculum (0-based) */
  @NumberField()
  topicIndex: number;

  /** Attempt number for this topic (1-based) */
  @NumberField()
  round: number;

  /** Teacher-generated exam questions */
  @JsonField()
  questions: ExamQuestion[];

  /** Student responses (populated after exam is taken) */
  @JsonField()
  responses: ExamResponse[];

  /** Overall score (0-100, populated after grading) */
  @NumberField()
  overallScore: number;

  /** Whether the student passed this exam */
  @BooleanField()
  passed: boolean;

  /** Model that graded the exam */
  @TextField({ nullable: true })
  gradedBy?: string;

  /** Grading feedback summary */
  @TextField({ maxLength: 0, nullable: true })
  feedback?: string;

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super();
    this.sessionId = '' as UUID;
    this.topicIndex = 0;
    this.round = 1;
    this.questions = [];
    this.responses = [];
    this.overallScore = 0;
    this.passed = false;
  }

  get collection(): string {
    return AcademyExaminationEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.sessionId?.trim()) {
      return { success: false, error: 'sessionId is required' };
    }

    if (this.topicIndex < 0) {
      return { success: false, error: 'topicIndex must be >= 0' };
    }

    if (this.round < 1) {
      return { success: false, error: 'round must be >= 1' };
    }

    if (!Array.isArray(this.questions)) {
      return { success: false, error: 'questions must be an array' };
    }

    for (let i = 0; i < this.questions.length; i++) {
      const q = this.questions[i];
      if (!q.question?.trim()) {
        return { success: false, error: `questions[${i}].question is required` };
      }
      if (!q.expectedAnswer?.trim()) {
        return { success: false, error: `questions[${i}].expectedAnswer is required` };
      }
      if (!q.category?.trim()) {
        return { success: false, error: `questions[${i}].category is required` };
      }
    }

    if (!Array.isArray(this.responses)) {
      return { success: false, error: 'responses must be an array' };
    }

    if (this.overallScore < 0 || this.overallScore > 100) {
      return { success: false, error: 'overallScore must be between 0 and 100' };
    }

    return { success: true };
  }
}
