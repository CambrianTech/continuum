/**
 * TrainingJobEntity — Persistent local training job state
 *
 * Tracks the full lifecycle of a local PEFT/LoRA training run:
 *   pending → running → checkpointed → completed | failed | crashed
 *
 * Survives server restarts and node crashes. On startup, scan for
 * status='running' jobs → check for checkpoints → auto-resume.
 *
 * This is for LOCAL training (peft-train.py via Sentinel).
 * Cloud fine-tuning uses FineTuningJobEntity (separate, to be unified later).
 *
 * @see #365 — Training job persistence: checkpoint resume, crash recovery
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  EnumField,
  JsonField,
  ForeignKeyField,
  DateField,
  TEXT_LENGTH,
} from '../../data/decorators/FieldDecorators';
import { BaseEntity } from '../../data/entities/BaseEntity';

/** Training job lifecycle status */
export type TrainingJobStatus =
  | 'pending'       // Created, not yet started
  | 'running'       // Actively training
  | 'checkpointed'  // Running, has at least one checkpoint saved
  | 'completed'     // Finished successfully
  | 'failed'        // Finished with error
  | 'crashed'       // Process died unexpectedly (recoverable if checkpoint exists)
  | 'cancelled';    // User-cancelled

/** Checkpoint snapshot */
export interface CheckpointInfo {
  path: string;
  step: number;
  timestamp: number;
  sizeMb: number;
}

/** Training hyperparameters (recorded for resume) */
export interface TrainingConfig {
  baseModel: string;
  rank: number;
  alpha: number;
  epochs: number;
  learningRate: number;
  batchSize: number;
  quantize: boolean;
  quantizeBits: 4 | 8;
}

export class TrainingJobEntity extends BaseEntity {
  static readonly collection = 'training_jobs';

  get collection(): string {
    return TrainingJobEntity.collection;
  }

  // ── Identity ──────────────────────────────────────────────────────────

  /** Persona being trained */
  @ForeignKeyField({ references: 'users', index: true })
  personaId!: UUID;

  /** Display name for UI */
  @TextField({ maxLength: TEXT_LENGTH.DEFAULT })
  personaName!: string;

  /** Domain/trait being trained (e.g., 'conversational', 'skill', 'code') */
  @TextField({ maxLength: TEXT_LENGTH.DEFAULT, index: true })
  domain!: string;

  // ── Status ────────────────────────────────────────────────────────────

  @EnumField({ index: true })
  status!: TrainingJobStatus;

  /** Error message if status is 'failed' or 'crashed' */
  @TextField({ maxLength: TEXT_LENGTH.LONG, nullable: true })
  error?: string;

  // ── Sentinel/Process ──────────────────────────────────────────────────

  /** Rust sentinel handle for the training process */
  @TextField({ maxLength: TEXT_LENGTH.DEFAULT, nullable: true, index: true })
  sentinelHandle?: string;

  /** Grid node running this job (nodeId or 'local') */
  @TextField({ maxLength: TEXT_LENGTH.DEFAULT, index: true })
  nodeId!: string;

  // ── Paths ─────────────────────────────────────────────────────────────

  /** Output directory for adapter weights + checkpoints */
  @TextField({ maxLength: TEXT_LENGTH.LONG })
  outputDir!: string;

  /** Path to JSONL training dataset */
  @TextField({ maxLength: TEXT_LENGTH.LONG })
  datasetPath!: string;

  /** Path to training config JSON (passed to peft-train.py) */
  @TextField({ maxLength: TEXT_LENGTH.LONG, nullable: true })
  configPath?: string;

  // ── Training Config ───────────────────────────────────────────────────

  @JsonField()
  trainingConfig!: TrainingConfig;

  /** Number of training examples */
  @NumberField()
  exampleCount!: number;

  // ── Progress ──────────────────────────────────────────────────────────

  /** Current training step (updated by TrainingStepBridge) */
  @NumberField()
  currentStep!: number;

  /** Total estimated steps (computed from epochs × examples / batchSize) */
  @NumberField()
  totalSteps!: number;

  /** Current loss value (updated per step) */
  @NumberField()
  currentLoss!: number;

  /** Current learning rate */
  @NumberField()
  currentLearningRate!: number;

  /** Current epoch (fractional) */
  @NumberField()
  currentEpoch!: number;

  // ── Checkpoints ───────────────────────────────────────────────────────

  /** All known checkpoints (sorted by step, max ~6 kept on disk) */
  @JsonField()
  checkpoints!: CheckpointInfo[];

  /** Path to the latest checkpoint (shortcut for resume) */
  @TextField({ maxLength: TEXT_LENGTH.LONG, nullable: true })
  latestCheckpointPath?: string;

  /** Step of the latest checkpoint */
  @NumberField({ nullable: true })
  latestCheckpointStep?: number;

  /** Timestamp of the latest checkpoint */
  @DateField({ nullable: true })
  latestCheckpointAt?: Date;

  // ── Timing ────────────────────────────────────────────────────────────

  @DateField({ index: true })
  startedAt!: Date;

  @DateField({ nullable: true })
  completedAt?: Date;

  /** Total wall-clock training time in ms (accumulated across crashes/resumes) */
  @NumberField()
  totalTrainingTimeMs!: number;

  // ── Crash Recovery ────────────────────────────────────────────────────

  /** Number of times this job has been resumed after a crash */
  @NumberField()
  crashCount!: number;

  /** Loss history for charting (appended per step, truncated to last 1000) */
  @JsonField()
  lossHistory!: number[];

  // ── Validation ─────────────────────────────────────────────────────────

  validate(): { success: boolean; error?: string } {
    if (!this.personaId) return { success: false, error: 'personaId is required' };
    if (!this.personaName) return { success: false, error: 'personaName is required' };
    if (!this.domain) return { success: false, error: 'domain is required' };
    if (!this.status) return { success: false, error: 'status is required' };
    if (!this.outputDir) return { success: false, error: 'outputDir is required' };
    if (!this.datasetPath) return { success: false, error: 'datasetPath is required' };
    return { success: true };
  }

  // ── Lifecycle Methods ─────────────────────────────────────────────────

  /** Create a new training job in 'pending' state */
  static createJob(params: {
    personaId: UUID;
    personaName: string;
    domain: string;
    nodeId: string;
    outputDir: string;
    datasetPath: string;
    configPath?: string;
    trainingConfig: TrainingConfig;
    exampleCount: number;
    totalSteps: number;
  }): TrainingJobEntity {
    const job = new TrainingJobEntity();
    job.personaId = params.personaId;
    job.personaName = params.personaName;
    job.domain = params.domain;
    job.status = 'pending';
    job.nodeId = params.nodeId;
    job.outputDir = params.outputDir;
    job.datasetPath = params.datasetPath;
    job.configPath = params.configPath;
    job.trainingConfig = params.trainingConfig;
    job.exampleCount = params.exampleCount;
    job.currentStep = 0;
    job.totalSteps = params.totalSteps;
    job.currentLoss = 0;
    job.currentLearningRate = 0;
    job.currentEpoch = 0;
    job.checkpoints = [];
    job.startedAt = new Date();
    job.totalTrainingTimeMs = 0;
    job.crashCount = 0;
    job.lossHistory = [];
    return job;
  }

  /** Mark as running with a sentinel handle */
  markRunning(sentinelHandle: string): void {
    this.status = 'running';
    this.sentinelHandle = sentinelHandle;
  }

  /** Record a training step */
  recordStep(step: number, loss: number, lr: number, epoch?: number): void {
    this.currentStep = step;
    this.currentLoss = loss;
    this.currentLearningRate = lr;
    if (epoch != null) this.currentEpoch = epoch;

    // Append to loss history (cap at 1000 for DB size)
    this.lossHistory.push(loss);
    if (this.lossHistory.length > 1000) {
      this.lossHistory = this.lossHistory.slice(-1000);
    }

    // Upgrade status to 'checkpointed' if we have checkpoints
    if (this.status === 'running' && this.checkpoints.length > 0) {
      this.status = 'checkpointed';
    }
  }

  /** Record a checkpoint save */
  recordCheckpoint(checkpoint: CheckpointInfo): void {
    this.checkpoints.push(checkpoint);
    this.latestCheckpointPath = checkpoint.path;
    this.latestCheckpointStep = checkpoint.step;
    this.latestCheckpointAt = new Date(checkpoint.timestamp);

    if (this.status === 'running') {
      this.status = 'checkpointed';
    }
  }

  /** Mark as completed successfully */
  markCompleted(finalLoss: number, trainingTimeMs: number): void {
    this.status = 'completed';
    this.currentLoss = finalLoss;
    this.completedAt = new Date();
    this.totalTrainingTimeMs += trainingTimeMs;
  }

  /** Mark as failed with error */
  markFailed(error: string): void {
    this.status = 'failed';
    this.error = error;
    this.completedAt = new Date();
  }

  /** Mark as crashed (process died, may be resumable) */
  markCrashed(): void {
    this.status = 'crashed';
    this.sentinelHandle = undefined;
  }

  /** Mark as resumed from crash (increments crash count) */
  markResumed(newSentinelHandle: string): void {
    this.status = 'running';
    this.sentinelHandle = newSentinelHandle;
    this.crashCount++;
    this.error = undefined;
  }

  /** Mark as cancelled */
  markCancelled(): void {
    this.status = 'cancelled';
    this.completedAt = new Date();
  }

  /** Whether this job can be resumed (has a checkpoint and isn't terminal-success) */
  get resumable(): boolean {
    return (
      (this.status === 'crashed' || this.status === 'failed') &&
      this.checkpoints.length > 0
    );
  }

  /** Whether this job is actively running */
  get active(): boolean {
    return this.status === 'running' || this.status === 'checkpointed';
  }

  /** Whether this job is in a terminal state */
  get terminal(): boolean {
    return this.status === 'completed' || this.status === 'cancelled';
  }

  /** Progress percentage (0-100) */
  get progress(): number {
    if (this.totalSteps === 0) return 0;
    return Math.min(100, (this.currentStep / this.totalSteps) * 100);
  }
}
