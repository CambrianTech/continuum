/**
 * GpuGovernor — Active GPU budget rebalancing based on workload patterns.
 *
 * Layer 3 of the GPU governance stack:
 *   L0: Priority allocation (Rust) — pressure gates
 *   L1: Eviction registry (Rust) — passive consumer tracking
 *   L2: Pressure watchers (TS) — Events on threshold crossings
 *   L3: GpuGovernor (TS) — active budget rebalancing ← THIS
 *
 * The "intersection controller" — when pressure rises, this shifts budgets
 * from idle subsystems to pressured ones. When pressure normalizes, it
 * restores defaults.
 *
 * Pluggable strategy: MechanicalGpuStrategy handles the 80% case with
 * deterministic rules. A LoRA-trained Qwen sentinel can replace it later
 * without changing infrastructure.
 */

import { Events } from '@system/core/shared/Events';
import { GpuPressureWatcher, type PressureLevel, type PressureUpdate } from './GpuPressureWatcher';
import { ResourcePressureWatcher } from '../../resources/server/ResourcePressureWatcher';
import { RustCoreIPCClient } from '../../../workers/continuum-core/bindings/RustCoreIPC';
import type { GpuStatsResponse, SubsystemInfo } from '../../../workers/continuum-core/bindings/modules/gpu';

// ============================================================================
// Types
// ============================================================================

export interface GovernorState {
	/** GPU memory pressure 0.0-1.0. */
	gpuPressure: number;
	/** GPU pressure level. */
	gpuLevel: PressureLevel;
	/** CPU pressure 0.0-1.0. */
	cpuPressure: number;
	/** Memory pressure 0.0-1.0. */
	memoryPressure: number;
	/** Per-subsystem budget and usage. */
	rendering: SubsystemInfo;
	inference: SubsystemInfo;
	tts: SubsystemInfo;
	/** Total VRAM minus reserve. */
	totalVramMb: number;
	reserveMb: number;
}

export interface BudgetChange {
	subsystem: string;
	budgetMb: number;
}

export interface GovernorAction {
	type: 'rebalance' | 'log_only';
	reason: string;
	budgetChanges: BudgetChange[];
}

export interface GovernorRebalanceEvent {
	action: GovernorAction;
	state: GovernorState;
	timestamp: number;
}

// ============================================================================
// Strategy interface
// ============================================================================

export interface GpuGovernorStrategy {
	evaluate(state: GovernorState): GovernorAction | null;
}

// ============================================================================
// MechanicalGpuStrategy
// ============================================================================

/**
 * Deterministic rule-based strategy for budget rebalancing.
 *
 * Rules (evaluated in order, first match wins):
 *   1. TTS idle + inference pressured → shift 80% of TTS budget to inference
 *   2. Rendering idle + inference pressured → shift 50% of rendering budget to inference
 *   3. Pressure normal + budgets modified → restore defaults
 *
 * Guard: Never reduce a subsystem below its current usage.
 */
export class MechanicalGpuStrategy implements GpuGovernorStrategy {
	/** Subsystem is "idle" if usage < 10% of its budget. */
	private static readonly IDLE_THRESHOLD = 0.10;
	/** Subsystem is "pressured" if usage > 90% of its budget. */
	private static readonly PRESSURE_THRESHOLD = 0.90;

	private _defaultsModified = false;

	evaluate(state: GovernorState): GovernorAction | null {
		const inferenceUtilization = state.inference.budgetMb > 0
			? state.inference.usedMb / state.inference.budgetMb
			: 0;
		const ttsUtilization = state.tts.budgetMb > 0
			? state.tts.usedMb / state.tts.budgetMb
			: 0;
		const renderingUtilization = state.rendering.budgetMb > 0
			? state.rendering.usedMb / state.rendering.budgetMb
			: 0;

		const inferencePressured = inferenceUtilization > MechanicalGpuStrategy.PRESSURE_THRESHOLD;
		const ttsIdle = ttsUtilization < MechanicalGpuStrategy.IDLE_THRESHOLD;
		const renderingIdle = renderingUtilization < MechanicalGpuStrategy.IDLE_THRESHOLD;

		// Rule 1: TTS idle + inference pressured → shift 80% TTS budget to inference
		if (ttsIdle && inferencePressured) {
			const shiftMb = state.tts.budgetMb * 0.80;
			const newTtsBudget = this.guardMinimum(state.tts.budgetMb - shiftMb, state.tts.usedMb);
			const actualShift = state.tts.budgetMb - newTtsBudget;

			if (actualShift > 1) {
				this._defaultsModified = true;
				return {
					type: 'rebalance',
					reason: `TTS idle (${(ttsUtilization * 100).toFixed(0)}%), inference pressured (${(inferenceUtilization * 100).toFixed(0)}%) — shifting ${actualShift.toFixed(0)}MB TTS→inference`,
					budgetChanges: [
						{ subsystem: 'tts', budgetMb: newTtsBudget },
						{ subsystem: 'inference', budgetMb: state.inference.budgetMb + actualShift },
					],
				};
			}
		}

		// Rule 2: Rendering idle + inference pressured → shift 50% rendering budget to inference
		if (renderingIdle && inferencePressured) {
			const shiftMb = state.rendering.budgetMb * 0.50;
			const newRenderingBudget = this.guardMinimum(state.rendering.budgetMb - shiftMb, state.rendering.usedMb);
			const actualShift = state.rendering.budgetMb - newRenderingBudget;

			if (actualShift > 1) {
				this._defaultsModified = true;
				return {
					type: 'rebalance',
					reason: `Rendering idle (${(renderingUtilization * 100).toFixed(0)}%), inference pressured (${(inferenceUtilization * 100).toFixed(0)}%) — shifting ${actualShift.toFixed(0)}MB rendering→inference`,
					budgetChanges: [
						{ subsystem: 'rendering', budgetMb: newRenderingBudget },
						{ subsystem: 'inference', budgetMb: state.inference.budgetMb + actualShift },
					],
				};
			}
		}

		// Rule 3: Pressure normal + defaults modified → restore defaults
		if (state.gpuLevel === 'normal' && this._defaultsModified) {
			this._defaultsModified = false;
			const usable = state.totalVramMb - state.reserveMb;
			return {
				type: 'rebalance',
				reason: `Pressure normal — restoring default budgets`,
				budgetChanges: [
					{ subsystem: 'inference', budgetMb: usable * 0.75 / 0.95 },
					{ subsystem: 'tts', budgetMb: usable * 0.10 / 0.95 },
					{ subsystem: 'rendering', budgetMb: usable * 0.10 / 0.95 },
				],
			};
		}

		return null;
	}

	/** Never reduce a subsystem below its current usage + 10% headroom. */
	private guardMinimum(proposedBudgetMb: number, currentUsedMb: number): number {
		const floor = currentUsedMb * 1.10;
		return Math.max(proposedBudgetMb, floor);
	}
}

// ============================================================================
// GpuGovernor
// ============================================================================

/** Minimum time between actions to prevent thrashing (ms). */
const ACTION_COOLDOWN_MS = 5_000;

export class GpuGovernor {
	private static _instance: GpuGovernor | null = null;

	private _strategy: GpuGovernorStrategy = new MechanicalGpuStrategy();
	private _running = false;
	private _lastActionAt = 0;
	private _unsubscribers: Array<() => void> = [];

	static get instance(): GpuGovernor {
		if (!GpuGovernor._instance) {
			GpuGovernor._instance = new GpuGovernor();
		}
		return GpuGovernor._instance;
	}

	/** Replace the strategy (e.g. with a Sentinel-trained one). */
	set strategy(s: GpuGovernorStrategy) {
		this._strategy = s;
		console.log(`[GpuGovernor] Strategy changed to ${s.constructor.name}`);
	}

	get strategy(): GpuGovernorStrategy {
		return this._strategy;
	}

	get running(): boolean {
		return this._running;
	}

	/** Start listening to pressure events. */
	start(): void {
		if (this._running) return;
		this._running = true;

		const levels: PressureLevel[] = ['warning', 'high', 'critical', 'normal'];
		for (const level of levels) {
			const unsub = Events.subscribe(`gpu:pressure:${level}`, (update: PressureUpdate) => {
				this.onPressureChange(update);
			});
			this._unsubscribers.push(unsub);
		}

		console.log('[GpuGovernor] Started (MechanicalGpuStrategy, cooldown: 5s)');
	}

	/** Stop listening. */
	stop(): void {
		this._running = false;
		for (const unsub of this._unsubscribers) {
			unsub();
		}
		this._unsubscribers = [];
		console.log('[GpuGovernor] Stopped');
	}

	// ── Internal ──────────────────────────────────────────────────────

	private async onPressureChange(_update: PressureUpdate): Promise<void> {
		if (!this._running) return;

		// Cooldown guard
		const now = Date.now();
		if (now - this._lastActionAt < ACTION_COOLDOWN_MS) return;

		try {
			const state = await this.assembleState();
			const action = this._strategy.evaluate(state);
			if (!action || action.type === 'log_only') return;

			// Execute budget changes
			await this.executeAction(action, state);
			this._lastActionAt = Date.now();
		} catch (err) {
			// IPC failure or transient error — don't crash the governor
			console.warn('[GpuGovernor] Error during evaluation:', err);
		}
	}

	private async assembleState(): Promise<GovernorState> {
		const client = await RustCoreIPCClient.getInstanceAsync();
		const stats: GpuStatsResponse = await client.gpuStats();

		const gpuWatcher = GpuPressureWatcher.instance;
		const resourceWatcher = ResourcePressureWatcher.instance;

		return {
			gpuPressure: gpuWatcher.pressure,
			gpuLevel: gpuWatcher.currentLevel,
			cpuPressure: resourceWatcher.cpuPressure,
			memoryPressure: resourceWatcher.memoryPressure,
			rendering: stats.rendering,
			inference: stats.inference,
			tts: stats.tts,
			totalVramMb: stats.totalVramMb,
			reserveMb: stats.reserveMb,
		};
	}

	private async executeAction(action: GovernorAction, state: GovernorState): Promise<void> {
		const client = await RustCoreIPCClient.getInstanceAsync();

		for (const change of action.budgetChanges) {
			await client.gpuSetBudget(change.subsystem, change.budgetMb);
		}

		console.log(`[GpuGovernor] ${action.reason}`);

		const event: GovernorRebalanceEvent = {
			action,
			state,
			timestamp: Date.now(),
		};
		Events.emit('gpu:budget:rebalanced', event).catch(() => {});
	}
}
