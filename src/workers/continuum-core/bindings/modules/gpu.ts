/**
 * RustCoreIPC GPU Module - GPU memory manager stats and pressure
 *
 * Queries the GpuMemoryManager singleton in continuum-core for real
 * VRAM detection, per-subsystem budgets, and memory pressure.
 */

import type { RustCoreIPCClientBase } from './base';
import type { GpuStats as RustGpuStats, SubsystemStats as RustSubsystemStats, AllocationsByPriority as RustAllocationsByPriority } from '../../../../shared/generated/gpu';

// ============================================================================
// Types (camelCase for TypeScript consumers)
// ============================================================================

export interface SubsystemInfo {
	budgetMb: number;
	usedMb: number;
}

export interface AllocationsByPriorityInfo {
	realtime: number;
	interactive: number;
	background: number;
	batch: number;
}

export interface GpuStatsResponse {
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
}

// ============================================================================
// Mixin
// ============================================================================

export interface GpuMixin {
	gpuStats(): Promise<GpuStatsResponse>;
	gpuPressure(): Promise<number>;
}

function mapSubsystem(s: RustSubsystemStats): SubsystemInfo {
	return { budgetMb: Number(s.budget_mb), usedMb: Number(s.used_mb) };
}

export function GpuMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements GpuMixin {
		/**
		 * Get full GPU memory manager stats snapshot.
		 */
		async gpuStats(): Promise<GpuStatsResponse> {
			const response = await this.request({ command: 'gpu/stats' });
			if (!response.success) throw new Error(response.error || 'Failed to get GPU stats');

			// Rust returns snake_case; convert to camelCase
			const r = response.result as RustGpuStats;
			const abp = r.allocations_by_priority;
			return {
				gpuName: r.gpu_name,
				totalVramMb: Number(r.total_vram_mb),
				totalUsedMb: Number(r.total_used_mb),
				pressure: Number(r.pressure),
				reserveMb: Number(r.reserve_mb),
				rendering: mapSubsystem(r.rendering),
				inference: mapSubsystem(r.inference),
				tts: mapSubsystem(r.tts),
				warningThreshold: Number(r.warning_threshold),
				highThreshold: Number(r.high_threshold),
				criticalThreshold: Number(r.critical_threshold),
				allocationsByPriority: {
					realtime: Number(abp.realtime),
					interactive: Number(abp.interactive),
					background: Number(abp.background),
					batch: Number(abp.batch),
				},
			};
		}

		/**
		 * Get current GPU memory pressure (0.0-1.0).
		 */
		async gpuPressure(): Promise<number> {
			const response = await this.request({ command: 'gpu/pressure' });
			if (!response.success) throw new Error(response.error || 'Failed to get GPU pressure');
			return Number((response.result as { pressure: number }).pressure);
		}
	};
}
