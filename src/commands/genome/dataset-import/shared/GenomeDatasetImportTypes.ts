/**
 * Genome Dataset Import Command - Shared Types
 *
 * Imports training datasets from external sources (CSV, RealClassEval) into
 * the .continuum/datasets/ directory. All heavy I/O delegated to Rust DatasetModule.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Dataset Import Command Parameters
 */
export interface GenomeDatasetImportParams extends CommandParams {
  /** Dataset source type: 'csv' for generic CSV, 'realclasseval' for RealClassEval benchmark */
  source: 'csv' | 'realclasseval';
  /** Path to the CSV file (auto-resolved for realclasseval if omitted) */
  csvPath?: string;
  /** [realclasseval] Path to tests directory */
  testsDir?: string;
  /** Output directory (default: .continuum/datasets/<source>/) */
  outputDir?: string;
  /** Train/eval split ratio (default: 0.8) */
  splitRatio?: number;
  /** [csv] Column name for user/input content (default: 'input') */
  userColumn?: string;
  /** [csv] Column name for assistant/output content (default: 'output') */
  assistantColumn?: string;
  /** [csv] Dataset name for manifest (default: 'imported') */
  name?: string;
  /** List available datasets instead of importing */
  list?: boolean;
}

/**
 * Factory function for creating GenomeDatasetImportParams
 */
export const createGenomeDatasetImportParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    source: 'csv' | 'realclasseval';
    csvPath: string;
    testsDir?: string;
    outputDir?: string;
    splitRatio?: number;
    userColumn?: string;
    assistantColumn?: string;
    name?: string;
    list?: boolean;
  }
): GenomeDatasetImportParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  splitRatio: data.splitRatio ?? 0.8,
  ...data
});

/**
 * Genome Dataset Import Command Result
 */
export interface GenomeDatasetImportResult extends CommandResult {
  success: boolean;
  /** Dataset name */
  name: string;
  /** Total training examples */
  totalExamples: number;
  /** Train split example count */
  trainExamples: number;
  /** Eval split example count */
  evalExamples: number;
  /** Path to train.jsonl */
  trainPath: string;
  /** Path to eval.jsonl */
  evalPath: string;
  /** Path to manifest.json */
  manifestPath: string;
  /** Source attribution (e.g., "arxiv:2510.26130") */
  source?: string;
  error?: string;
}

/**
 * Factory function for creating GenomeDatasetImportResult with defaults
 */
export const createGenomeDatasetImportResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    name?: string;
    totalExamples?: number;
    trainExamples?: number;
    evalExamples?: number;
    trainPath?: string;
    evalPath?: string;
    manifestPath?: string;
    source?: string;
    error?: string;
  }
): GenomeDatasetImportResult => createPayload(context, sessionId, {
  name: data.name ?? '',
  totalExamples: data.totalExamples ?? 0,
  trainExamples: data.trainExamples ?? 0,
  evalExamples: data.evalExamples ?? 0,
  trainPath: data.trainPath ?? '',
  evalPath: data.evalPath ?? '',
  manifestPath: data.manifestPath ?? '',
  ...data
});

/**
 * Smart inheritance from params
 */
export const createGenomeDatasetImportResultFromParams = (
  params: GenomeDatasetImportParams,
  differences: Omit<GenomeDatasetImportResult, 'context' | 'sessionId' | 'userId'>
): GenomeDatasetImportResult => transformPayload(params, differences);

/**
 * Genome Dataset Import — Type-safe command executor
 */
export const GenomeDatasetImport = {
  execute(params: CommandInput<GenomeDatasetImportParams>): Promise<GenomeDatasetImportResult> {
    return Commands.execute<GenomeDatasetImportParams, GenomeDatasetImportResult>(
      'genome/dataset-import',
      params as Partial<GenomeDatasetImportParams>
    );
  },
  commandName: 'genome/dataset-import' as const,
} as const;
