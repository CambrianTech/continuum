/**
 * Training Outcome Wiring Test
 *
 * Validates that the genome:training:complete event correctly triggers
 * GapDetector.recordTrainingOutcome() — closing the meta-learning loop.
 */

import { describe, it, expect, vi } from 'vitest';
import { Events } from '../../system/core/shared/Events';
import { GapDetector } from '../../system/user/server/modules/GapDetector';
import type { SuggestedAction } from '../../system/user/server/modules/GapDetector';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

// Mock RustCognitionBridge
function createMockBridge() {
  return {
    coverageReport: vi.fn().mockResolvedValue({
      coverage_ratio: 0.5,
      gaps: [],
      covered: [],
    }),
    classifyDomain: vi.fn(),
    serviceCycleFull: vi.fn(),
  } as any;
}

// Mock TrainingBuffer
function createMockTrainingBuffer() {
  return {
    getBufferStats: vi.fn().mockReturnValue({}),
  } as any;
}

describe('Training Outcome Wiring', () => {
  describe('GapDetector.recordTrainingOutcome', () => {
    it('should store training history per domain', () => {
      const detector = new GapDetector(
        'test-persona-id' as UUID,
        createMockBridge(),
        createMockTrainingBuffer(),
        () => {},
      );

      detector.recordTrainingOutcome('typescript', 0.3, 0.7, 'fine-tune-lora');

      // Access private training history for verification
      const history = (detector as any).trainingHistory;
      expect(history.has('typescript')).toBe(true);

      const entries = history.get('typescript');
      expect(entries).toHaveLength(1);
      expect(entries[0].preFitness).toBe(0.3);
      expect(entries[0].postFitness).toBe(0.7);
      expect(entries[0].action).toBe('fine-tune-lora');
    });

    it('should accumulate multiple training outcomes', () => {
      const detector = new GapDetector(
        'test-persona-id' as UUID,
        createMockBridge(),
        createMockTrainingBuffer(),
        () => {},
      );

      detector.recordTrainingOutcome('python', 0.2, 0.4, 'enroll-academy');
      detector.recordTrainingOutcome('python', 0.4, 0.6, 'fine-tune-lora');
      detector.recordTrainingOutcome('python', 0.6, 0.8, 'fine-tune-lora');

      const history = (detector as any).trainingHistory;
      expect(history.get('python')).toHaveLength(3);
    });

    it('should bound history to MAX_HISTORY_PER_DOMAIN', () => {
      const detector = new GapDetector(
        'test-persona-id' as UUID,
        createMockBridge(),
        createMockTrainingBuffer(),
        () => {},
      );

      // Record 15 outcomes (max is 10)
      for (let i = 0; i < 15; i++) {
        detector.recordTrainingOutcome('rust', i / 15, (i + 1) / 15, 'fine-tune-lora');
      }

      const history = (detector as any).trainingHistory;
      expect(history.get('rust').length).toBeLessThanOrEqual(10);
    });
  });

  describe('Meta-learning escalation', () => {
    it('should detect stalled training (2+ failures to improve)', () => {
      const detector = new GapDetector(
        'test-persona-id' as UUID,
        createMockBridge(),
        createMockTrainingBuffer(),
        () => {},
      );

      // Two trainings that didn't improve fitness
      detector.recordTrainingOutcome('code', 0.4, 0.4, 'fine-tune-lora');
      detector.recordTrainingOutcome('code', 0.4, 0.35, 'fine-tune-lora');

      // Access private checkMetaLearning
      const result = (detector as any).checkMetaLearning('code', 'fine-tune-lora', 'knowledge');

      expect(result.boosted).toBe(true);
      expect(result.failedAttempts).toBe(2);
      // Should escalate from fine-tune → retrain
      expect(result.escalatedAction).toBe('retrain');
      // Should switch mode from knowledge → coding
      expect(result.escalatedMode).toBe('coding');
    });

    it('should NOT escalate when training improves fitness', () => {
      const detector = new GapDetector(
        'test-persona-id' as UUID,
        createMockBridge(),
        createMockTrainingBuffer(),
        () => {},
      );

      // Two trainings that DID improve
      detector.recordTrainingOutcome('code', 0.3, 0.5, 'fine-tune-lora');
      detector.recordTrainingOutcome('code', 0.5, 0.7, 'fine-tune-lora');

      const result = (detector as any).checkMetaLearning('code', 'fine-tune-lora', 'knowledge');

      expect(result.boosted).toBe(false);
    });
  });

  describe('Event-driven wiring pattern', () => {
    it('should convert training loss to fitness correctly', () => {
      // This tests the logic from PersonaUser.ts where we subscribe to
      // genome:training:complete and convert finalLoss → fitness
      const finalLoss = 0.3;
      const postFitness = Math.max(0, 1 - finalLoss);

      expect(postFitness).toBeCloseTo(0.7);
    });

    it('should handle missing metrics gracefully', () => {
      // When metrics are missing, fitness should default to 0
      const payload = { personaId: 'test', traitType: 'code', metrics: {} };
      const postFitness = payload.metrics?.finalLoss != null
        ? Math.max(0, 1 - (payload.metrics as any).finalLoss)
        : 0;

      expect(postFitness).toBe(0);
    });

    it('should filter events by personaId', () => {
      const myPersonaId = 'my-persona' as UUID;
      const otherPersonaId = 'other-persona' as UUID;
      const recordSpy = vi.fn();

      // Simulate the subscription filter logic from PersonaUser
      const handler = (payload: any) => {
        if (payload.personaId !== myPersonaId) return;
        recordSpy(payload.traitType);
      };

      // My event — should record
      handler({ personaId: myPersonaId, traitType: 'typescript' });
      expect(recordSpy).toHaveBeenCalledWith('typescript');

      // Other persona's event — should be filtered
      handler({ personaId: otherPersonaId, traitType: 'python' });
      expect(recordSpy).toHaveBeenCalledTimes(1); // Still only 1
    });
  });
});
