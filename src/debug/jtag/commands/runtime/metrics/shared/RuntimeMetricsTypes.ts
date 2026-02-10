/**
 * Runtime Metrics Command - Shared Types
 *
 * Query Rust module performance metrics including latency percentiles, command counts, and slow command tracking.
 * Enables AI-driven system analysis and optimization (Ares pattern).
 *
 * Uses ts-rs generated types from Rust as source of truth for wire format.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

// Re-export ts-rs generated types from Rust (source of truth)
export type { ModuleStats, ModulePriority, CommandTiming, ModuleInfo } from '@shared/generated/runtime';

/**
 * Module performance metrics (flattened from Rust ModuleStats)
 */
export interface ModuleMetrics {
  moduleName: string;
  totalCommands: number;
  avgTimeMs: number;
  slowCommandCount: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
}

/**
 * Slow command record
 */
export interface SlowCommand {
  module: string;
  command: string;
  totalMs: number;
  executeMs: number;
  queueMs: number;
}

/**
 * Module configuration (from runtime/list)
 */
export interface ModuleConfig {
  name: string;
  priority: string;
  commandPrefixes: string[];
  needsDedicatedThread: boolean;
  maxConcurrency: number;
}

/**
 * Runtime Metrics Command Parameters
 */
export interface RuntimeMetricsParams extends CommandParams {
  // Query mode: 'all' for all modules (default), 'module' for specific module, 'slow' for recent slow commands, 'list' for module configs
  mode?: 'all' | 'module' | 'slow' | 'list';
  // Module name when mode='module' (e.g., 'data', 'embedding', 'cognition')
  module?: string;
}

/**
 * Factory function for creating RuntimeMetricsParams
 */
export const createRuntimeMetricsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Query mode: 'all' for all modules (default), 'module' for specific module, 'slow' for recent slow commands, 'list' for module configs
    mode?: 'all' | 'module' | 'slow' | 'list';
    // Module name when mode='module' (e.g., 'data', 'embedding', 'cognition')
    module?: string;
  }
): RuntimeMetricsParams => createPayload(context, sessionId, {
  mode: data.mode ?? undefined,
  module: data.module ?? '',
  ...data
});

/**
 * Runtime Metrics Command Result
 */
export interface RuntimeMetricsResult extends CommandResult {
  success: boolean;
  // Array of module metrics (when mode='all' or 'module')
  modules: ModuleMetrics[];
  // Array of slow commands (when mode='slow')
  slowCommands: SlowCommand[];
  // Array of module configurations (when mode='list')
  moduleConfigs: ModuleConfig[];
  // Number of items in the result
  count: number;
  // Slow command threshold in ms (when mode='slow')
  thresholdMs: number;
  error?: JTAGError;
}

/**
 * Factory function for creating RuntimeMetricsResult with defaults
 */
export const createRuntimeMetricsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Array of module metrics (when mode='all' or 'module')
    modules?: ModuleMetrics[];
    // Array of slow commands (when mode='slow')
    slowCommands?: SlowCommand[];
    // Array of module configurations (when mode='list')
    moduleConfigs?: ModuleConfig[];
    // Number of items in the result
    count?: number;
    // Slow command threshold in ms (when mode='slow')
    thresholdMs?: number;
    error?: JTAGError;
  }
): RuntimeMetricsResult => createPayload(context, sessionId, {
  modules: data.modules ?? [],
  slowCommands: data.slowCommands ?? [],
  moduleConfigs: data.moduleConfigs ?? [],
  count: data.count ?? 0,
  thresholdMs: data.thresholdMs ?? 0,
  ...data
});

/**
 * Smart Runtime Metrics-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createRuntimeMetricsResultFromParams = (
  params: RuntimeMetricsParams,
  differences: Omit<RuntimeMetricsResult, 'context' | 'sessionId'>
): RuntimeMetricsResult => transformPayload(params, differences);

/**
 * Runtime Metrics — Type-safe command executor
 *
 * Usage:
 *   import { RuntimeMetrics } from '...shared/RuntimeMetricsTypes';
 *   const result = await RuntimeMetrics.execute({ ... });
 */
export const RuntimeMetrics = {
  execute(params: CommandInput<RuntimeMetricsParams>): Promise<RuntimeMetricsResult> {
    return Commands.execute<RuntimeMetricsParams, RuntimeMetricsResult>('runtime/metrics', params as Partial<RuntimeMetricsParams>);
  },
  commandName: 'runtime/metrics' as const,
} as const;
