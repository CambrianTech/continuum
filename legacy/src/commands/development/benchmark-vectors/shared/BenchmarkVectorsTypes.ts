/**
 * BenchmarkVectorsTypes - Types for vector operation benchmarking
 *
 * Measures performance of:
 * - Cosine similarity (single pair)
 * - Batch similarity (one-to-many)
 * - Full vector search (all-to-all ranking)
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Benchmark parameters
 */
export interface BenchmarkVectorsParams extends CommandParams {
  /** Number of vectors to generate for testing (default: 1000) */
  vectorCount?: number;

  /** Vector dimensions (default: 384 for all-minilm) */
  dimensions?: number;

  /** Number of iterations for averaging (default: 10) */
  iterations?: number;

  /** Run specific benchmark only: 'single' | 'batch' | 'search' | 'all' */
  benchmark?: 'single' | 'batch' | 'search' | 'all';
}

/**
 * Individual benchmark result
 */
export interface BenchmarkResult {
  name: string;
  iterations: number;
  totalTimeMs: number;
  avgTimeMs: number;
  minTimeMs: number;
  maxTimeMs: number;
  opsPerSecond: number;
  details?: string;
}

/**
 * Full benchmark results
 */
export interface BenchmarkVectorsResult extends CommandResult {
  success: boolean;
  error?: string;
  vectorCount: number;
  dimensions: number;
  benchmarks: BenchmarkResult[];
  summary: {
    totalTimeMs: number;
    recommendations: string[];
  };
}

/**
 * BenchmarkVectors — Type-safe command executor
 *
 * Usage:
 *   import { BenchmarkVectors } from '...shared/BenchmarkVectorsTypes';
 *   const result = await BenchmarkVectors.execute({ ... });
 */
export const BenchmarkVectors = {
  execute(params: CommandInput<BenchmarkVectorsParams>): Promise<BenchmarkVectorsResult> {
    return Commands.execute<BenchmarkVectorsParams, BenchmarkVectorsResult>('development/benchmark-vectors', params as Partial<BenchmarkVectorsParams>);
  },
  commandName: 'development/benchmark-vectors' as const,
} as const;

/**
 * Factory function for creating DevelopmentBenchmarkVectorsParams
 */
export const createDevelopmentBenchmarkVectorsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<BenchmarkVectorsParams, 'context' | 'sessionId' | 'userId'>
): BenchmarkVectorsParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating DevelopmentBenchmarkVectorsResult with defaults
 */
export const createDevelopmentBenchmarkVectorsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<BenchmarkVectorsResult, 'context' | 'sessionId' | 'userId'>
): BenchmarkVectorsResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart development/benchmark-vectors-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDevelopmentBenchmarkVectorsResultFromParams = (
  params: BenchmarkVectorsParams,
  differences: Omit<BenchmarkVectorsResult, 'context' | 'sessionId' | 'userId'>
): BenchmarkVectorsResult => transformPayload(params, differences);

