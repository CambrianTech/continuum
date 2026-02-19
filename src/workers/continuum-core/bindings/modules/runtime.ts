/**
 * RustCoreIPC Runtime Module - System monitoring and observability
 */

import type { RustCoreIPCClientBase } from './base';

// ============================================================================
// Types
// ============================================================================

export interface ModuleInfo {
	name: string;
	priority: string;
	command_prefixes: string[];
	needs_dedicated_thread: boolean;
	max_concurrency: number;
}

export interface ModuleMetrics {
	moduleName: string;
	totalCommands: number;
	avgTimeMs: number;
	slowCommandCount: number;
	p50Ms: number;
	p95Ms: number;
	p99Ms: number;
}

export interface SlowCommand {
	module: string;
	command: string;
	total_ms: number;
	execute_ms: number;
	queue_ms: number;
}

// ============================================================================
// Mixin
// ============================================================================

export interface RuntimeMixin {
	runtimeList(): Promise<{ modules: ModuleInfo[]; count: number }>;
	runtimeMetricsAll(): Promise<{ modules: ModuleMetrics[]; count: number }>;
	runtimeMetricsModule(moduleName: string): Promise<ModuleMetrics>;
	runtimeMetricsSlow(): Promise<{ slow_commands: SlowCommand[]; count: number; threshold_ms: number }>;
}

export function RuntimeMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements RuntimeMixin {
		/**
		 * List all registered modules with their configurations.
		 */
		async runtimeList(): Promise<{ modules: ModuleInfo[]; count: number }> {
			const response = await this.request({ command: 'runtime/list' });
			if (!response.success) throw new Error(response.error || 'Failed to list runtime modules');
			return response.result as { modules: ModuleInfo[]; count: number };
		}

		/**
		 * Get metrics for all modules.
		 */
		async runtimeMetricsAll(): Promise<{ modules: ModuleMetrics[]; count: number }> {
			const response = await this.request({ command: 'runtime/metrics/all' });
			if (!response.success) throw new Error(response.error || 'Failed to get runtime metrics');

			// Convert bigint fields to number (ts-rs exports u64 as bigint)
			const result = response.result as { modules: any[]; count: number };
			return {
				modules: result.modules.map((m: any) => ({
					moduleName: m.moduleName,
					totalCommands: Number(m.totalCommands),
					avgTimeMs: Number(m.avgTimeMs),
					slowCommandCount: Number(m.slowCommandCount),
					p50Ms: Number(m.p50Ms),
					p95Ms: Number(m.p95Ms),
					p99Ms: Number(m.p99Ms),
				})),
				count: result.count,
			};
		}

		/**
		 * Get metrics for a specific module.
		 */
		async runtimeMetricsModule(moduleName: string): Promise<ModuleMetrics> {
			const response = await this.request({
				command: 'runtime/metrics/module',
				module: moduleName,
			});
			if (!response.success) throw new Error(response.error || `Failed to get metrics for module: ${moduleName}`);

			// Convert bigint fields to number
			const m = response.result as any;
			return {
				moduleName: m.moduleName,
				totalCommands: Number(m.totalCommands),
				avgTimeMs: Number(m.avgTimeMs),
				slowCommandCount: Number(m.slowCommandCount),
				p50Ms: Number(m.p50Ms),
				p95Ms: Number(m.p95Ms),
				p99Ms: Number(m.p99Ms),
			};
		}

		/**
		 * Get list of recent slow commands.
		 */
		async runtimeMetricsSlow(): Promise<{ slow_commands: SlowCommand[]; count: number; threshold_ms: number }> {
			const response = await this.request({ command: 'runtime/metrics/slow' });
			if (!response.success) throw new Error(response.error || 'Failed to get slow commands');
			return response.result as { slow_commands: SlowCommand[]; count: number; threshold_ms: number };
		}
	};
}
