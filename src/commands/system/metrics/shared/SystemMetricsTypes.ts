/**
 * System Metrics Command - Shared Types
 *
 * Query historical system metrics (CPU, GPU, memory) for dashboard graphs.
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';

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
