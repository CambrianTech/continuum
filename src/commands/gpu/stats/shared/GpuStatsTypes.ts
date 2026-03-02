/**
 * Gpu Stats Command - Shared Types
 *
 * Query GPU memory manager stats including VRAM detection, per-subsystem budgets (inference, TTS, rendering), usage tracking, and memory pressure. Returns real hardware data from Metal (macOS) or CUDA APIs.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

// SubsystemInfo and AllocationsByPriorityInfo from IPC mixin (canonical types)
import type { SubsystemInfo, AllocationsByPriorityInfo } from '../../../../workers/continuum-core/bindings/modules/gpu';
export type { SubsystemInfo, AllocationsByPriorityInfo };

/**
 * Gpu Stats Command Parameters
 */
export interface GpuStatsParams extends CommandParams {
  // Filter to specific subsystem: 'inference', 'tts', or 'rendering'. Omit for full stats.
  subsystem?: string;
}

/**
 * Factory function for creating GpuStatsParams
 */
export const createGpuStatsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Filter to specific subsystem: 'inference', 'tts', or 'rendering'. Omit for full stats.
    subsystem?: string;
  }
): GpuStatsParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  subsystem: data.subsystem ?? '',
  ...data
});

/**
 * Gpu Stats Command Result
 */
export interface GpuStatsResult extends CommandResult {
  success: boolean;
  // GPU hardware name (e.g., 'Apple M3 Max', 'NVIDIA RTX 5090')
  gpuName: string;
  // Total detected VRAM in MB
  totalVramMb: number;
  // Total VRAM used across all subsystems in MB
  totalUsedMb: number;
  // Memory pressure 0.0-1.0 (0=idle, 0.6=warning, 0.8=high, 0.95=critical)
  pressure: number;
  // Reserved headroom in MB (5% of total, prevents OOM)
  reserveMb: number;
  // Rendering subsystem budget and usage
  rendering: SubsystemInfo;
  // Inference subsystem budget and usage (models, LoRA adapters)
  inference: SubsystemInfo;
  // TTS subsystem budget and usage
  tts: SubsystemInfo;
  // Pressure thresholds
  warningThreshold: number;
  highThreshold: number;
  criticalThreshold: number;
  // Live allocation counts per priority level (Realtime/Interactive/Background/Batch)
  allocationsByPriority: AllocationsByPriorityInfo;
  error?: JTAGError;
}

/**
 * Factory function for creating GpuStatsResult.
 * All fields are REQUIRED — GPU stats must come from real hardware detection.
 * Zero defaults would silently hide missing data.
 */
export const createGpuStatsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    gpuName: string;
    totalVramMb: number;
    totalUsedMb: number;
    pressure: number;
    reserveMb: number;
    rendering: SubsystemInfo;
    inference: SubsystemInfo;
    tts: SubsystemInfo;
    warningThreshold: number;
    highThreshold: number;
    criticalThreshold: number;
    allocationsByPriority: AllocationsByPriorityInfo;
    error?: JTAGError;
  }
): GpuStatsResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart Gpu Stats-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGpuStatsResultFromParams = (
  params: GpuStatsParams,
  differences: Omit<GpuStatsResult, 'context' | 'sessionId' | 'userId'>
): GpuStatsResult => transformPayload(params, differences);

/**
 * Gpu Stats — Type-safe command executor
 *
 * Usage:
 *   import { GpuStats } from '...shared/GpuStatsTypes';
 *   const result = await GpuStats.execute({ ... });
 */
export const GpuStats = {
  execute(params: CommandInput<GpuStatsParams>): Promise<GpuStatsResult> {
    return Commands.execute<GpuStatsParams, GpuStatsResult>('gpu/stats', params as Partial<GpuStatsParams>);
  },
  commandName: 'gpu/stats' as const,
} as const;
