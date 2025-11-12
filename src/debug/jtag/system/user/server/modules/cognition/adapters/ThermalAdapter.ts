/**
 * ThermalAdapter - Temperature-based decision adapter
 *
 * Uses thermal dynamics to decide when to respond:
 * - Temperature rises with activity (messages, mentions)
 * - Temperature decays over time (cooling)
 * - Responds when temperature crosses activation threshold
 *
 * Philosophy: Personas have "moods" that build up with conversation energy.
 * Hot = eager to respond, Cold = selective/quiet.
 *
 * Phase 4: Hardcoded profiles (will move to entity definitions in Phase 5)
 */

import type { IDecisionAdapter, DecisionContext, CognitiveDecision } from './IDecisionAdapter';
import type { BaseEntity } from '../../../../../data/entities/BaseEntity';

/**
 * Thermal profile configuration
 * All values in [0.0, 1.0] range for float-based dynamics
 */
interface ThermalProfile {
  heatRate: number;              // How much temp increases per event (0.0-1.0)
  decayRate: number;             // How much temp decreases per second (0.0-1.0)
  ambientAbsorption: number;     // How much ambient activity affects temp (0.0-1.0)
  mentionBoost: number;          // Extra heat from mentions (0.0-1.0)
  activationThreshold: number;   // Temp needed to respond (0.0-1.0)
}

/**
 * Hardcoded thermal profiles by domain
 * TODO Phase 5: Move to PersonaEntity.personaConfig.thermalProfile
 */
const THERMAL_PROFILES: Record<string, ThermalProfile> = {
  // CHAT: Moderate, balanced dynamics
  chat: {
    heatRate: 0.15,
    decayRate: 0.001,
    ambientAbsorption: 0.03,
    mentionBoost: 0.4,
    activationThreshold: 0.6
  },

  // GAME: Fast, urgent dynamics
  game: {
    heatRate: 0.25,
    decayRate: 0.0005,
    ambientAbsorption: 0.05,
    mentionBoost: 0.6,
    activationThreshold: 0.5
  },

  // CODE: Slow, thoughtful dynamics
  code: {
    heatRate: 0.08,
    decayRate: 0.0002,
    ambientAbsorption: 0.01,
    mentionBoost: 0.3,
    activationThreshold: 0.7
  },

  // ACADEMY: Balanced, educational dynamics
  academy: {
    heatRate: 0.12,
    decayRate: 0.0008,
    ambientAbsorption: 0.02,
    mentionBoost: 0.35,
    activationThreshold: 0.65
  }
};

/**
 * Thermal state for a persona
 * Tracks current temperature and last update time
 */
interface ThermalState {
  temperature: number;      // Current temp [0.0-1.0]
  lastUpdateTime: number;   // Timestamp of last update
}

export class ThermalAdapter implements IDecisionAdapter {
  readonly name = 'ThermalAdapter';
  readonly priority = 50; // Medium priority - runs after FastPath, before LLM

  // Thermal state per persona (in-memory for now)
  // TODO Phase 5: Move to AmbientState entity in database
  private thermalStates = new Map<string, ThermalState>();

  /**
   * Evaluate if should respond based on thermal dynamics
   *
   * @returns CognitiveDecision if temperature crosses threshold, null to try next adapter
   */
  async evaluate<TEvent extends BaseEntity>(context: DecisionContext<TEvent>): Promise<CognitiveDecision | null> {
    // Get domain from context (default to 'chat')
    const domain = this.getDomain(context);
    const profile = THERMAL_PROFILES[domain] || THERMAL_PROFILES.chat;

    // Get or initialize thermal state
    const state = this.getOrInitState(context.personaId);

    // Apply decay since last update
    this.applyDecay(state, profile);

    // Heat from this event
    let heat = profile.heatRate;

    // Ambient heat (if this is part of active conversation)
    if (context.senderIsHuman) {
      heat += profile.ambientAbsorption;
    }

    // Mention boost (even if not direct @mention, name in content)
    if (context.isMentioned || this.detectNameInContent(context)) {
      heat += profile.mentionBoost;
    }

    // Apply heat (clamped to [0.0, 1.0])
    state.temperature = Math.min(1.0, state.temperature + heat);
    state.lastUpdateTime = Date.now();

    // Decision: respond if temperature crosses threshold
    const shouldRespond = state.temperature >= profile.activationThreshold;

    // If temperature too low, pass to next adapter
    if (!shouldRespond) {
      return null;
    }

    // Temperature crossed threshold - return decision
    // Confidence = how far above threshold (0.6-1.0 range)
    const confidence = 0.6 + (state.temperature - profile.activationThreshold) * 0.4;

    return {
      shouldRespond: true,
      confidence,
      reason: `Thermal activation (temp=${state.temperature.toFixed(2)}, threshold=${profile.activationThreshold.toFixed(2)}, domain=${domain})`,
      model: this.name
    };
  }

  /**
   * Get domain from context
   */
  private getDomain<TEvent extends BaseEntity>(context: DecisionContext<TEvent>): string {
    // Check if triggerEvent has domain-specific properties
    if ('roomId' in context.triggerEvent) {
      return 'chat';
    }
    // TODO: Add other domain detection (game, code, academy)
    return 'chat';
  }

  /**
   * Get or initialize thermal state for persona
   */
  private getOrInitState(personaId: string): ThermalState {
    if (!this.thermalStates.has(personaId)) {
      this.thermalStates.set(personaId, {
        temperature: 0.5,  // Start at neutral
        lastUpdateTime: Date.now()
      });
    }
    return this.thermalStates.get(personaId)!;
  }

  /**
   * Apply thermal decay based on time elapsed
   */
  private applyDecay(state: ThermalState, profile: ThermalProfile): void {
    const now = Date.now();
    const elapsedSeconds = (now - state.lastUpdateTime) / 1000;

    // Decay temperature
    const decay = profile.decayRate * elapsedSeconds;
    state.temperature = Math.max(0.0, state.temperature - decay);
  }

  /**
   * Detect if persona name appears in event content (even without @)
   */
  private detectNameInContent<TEvent extends BaseEntity>(context: DecisionContext<TEvent>): boolean {
    const content = context.eventContent.toLowerCase();
    const name = context.personaDisplayName.toLowerCase();

    // Check for name in content (case-insensitive)
    return content.includes(name);
  }

  /**
   * Get current temperature for a persona (for debugging/reporting)
   */
  getTemperature(personaId: string): number {
    const state = this.thermalStates.get(personaId);
    return state?.temperature ?? 0.5;
  }
}
