/**
 * RustCoreIPC GPU Module - GPU memory manager stats and pressure
 *
 * Queries the GpuMemoryManager singleton in continuum-core for real
 * VRAM detection, per-subsystem budgets, and memory pressure.
 */

import type { RustCoreIPCClientBase } from './base';
import type { GpuStats as RustGpuStats, SubsystemStats as RustSubsystemStats } from '../../../../shared/generated/gpu';

// ============================================================================
// Types (camelCase for TypeScript consumers)
// ============================================================================

export interface SubsystemInfo {
	budgetMb: number;
	usedMb: number;
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
			return {
				gpuName: r.gpu_name,
				totalVramMb: Number(r.total_vram_mb),
				totalUsedMb: Number(r.total_used_mb),
				pressure: Number(r.pressure),
				reserveMb: Number(r.reserve_mb),
				rendering: mapSubsystem(r.rendering),
				inference: mapSubsystem(r.inference),
				tts: mapSubsystem(r.tts),
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
