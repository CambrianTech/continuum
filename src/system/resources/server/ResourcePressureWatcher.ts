/**
 * ResourcePressureWatcher — Singleton that monitors CPU and memory pressure
 * and emits Events on threshold crossings.
 *
 * ONE place monitors system resources, ALL consumers subscribe. The compression principle.
 * Same architecture as GpuPressureWatcher — different concern, same interface.
 *
 * Adaptive polling intervals:
 *   Normal   (< 60%):  10s  — idle system, no urgency
 *   Warning  (60-80%):  5s  — need to track trends
 *   High     (80-95%):  2s  — active management
 *   Critical (>= 95%):  1s  — emergency
 *
 * Events emitted on threshold crossings:
 *   cpu:pressure:normal    — crossed DOWN below 60%
 *   cpu:pressure:warning   — crossed UP above 60%
 *   cpu:pressure:high      — crossed UP above 80%
 *   cpu:pressure:critical  — crossed UP above 95%
 *   cpu:pressure:update    — every poll (for trend tracking)
 *
 *   memory:pressure:normal    — crossed DOWN below 60%
 *   memory:pressure:warning   — crossed UP above 60%
 *   memory:pressure:high      — crossed UP above 80%
 *   memory:pressure:critical  — crossed UP above 95%
 *   memory:pressure:update    — every poll (for trend tracking)
 */

import { Events } from '@system/core/shared/Events';
import { RustCoreIPCClient } from '../../../workers/continuum-core/bindings/RustCoreIPC';
import type { CpuStatsInfo, MemoryStatsInfo } from '../../../workers/continuum-core/bindings/modules/system_resources';

// ============================================================================
// Types
// ============================================================================

export type PressureLevel = 'normal' | 'warning' | 'high' | 'critical';

export interface ResourcePressureUpdate {
	resource: 'cpu' | 'memory';
	pressure: number;
	level: PressureLevel;
	previousLevel: PressureLevel;
	timestamp: number;
}

// ============================================================================
// Constants
// ============================================================================

const THRESHOLD_WARNING = 0.60;
const THRESHOLD_HIGH = 0.80;
const THRESHOLD_CRITICAL = 0.95;

const INTERVAL_NORMAL_MS = 10_000;
const INTERVAL_WARNING_MS = 5_000;
const INTERVAL_HIGH_MS = 2_000;
const INTERVAL_CRITICAL_MS = 1_000;

// ============================================================================
// ResourcePressureWatcher
// ============================================================================

export class ResourcePressureWatcher {
	private static _instance: ResourcePressureWatcher | null = null;

	private _cpuPressure = 0;
	private _cpuLevel: PressureLevel = 'normal';
	private _memoryPressure = 0;
	private _memoryLevel: PressureLevel = 'normal';
	private _cpu: CpuStatsInfo | null = null;
	private _memory: MemoryStatsInfo | null = null;
	private _timer: ReturnType<typeof setTimeout> | null = null;
	private _running = false;

	static get instance(): ResourcePressureWatcher {
		if (!ResourcePressureWatcher._instance) {
			ResourcePressureWatcher._instance = new ResourcePressureWatcher();
		}
		return ResourcePressureWatcher._instance;
	}

	/** Current cached CPU pressure (0.0 - 1.0). */
	get cpuPressure(): number {
		return this._cpuPressure;
	}

	/** Current CPU pressure level. */
	get cpuLevel(): PressureLevel {
		return this._cpuLevel;
	}

	/** Current cached memory pressure (0.0 - 1.0). */
	get memoryPressure(): number {
		return this._memoryPressure;
	}

	/** Current memory pressure level. */
	get memoryLevel(): PressureLevel {
		return this._memoryLevel;
	}

	/** Full cached CPU stats. */
	get cpu(): CpuStatsInfo | null {
		return this._cpu;
	}

	/** Full cached memory stats. */
	get memory(): MemoryStatsInfo | null {
		return this._memory;
	}

	/** Worst pressure level (max of CPU and memory). */
	get worstLevel(): PressureLevel {
		return ResourcePressureWatcher.worstOf(this._cpuLevel, this._memoryLevel);
	}

	/** Whether the watcher is actively polling. */
	get running(): boolean {
		return this._running;
	}

	/** Start the adaptive polling loop. */
	start(): void {
		if (this._running) return;
		this._running = true;
		this.scheduleNextPoll();
		console.log('[ResourcePressureWatcher] Started (interval: 10s)');
	}

	/** Stop polling. */
	stop(): void {
		this._running = false;
		if (this._timer) {
			clearTimeout(this._timer);
			this._timer = null;
		}
		console.log('[ResourcePressureWatcher] Stopped');
	}

	// ── Internal ──────────────────────────────────────────────────────

	private scheduleNextPoll(): void {
		if (!this._running) return;
		// Use the worst level to drive polling interval — if either is stressed, poll faster
		const interval = ResourcePressureWatcher.intervalForLevel(this.worstLevel);
		this._timer = setTimeout(() => this.poll(), interval);
	}

	private async poll(): Promise<void> {
		if (!this._running) return;

		try {
			const client = await RustCoreIPCClient.getInstanceAsync();
			const snapshot = await client.systemResources();

			// Update CPU
			this._cpu = snapshot.cpu;
			this._cpuPressure = snapshot.cpu.globalUsage;
			const newCpuLevel = ResourcePressureWatcher.levelForPressure(this._cpuPressure);
			const prevCpuLevel = this._cpuLevel;

			const cpuUpdate: ResourcePressureUpdate = {
				resource: 'cpu',
				pressure: this._cpuPressure,
				level: newCpuLevel,
				previousLevel: prevCpuLevel,
				timestamp: Date.now(),
			};
			Events.emit('cpu:pressure:update', cpuUpdate).catch(() => {});

			if (newCpuLevel !== prevCpuLevel) {
				this._cpuLevel = newCpuLevel;
				Events.emit(`cpu:pressure:${newCpuLevel}`, cpuUpdate).catch(() => {});
				console.log(
					`[ResourcePressureWatcher] CPU ${prevCpuLevel} → ${newCpuLevel} ` +
					`(${(this._cpuPressure * 100).toFixed(0)}%, interval: ${ResourcePressureWatcher.intervalForLevel(this.worstLevel)}ms)`
				);
			}

			// Update memory
			this._memory = snapshot.memory;
			this._memoryPressure = snapshot.memory.pressure;
			const newMemLevel = ResourcePressureWatcher.levelForPressure(this._memoryPressure);
			const prevMemLevel = this._memoryLevel;

			const memUpdate: ResourcePressureUpdate = {
				resource: 'memory',
				pressure: this._memoryPressure,
				level: newMemLevel,
				previousLevel: prevMemLevel,
				timestamp: Date.now(),
			};
			Events.emit('memory:pressure:update', memUpdate).catch(() => {});

			if (newMemLevel !== prevMemLevel) {
				this._memoryLevel = newMemLevel;
				Events.emit(`memory:pressure:${newMemLevel}`, memUpdate).catch(() => {});
				console.log(
					`[ResourcePressureWatcher] Memory ${prevMemLevel} → ${newMemLevel} ` +
					`(${(this._memoryPressure * 100).toFixed(0)}%, interval: ${ResourcePressureWatcher.intervalForLevel(this.worstLevel)}ms)`
				);
			}
		} catch {
			// IPC failure — don't crash, just retry at current interval
		}

		this.scheduleNextPoll();
	}

	// ── Static helpers ────────────────────────────────────────────────

	static levelForPressure(pressure: number): PressureLevel {
		if (pressure >= THRESHOLD_CRITICAL) return 'critical';
		if (pressure >= THRESHOLD_HIGH) return 'high';
		if (pressure >= THRESHOLD_WARNING) return 'warning';
		return 'normal';
	}

	static intervalForLevel(level: PressureLevel): number {
		switch (level) {
			case 'critical': return INTERVAL_CRITICAL_MS;
			case 'high':     return INTERVAL_HIGH_MS;
			case 'warning':  return INTERVAL_WARNING_MS;
			case 'normal':   return INTERVAL_NORMAL_MS;
		}
	}

	static worstOf(a: PressureLevel, b: PressureLevel): PressureLevel {
		const order: PressureLevel[] = ['normal', 'warning', 'high', 'critical'];
		const ai = order.indexOf(a);
		const bi = order.indexOf(b);
		return ai >= bi ? a : b;
	}
}
