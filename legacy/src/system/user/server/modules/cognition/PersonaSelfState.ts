/**
 * Persona Self-State Manager
 *
 * Manages the universal self-awareness state for a persona
 * Tracks focus, cognitive load, preoccupations, and decisions
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { cognitionStorage, type SelfStateEntry } from './memory/InMemoryCognitionStorage';

export class PersonaSelfState {
  constructor(private personaId: UUID) {}

  /**
   * Get current self-state (creates default if doesn't exist)
   */
  async get(): Promise<SelfStateEntry> {
    let state = cognitionStorage.getSelfState(this.personaId);

    if (!state) {
      // Initialize default state
      state = {
        personaId: this.personaId,
        currentFocus: {
          primaryActivity: null,
          objective: '',
          focusIntensity: 0.0,
          startedAt: Date.now()
        },
        cognitiveLoad: 0.0,
        availableCapacity: 1.0,
        activePreoccupations: [],
        updatedAt: Date.now()
      };
      cognitionStorage.setSelfState(this.personaId, state);
    }

    return state;
  }

  /**
   * Update focus to new activity
   */
  async updateFocus(focus: {
    activity: string | null;
    objective: string;
    intensity: number;
  }): Promise<void> {
    const state = await this.get();

    state.currentFocus = {
      primaryActivity: focus.activity,
      objective: focus.objective,
      focusIntensity: focus.intensity,
      startedAt: Date.now()
    };
    state.updatedAt = Date.now();

    cognitionStorage.setSelfState(this.personaId, state);
  }

  /**
   * Update cognitive load
   */
  async updateLoad(delta: number): Promise<void> {
    const state = await this.get();

    state.cognitiveLoad = Math.max(0, Math.min(1.0, state.cognitiveLoad + delta));
    state.availableCapacity = 1.0 - state.cognitiveLoad;
    state.updatedAt = Date.now();

    cognitionStorage.setSelfState(this.personaId, state);
  }

  /**
   * Set cognitive load directly
   */
  async setLoad(load: number): Promise<void> {
    const state = await this.get();

    state.cognitiveLoad = Math.max(0, Math.min(1.0, load));
    state.availableCapacity = 1.0 - state.cognitiveLoad;
    state.updatedAt = Date.now();

    cognitionStorage.setSelfState(this.personaId, state);
  }

  /**
   * Add a preoccupation (something on the persona's mind)
   */
  async addPreoccupation(concern: string, priority: number, domain: string): Promise<void> {
    const state = await this.get();

    // Don't add duplicates
    const exists = state.activePreoccupations.some(p => p.concern === concern);
    if (exists) return;

    state.activePreoccupations.push({
      concern,
      priority,
      domain,
      createdAt: Date.now()
    });
    state.updatedAt = Date.now();

    cognitionStorage.setSelfState(this.personaId, state);
  }

  /**
   * Remove a preoccupation (when it's been addressed)
   */
  async removePreoccupation(concern: string): Promise<void> {
    const state = await this.get();

    state.activePreoccupations = state.activePreoccupations.filter(
      p => p.concern !== concern
    );
    state.updatedAt = Date.now();

    cognitionStorage.setSelfState(this.personaId, state);
  }

  /**
   * Clear focus (reset to idle)
   */
  async clearFocus(): Promise<void> {
    const state = await this.get();

    state.currentFocus = {
      primaryActivity: null,
      objective: '',
      focusIntensity: 0.0,
      startedAt: Date.now()
    };
    state.updatedAt = Date.now();

    cognitionStorage.setSelfState(this.personaId, state);
  }

  /**
   * Reset to idle state (clear everything)
   */
  async reset(): Promise<void> {
    const state: SelfStateEntry = {
      personaId: this.personaId,
      currentFocus: {
        primaryActivity: null,
        objective: '',
        focusIntensity: 0.0,
        startedAt: Date.now()
      },
      cognitiveLoad: 0.0,
      availableCapacity: 1.0,
      activePreoccupations: [],
      updatedAt: Date.now()
    };

    cognitionStorage.setSelfState(this.personaId, state);
  }
}
