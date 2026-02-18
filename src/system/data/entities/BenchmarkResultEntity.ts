/**
 * BenchmarkResultEntity — Records of persona performance against benchmarks
 *
 * Each result captures a single persona's answers, scores, and feedback
 * for one benchmark run. Used to track improvement over time and
 * compare adapter effectiveness.
 *
 * Previously stored as raw data in the 'academy_benchmark_results' collection
 * without a registered entity. This entity provides proper schema,
 * validation, and type safety.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { QuestionScore } from '../../genome/shared/KnowledgeTypes';
import {
  TextField,
  NumberField,
  JsonField,
  ForeignKeyField,
  CompositeIndex,
} from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';

@CompositeIndex({
  name: 'idx_benchmark_results_persona_benchmark',
  fields: ['personaId', 'benchmarkId'],
  direction: 'DESC',
})
export class BenchmarkResultEntity extends BaseEntity {
  static readonly collection = 'academy_benchmark_results';

  /** Benchmark this result is for */
  @ForeignKeyField({ references: 'academy_benchmarks.id', index: true })
  benchmarkId!: UUID;

  /** Persona that was tested */
  @TextField({ index: true })
  personaId!: UUID;

  /** Persona name (denormalized for convenience) */
  @TextField()
  personaName!: string;

  /** Benchmark name (denormalized for convenience) */
  @TextField()
  benchmarkName!: string;

  /** Overall score (0-100) */
  @NumberField()
  overallScore!: number;

  /** Per-question scores with answers and feedback */
  @JsonField()
  questionScores!: QuestionScore[];

  /** Per-category average scores */
  @JsonField()
  categoryScores!: Record<string, number>;

  /** Which adapter (if any) was active during the test */
  @TextField({ nullable: true })
  adapterId?: UUID;

  /** When this benchmark was run (ISO string) */
  @TextField()
  runAt!: string;

  /** Duration of the benchmark run in milliseconds */
  @NumberField()
  durationMs!: number;

  [key: string]: unknown;

  constructor() {
    super();
    this.benchmarkId = '' as UUID;
    this.personaId = '' as UUID;
    this.personaName = '';
    this.benchmarkName = '';
    this.overallScore = 0;
    this.questionScores = [];
    this.categoryScores = {};
    this.runAt = '';
    this.durationMs = 0;
  }

  get collection(): string {
    return BenchmarkResultEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.benchmarkId?.trim()) {
      return { success: false, error: 'benchmarkId is required' };
    }
    if (!this.personaId?.trim()) {
      return { success: false, error: 'personaId is required' };
    }
    if (typeof this.overallScore !== 'number' || this.overallScore < 0 || this.overallScore > 100) {
      return { success: false, error: 'overallScore must be a number between 0 and 100' };
    }
    return { success: true };
  }
}
