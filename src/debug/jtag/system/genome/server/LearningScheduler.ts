/**
 * LearningScheduler - RTOS-style periodic training scheduler
 *
 * Monitors all active personas and triggers training for those with
 * enough accumulated data. Priority: personas with the most interactions
 * get trained first. Prevents training storms — max 1 concurrent training
 * job per GPU at a time.
 *
 * Integrates with PersonaUser's serviceInbox loop:
 * - Called periodically (every N service cycles)
 * - Checks training readiness per persona
 * - Throttles to prevent overwhelming the GPU
 */

import type { PersonaTrainingManager } from '../../user/server/modules/PersonaTrainingManager';
import type { TrainingDataAccumulator } from '../../user/server/modules/TrainingDataAccumulator';

/**
 * Registered persona for learning scheduling
 */
interface ScheduledPersona {
  personaId: string;
  displayName: string;
  trainingManager: PersonaTrainingManager;
  accumulator: TrainingDataAccumulator;
}

/**
 * LearningScheduler - Coordinates periodic training across all personas
 *
 * Design principles:
 * - Non-blocking: checks are fast, training runs asynchronously
 * - Priority-sorted: busier personas train first
 * - Throttled: max 1 concurrent training job
 * - Adaptive: check frequency scales with activity
 */
export class LearningScheduler {
  /** Check training readiness every N service cycles */
  private readonly checkIntervalCycles: number;

  /** Current cycle counter (per-persona) */
  private cycleCounts: Map<string, number> = new Map();

  /** Whether a training job is currently running */
  private _isTraining = false;

  /** ID of persona currently being trained */
  private _trainingPersonaId: string | null = null;

  /** Registered personas for scheduling */
  private personas: Map<string, ScheduledPersona> = new Map();

  private log: (message: string) => void;

  /** Singleton instance */
  private static _instance: LearningScheduler | null = null;

  /** Get or create the singleton instance */
  static sharedInstance(options?: {
    checkIntervalCycles?: number;
    logger?: (message: string) => void;
  }): LearningScheduler {
    if (!LearningScheduler._instance) {
      LearningScheduler._instance = new LearningScheduler(options);
    }
    return LearningScheduler._instance;
  }

  /** Reset singleton (for testing) */
  static resetInstance(): void {
    LearningScheduler._instance = null;
  }

  constructor(options?: {
    checkIntervalCycles?: number;
    logger?: (message: string) => void;
  }) {
    this.checkIntervalCycles = options?.checkIntervalCycles ?? 100;
    this.log = options?.logger ?? console.log.bind(console);
  }

  /**
   * Whether a training job is currently running
   */
  get isTraining(): boolean {
    return this._isTraining;
  }

  /**
   * ID of persona currently being trained (null if idle)
   */
  get trainingPersonaId(): string | null {
    return this._trainingPersonaId;
  }

  /**
   * Register a persona for scheduled learning.
   * Called once when PersonaUser initializes.
   */
  registerPersona(
    personaId: string,
    displayName: string,
    trainingManager: PersonaTrainingManager,
    accumulator: TrainingDataAccumulator,
  ): void {
    this.personas.set(personaId, {
      personaId,
      displayName,
      trainingManager,
      accumulator,
    });
    this.cycleCounts.set(personaId, 0);
  }

  /**
   * Unregister a persona from scheduled learning.
   * Called when PersonaUser shuts down.
   */
  unregisterPersona(personaId: string): void {
    this.personas.delete(personaId);
    this.cycleCounts.delete(personaId);
  }

  /**
   * Called from PersonaUser.serviceInbox() on each cycle.
   *
   * Increments this persona's cycle counter. When the counter hits the
   * check interval, evaluates all personas for training readiness.
   * The persona with the most accumulated data trains first.
   *
   * Returns true if training was triggered.
   */
  async tick(personaId: string): Promise<boolean> {
    const count = (this.cycleCounts.get(personaId) ?? 0) + 1;
    this.cycleCounts.set(personaId, count);

    if (count < this.checkIntervalCycles) {
      return false;
    }

    // Reset counter
    this.cycleCounts.set(personaId, 0);

    // Don't start new training if one is already running
    if (this._isTraining) {
      return false;
    }

    // Find the persona with the most accumulated data across all domains
    const candidates = this.rankByDataVolume();
    if (candidates.length === 0) {
      return false;
    }

    // Train the top candidate
    const top = candidates[0];
    return await this.triggerTraining(top);
  }

  /**
   * Force an immediate check across all personas (bypass cycle counter).
   * Used for system-wide periodic scans (e.g., from GenomeDaemon every 30 minutes).
   */
  async scanAll(): Promise<{ triggered: boolean; personaId?: string }> {
    if (this._isTraining) {
      return { triggered: false };
    }

    const candidates = this.rankByDataVolume();
    if (candidates.length === 0) {
      return { triggered: false };
    }

    const top = candidates[0];
    const triggered = await this.triggerTraining(top);
    return { triggered, personaId: triggered ? top.personaId : undefined };
  }

  /**
   * Rank all personas by total accumulated training data volume (descending).
   * Only includes personas that have at least one domain ready for training.
   */
  private rankByDataVolume(): ScheduledPersona[] {
    const scored: Array<{ persona: ScheduledPersona; totalReady: number }> = [];

    for (const persona of this.personas.values()) {
      const domains = persona.accumulator.getDomains();
      let totalReady = 0;

      for (const domain of domains) {
        if (persona.accumulator.shouldMicroTune(domain)) {
          totalReady += persona.accumulator.getBufferSize(domain);
        }
      }

      if (totalReady > 0) {
        scored.push({ persona, totalReady });
      }
    }

    // Sort descending by data volume — busiest persona trains first
    scored.sort((a, b) => b.totalReady - a.totalReady);

    return scored.map(s => s.persona);
  }

  /**
   * Trigger training for a specific persona.
   * Sets the training lock and delegates to PersonaTrainingManager.
   */
  private async triggerTraining(persona: ScheduledPersona): Promise<boolean> {
    this._isTraining = true;
    this._trainingPersonaId = persona.personaId;

    this.log(`🎓 LearningScheduler: Triggering training for ${persona.displayName} (${persona.personaId.slice(0, 8)})`);

    try {
      await persona.trainingManager.checkTrainingReadiness();
      return true;
    } catch (error) {
      this.log(`❌ LearningScheduler: Training failed for ${persona.displayName}: ${error}`);
      return false;
    } finally {
      this._isTraining = false;
      this._trainingPersonaId = null;
    }
  }

  /**
   * Get stats for all registered personas.
   */
  getStats(): Array<{
    personaId: string;
    displayName: string;
    cycleCount: number;
    domains: Record<string, { count: number; threshold: number; ready: boolean }>;
  }> {
    return Array.from(this.personas.values()).map(p => ({
      personaId: p.personaId,
      displayName: p.displayName,
      cycleCount: this.cycleCounts.get(p.personaId) ?? 0,
      domains: p.accumulator.getStats(),
    }));
  }
}
