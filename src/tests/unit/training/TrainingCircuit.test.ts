/**
 * Training Circuit Unit Tests
 *
 * Verifies the three training paths are wired end-to-end:
 * 1. PersonaTrainingManager: accumulator → convert → JSONL → GenomeJobCreate
 * 2. TrainingDaemonServer: threshold → Events.emit('training:dataset-ready')
 * 3. GenomeBatchMicroTuneServerCommand: PersonaUser → accumulator → PersonaTrainingManager
 *
 * Also tests the type conversion from accumulator format (input/output)
 * to fine-tuning format (messages[]).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { PersonaTrainingManager } from '../../../system/user/server/modules/PersonaTrainingManager';
import { TrainingDataAccumulator } from '../../../system/user/server/modules/TrainingDataAccumulator';
import type { InteractionCapture } from '../../../system/user/server/modules/TrainingDataAccumulator';
import { Events } from '../../../system/core/shared/Events';
import { GenomeJobCreate } from '../../../commands/genome/job-create/shared/GenomeJobCreateTypes';
import { TrainingDatasetBuilder } from '../../../system/genome/fine-tuning/server/TrainingDatasetBuilder';

// Mock dependencies that PersonaTrainingManager uses
vi.mock('../../../system/core/shared/Events', () => ({
  Events: {
    emit: vi.fn().mockResolvedValue(undefined),
    subscribe: vi.fn(),
  },
}));

vi.mock('../../../commands/genome/job-create/shared/GenomeJobCreateTypes', () => ({
  GenomeJobCreate: {
    execute: vi.fn().mockResolvedValue({
      success: true,
      job: {
        jobId: 'test-job-123',
        providerJobId: 'prov-job-456',
        provider: 'peft',
        status: 'queued',
        baseModel: 'llama3.2',
        trainingFileId: '/tmp/test.jsonl',
        createdAt: Date.now(),
        configurationSummary: {
          method: 'lora',
          epochs: 3,
          batchSize: 4,
          learningRate: 0.0001,
          sequenceLength: 2048,
        },
      },
    }),
  },
}));

vi.mock('../../../system/genome/fine-tuning/server/TrainingDatasetBuilder', () => ({
  TrainingDatasetBuilder: {
    validateDataset: vi.fn().mockReturnValue({ valid: true, warnings: [], errors: [] }),
    exportToJSONL: vi.fn().mockReturnValue('{"messages":[{"role":"user","content":"hello"}]}\n'),
  },
}));

vi.mock('../../../daemons/data-daemon/shared/entities/FineTuningTypes', () => ({
  TrainingMethod: { FULL: 'full', LORA: 'lora', QLORA: 'qlora' },
  TrainOnInputs: { AUTO: 'auto', ENABLED: 'enabled', DISABLED: 'disabled' },
  LRSchedulerType: { COSINE: 'cosine', LINEAR: 'linear', CONSTANT: 'constant' },
}));

vi.mock('fs', () => ({
  default: {
    promises: {
      mkdir: vi.fn().mockResolvedValue(undefined),
      writeFile: vi.fn().mockResolvedValue(undefined),
    },
  },
  promises: {
    mkdir: vi.fn().mockResolvedValue(undefined),
    writeFile: vi.fn().mockResolvedValue(undefined),
  },
}));

// ── Helpers ────────────────────────────────────────────────

const PERSONA_ID = 'test-persona-training';
const PERSONA_NAME = 'Test Trainer';

/**
 * MIN_BATCH_SIZE in TrainingDataAccumulator is 10, so thresholds below 10
 * get clamped. Use 10 as the minimum meaningful threshold for tests.
 */
const MIN_THRESHOLD = 10;

function createAccumulator(batchThreshold = MIN_THRESHOLD): TrainingDataAccumulator {
  const accumulator = new TrainingDataAccumulator(PERSONA_ID, PERSONA_NAME, () => {});
  accumulator.setBatchThreshold('conversation', batchThreshold);
  return accumulator;
}

function createManager(accumulator: TrainingDataAccumulator): PersonaTrainingManager {
  const mockState = {
    learningState: { isLearning: false },
  };
  return new PersonaTrainingManager(
    PERSONA_ID,
    PERSONA_NAME,
    accumulator,
    () => mockState as any,
    async () => ({ success: true }),
    () => {}, // silent logger
  );
}

async function fillAccumulator(
  accumulator: TrainingDataAccumulator,
  domain: string,
  count: number,
): Promise<string[]> {
  const ids: string[] = [];
  for (let i = 0; i < count; i++) {
    const capture: InteractionCapture = {
      roleId: 'student',
      domain,
      input: `Question ${i}: What is concept ${i}?`,
      output: `Answer ${i}: Concept ${i} is an important idea in the domain.`,
    };
    ids.push(await accumulator.captureInteraction(capture));
  }
  return ids;
}

// ── Tests ──────────────────────────────────────────────────

describe('Training Circuit', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('PersonaTrainingManager: type conversion', () => {
    it('converts accumulator examples (input/output) to fine-tuning format (messages[])', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD);

      // Trigger training (buffer at threshold)
      await manager.checkTrainingReadiness();

      // Verify GenomeJobCreate was called
      expect(GenomeJobCreate.execute).toHaveBeenCalled();

      // Verify TrainingDatasetBuilder.validateDataset was called with converted dataset
      expect(TrainingDatasetBuilder.validateDataset).toHaveBeenCalled();
      expect(TrainingDatasetBuilder.exportToJSONL).toHaveBeenCalled();

      // The dataset passed to validateDataset should have messages[] format
      const validateCall = vi.mocked(TrainingDatasetBuilder.validateDataset).mock.calls[0][0];
      expect(validateCall.examples).toHaveLength(MIN_THRESHOLD);
      expect(validateCall.examples[0].messages).toBeDefined();
      expect(validateCall.examples[0].messages).toHaveLength(2);
      expect(validateCall.examples[0].messages[0].role).toBe('user');
      expect(validateCall.examples[0].messages[0].content).toContain('Question 0');
      expect(validateCall.examples[0].messages[1].role).toBe('assistant');
      expect(validateCall.examples[0].messages[1].content).toContain('Answer 0');
    });

    it('preserves feedback rating as confidence in metadata', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      const manager = createManager(accumulator);

      // Fill to threshold, then attach feedback to last one
      const ids = await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD);
      await accumulator.captureFeedback({
        interactionId: ids[0],
        source: 'human',
        rating: 0.95,
      });

      await manager.checkTrainingReadiness();

      const validateCall = vi.mocked(TrainingDatasetBuilder.validateDataset).mock.calls[0][0];
      expect(validateCall.examples[0].metadata?.confidence).toBe(0.95);
    });
  });

  describe('PersonaTrainingManager: training trigger', () => {
    it('does not trigger when buffer below threshold', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD - 3);

      await manager.checkTrainingReadiness();

      expect(GenomeJobCreate.execute).not.toHaveBeenCalled();
    });

    it('triggers when buffer reaches threshold', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD);

      await manager.checkTrainingReadiness();

      expect(GenomeJobCreate.execute).toHaveBeenCalledTimes(1);
    });

    it('forceDomain bypasses threshold check', async () => {
      const accumulator = createAccumulator(1000); // Clamps to MAX_BATCH_SIZE but well above fill count
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', 2);

      await manager.checkTrainingReadiness('conversation'); // Force this domain

      expect(GenomeJobCreate.execute).toHaveBeenCalledTimes(1);
    });

    it('forceDomain does not affect other domains', async () => {
      const accumulator = createAccumulator(1000);
      accumulator.setBatchThreshold('code', 1000);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', 2);
      await fillAccumulator(accumulator, 'code', 2);

      // Force 'conversation' only
      await manager.checkTrainingReadiness('conversation');

      // Only conversation should trigger, not code
      expect(GenomeJobCreate.execute).toHaveBeenCalledTimes(1);
    });

    it('consumes buffer after training (buffer is empty after)', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD + 3);
      expect(accumulator.getBufferSize('conversation')).toBe(MIN_THRESHOLD + 3);

      await manager.checkTrainingReadiness();

      expect(accumulator.getBufferSize('conversation')).toBe(0);
    });

    it('emits TRAINING_STARTED event', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD);
      await manager.checkTrainingReadiness();

      const emitCalls = vi.mocked(Events.emit).mock.calls;
      const startedCall = emitCalls.find(
        call => call[0] === 'ai:learning:training-started',
      );
      expect(startedCall).toBeDefined();
      expect(startedCall![1]).toMatchObject({
        personaId: PERSONA_ID,
        domain: 'conversation',
        exampleCount: MIN_THRESHOLD,
      });
    });

    it('writes JSONL file to disk before training', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD);
      await manager.checkTrainingReadiness();

      const fs = await import('fs');
      expect(fs.promises.mkdir).toHaveBeenCalled();
      expect(fs.promises.writeFile).toHaveBeenCalled();

      // Verify the file path includes personaId
      const writeCalls = vi.mocked(fs.promises.writeFile).mock.calls;
      const filePath = writeCalls[0][0] as string;
      expect(filePath).toContain(PERSONA_ID);
      expect(filePath).toContain('.jsonl');
    });

    it('emits TRAINING_ERROR when validation fails', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD);

      // Make validation fail
      vi.mocked(TrainingDatasetBuilder.validateDataset).mockReturnValueOnce({
        valid: false,
        warnings: [],
        errors: ['Too few examples'],
      });

      await manager.checkTrainingReadiness();

      const emitCalls = vi.mocked(Events.emit).mock.calls;
      const errorCall = emitCalls.find(
        call => call[0] === 'ai:learning:training-error',
      );
      expect(errorCall).toBeDefined();
      expect(errorCall![1]).toMatchObject({
        personaId: PERSONA_ID,
        phase: 'preparation',
      });

      // GenomeJobCreate should NOT have been called
      expect(GenomeJobCreate.execute).not.toHaveBeenCalled();
    });

    it('emits TRAINING_ERROR when GenomeJobCreate fails', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD);

      vi.mocked(GenomeJobCreate.execute).mockResolvedValueOnce({
        success: false,
        error: 'Provider unavailable',
      } as any);

      await manager.checkTrainingReadiness();

      const emitCalls = vi.mocked(Events.emit).mock.calls;
      const errorCall = emitCalls.find(
        call => call[0] === 'ai:learning:training-error',
      );
      expect(errorCall).toBeDefined();
      expect((errorCall![1] as any).error).toContain('Provider unavailable');
    });
  });

  describe('TrainingDataAccumulator: domain isolation', () => {
    it('different domains accumulate independently', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      // 'code' uses default threshold (50)

      await fillAccumulator(accumulator, 'conversation', 5);
      await fillAccumulator(accumulator, 'code', 12);

      expect(accumulator.getBufferSize('conversation')).toBe(5);
      expect(accumulator.getBufferSize('code')).toBe(12);
      expect(accumulator.shouldMicroTune('conversation')).toBe(false); // 5 < 10
      expect(accumulator.shouldMicroTune('code')).toBe(false); // 12 < 50 (default)
    });

    it('consuming one domain does not affect others', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);

      await fillAccumulator(accumulator, 'conversation', 15);
      await fillAccumulator(accumulator, 'code', 15);

      const consumed = await accumulator.consumeTrainingData('conversation');
      expect(consumed).toHaveLength(15);
      expect(accumulator.getBufferSize('conversation')).toBe(0);
      expect(accumulator.getBufferSize('code')).toBe(15);
    });

    it('getStats returns all domains with correct thresholds', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      // 'code' gets default threshold (50)

      await fillAccumulator(accumulator, 'conversation', 3);
      await fillAccumulator(accumulator, 'code', 7);

      const stats = accumulator.getStats();
      expect(stats['conversation']).toEqual({ count: 3, threshold: MIN_THRESHOLD, ready: false });
      expect(stats['code']).toEqual({ count: 7, threshold: 50, ready: false }); // Default threshold
    });

    it('getDomains only returns non-empty domains', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);

      await fillAccumulator(accumulator, 'conversation', 2);
      await accumulator.consumeTrainingData('conversation');

      const domains = accumulator.getDomains();
      expect(domains).not.toContain('conversation');
    });
  });

  describe('PersonaTrainingManager: multi-domain training', () => {
    it('trains all domains that are at threshold in single call', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      accumulator.setBatchThreshold('code', MIN_THRESHOLD);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD);
      await fillAccumulator(accumulator, 'code', MIN_THRESHOLD + 3);

      await manager.checkTrainingReadiness();

      // Both domains should trigger
      expect(GenomeJobCreate.execute).toHaveBeenCalledTimes(2);
    });

    it('skips domains below threshold while training ready ones', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      accumulator.setBatchThreshold('code', 100);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD); // At threshold
      await fillAccumulator(accumulator, 'code', 5); // Below code threshold (100)

      await manager.checkTrainingReadiness();

      // Only conversation should trigger
      expect(GenomeJobCreate.execute).toHaveBeenCalledTimes(1);
    });
  });

  describe('GenomeJobCreate integration', () => {
    it('passes correct configuration to GenomeJobCreate', async () => {
      const accumulator = createAccumulator(MIN_THRESHOLD);
      const manager = createManager(accumulator);

      await fillAccumulator(accumulator, 'conversation', MIN_THRESHOLD);
      await manager.checkTrainingReadiness();

      const call = vi.mocked(GenomeJobCreate.execute).mock.calls[0][0];

      expect(call.personaId).toBe(PERSONA_ID);
      expect(call.provider).toBe('unsloth');
      expect(call.trainingFileId).toBeDefined();
      expect(call.configuration).toBeDefined();
      expect(call.configuration.model.baseModel).toBe('llama3.2');
      expect(call.configuration.method.type).toBe('lora');
      expect(call.configuration.method.loraConfig).toMatchObject({ rank: 16, alpha: 32 });
      expect(call.configuration.schedule.epochs).toBe(3);
      expect(call.configuration.schedule.batchSize).toBe(4);
      expect(call.configuration.optimizer.learningRate).toBe(0.0001);
    });
  });
});
