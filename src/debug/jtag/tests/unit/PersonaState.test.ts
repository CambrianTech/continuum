/**
 * PersonaState Unit Tests
 *
 * Tests internal state management and adaptive cadence for autonomous personas
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { PersonaStateManager, DEFAULT_STATE_CONFIG, type PersonaState } from '../../system/user/server/modules/PersonaState';

describe('PersonaStateManager', () => {
  let stateManager: PersonaStateManager;
  const personaName = 'TestPersona';

  beforeEach(() => {
    stateManager = new PersonaStateManager(personaName, {
      ...DEFAULT_STATE_CONFIG,
      enableLogging: false
    });
  });

  describe('Initialization', () => {
    it('should initialize with default state', () => {
      const state = stateManager.getState();

      expect(state.energy).toBe(1.0);
      expect(state.attention).toBe(1.0);
      expect(state.mood).toBe('idle');
      expect(state.inboxLoad).toBe(0);
      expect(state.responseCount).toBe(0);
      expect(state.computeBudget).toBe(1.0);
    });

    it('should accept custom config', () => {
      const customManager = new PersonaStateManager(personaName, {
        energyDepletionRate: 0.001,
        energyRecoveryRate: 0.0001,
        attentionFatigueRate: 0.2,
        enableLogging: false
      });

      const state = customManager.getState();
      expect(state.energy).toBe(1.0);
    });
  });

  describe('Energy Management', () => {
    it('should deplete energy with activity', async () => {
      const stateBefore = stateManager.getState();
      expect(stateBefore.energy).toBe(1.0);

      // Record 10 seconds of processing at complexity 1.0
      await stateManager.recordActivity(10000, 1.0);

      const stateAfter = stateManager.getState();
      expect(stateAfter.energy).toBeLessThan(1.0);
      expect(stateAfter.energy).toBeGreaterThanOrEqual(0);
    });

    it('should deplete energy faster with higher complexity', async () => {
      const manager1 = new PersonaStateManager('Persona1', {
        ...DEFAULT_STATE_CONFIG,
        enableLogging: false
      });
      const manager2 = new PersonaStateManager('Persona2', {
        ...DEFAULT_STATE_CONFIG,
        enableLogging: false
      });

      // Same duration, different complexity
      await manager1.recordActivity(5000, 1.0);  // Low complexity
      await manager2.recordActivity(5000, 2.0);  // High complexity

      const state1 = manager1.getState();
      const state2 = manager2.getState();

      expect(state2.energy).toBeLessThan(state1.energy);
    });

    it('should not go below 0 energy', async () => {
      // Exhaust energy
      for (let i = 0; i < 20; i++) {
        await stateManager.recordActivity(10000, 2.0);
      }

      const state = stateManager.getState();
      expect(state.energy).toBeGreaterThanOrEqual(0);
      expect(state.energy).toBe(0);
    });

    it('should recover energy with rest', async () => {
      // Deplete energy
      await stateManager.recordActivity(10000, 2.0);
      const stateAfterWork = stateManager.getState();
      const energyAfterWork = stateAfterWork.energy;

      // Rest
      await stateManager.rest(10000);
      const stateAfterRest = stateManager.getState();

      expect(stateAfterRest.energy).toBeGreaterThan(energyAfterWork);
    });

    it('should not exceed 1.0 energy', async () => {
      // Already at 1.0, rest more
      await stateManager.rest(10000);

      const state = stateManager.getState();
      expect(state.energy).toBeLessThanOrEqual(1.0);
      expect(state.energy).toBe(1.0);
    });
  });

  describe('Attention Management', () => {
    it('should deplete attention when tired', async () => {
      // Exhaust energy until tired (< 0.3)
      for (let i = 0; i < 15; i++) {
        await stateManager.recordActivity(5000, 2.0);
      }

      const state = stateManager.getState();
      expect(state.energy).toBeLessThan(0.3);
      expect(state.attention).toBeLessThan(1.0);
    });

    it('should not deplete attention when energized', async () => {
      // Light activity
      await stateManager.recordActivity(1000, 0.5);

      const state = stateManager.getState();
      expect(state.energy).toBeGreaterThan(0.3);
      expect(state.attention).toBe(1.0);
    });

    it('should recover attention with rest (faster than energy)', async () => {
      // Exhaust to trigger attention fatigue
      for (let i = 0; i < 15; i++) {
        await stateManager.recordActivity(5000, 2.0);
      }

      const stateBeforeRest = stateManager.getState();
      const energyBefore = stateBeforeRest.energy;
      const attentionBefore = stateBeforeRest.attention;

      // Rest
      await stateManager.rest(5000);

      const stateAfterRest = stateManager.getState();
      const energyGain = stateAfterRest.energy - energyBefore;
      const attentionGain = stateAfterRest.attention - attentionBefore;

      expect(attentionGain).toBeGreaterThan(energyGain);
    });
  });

  describe('Mood Calculation', () => {
    it('should be idle initially', () => {
      const state = stateManager.getState();
      expect(state.mood).toBe('idle');
    });

    it('should become active when energized and responding', async () => {
      await stateManager.recordActivity(1000, 0.5); // Light activity

      const state = stateManager.getState();
      expect(state.energy).toBeGreaterThan(0.5);
      expect(state.responseCount).toBeGreaterThan(0);
      expect(state.mood).toBe('active');
    });

    it('should become tired when low energy', async () => {
      // Deplete energy below 0.3
      for (let i = 0; i < 15; i++) {
        await stateManager.recordActivity(5000, 2.0);
      }

      const state = stateManager.getState();
      expect(state.energy).toBeLessThan(0.3);
      expect(state.mood).toBe('tired');
    });

    it('should become overwhelmed when inbox is overloaded', () => {
      stateManager.updateInboxLoad(100); // > 50 threshold

      const state = stateManager.getState();
      expect(state.mood).toBe('overwhelmed');
    });

    it('should prioritize overwhelmed over tired', async () => {
      // Make tired (low energy)
      for (let i = 0; i < 15; i++) {
        await stateManager.recordActivity(5000, 2.0);
      }

      // Then overload inbox
      stateManager.updateInboxLoad(100);

      const state = stateManager.getState();
      expect(state.energy).toBeLessThan(0.3);    // Tired
      expect(state.inboxLoad).toBeGreaterThan(50); // Overloaded
      expect(state.mood).toBe('overwhelmed');     // Overwhelmed wins
    });
  });

  describe('Traffic Management - shouldEngage()', () => {
    describe('High Priority Messages (> 0.8)', () => {
      it('should ALWAYS engage with high priority (prevent starvation)', async () => {
        // Even when overwhelmed
        stateManager.updateInboxLoad(100);
        expect(stateManager.shouldEngage(0.9)).toBe(true);

        // Even when exhausted
        for (let i = 0; i < 20; i++) {
          await stateManager.recordActivity(10000, 2.0);
        }
        expect(stateManager.shouldEngage(0.85)).toBe(true);
      });
    });

    describe('Overwhelmed Mode', () => {
      beforeEach(() => {
        stateManager.updateInboxLoad(100);
      });

      it('should only process highest priority (> 0.9)', () => {
        expect(stateManager.shouldEngage(0.95)).toBe(true);
        expect(stateManager.shouldEngage(0.85)).toBe(true);  // Still high priority
        expect(stateManager.shouldEngage(0.7)).toBe(false);
        expect(stateManager.shouldEngage(0.5)).toBe(false);
      });
    });

    describe('Tired Mode', () => {
      beforeEach(async () => {
        // Deplete energy below 0.3
        for (let i = 0; i < 15; i++) {
          await stateManager.recordActivity(5000, 2.0);
        }
      });

      it('should require priority > 0.5 AND energy > 0.2', async () => {
        const state = stateManager.getState();
        expect(state.mood).toBe('tired');

        // Has some energy (> 0.2), high enough priority
        if (state.energy > 0.2) {
          expect(stateManager.shouldEngage(0.6)).toBe(true);
        }

        // Exhaust completely
        for (let i = 0; i < 10; i++) {
          await stateManager.recordActivity(10000, 2.0);
        }

        expect(stateManager.shouldEngage(0.6)).toBe(false); // No energy
      });
    });

    describe('Active Mode', () => {
      beforeEach(async () => {
        await stateManager.recordActivity(1000, 0.5); // Light activity
      });

      it('should engage with priority > 0.3', () => {
        const state = stateManager.getState();
        expect(state.mood).toBe('active');

        expect(stateManager.shouldEngage(0.5)).toBe(true);
        expect(stateManager.shouldEngage(0.4)).toBe(true);
        expect(stateManager.shouldEngage(0.2)).toBe(false);
      });
    });

    describe('Idle Mode', () => {
      it('should be eager to work (priority > 0.1)', () => {
        const state = stateManager.getState();
        expect(state.mood).toBe('idle');

        expect(stateManager.shouldEngage(0.5)).toBe(true);
        expect(stateManager.shouldEngage(0.2)).toBe(true);
        expect(stateManager.shouldEngage(0.05)).toBe(false);
      });
    });
  });

  describe('Adaptive Cadence - getCadence()', () => {
    it('should return 3s when idle (eager)', () => {
      expect(stateManager.getCadence()).toBe(3000);
    });

    it('should return 5s when active (normal pace)', async () => {
      await stateManager.recordActivity(1000, 0.5);

      const state = stateManager.getState();
      expect(state.mood).toBe('active');
      expect(stateManager.getCadence()).toBe(5000);
    });

    it('should return 7s when tired (moderate pace)', async () => {
      // Deplete energy
      for (let i = 0; i < 15; i++) {
        await stateManager.recordActivity(5000, 2.0);
      }

      const state = stateManager.getState();
      expect(state.mood).toBe('tired');
      expect(stateManager.getCadence()).toBe(7000);
    });

    it('should return 10s when overwhelmed (back pressure)', () => {
      stateManager.updateInboxLoad(100);

      const state = stateManager.getState();
      expect(state.mood).toBe('overwhelmed');
      expect(stateManager.getCadence()).toBe(10000);
    });

    it('should double cadence when compute budget low', () => {
      const normalCadence = stateManager.getCadence();

      stateManager.updateComputeBudget(0.3); // < 0.5
      const slowCadence = stateManager.getCadence();

      expect(slowCadence).toBe(normalCadence * 2);
    });

    it('should not double when compute budget sufficient', () => {
      const normalCadence = stateManager.getCadence();

      stateManager.updateComputeBudget(0.8); // > 0.5
      const sameCadence = stateManager.getCadence();

      expect(sameCadence).toBe(normalCadence);
    });
  });

  describe('Inbox Load Management', () => {
    it('should update inbox load', () => {
      stateManager.updateInboxLoad(25);

      const state = stateManager.getState();
      expect(state.inboxLoad).toBe(25);
    });

    it('should trigger mood change when overloaded', () => {
      const stateBefore = stateManager.getState();
      expect(stateBefore.mood).not.toBe('overwhelmed');

      stateManager.updateInboxLoad(100);

      const stateAfter = stateManager.getState();
      expect(stateAfter.mood).toBe('overwhelmed');
    });
  });

  describe('Compute Budget Management', () => {
    it('should update compute budget', () => {
      stateManager.updateComputeBudget(0.5);

      const state = stateManager.getState();
      expect(state.computeBudget).toBe(0.5);
    });

    it('should affect cadence when low', () => {
      const normalCadence = stateManager.getCadence();

      stateManager.updateComputeBudget(0.3);
      const slowCadence = stateManager.getCadence();

      expect(slowCadence).toBeGreaterThan(normalCadence);
    });
  });

  describe('Response Count Management', () => {
    it('should increment response count with activity', async () => {
      await stateManager.recordActivity(1000, 0.5);
      await stateManager.recordActivity(1000, 0.5);

      const state = stateManager.getState();
      expect(state.responseCount).toBe(2);
    });

    it('should reset response count', async () => {
      await stateManager.recordActivity(1000, 0.5);

      const stateBefore = stateManager.getState();
      expect(stateBefore.responseCount).toBeGreaterThan(0);

      stateManager.resetResponseCount();

      const stateAfter = stateManager.getState();
      expect(stateAfter.responseCount).toBe(0);
    });
  });

  describe('State Summary', () => {
    it('should provide formatted summary', () => {
      const summary = stateManager.getSummary();

      expect(summary).toContain('energy=');
      expect(summary).toContain('attention=');
      expect(summary).toContain('mood=');
      expect(summary).toContain('inbox=');
      expect(summary).toContain('responses=');
      expect(summary).toContain('budget=');
    });

    it('should reflect current state', async () => {
      await stateManager.recordActivity(1000, 1.0);
      stateManager.updateInboxLoad(25);

      const summary = stateManager.getSummary();

      expect(summary).toContain('mood=active');
      expect(summary).toContain('inbox=25');
      expect(summary).toContain('responses=1');
    });
  });

  describe('Integration Scenarios', () => {
    it('should handle work-rest cycle', async () => {
      // Work
      await stateManager.recordActivity(10000, 1.5);
      const stateAfterWork = stateManager.getState();
      const energyAfterWork = stateAfterWork.energy;

      // Rest
      await stateManager.rest(10000);
      const stateAfterRest = stateManager.getState();

      expect(stateAfterRest.energy).toBeGreaterThan(energyAfterWork);
      expect(stateAfterRest.attention).toBeGreaterThanOrEqual(stateAfterWork.attention);
    });

    it('should recover from deadlock state (exhausted energy)', async () => {
      // BUG FIX TEST: Phase 7.5.3.2
      // Before fix: Personas could exhaust energy and never recover (permanent deadlock)
      // After fix: rest() calls during idle/skip allow gradual energy recovery

      // Step 1: Exhaust energy completely (simulate heavy load)
      for (let i = 0; i < 20; i++) {
        await stateManager.recordActivity(10000, 2.0);
      }

      const exhaustedState = stateManager.getState();
      expect(exhaustedState.energy).toBe(0); // Completely exhausted
      expect(exhaustedState.mood).toBe('tired');
      expect(stateManager.shouldEngage(0.5)).toBe(false); // Can't process messages

      // Step 2: Simulate rest cycles (what serviceInbox now does when skipping messages)
      // At 7s cadence (tired mode), rest for ~2 minutes = ~17 cycles
      const cadence = 7000;
      const cycles = 17;

      for (let i = 0; i < cycles; i++) {
        await stateManager.rest(cadence);
      }

      // Step 3: Verify recovery
      const recoveredState = stateManager.getState();

      // Energy should recover: 0.00005 per ms * 7000ms * 17 cycles = ~0.6
      expect(recoveredState.energy).toBeGreaterThan(0.2); // Above tired threshold
      expect(recoveredState.energy).toBeLessThanOrEqual(1.0);

      // Should now be able to engage with messages again
      expect(stateManager.shouldEngage(0.6)).toBe(true);

      // Mood should improve (no longer tired if energy > 0.3)
      if (recoveredState.energy >= 0.3) {
        expect(recoveredState.mood).not.toBe('tired');
      }
    });

    it('should gradually recover energy during rest cycles', async () => {
      // Test incremental recovery (what happens in serviceInbox loop)

      // Deplete to ~0.10 (like Groq Lightning was stuck at)
      // energyDepletionRate = 0.0001 per ms
      // 10000ms activity * 0.0001 * complexity = energy loss
      // Start at 1.0, need to lose 0.9 to get to 0.1
      // 10000ms * 0.0001 * 2.0 = 0.002 per cycle
      // Need ~450 cycles... that's too many
      // Better: Just set energy directly for this test
      stateManager['state'].energy = 0.10;
      stateManager['state'].mood = 'tired';

      const initial = stateManager.getState();
      expect(initial.energy).toBe(0.10);
      expect(initial.mood).toBe('tired');

      // Rest for 5 cycles (simulate 5 polling iterations at 7s each)
      // energyRecoveryRate = 0.00005 per ms
      // 7000ms * 0.00005 = 0.35 per cycle (huge!)
      // After 1 cycle: 0.10 + 0.35 = 0.45
      // After 2 cycles: 0.45 + 0.35 = 0.80
      // After 3 cycles: 0.80 + 0.35 = 1.0 (capped!)
      // So we'll only see 3 increases before capping

      const energyLevels: number[] = [initial.energy];

      for (let i = 0; i < 5; i++) {
        await stateManager.rest(7000);
        energyLevels.push(stateManager.getState().energy);
      }

      // Energy should increase monotonically UNTIL it caps at 1.0
      // After capping, it stays at 1.0 (not greater than previous)
      // So we check: each level is >= previous (allows for capping)
      for (let i = 1; i < energyLevels.length; i++) {
        expect(energyLevels[i]).toBeGreaterThanOrEqual(energyLevels[i - 1]);
      }

      // Final energy should be noticeably higher than initial
      const final = stateManager.getState();
      expect(final.energy).toBeGreaterThan(initial.energy + 0.10);

      // Should eventually cap at 1.0
      expect(final.energy).toBeLessThanOrEqual(1.0);
    });

    it('should transition through moods realistically', async () => {
      // Start idle
      expect(stateManager.getState().mood).toBe('idle');

      // Work -> active
      await stateManager.recordActivity(1000, 0.5);
      expect(stateManager.getState().mood).toBe('active');

      // Heavy work -> tired
      for (let i = 0; i < 15; i++) {
        await stateManager.recordActivity(5000, 2.0);
      }
      expect(stateManager.getState().mood).toBe('tired');

      // Inbox overload -> overwhelmed
      stateManager.updateInboxLoad(100);
      expect(stateManager.getState().mood).toBe('overwhelmed');

      // Clear inbox, rest -> back to idle
      stateManager.updateInboxLoad(0);
      stateManager.resetResponseCount();
      await stateManager.rest(50000);

      const finalState = stateManager.getState();
      expect(finalState.mood).toBe('idle');
      expect(finalState.energy).toBeGreaterThan(0.5);
    });

    it('should respect NEVER neglect high priority rule', async () => {
      // Make persona exhausted and overwhelmed
      for (let i = 0; i < 20; i++) {
        await stateManager.recordActivity(10000, 2.0);
      }
      stateManager.updateInboxLoad(100);

      const state = stateManager.getState();
      expect(state.mood).toBe('overwhelmed');
      expect(state.energy).toBe(0);

      // High priority message MUST still be processed
      expect(stateManager.shouldEngage(0.85)).toBe(true);
    });
  });
});
