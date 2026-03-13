/**
 * System Metrics Command - Shared Types
 *
 * Query historical system metrics (CPU, GPU, memory) for dashboard graphs.
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Time-series data point for system metrics
 */
export interface SystemMetricsPoint {
  timestamp: number;
  cpuUsage: number;
  memoryPressure: number;
  memoryUsedMb: number;
  memoryTotalMb: number;
  gpuPressure: number;
  gpuUsedMb: number;
  gpuTotalMb: number;
}

/**
 * System Metrics Command Parameters
 */
export interface SystemMetricsParams extends CommandParams {
  /** Time range: '1h', '6h', '24h', '7d', '30d' */
  range?: string;

  /** Max number of data points to return (downsampled if needed) */
  maxPoints?: number;
}

/**
 * System Metrics Command Result
 */
export interface SystemMetricsResult extends CommandResult {
  success: boolean;

  /** Time-series data points (oldest first) */
  timeSeries: SystemMetricsPoint[];

  /** Summary of current/latest values */
  current: {
    cpuUsage: number;
    memoryPressure: number;
    memoryUsedMb: number;
    memoryTotalMb: number;
    gpuPressure: number;
    gpuUsedMb: number;
    gpuTotalMb: number;
    gpuName?: string;
  };

  /** Total number of samples in the database */
  totalSamples: number;

  error?: string;
}

/**
 * SystemMetrics — Type-safe command executor
 */
export const SystemMetrics = {
  execute(params: CommandInput<SystemMetricsParams>): Promise<SystemMetricsResult> {
    return Commands.execute<SystemMetricsParams, SystemMetricsResult>(
      'system/metrics', params as Partial<SystemMetricsParams>
    );
  },
  commandName: 'system/metrics' as const,
} as const;

/**
 * Factory function for creating SystemMetricsParams
 */
export const createSystemMetricsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SystemMetricsParams, 'context' | 'sessionId' | 'userId'>
): SystemMetricsParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SystemMetricsResult with defaults
 */
export const createSystemMetricsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SystemMetricsResult, 'context' | 'sessionId' | 'userId'>
): SystemMetricsResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart system/metrics-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSystemMetricsResultFromParams = (
  params: SystemMetricsParams,
  differences: Omit<SystemMetricsResult, 'context' | 'sessionId' | 'userId'>
): SystemMetricsResult => transformPayload(params, differences);

