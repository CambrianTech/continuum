/**
 * RustCoreIPC GPU Module - GPU memory manager stats and pressure
 *
 * Queries the GpuMemoryManager singleton in continuum-core for real
 * VRAM detection, per-subsystem budgets, and memory pressure.
 */

import type { RustCoreIPCClientBase } from './base';
import type { GpuStats as RustGpuStats, SubsystemStats as RustSubsystemStats, AllocationsByPriority as RustAllocationsByPriority, EvictableEntry as RustEvictableEntry, EvictionRegistrySnapshot as RustEvictionRegistrySnapshot } from '../../../../shared/generated/gpu';

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

export interface EvictableEntryInfo {
	id: string;
	label: string;
	priority: string;
	bytes: number;
	allocatedAtMs: number;
	lastUsedMs: number;
	evictable: boolean;
}

export interface EvictionRegistrySnapshotInfo {
	entries: EvictableEntryInfo[];
	totalTrackedBytes: number;
	evictableCount: number;
}

// ============================================================================
// Mixin
// ============================================================================

export interface GpuMixin {
	gpuStats(): Promise<GpuStatsResponse>;
	gpuPressure(): Promise<number>;
	gpuEvictionRegistry(): Promise<EvictionRegistrySnapshotInfo>;
	gpuEvictionCandidates(): Promise<EvictableEntryInfo[]>;
}

function mapSubsystem(s: RustSubsystemStats): SubsystemInfo {
	return { budgetMb: Number(s.budget_mb), usedMb: Number(s.used_mb) };
}

function mapEvictableEntry(e: RustEvictableEntry): EvictableEntryInfo {
	return {
		id: e.id,
		label: e.label,
		priority: e.priority,
		bytes: Number(e.bytes),
		allocatedAtMs: Number(e.allocated_at_ms),
		lastUsedMs: Number(e.last_used_ms),
		evictable: e.evictable,
	};
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

		/**
		 * Get full eviction registry snapshot (all tracked GPU consumers).
		 */
		async gpuEvictionRegistry(): Promise<EvictionRegistrySnapshotInfo> {
			const response = await this.request({ command: 'gpu/eviction-registry' });
			if (!response.success) throw new Error(response.error || 'Failed to get eviction registry');
			const r = response.result as RustEvictionRegistrySnapshot;
			return {
				entries: r.entries.map(mapEvictableEntry),
				totalTrackedBytes: Number(r.total_tracked_bytes),
				evictableCount: Number(r.evictable_count),
			};
		}

		/**
		 * Get eviction candidates sorted by score (highest = evict first).
		 * Excludes non-evictable entries (Realtime priority).
		 */
		async gpuEvictionCandidates(): Promise<EvictableEntryInfo[]> {
			const response = await this.request({ command: 'gpu/eviction-candidates' });
			if (!response.success) throw new Error(response.error || 'Failed to get eviction candidates');
			return (response.result as RustEvictableEntry[]).map(mapEvictableEntry);
		}
	};
}
