/**
 * RustCoreIPC System Resources Module — CPU, memory, and process monitoring
 *
 * Queries the SystemResourceMonitor singleton in continuum-core for
 * cross-platform system resource stats. Same pattern as GPU module.
 */

import type { RustCoreIPCClientBase } from './base';
import type {
	CpuStats as RustCpuStats,
	MemoryStats as RustMemoryStats,
	SystemResourceSnapshot as RustSnapshot,
	TopProcess as RustTopProcess,
	ProcessStats as RustProcessStats,
	PressureSnapshot as RustPressureSnapshot,
	PressureLevel,
} from '../../../../shared/generated/system';

// ============================================================================
// Types (camelCase for TypeScript consumers)
// ============================================================================

export interface CpuStatsInfo {
	physicalCores: number;
	logicalCores: number;
	globalUsage: number;
	perCoreUsage: number[];
	brand: string;
}

export interface MemoryStatsInfo {
	totalBytes: number;
	usedBytes: number;
	availableBytes: number;
	pressure: number;
	swapTotalBytes: number;
	swapUsedBytes: number;
}

export interface TopProcessInfo {
	pid: number;
	name: string;
	cpuPercent: number;
	memoryBytes: number;
}

export interface ProcessStatsInfo {
	topByCpu: TopProcessInfo[];
	topByMemory: TopProcessInfo[];
}

export interface SystemResourceSnapshotInfo {
	cpu: CpuStatsInfo;
	memory: MemoryStatsInfo;
	processes?: ProcessStatsInfo;
	timestampMs: number;
	uptimeSeconds: number;
}

// ============================================================================
// Helpers
// ============================================================================

function mapCpu(c: RustCpuStats): CpuStatsInfo {
	return {
		physicalCores: Number(c.physical_cores),
		logicalCores: Number(c.logical_cores),
		globalUsage: Number(c.global_usage),
		perCoreUsage: c.per_core_usage.map(Number),
		brand: c.brand,
	};
}

function mapMemory(m: RustMemoryStats): MemoryStatsInfo {
	return {
		totalBytes: Number(m.total_bytes),
		usedBytes: Number(m.used_bytes),
		availableBytes: Number(m.available_bytes),
		pressure: Number(m.pressure),
		swapTotalBytes: Number(m.swap_total_bytes),
		swapUsedBytes: Number(m.swap_used_bytes),
	};
}

function mapProcess(p: RustTopProcess): TopProcessInfo {
	return {
		pid: Number(p.pid),
		name: p.name,
		cpuPercent: Number(p.cpu_percent),
		memoryBytes: Number(p.memory_bytes),
	};
}

function mapProcessStats(ps: RustProcessStats): ProcessStatsInfo {
	return {
		topByCpu: ps.top_by_cpu.map(mapProcess),
		topByMemory: ps.top_by_memory.map(mapProcess),
	};
}

// ============================================================================
// Mixin
// ============================================================================

export interface MemoryGateStatus {
	closed: boolean;
	pressure: number;
	rssBytes: number;
}

export interface PressureSnapshotInfo {
	level: PressureLevel;
	pressure: number;
	rssBytes: number;
	consecutiveAtLevel: number;
}

export interface SystemResourceMixin {
	systemCpu(): Promise<CpuStatsInfo>;
	systemMemory(): Promise<MemoryStatsInfo>;
	systemResources(options?: { includeProcesses?: boolean; topN?: number }): Promise<SystemResourceSnapshotInfo>;
	memoryGateStatus(): Promise<MemoryGateStatus>;
	pressureSnapshot(): Promise<PressureSnapshotInfo>;
}

export function SystemResourceMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements SystemResourceMixin {
		/**
		 * Get CPU stats (refreshes on each call).
		 */
		async systemCpu(): Promise<CpuStatsInfo> {
			const response = await this.request({ command: 'system/cpu' });
			if (!response.success) throw new Error(response.error || 'Failed to get CPU stats');
			return mapCpu(response.result as RustCpuStats);
		}

		/**
		 * Get memory stats (refreshes on each call).
		 */
		async systemMemory(): Promise<MemoryStatsInfo> {
			const response = await this.request({ command: 'system/memory' });
			if (!response.success) throw new Error(response.error || 'Failed to get memory stats');
			return mapMemory(response.result as RustMemoryStats);
		}

		/**
		 * Get full system resource snapshot (CPU + memory + optional processes).
		 */
		async systemResources(options?: { includeProcesses?: boolean; topN?: number }): Promise<SystemResourceSnapshotInfo> {
			const response = await this.request({
				command: 'system/resources',
				...(options?.includeProcesses ? { includeProcesses: true } : {}),
				...(options?.topN ? { topN: options.topN } : {}),
			});
			if (!response.success) throw new Error(response.error || 'Failed to get system resources');

			const r = response.result as RustSnapshot;
			return {
				cpu: mapCpu(r.cpu),
				memory: mapMemory(r.memory),
				processes: r.processes ? mapProcessStats(r.processes) : undefined,
				timestampMs: Number(r.timestamp_ms),
				uptimeSeconds: Number(r.uptime_seconds),
			};
		}

		/**
		 * Check if the memory gate is closed (critical pressure sustained).
		 * TypeScript should check this before expensive operations (LLM calls, training, etc.)
		 */
		async memoryGateStatus(): Promise<MemoryGateStatus> {
			const response = await this.request({ command: 'system/memory-gate' });
			if (!response.success) {
				// If IPC fails, assume gate is open (don't block everything)
				return { closed: false, pressure: 0, rssBytes: 0 };
			}
			const r = response.result as { closed: boolean; pressure: number; rss_bytes: number };
			return {
				closed: r.closed,
				pressure: r.pressure,
				rssBytes: Number(r.rss_bytes),
			};
		}

		/**
		 * Get full pressure snapshot (graduated levels, not just binary gate).
		 * Returns { level, pressure, rssBytes, consecutiveAtLevel }.
		 */
		async pressureSnapshot(): Promise<PressureSnapshotInfo> {
			const response = await this.request({ command: 'system/pressure' });
			if (!response.success) {
				return { level: 'normal' as PressureLevel, pressure: 0, rssBytes: 0, consecutiveAtLevel: 0 };
			}
			const r = response.result as RustPressureSnapshot;
			return {
				level: r.level,
				pressure: r.pressure,
				rssBytes: Number(r.rss_bytes),
				consecutiveAtLevel: r.consecutive_at_level,
			};
		}
	};
}
