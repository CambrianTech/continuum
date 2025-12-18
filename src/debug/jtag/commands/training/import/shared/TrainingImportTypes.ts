/**
 * Training Import Command - Shared Types
 *
 * Imports JSONL training data into SQLite database for MLX fine-tuning.
 * Uses multi-database handle system for isolation.
 */

import type { CommandParams, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DbHandle } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';

/**
 * Training Import Parameters
 */
export interface TrainingImportParams extends CommandParams {
  /** Path to JSONL file (absolute or relative to datasets/parsed/) */
  readonly jsonlPath: string;

  /** Target SQLite database path (if not provided, creates in .continuum/training/) */
  readonly outputPath?: string;

  /** Dataset name for tracking */
  readonly datasetName: string;

  /** Target skill this dataset trains */
  readonly targetSkill: string;

  /** Batch size for import (default: 100) */
  readonly batchSize?: number;

  /** Maximum examples to import (default: all) */
  readonly maxExamples?: number;

  /** Whether to create indices for fast querying (default: true) */
  readonly createIndices?: boolean;
}

/**
 * Training example schema (matches MLX expectations)
 */
export interface TrainingExample {
  readonly id: string;                           // UUID for tracking
  readonly messages: readonly TrainingMessage[]; // Conversation turns
  readonly metadata?: Record<string, unknown>;   // Optional metadata
}

/**
 * Individual message in training conversation
 */
export interface TrainingMessage {
  readonly role: 'system' | 'user' | 'assistant';
  readonly content: string;
}

/**
 * Import statistics
 */
export interface ImportStats {
  readonly totalExamples: number;
  readonly importedExamples: number;
  readonly skippedExamples: number;
  readonly totalTokens: number;
  readonly averageLength: number;
  readonly durationMs: number;
}

/**
 * Training Import Result
 */
export interface TrainingImportResult extends JTAGPayload {
  readonly success: boolean;
  readonly dbHandle: DbHandle;       // Handle for the training database
  readonly dbPath: string;           // Path to created database
  readonly stats: ImportStats;
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * Factory function for creating training/import params
 */
export const createTrainingImportParams = (
  context: JTAGPayload['context'],
  sessionId: UUID,
  data: Omit<TrainingImportParams, 'context' | 'sessionId'>
): TrainingImportParams => createPayload(context, sessionId, data);

/**
 * Transform params to result
 */
export const createTrainingImportResultFromParams = (
  params: TrainingImportParams,
  differences: Omit<Partial<TrainingImportResult>, 'context' | 'sessionId'>
): TrainingImportResult => transformPayload(params, {
  success: false,
  dbHandle: 'default' as DbHandle,
  dbPath: '',
  stats: {
    totalExamples: 0,
    importedExamples: 0,
    skippedExamples: 0,
    totalTokens: 0,
    averageLength: 0,
    durationMs: 0
  },
  timestamp: new Date().toISOString(),
  ...differences
});

// Re-export DbHandle type
export type { DbHandle };
