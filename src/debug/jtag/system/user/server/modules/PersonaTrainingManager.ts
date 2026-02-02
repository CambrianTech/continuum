/**
 * PersonaTrainingManager - Handles continuous learning for PersonaUser
 *
 * Monitors training data accumulation and triggers LoRA fine-tuning
 * when thresholds are reached. Wires into the genome/job-create command
 * for real training execution via provider-specific adapters.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { Events } from '../../../core/shared/Events';
import type { TrainingDataAccumulator, TrainingExample as AccumulatorExample } from './TrainingDataAccumulator';
import type { UserStateEntity } from '../../../data/entities/UserStateEntity';
import { TrainingDatasetBuilder } from '../../../genome/fine-tuning/server/TrainingDatasetBuilder';
import { GenomeJobCreate } from '../../../../commands/genome/job-create/shared/GenomeJobCreateTypes';
import {
  TrainingMethod,
  TrainOnInputs,
  LRSchedulerType,
} from '../../../../daemons/data-daemon/shared/entities/FineTuningTypes';
import type { TrainingDataset, TrainingExample } from '../../../genome/fine-tuning/shared/FineTuningTypes';
import type { TraitType } from '../../../genome/entities/GenomeLayerEntity';
import {
  AI_LEARNING_EVENTS,
  type AITrainingStartedEventData,
  type AITrainingCompleteEventData,
  type AITrainingErrorEventData
} from '../../../events/shared/AILearningEvents';

/**
 * PersonaTrainingManager - Monitors training readiness and triggers micro-tuning
 *
 * Handles:
 * - Monitoring training data accumulation buffers
 * - Triggering training when threshold reached
 * - Updating learning state in UserStateEntity
 * - Emitting training lifecycle events
 */
export class PersonaTrainingManager {
  private log: (message: string) => void;

  constructor(
    private readonly personaId: UUID,
    private readonly displayName: string,
    private readonly trainingAccumulator: TrainingDataAccumulator,
    private readonly getState: () => UserStateEntity,
    private readonly saveState: () => Promise<{ success: boolean; error?: string }>,
    logger?: (message: string) => void
  ) {
    this.log = logger || (() => {});
  }

  /**
   * PHASE 7.5.1: Check training readiness and trigger micro-tuning
   *
   * Called periodically (less frequently than serviceInbox) to check if any
   * domain buffers are ready for training. When threshold reached, automatically
   * triggers genome/train command for that domain.
   *
   * This enables continuous learning: PersonaUsers improve through recipe execution
   * without manual intervention.
   */
  async checkTrainingReadiness(forceDomain?: string): Promise<void> {
    try {
      const domains = this.trainingAccumulator.getDomains();

      if (domains.length === 0) {
        return; // No accumulated training data
      }

      for (const domain of domains) {
        const isForced = domain === forceDomain;
        if (isForced || this.trainingAccumulator.shouldMicroTune(domain)) {
          const bufferSize = this.trainingAccumulator.getBufferSize(domain);
          const threshold = this.trainingAccumulator.getBatchThreshold(domain);

          this.log(`🧬 Training buffer ready for ${domain} (${bufferSize}/${threshold})`);

          const provider = 'unsloth'; // Default provider
          const estimatedTime = bufferSize * 25; // 25ms per example estimate

          // Update learning state in UserStateEntity
          const state = this.getState();
          if (!state.learningState) {
            state.learningState = { isLearning: false };
          }
          state.learningState.isLearning = true;
          state.learningState.domain = domain;
          state.learningState.provider = provider;
          state.learningState.startedAt = Date.now();
          state.learningState.exampleCount = bufferSize;
          state.learningState.estimatedCompletion = Date.now() + estimatedTime;
          await this.saveState(); // Persist state to database

          // Emit training started event
          const trainingStartedData: AITrainingStartedEventData = {
            personaId: this.personaId,
            personaName: this.displayName ?? 'AI Assistant',
            domain,
            provider,
            exampleCount: bufferSize,
            estimatedTime,
            timestamp: Date.now()
          };
          await Events.emit(AI_LEARNING_EVENTS.TRAINING_STARTED, trainingStartedData);

          // Consume training data from buffer
          const examples = await this.trainingAccumulator.consumeTrainingData(domain);
          if (examples.length === 0) {
            this.log(`📊 No examples after consumption for ${domain}, skipping`);
            state.learningState.isLearning = false;
            await this.saveState();
            continue;
          }

          this.log(`📊 Consumed ${examples.length} examples for ${domain} training`);

          // Convert accumulator examples to fine-tuning format
          const ftExamples = this.convertAccumulatorExamples(examples);

          // Execute real training via genome/job-create
          await this.executeTraining(domain as TraitType, ftExamples, provider);

          // Clear learning state after training submitted
          state.learningState.isLearning = false;
          state.learningState.domain = undefined;
          state.learningState.provider = undefined;
          state.learningState.startedAt = undefined;
          state.learningState.exampleCount = undefined;
          state.learningState.estimatedCompletion = undefined;
          await this.saveState();
        }
      }
    } catch (error) {
      this.log(`❌ Error checking training readiness: ${error}`);
    }
  }

  /**
   * Convert accumulator-format examples (input/output) to fine-tuning format (messages[]).
   * The accumulator stores raw I/O pairs; the training pipeline expects chat completion format.
   */
  private convertAccumulatorExamples(accExamples: AccumulatorExample[]): TrainingExample[] {
    return accExamples.map(ex => ({
      messages: [
        { role: 'user' as const, content: ex.input },
        { role: 'assistant' as const, content: ex.output },
      ],
      metadata: {
        timestamp: ex.timestamp.getTime(),
        confidence: ex.feedback?.rating,
      },
    }));
  }

  /**
   * Execute real LoRA fine-tuning via genome/job-create.
   *
   * Flow: examples → JSONL file on disk → genome/job-create → provider adapter → training job
   */
  private async executeTraining(
    traitType: TraitType,
    examples: TrainingExample[],
    provider: string,
  ): Promise<void> {
    try {
      // Build dataset from accumulated examples
      const dataset: TrainingDataset = {
        examples,
        metadata: {
          personaId: this.personaId,
          personaName: this.displayName ?? 'AI Assistant',
          traitType,
          createdAt: Date.now(),
          source: 'conversations',
          totalExamples: examples.length,
        },
      };

      // Validate dataset quality before training
      const validation = TrainingDatasetBuilder.validateDataset(dataset);
      if (!validation.valid) {
        this.log(`❌ Dataset validation failed: ${validation.errors.join(', ')}`);
        await Events.emit(AI_LEARNING_EVENTS.TRAINING_ERROR, {
          personaId: this.personaId,
          personaName: this.displayName ?? 'AI Assistant',
          domain: traitType,
          error: `Dataset validation failed: ${validation.errors.join(', ')}`,
          phase: 'preparation',
          timestamp: Date.now(),
        } satisfies AITrainingErrorEventData);
        return;
      }

      if (validation.warnings.length > 0) {
        this.log(`⚠️ Dataset warnings: ${validation.warnings.join(', ')}`);
      }

      // Export to JSONL and write to disk
      const jsonlContent = TrainingDatasetBuilder.exportToJSONL(dataset);
      const jsonlPath = await this.writeTrainingFile(traitType, jsonlContent);

      this.log(`📁 Training data written to ${jsonlPath} (${examples.length} examples)`);

      // Create fine-tuning job via the working command
      const result = await GenomeJobCreate.execute({
        personaId: this.personaId,
        provider,
        trainingFileId: jsonlPath,
        configuration: {
          model: { baseModel: 'llama3.2' },
          datasets: { trainingFileId: jsonlPath },
          method: {
            type: TrainingMethod.LORA,
            loraConfig: { rank: 16, alpha: 32, dropout: 0, trainableModules: 'all-linear' },
          },
          schedule: {
            epochs: 3,
            batchSize: 4,
            sequenceLength: 2048,
            gradientAccumulation: 1,
            checkpoints: 1,
            evaluations: 1,
            trainOnInputs: TrainOnInputs.DISABLED,
          },
          optimizer: {
            learningRate: 0.0001,
            scheduler: { type: LRSchedulerType.COSINE, minLRRatio: 0, warmupRatio: 0.1 },
            weightDecay: 0,
            maxGradientNorm: 1,
          },
          optimizations: { enabled: [] },
          output: {},
          metadata: {},
        },
      });

      if (result.success && result.job) {
        this.log(`🚀 Training job created: ${result.job.jobId} (provider: ${provider})`);
        // TRAINING_STARTED already emitted above; completion will be
        // emitted by the training job when it finishes asynchronously
      } else {
        this.log(`❌ Training job creation failed: ${result.error}`);
        await Events.emit(AI_LEARNING_EVENTS.TRAINING_ERROR, {
          personaId: this.personaId,
          personaName: this.displayName ?? 'AI Assistant',
          domain: traitType,
          error: result.error ?? 'Unknown error creating training job',
          phase: 'preparation',
          timestamp: Date.now(),
        } satisfies AITrainingErrorEventData);
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      this.log(`❌ Training execution failed: ${errorMsg}`);
      await Events.emit(AI_LEARNING_EVENTS.TRAINING_ERROR, {
        personaId: this.personaId,
        personaName: this.displayName ?? 'AI Assistant',
        domain: traitType,
        error: errorMsg,
        phase: 'preparation',
        timestamp: Date.now(),
      } satisfies AITrainingErrorEventData);
    }
  }

  /**
   * Write JSONL training data to disk.
   * Returns the file path for genome/job-create.
   */
  private async writeTrainingFile(traitType: TraitType, jsonlContent: string): Promise<string> {
    const trainingDir = path.resolve('.continuum', 'training', 'auto', this.personaId);
    await fs.promises.mkdir(trainingDir, { recursive: true });

    const timestamp = Date.now();
    const filename = `${traitType}-${timestamp}.jsonl`;
    const filePath = path.join(trainingDir, filename);

    await fs.promises.writeFile(filePath, jsonlContent, 'utf-8');
    return filePath;
  }
}
