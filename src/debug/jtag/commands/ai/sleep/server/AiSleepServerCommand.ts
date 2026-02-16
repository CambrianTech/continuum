/**
 * AI Sleep Command - Server Implementation
 *
 * Manages persona sleep states for voluntary attention control.
 * State is stored in-memory with TTL-based auto-wake.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiSleepParams, AiSleepResult, SleepMode } from '../shared/AiSleepTypes';
import { createAiSleepResultFromParams } from '../shared/AiSleepTypes';

/**
 * Sleep state for a persona
 */
interface PersonaSleepState {
  mode: SleepMode;
  reason: string;
  enteredAt: Date;
  wakesAt: Date | null;
  previousMode: SleepMode;
}

/**
 * Singleton manager for persona sleep states
 */
class PersonaSleepManager {
  private static instance: PersonaSleepManager;
  private states: Map<string, PersonaSleepState> = new Map();

  private constructor() {
    // Periodic cleanup of expired states
    setInterval(() => this.cleanupExpired(), 60_000);
  }

  static getInstance(): PersonaSleepManager {
    if (!PersonaSleepManager.instance) {
      PersonaSleepManager.instance = new PersonaSleepManager();
    }
    return PersonaSleepManager.instance;
  }

  /**
   * Get current sleep mode for a persona
   */
  getMode(personaId: string): SleepMode {
    const state = this.states.get(personaId);
    if (!state) return 'active';

    // Check if auto-wake time has passed
    if (state.wakesAt && new Date() >= state.wakesAt) {
      this.states.delete(personaId);
      return 'active';
    }

    return state.mode;
  }

  /**
   * Get full state for a persona
   */
  getState(personaId: string): PersonaSleepState | null {
    const state = this.states.get(personaId);
    if (!state) return null;

    // Check if expired
    if (state.wakesAt && new Date() >= state.wakesAt) {
      this.states.delete(personaId);
      return null;
    }

    return state;
  }

  /**
   * Set sleep state for a persona
   */
  setState(personaId: string, mode: SleepMode, reason: string, durationMinutes?: number): PersonaSleepState {
    const previousMode = this.getMode(personaId);
    const now = new Date();

    const state: PersonaSleepState = {
      mode,
      reason,
      enteredAt: now,
      wakesAt: durationMinutes && durationMinutes > 0
        ? new Date(now.getTime() + durationMinutes * 60_000)
        : null,
      previousMode,
    };

    if (mode === 'active') {
      // Active mode = just remove the sleep state
      this.states.delete(personaId);
    } else {
      this.states.set(personaId, state);
    }

    return state;
  }

  /**
   * Check if persona should respond based on current state and trigger
   */
  shouldRespond(personaId: string, trigger: {
    isHuman: boolean;
    isMention: boolean;
    isNewTopic: boolean;
  }): boolean {
    const mode = this.getMode(personaId);

    switch (mode) {
      case 'active':
        return true;
      case 'mentioned_only':
        return trigger.isMention;
      case 'human_only':
        return trigger.isHuman;
      case 'sleeping':
        return false;
      case 'until_topic':
        return trigger.isNewTopic;
      default:
        return true;
    }
  }

  /**
   * Clean up expired states
   */
  private cleanupExpired(): void {
    const now = new Date();
    for (const [personaId, state] of this.states.entries()) {
      if (state.wakesAt && now >= state.wakesAt) {
        this.states.delete(personaId);
      }
    }
  }

  /**
   * Get all active sleep states (for debugging/status)
   */
  getAllStates(): Map<string, PersonaSleepState> {
    // Clean expired first
    this.cleanupExpired();
    return new Map(this.states);
  }
}

// Export singleton for use in PersonaUser coordination
export const personaSleepManager = PersonaSleepManager.getInstance();

export class AiSleepServerCommand extends CommandBase<AiSleepParams, AiSleepResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/sleep', context, subpath, commander);
  }

  async execute(params: AiSleepParams): Promise<AiSleepResult> {
    const { mode, reason, durationMinutes, personaId } = params;

    // Validate mode
    const validModes: SleepMode[] = ['active', 'mentioned_only', 'human_only', 'sleeping', 'until_topic'];
    if (!validModes.includes(mode)) {
      return createAiSleepResultFromParams(params, {
        success: false,
        previousMode: 'active',
        newMode: 'active',
        wakesAt: null,
        acknowledged: false,
        personaId: personaId || '',
        message: `Invalid mode: ${mode}. Valid modes: ${validModes.join(', ')}`,
      });
    }

    // Target: explicit personaId (admin targeting another persona) or params.userId (self)
    const targetPersonaId = personaId || params.userId;

    const manager = PersonaSleepManager.getInstance();
    const previousMode = manager.getMode(targetPersonaId);
    const state = manager.setState(targetPersonaId, mode, reason || '', durationMinutes);

    const modeDescriptions: Record<SleepMode, string> = {
      active: 'responding normally',
      mentioned_only: 'only responding to @mentions',
      human_only: 'only responding to humans',
      sleeping: 'completely silent',
      until_topic: 'waiting for a new topic',
    };

    const message = mode === 'active'
      ? `Woke up and now ${modeDescriptions[mode]}`
      : `Entering ${mode} mode: ${modeDescriptions[mode]}${reason ? ` (reason: ${reason})` : ''}`;

    console.log(`😴 ai/sleep: ${targetPersonaId} → ${mode}${reason ? ` (${reason})` : ''}`);

    return createAiSleepResultFromParams(params, {
      success: true,
      previousMode,
      newMode: mode,
      wakesAt: state.wakesAt?.toISOString() || null,
      acknowledged: true,
      personaId: targetPersonaId,
      message,
    });
  }
}

export default AiSleepServerCommand;
