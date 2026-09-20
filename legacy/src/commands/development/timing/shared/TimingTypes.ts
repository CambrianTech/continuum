/**
 * TimingTypes - Types for analyzing Rust worker timing metrics
 *
 * NOTE: The data-daemon worker has been absorbed into continuum-core DataModule.
 * The timing file /tmp/jtag-data-daemon-timing.jsonl may no longer be written.
 * See TimingServerCommand.ts for details.
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Individual timing record from Rust worker
 */
export interface RustTimingRecord {
  request_id: string;
  timestamp_ms: number;
  request_type: string;
  collection: string | null;
  adapter_handle: string | null;
  socket_read_ns: number;
  parse_ns: number;
  route_ns: number;
  query_build_ns: number;
  lock_wait_ns: number;
  execute_ns: number;
  serialize_ns: number;
  socket_write_ns: number;
  total_ns: number;
  handle_ns: number;
  concurrent_requests: number;
  queue_depth: number;
  result_count: number | null;
  success: boolean;
  error: string | null;
}

/**
 * Timing analysis parameters
 */
export interface TimingParams extends CommandParams {
  /** Time window in minutes (default: 60, 0 = all data) */
  windowMinutes?: number;

  /** Filter by request type (e.g., 'vector/search', 'ping') */
  requestType?: string;

  /** Show detailed breakdown of time spent in each phase */
  showBreakdown?: boolean;

  /** Output format: 'summary' | 'json' | 'table' */
  format?: 'summary' | 'json' | 'table';

  /** Clear timing data after reading */
  clear?: boolean;
}

/**
 * Percentile statistics for a metric
 */
export interface PercentileStats {
  count: number;
  avg: number;
  min: number;
  max: number;
  p50: number;
  p95: number;
  p99: number;
}

/**
 * Timing breakdown by phase
 */
export interface TimingBreakdown {
  socket_read_ms: PercentileStats;
  parse_ms: PercentileStats;
  route_ms: PercentileStats;
  lock_wait_ms: PercentileStats;
  execute_ms: PercentileStats;
  serialize_ms: PercentileStats;
  socket_write_ms: PercentileStats;
  total_ms: PercentileStats;
}

/**
 * Statistics for a single request type
 */
export interface RequestTypeStats {
  type: string;
  count: number;
  total_ms: PercentileStats;
  execute_ms: PercentileStats;
  breakdown?: TimingBreakdown;
}

/**
 * Full timing analysis result
 */
export interface TimingResult extends CommandResult {
  success: boolean;
  error?: string;

  /** Time window analyzed */
  windowMinutes: number;
  recordCount: number;
  oldestRecord?: number;
  newestRecord?: number;

  /** Stats by request type */
  byType: RequestTypeStats[];

  /** Overall stats */
  overall: {
    totalRequests: number;
    successRate: number;
    avgConcurrentRequests: number;
    maxConcurrentRequests: number;
  };

  /** Recommendations based on analysis */
  recommendations: string[];
}

/**
 * Timing — Type-safe command executor
 *
 * Usage:
 *   import { Timing } from '...shared/TimingTypes';
 *   const result = await Timing.execute({ ... });
 */
export const Timing = {
  execute(params: CommandInput<TimingParams>): Promise<TimingResult> {
    return Commands.execute<TimingParams, TimingResult>('development/timing', params as Partial<TimingParams>);
  },
  commandName: 'development/timing' as const,
} as const;

/**
 * Factory function for creating DevelopmentTimingParams
 */
export const createDevelopmentTimingParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<TimingParams, 'context' | 'sessionId' | 'userId'>
): TimingParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating DevelopmentTimingResult with defaults
 */
export const createDevelopmentTimingResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<TimingResult, 'context' | 'sessionId' | 'userId'>
): TimingResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart development/timing-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDevelopmentTimingResultFromParams = (
  params: TimingParams,
  differences: Omit<TimingResult, 'context' | 'sessionId' | 'userId'>
): TimingResult => transformPayload(params, differences);

