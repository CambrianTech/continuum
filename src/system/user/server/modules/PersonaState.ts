/**
 * PersonaState - Internal state management for autonomous personas
 *
 * Philosophy: "in a good rtos you arent at 100% duty cycle, same goes for persona"
 *
 * State influences traffic management decisions:
 * - Energy: Depletes with processing, recovers with rest
 * - Attention: Focus level, influenced by complexity and fatigue
 * - Mood: active/tired/overwhelmed/idle (affects engagement threshold)
 * - Compute budget: Resource constraints (rate limits, API quotas)
 */

import type { SubsystemLogger } from './being/logging/SubsystemLogger';
import { Events } from '../../../core/shared/Events';
import { PersonaTimingConfig } from './PersonaTimingConfig';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { BackpressureService } from '../../../core/services/BackpressureService';

/**
 * Persona internal state
 */
export interface PersonaState {
  energy: number;           // 0.0-1.0 (depletes with processing, recovers with rest)
  attention: number;        // 0.0-1.0 (focus level, influenced by complexity)
  mood: 'active' | 'tired' | 'overwhelmed' | 'idle';
  inboxLoad: number;        // Current inbox size (messages waiting)
  lastActivityTime: number; // When last processed message (timestamp)
  responseCount: number;    // Responses in current window
  computeBudget: number;    // 0.0-1.0 (available compute, influenced by rate limits)
}

/**
 * State configuration
 */
export interface StateConfig {
  energyDepletionRate: number;  // How fast energy depletes (per ms of processing)
  energyRecoveryRate: number;   // How fast energy recovers (per ms of rest)
  attentionFatigueRate: number; // How fast attention decays when tired
  enableLogging: boolean;
  logger?: SubsystemLogger;     // Optional logger for state changes
}

export const DEFAULT_STATE_CONFIG: StateConfig = {
  energyDepletionRate: PersonaTimingConfig.energy.depletionRatePerMs,
  energyRecoveryRate: PersonaTimingConfig.energy.recoveryRatePerMs,
  attentionFatigueRate: PersonaTimingConfig.energy.attentionFatigueRate,
  enableLogging: true
};

/**
 * PersonaStateManager: Manages internal state and traffic decisions
 */
/** Minimum interval between snapshot emissions per persona (ms) */
const SNAPSHOT_THROTTLE_MS = 2000;

export class PersonaStateManager {
  private readonly config: StateConfig;
  private state: PersonaState;
  private readonly personaName: string;
  private readonly personaId?: string;
  private readonly logger?: SubsystemLogger;
  private _lastSnapshotTime = 0;
  private _snapshotPending = false;
  private _periodicInterval: ReturnType<typeof setInterval> | null = null;
  private _requestUnsub: (() => void) | null = null;

  constructor(personaName: string, config: Partial<StateConfig> = {}, personaId?: string) {
    this.personaName = personaName;
    this.personaId = personaId;
    this.config = { ...DEFAULT_STATE_CONFIG, ...config };
    this.logger = config.logger;

    // Initialize state
    this.state = {
      energy: 1.0,
      attention: 1.0,
      mood: 'idle',
      inboxLoad: 0,
      lastActivityTime: Date.now(),
      responseCount: 0,
      computeBudget: 1.0
    };

    this.log(`State initialized (energy=${this.state.energy.toFixed(2)}, mood=${this.state.mood})`);

    // Emit initial snapshot after a short delay so the browser widget
    // gets state on page load even if the persona is idle.
    setTimeout(() => this.emitSnapshotNow(), 2000);

    // Periodic snapshot every 10s ensures browser always gets fresh state,
    // even when persona is idle and no activity/rest events fire.
    this._periodicInterval = setInterval(() => this.emitSnapshotNow(), 10_000);

    // Listen for browser-side state requests (widget mount, page refresh).
    // When a PersonaTile mounts it emits 'persona:state:request' — we respond
    // with our current snapshot so the widget doesn't sit at defaults.
    this._requestUnsub = Events.subscribe('persona:state:request', (data: { personaId?: string }) => {
      if (!data.personaId || data.personaId === this.personaId) {
        this.emitSnapshotNow();
      }
    });
  }

  /**
   * Record activity (depletes energy/attention)
   */
  async recordActivity(durationMs: number, complexity: number): Promise<void> {
    // Deplete energy based on duration and complexity
    const energyLoss = durationMs * this.config.energyDepletionRate * complexity;
    this.state.energy -= energyLoss;
    this.state.energy = Math.max(PersonaTimingConfig.energy.floor, this.state.energy);

    // Update attention (fatigue when low energy)
    if (this.state.energy < 0.3) {
      this.state.attention *= (1 - this.config.attentionFatigueRate);
      this.state.attention = Math.max(0, this.state.attention);
    }

    // Update state counters first (needed for mood calculation)
    this.state.lastActivityTime = Date.now();
    this.state.responseCount++;

    // Calculate mood after updating state
    this.state.mood = this.calculateMood();

    this.log(`📊 Activity recorded: energy=${this.state.energy.toFixed(2)}, attention=${this.state.attention.toFixed(2)}, mood=${this.state.mood}`);

    this.emitSnapshot();
  }

  /**
   * Rest (recover energy/attention)
   */
  async rest(durationMs: number): Promise<void> {
    // Recover energy
    const energyGain = durationMs * this.config.energyRecoveryRate;
    this.state.energy += energyGain;
    this.state.energy = Math.min(1.0, this.state.energy);

    // Recover attention (faster than energy)
    const attentionGain = energyGain * 2;
    this.state.attention += attentionGain;
    this.state.attention = Math.min(1.0, this.state.attention);

    // Update mood
    this.state.mood = this.calculateMood();

    this.log(`💤 Rested: energy=${this.state.energy.toFixed(2)}, attention=${this.state.attention.toFixed(2)}, mood=${this.state.mood}`);

    this.emitSnapshot();
  }

  /**
   * Calculate mood from state
   */
  private calculateMood(): 'active' | 'tired' | 'overwhelmed' | 'idle' {
    // Overwhelmed: inbox overloaded
    if (this.state.inboxLoad > 50) {
      return 'overwhelmed';
    }

    // Tired: low energy
    if (this.state.energy < 0.3) {
      return 'tired';
    }

    // Active: engaged and energized
    if (this.state.responseCount > 0 && this.state.energy >= 0.5) {
      return 'active';
    }

    // Idle: waiting for work
    return 'idle';
  }

  /**
   * Should persona engage with message? (traffic management decision)
   *
   * Energy floor (0.1) prevents the old "15-minute death spiral" —
   * personas degrade gracefully but never fully die.
   *
   * Traffic rules:
   * 1. High priority (>0.5) - always engage
   * 2. Medium priority (>0.2) - engage
   * 3. Low priority (<0.2) - skip
   */
  shouldEngage(priority: number): boolean {
    // Simple threshold: engage if priority > 0.15
    // This allows normal messages (0.2 base) to always get through
    const engage = priority > 0.15;

    if (this.config.enableLogging) {
      this.log(`${engage ? '✅' : '❌'} Priority ${priority.toFixed(2)} ${engage ? 'above' : 'below'} threshold 0.15`);
    }

    return engage;
  }

  /**
   * Get cadence (max wait time for signal before timeout, in ms)
   *
   * This is NOT a polling interval — the service loop uses signal-based wakeup.
   * Cadence is the MAXIMUM time to wait if no signal arrives.
   * Actual response is near-instant when a signal fires.
   *
   * Adaptive timing based on mood:
   * - Idle: 1s (first message gets fast response)
   * - Active: 500ms (stay responsive during conversations)
   * - Tired: 2s (moderate pace)
   * - Overwhelmed: 3s (back pressure, but still responsive)
   *
   * Constrained by compute budget (slow down when limited)
   */
  getCadence(): number {
    let cadence: number;

    switch (this.state.mood) {
      case 'idle':
        cadence = PersonaTimingConfig.cadence.idleMs;
        break;
      case 'active':
        cadence = PersonaTimingConfig.cadence.activeMs;
        break;
      case 'tired':
        cadence = PersonaTimingConfig.cadence.tiredMs;
        break;
      case 'overwhelmed':
        cadence = PersonaTimingConfig.cadence.overwhelmedMs;
        break;
    }

    // Constrain by compute budget (slow down when limited)
    if (this.state.computeBudget < 0.5) {
      cadence *= 2; // Double cadence when low compute
    }

    // Constrain by system memory pressure (stacks with compute budget)
    const pressure = BackpressureService.pressureLevel;
    if (pressure === 'critical') cadence *= 4;
    else if (pressure === 'high') cadence *= 2;
    else if (pressure === 'warning') cadence *= 1.5;

    return cadence;
  }

  /**
   * Update inbox load (for mood calculation)
   */
  updateInboxLoad(size: number): void {
    this.state.inboxLoad = size;

    // Recalculate mood (might transition to overwhelmed)
    const oldMood = this.state.mood;
    this.state.mood = this.calculateMood();

    if (oldMood !== this.state.mood) {
      this.log(`🎭 Mood changed: ${oldMood} → ${this.state.mood} (inboxLoad=${size})`);
    }

    // Emit snapshot so browser-side meters update in real time
    this.emitSnapshot();
  }

  /**
   * Update compute budget (from rate limiter)
   */
  updateComputeBudget(budget: number): void {
    this.state.computeBudget = budget;
  }

  /**
   * Reset response count (new window)
   */
  resetResponseCount(): void {
    this.state.responseCount = 0;
  }

  /**
   * Get current state (for diagnostics)
   */
  getState(): PersonaState {
    return { ...this.state };
  }

  /**
   * Get state summary (for logging)
   */
  getSummary(): string {
    return `energy=${this.state.energy.toFixed(2)}, ` +
           `attention=${this.state.attention.toFixed(2)}, ` +
           `mood=${this.state.mood}, ` +
           `inbox=${this.state.inboxLoad}, ` +
           `responses=${this.state.responseCount}, ` +
           `budget=${this.state.computeBudget.toFixed(2)}`;
  }

  /**
   * Emit state snapshot event for browser-side consumers (PersonaTile meters).
   * Uses DataDaemon.jtagContext for cross-context (server→browser) delivery.
   * Without the context, bare Events.emit() stays server-local.
   */
  /**
   * Throttled snapshot emission — max once per SNAPSHOT_THROTTLE_MS.
   * With 15 personas each calling this on every cycle (3-5s) plus rest(),
   * unthrottled emission hit 200+/s and flooded the WebSocket to browser.
   */
  private emitSnapshot(): void {
    if (!this.personaId) return;

    const now = Date.now();
    if (now - this._lastSnapshotTime < SNAPSHOT_THROTTLE_MS) {
      // Schedule a trailing emit so the latest state always gets sent
      if (!this._snapshotPending) {
        this._snapshotPending = true;
        setTimeout(() => {
          this._snapshotPending = false;
          this.emitSnapshotNow();
        }, SNAPSHOT_THROTTLE_MS - (now - this._lastSnapshotTime));
      }
      return;
    }

    this.emitSnapshotNow();
  }

  private emitSnapshotNow(): void {
    if (!this.personaId) return;
    this._lastSnapshotTime = Date.now();

    const payload = {
      personaId: this.personaId,
      energy: this.state.energy,
      attention: this.state.attention,
      mood: this.state.mood,
      inboxLoad: this.state.inboxLoad,
      computeBudget: this.state.computeBudget,
      timestamp: this._lastSnapshotTime
    };

    const ctx = DataDaemon.jtagContext;
    if (ctx) {
      Events.emit(ctx, 'persona:state:snapshot', payload);
    } else {
      Events.emit('persona:state:snapshot', payload);
    }
  }

  /**
   * Cleanup intervals and subscriptions (call if PersonaStateManager is ever disposed).
   */
  dispose(): void {
    if (this._periodicInterval) {
      clearInterval(this._periodicInterval);
      this._periodicInterval = null;
    }
    if (this._requestUnsub) {
      this._requestUnsub();
      this._requestUnsub = null;
    }
  }

  /**
   * Logging helper
   */
  private log(message: string): void {
    if (!this.config.enableLogging) return;

    // Use file logger if available, otherwise fallback to console
    if (this.logger) {
      this.logger.debug(`State] ${message}`);
    } else {
      console.log(`[${this.personaName}:State] ${message}`);
    }
  }
}
