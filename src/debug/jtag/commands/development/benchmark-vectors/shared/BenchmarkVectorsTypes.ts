/**
 * BenchmarkVectorsTypes - Types for vector operation benchmarking
 *
 * Measures performance of:
 * - Cosine similarity (single pair)
 * - Batch similarity (one-to-many)
 * - Full vector search (all-to-all ranking)
 */

import type { CommandParams, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

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
export interface BenchmarkVectorsResult {
  context: JTAGContext;
  sessionId: UUID;
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
