/**
 * BenchmarkEntity — Persistent benchmark definitions (auto-generated test suites)
 *
 * Benchmarks are sets of questions with expected answers and rubrics,
 * derived from extracted facts. They are reusable across sessions and personas.
 *
 * Previously stored as raw data in the 'academy_benchmarks' collection
 * without a registered entity. This entity provides proper schema,
 * validation, and type safety.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { BenchmarkQuestion } from '../../genome/shared/KnowledgeTypes';
import {
  TextField,
  NumberField,
  JsonField,
} from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';

export class BenchmarkEntity extends BaseEntity {
  static readonly collection = 'academy_benchmarks';

  /** Human-readable name (e.g., "Nexaflux Corporation Knowledge") */
  @TextField({ index: true })
  name!: string;

  /** Domain this benchmark tests */
  @TextField({ index: true })
  domain!: string;

  /** The benchmark questions with expected answers and rubrics */
  @JsonField()
  questions!: BenchmarkQuestion[];

  /** Summary of the source knowledge this was generated from */
  @TextField()
  knowledgeSummary!: string;

  /** Number of facts the benchmark covers */
  @NumberField()
  factCount!: number;

  /** Who/what created this benchmark */
  @TextField({ index: true })
  createdBy!: string;

  [key: string]: unknown;

  constructor() {
    super();
    this.name = '';
    this.domain = '';
    this.questions = [];
    this.knowledgeSummary = '';
    this.factCount = 0;
    this.createdBy = '';
  }

  get collection(): string {
    return BenchmarkEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.name?.trim()) {
      return { success: false, error: 'Benchmark name is required' };
    }
    if (!this.domain?.trim()) {
      return { success: false, error: 'Benchmark domain is required' };
    }
    if (!Array.isArray(this.questions) || this.questions.length === 0) {
      return { success: false, error: 'Benchmark must have at least one question' };
    }
    return { success: true };
  }
}
