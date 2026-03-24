/**
 * Genome Benchmark Command - Shared Types
 *
 * Run standard and Continuum-specific benchmarks against a model or adapter. Stores results in BenchmarkResultEntity and embeds in adapter manifest for model card publishing. Supports HumanEval, MBPP, RealClassEval, and collaborative team benchmarks.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Benchmark Command Parameters
 */
export interface GenomeBenchmarkParams extends CommandParams {
  // Model path or HuggingFace ID to benchmark. Uses LOCAL_MODELS.DEFAULT if not specified.
  model?: string;
  // Path to LoRA adapter directory to apply on top of base model
  adapter?: string;
  // Comma-separated benchmark suites: humaneval, mbpp, realclasseval, mmlu, collaborative
  suite: string;
  // Team project ID for collaborative benchmark (computes from existing session data)
  teamProjectId?: string;
  // Academy session ID for phenotype benchmark (extracts before/after scores)
  academySessionId?: string;
  // Path to write benchmark results JSON. Default: stdout
  output?: string;
  // Max problems to run per suite (for quick testing). Default: all
  limit?: number;
}

/**
 * Factory function for creating GenomeBenchmarkParams
 */
export const createGenomeBenchmarkParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Model path or HuggingFace ID to benchmark. Uses LOCAL_MODELS.DEFAULT if not specified.
    model?: string;
    // Path to LoRA adapter directory to apply on top of base model
    adapter?: string;
    // Comma-separated benchmark suites: humaneval, mbpp, realclasseval, mmlu, collaborative
    suite: string;
    // Team project ID for collaborative benchmark (computes from existing session data)
    teamProjectId?: string;
    // Academy session ID for phenotype benchmark (extracts before/after scores)
    academySessionId?: string;
    // Path to write benchmark results JSON. Default: stdout
    output?: string;
    // Max problems to run per suite (for quick testing). Default: all
    limit?: number;
  }
): GenomeBenchmarkParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  model: data.model ?? '',
  adapter: data.adapter ?? '',
  teamProjectId: data.teamProjectId ?? '',
  academySessionId: data.academySessionId ?? '',
  output: data.output ?? '',
  limit: data.limit ?? 0,
  ...data
});

/**
 * Genome Benchmark Command Result
 */
export interface GenomeBenchmarkResult extends CommandResult {
  success: boolean;
  // Per-suite results: { humaneval: { score, total, passed, ... }, mbpp: { ... } }
  suites: object;
  // Weighted average across all suites (0-100)
  overallScore: number;
  // ID of the stored BenchmarkResultEntity
  benchmarkId: string;
  error?: JTAGError;
}

/**
 * Factory function for creating GenomeBenchmarkResult with defaults
 */
export const createGenomeBenchmarkResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Per-suite results: { humaneval: { score, total, passed, ... }, mbpp: { ... } }
    suites?: object;
    // Weighted average across all suites (0-100)
    overallScore?: number;
    // ID of the stored BenchmarkResultEntity
    benchmarkId?: string;
    error?: JTAGError;
  }
): GenomeBenchmarkResult => createPayload(context, sessionId, {
  suites: data.suites ?? {},
  overallScore: data.overallScore ?? 0,
  benchmarkId: data.benchmarkId ?? '',
  ...data
});

/**
 * Smart Genome Benchmark-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGenomeBenchmarkResultFromParams = (
  params: GenomeBenchmarkParams,
  differences: Omit<GenomeBenchmarkResult, 'context' | 'sessionId' | 'userId'>
): GenomeBenchmarkResult => transformPayload(params, differences);

/**
 * Genome Benchmark — Type-safe command executor
 *
 * Usage:
 *   import { GenomeBenchmark } from '...shared/GenomeBenchmarkTypes';
 *   const result = await GenomeBenchmark.execute({ ... });
 */
export const GenomeBenchmark = {
  execute(params: CommandInput<GenomeBenchmarkParams>): Promise<GenomeBenchmarkResult> {
    return Commands.execute<GenomeBenchmarkParams, GenomeBenchmarkResult>('genome/benchmark', params as Partial<GenomeBenchmarkParams>);
  },
  commandName: 'genome/benchmark' as const,
} as const;
