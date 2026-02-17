/**
 * Academy Curriculum Entity — A teacher-generated curriculum for a skill
 *
 * The curriculum is designed by the Teacher Sentinel using an LLM.
 * It contains an ordered list of progressive topics, each with a description
 * and difficulty level. Topics are taught sequentially, with the student
 * training on synthesized data and proving mastery through examinations.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  JsonField,
  ForeignKeyField,
} from '../../data/decorators/FieldDecorators';
import { BaseEntity } from '../../data/entities/BaseEntity';
import type { CurriculumTopic } from '../shared/AcademyTypes';

export class AcademyCurriculumEntity extends BaseEntity {
  static readonly collection = 'academy_curricula';

  /** Owning Academy session */
  @ForeignKeyField({ references: 'academy_sessions.id', index: true })
  sessionId: UUID;

  /** Target skill (matches session skill) */
  @TextField({ index: true })
  skill: string;

  /** Ordered list of curriculum topics */
  @JsonField()
  topics: CurriculumTopic[];

  /** Model that designed the curriculum */
  @TextField()
  generatedBy: string;

  /** Total number of topics */
  @NumberField()
  totalTopics: number;

  /** Number of topics completed (passed) */
  @NumberField()
  completedTopics: number;

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super();
    this.sessionId = '' as UUID;
    this.skill = '';
    this.topics = [];
    this.generatedBy = '';
    this.totalTopics = 0;
    this.completedTopics = 0;
  }

  get collection(): string {
    return AcademyCurriculumEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.sessionId?.trim()) {
      return { success: false, error: 'sessionId is required' };
    }

    if (!this.skill?.trim()) {
      return { success: false, error: 'skill is required' };
    }

    if (!Array.isArray(this.topics)) {
      return { success: false, error: 'topics must be an array' };
    }

    if (this.topics.length === 0) {
      return { success: false, error: 'curriculum must have at least one topic' };
    }

    for (let i = 0; i < this.topics.length; i++) {
      const topic = this.topics[i];
      if (!topic.name?.trim()) {
        return { success: false, error: `topic[${i}].name is required` };
      }
      if (!topic.description?.trim()) {
        return { success: false, error: `topic[${i}].description is required` };
      }
      const validDifficulties = ['beginner', 'intermediate', 'advanced'];
      if (!validDifficulties.includes(topic.difficulty)) {
        return { success: false, error: `topic[${i}].difficulty must be one of: ${validDifficulties.join(', ')}` };
      }
    }

    if (!this.generatedBy?.trim()) {
      return { success: false, error: 'generatedBy is required' };
    }

    if (this.totalTopics !== this.topics.length) {
      return { success: false, error: 'totalTopics must match topics array length' };
    }

    if (this.completedTopics < 0 || this.completedTopics > this.totalTopics) {
      return { success: false, error: 'completedTopics must be between 0 and totalTopics' };
    }

    return { success: true };
  }
}
