/**
 * TestExecutionEntity - Long-running AI adapter test execution state
 *
 * Purpose: Track asynchronous test runs with progress events.
 * Pattern: Command returns UUID immediately, run tests in background.
 *
 * Related Commands:
 * - ai/adapter/test (creates test execution, returns testId)
 * - ai/adapter/test/status (check progress)
 * - ai/adapter/test/results (get final results)
 */

import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { TextField, BooleanField, JsonField, DateField } from '../../../../system/data/decorators/FieldDecorators';

/**
 * Test execution status lifecycle
 */
export type TestExecutionStatus =
  | 'queued'      // Waiting to start
  | 'running'     // Currently executing
  | 'completed'   // Finished successfully
  | 'failed'      // Finished with errors
  | 'cancelled';  // User cancelled

/**
 * Individual capability test result (embedded in execution)
 */
export interface CapabilityTestResult {
  capability: string;
  supported: boolean;
  tested: boolean;
  success?: boolean;
  responseTime?: number;
  error?: string;
}

/**
 * Test execution progress tracking
 */
export interface TestProgress {
  totalCapabilities: number;
  completedCapabilities: number;
  currentCapability?: string;
  percentComplete: number;
}

/**
 * Test execution entity - stored in database
 */
export class TestExecutionEntity extends BaseEntity {
  // Single source of truth for collection name
  static readonly collection = 'test_executions';

  /** Adapter being tested (e.g., 'ollama', 'openai', 'anthropic') */
  @TextField()
  adapterName!: string;

  /** Optional model to test with */
  @TextField({ nullable: true })
  modelName?: string;

  /** Optional specific capability to test */
  @TextField({ nullable: true })
  capability?: string;

  /** Full diagnostic suite vs quick test */
  @BooleanField()
  fullTest!: boolean;

  /** Current execution status */
  @TextField()
  status!: TestExecutionStatus;

  /** Progress tracking (stored as JSON) */
  @JsonField()
  progress!: TestProgress;

  /** Test results (stored as JSON array) */
  @JsonField()
  testResults!: CapabilityTestResult[];

  /** Adapter availability */
  @BooleanField()
  available!: boolean;

  /** Adapter health status */
  @BooleanField()
  healthy!: boolean;

  /** Capabilities this adapter claims to support (stored as JSON array) */
  @JsonField()
  declaredCapabilities!: string[];

  /** Available models (stored as JSON array, nullable) */
  @JsonField({ nullable: true })
  models?: string[];

  /** Performance metrics (stored as JSON, nullable) */
  @JsonField({ nullable: true })
  performance?: {
    healthCheckTime: number;
    totalTestTime: number;
  };

  /** Summary statistics (stored as JSON) */
  @JsonField()
  summary!: {
    total: number;
    passed: number;
    failed: number;
    skipped: number;
  };

  /** Error message (if failed) */
  @TextField({ nullable: true })
  error?: string;

  /** Additional timestamps beyond inherited createdAt/updatedAt */
  @DateField({ nullable: true })
  startedAt?: Date;

  @DateField({ nullable: true })
  completedAt?: Date;

  get collection(): string {
    return TestExecutionEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.adapterName) {
      return { success: false, error: 'Missing required field: adapterName' };
    }

    if (!this.status) {
      return { success: false, error: 'Missing required field: status' };
    }

    if (!this.progress || typeof this.progress.totalCapabilities !== 'number') {
      return { success: false, error: 'Invalid progress object' };
    }

    return { success: true };
  }
}
