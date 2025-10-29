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
}

export const DEFAULT_STATE_CONFIG: StateConfig = {
  energyDepletionRate: 0.0001,    // Lose 1% energy per 10 seconds of processing
  energyRecoveryRate: 0.00005,    // Gain 1% energy per 20 seconds of rest
  attentionFatigueRate: 0.1,      // Lose 10% attention when tired
  enableLogging: true
};

/**
 * PersonaStateManager: Manages internal state and traffic decisions
 */
export class PersonaStateManager {
  private readonly config: StateConfig;
  private state: PersonaState;
  private readonly personaName: string;

  constructor(personaName: string, config: Partial<StateConfig> = {}) {
    this.personaName = personaName;
    this.config = { ...DEFAULT_STATE_CONFIG, ...config };

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

    this.log(`🧠 State initialized (energy=${this.state.energy.toFixed(2)}, mood=${this.state.mood})`);
  }

  /**
   * Record activity (depletes energy/attention)
   */
  async recordActivity(durationMs: number, complexity: number): Promise<void> {
    // Deplete energy based on duration and complexity
    const energyLoss = durationMs * this.config.energyDepletionRate * complexity;
    this.state.energy -= energyLoss;
    this.state.energy = Math.max(0, this.state.energy);

    // Update attention (fatigue when low energy)
    if (this.state.energy < 0.3) {
      this.state.attention *= (1 - this.config.attentionFatigueRate);
      this.state.attention = Math.max(0, this.state.attention);
    }

    // Update mood
    this.state.mood = this.calculateMood();

    this.state.lastActivityTime = Date.now();
    this.state.responseCount++;

    this.log(`📊 Activity recorded: energy=${this.state.energy.toFixed(2)}, attention=${this.state.attention.toFixed(2)}, mood=${this.state.mood}`);
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
    if (this.state.responseCount > 0 && this.state.energy > 0.5) {
      return 'active';
    }

    // Idle: waiting for work
    return 'idle';
  }

  /**
   * Should persona engage with message? (traffic management decision)
   *
   * Traffic rules:
   * 1. NEVER neglect high priority (>0.8) - prevent starvation
   * 2. Overwhelmed: only process highest priority (>0.9) - shed load
   * 3. Tired: lower threshold but require energy (>0.5, energy>0.2) - conserve
   * 4. Active: normal processing (>0.3) - maintain flow
   * 5. Idle: eager to work (>0.1) - stay responsive
   */
  shouldEngage(priority: number): boolean {
    // HIGH PRIORITY: Never neglect (prevent starvation)
    if (priority > 0.8) {
      this.log(`✅ High priority (${priority.toFixed(2)}) - ALWAYS engage`);
      return true;
    }

    // OVERWHELMED: Only process highest priority (shed load)
    if (this.state.mood === 'overwhelmed') {
      const engage = priority > 0.9;
      this.log(`${engage ? '✅' : '❌'} Overwhelmed mode - priority ${priority.toFixed(2)} ${engage ? 'above' : 'below'} threshold 0.9`);
      return engage;
    }

    // TIRED: Lower threshold but require energy (conserve)
    if (this.state.mood === 'tired') {
      const engage = priority > 0.5 && this.state.energy > 0.2;
      this.log(`${engage ? '✅' : '❌'} Tired mode - priority ${priority.toFixed(2)}, energy ${this.state.energy.toFixed(2)}`);
      return engage;
    }

    // ACTIVE: Normal processing (maintain flow)
    if (this.state.mood === 'active') {
      const engage = priority > 0.3;
      this.log(`${engage ? '✅' : '❌'} Active mode - priority ${priority.toFixed(2)} ${engage ? 'above' : 'below'} threshold 0.3`);
      return engage;
    }

    // IDLE: Eager to work (stay responsive)
    const engage = priority > 0.1;
    this.log(`${engage ? '✅' : '❌'} Idle mode - priority ${priority.toFixed(2)} ${engage ? 'above' : 'below'} threshold 0.1`);
    return engage;
  }

  /**
   * Get cadence (how often to check inbox)
   *
   * Adaptive timing based on mood:
   * - Overwhelmed: 10s (back pressure)
   * - Tired: 7s (moderate pace)
   * - Active: 5s (normal pace)
   * - Idle: 3s (eager, stay responsive)
   *
   * Constrained by compute budget (slow down when limited)
   */
  getCadence(): number {
    let cadence: number;

    switch (this.state.mood) {
      case 'overwhelmed':
        cadence = 10000; // 10 seconds (back pressure)
        break;
      case 'tired':
        cadence = 7000; // 7 seconds (moderate)
        break;
      case 'active':
        cadence = 5000; // 5 seconds (normal)
        break;
      case 'idle':
        cadence = 3000; // 3 seconds (eager)
        break;
    }

    // Constrain by compute budget (slow down when limited)
    if (this.state.computeBudget < 0.5) {
      cadence *= 2; // Double cadence when low compute
    }

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
   * Logging helper
   */
  private log(message: string): void {
    if (!this.config.enableLogging) return;
    console.log(`[${this.personaName}:State] ${message}`);
  }
}
