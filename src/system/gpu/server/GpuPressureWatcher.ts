/**
 * GpuPressureWatcher — Singleton that monitors GPU memory pressure and emits
 * Events on threshold crossings.
 *
 * ONE place monitors pressure, ALL consumers subscribe. The compression principle.
 *
 * Adaptive polling intervals:
 *   Normal   (< 60%):  10s  — idle system, no urgency
 *   Warning  (60-80%):  3s  — need to track trends
 *   High     (80-95%):  1s  — active management
 *   Critical (>= 95%): 500ms — emergency
 *
 * Events emitted on threshold crossings:
 *   gpu:pressure:normal   — crossed DOWN below 60%
 *   gpu:pressure:warning  — crossed UP above 60%
 *   gpu:pressure:high     — crossed UP above 80%
 *   gpu:pressure:critical — crossed UP above 95%
 *   gpu:pressure:update   — every poll (for trend tracking)
 */

import { Events } from '@system/core/shared/Events';
import { RustCoreIPCClient } from '../../../workers/continuum-core/bindings/RustCoreIPC';

// ============================================================================
// Types
// ============================================================================

export type PressureLevel = 'normal' | 'warning' | 'high' | 'critical';

export interface PressureUpdate {
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
const INTERVAL_WARNING_MS = 3_000;
const INTERVAL_HIGH_MS = 1_000;
const INTERVAL_CRITICAL_MS = 500;

// ============================================================================
// GpuPressureWatcher
// ============================================================================

export class GpuPressureWatcher {
	private static _instance: GpuPressureWatcher | null = null;

	private _pressure = 0;
	private _level: PressureLevel = 'normal';
	private _timer: ReturnType<typeof setTimeout> | null = null;
	private _running = false;

	static get instance(): GpuPressureWatcher {
		if (!GpuPressureWatcher._instance) {
			GpuPressureWatcher._instance = new GpuPressureWatcher();
		}
		return GpuPressureWatcher._instance;
	}

	/** Current cached pressure value (0.0-1.0). */
	get pressure(): number {
		return this._pressure;
	}

	/** Current pressure level. */
	get currentLevel(): PressureLevel {
		return this._level;
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
		console.log('[GpuPressureWatcher] Started (interval: 10s)');
	}

	/** Stop polling. */
	stop(): void {
		this._running = false;
		if (this._timer) {
			clearTimeout(this._timer);
			this._timer = null;
		}
		console.log('[GpuPressureWatcher] Stopped');
	}

	// ── Internal ──────────────────────────────────────────────────────

	private scheduleNextPoll(): void {
		if (!this._running) return;
		const interval = GpuPressureWatcher.intervalForLevel(this._level);
		this._timer = setTimeout(() => this.poll(), interval);
	}

	private async poll(): Promise<void> {
		if (!this._running) return;

		try {
			const client = await RustCoreIPCClient.getInstanceAsync();
			const pressure = await client.gpuPressure();
			this._pressure = pressure;

			const newLevel = GpuPressureWatcher.levelForPressure(pressure);
			const previousLevel = this._level;

			// Emit on every poll for trend tracking
			const update: PressureUpdate = {
				pressure,
				level: newLevel,
				previousLevel,
				timestamp: Date.now(),
			};
			Events.emit('gpu:pressure:update', update).catch(() => {});

			// Emit threshold crossing events
			if (newLevel !== previousLevel) {
				this._level = newLevel;
				Events.emit(`gpu:pressure:${newLevel}`, update).catch(() => {});
				console.log(
					`[GpuPressureWatcher] ${previousLevel} → ${newLevel} ` +
					`(${(pressure * 100).toFixed(0)}%, interval: ${GpuPressureWatcher.intervalForLevel(newLevel)}ms)`
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
}
