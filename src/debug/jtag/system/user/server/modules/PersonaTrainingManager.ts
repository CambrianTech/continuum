/**
 * PersonaTrainingManager - Handles continuous learning for PersonaUser
 *
 * Monitors training data accumulation and triggers local LoRA fine-tuning
 * via genome/train. After training completes, triggers genome reload so
 * the new adapter is activated for inference immediately.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { Events } from '../../../core/shared/Events';
import { Commands } from '../../../core/shared/Commands';
import type { TrainingDataAccumulator, TrainingExample as AccumulatorExample } from './TrainingDataAccumulator';
import type { UserStateEntity } from '../../../data/entities/UserStateEntity';
import { TrainingDatasetBuilder } from '../../../genome/fine-tuning/server/TrainingDatasetBuilder';
import type { TrainingDataset, TrainingExample } from '../../../genome/fine-tuning/shared/FineTuningTypes';
import type { TraitType } from '../../../genome/entities/GenomeLayerEntity';
import type { GenomeTrainParams, GenomeTrainResult } from '../../../../commands/genome/train/shared/GenomeTrainTypes';
import {
  AI_LEARNING_EVENTS,
  type AITrainingStartedEventData,
  type AITrainingCompleteEventData,
  type AITrainingErrorEventData
} from '../../../events/shared/AILearningEvents';

/**
 * Callback invoked after training completes successfully.
 * LimbicSystem uses this to reload genome from database and activate the new adapter.
 */
export type OnTrainingCompleteCallback = (layerId: UUID, domain: string) => Promise<void>;

/**
 * PersonaTrainingManager - Monitors training readiness and triggers micro-tuning
 *
 * Handles:
 * - Monitoring training data accumulation buffers
 * - Triggering training when threshold reached
 * - Updating learning state in UserStateEntity
 * - Emitting training lifecycle events
 * - Post-training genome activation via callback
 */
export class PersonaTrainingManager {
  private log: (message: string) => void;
  private _onTrainingComplete: OnTrainingCompleteCallback | null = null;

  constructor(
    private readonly personaId: UUID,
    private readonly displayName: string,
    private readonly baseModel: string,
    private readonly trainingAccumulator: TrainingDataAccumulator,
    private readonly getState: () => UserStateEntity,
    private readonly saveState: () => Promise<{ success: boolean; error?: string }>,
    logger?: (message: string) => void
  ) {
    this.log = logger || (() => {});
  }

  /**
   * Set callback for post-training genome activation.
   * LimbicSystem provides this to reload genome from database after training.
   */
  set onTrainingComplete(callback: OnTrainingCompleteCallback) {
    this._onTrainingComplete = callback;
  }

  /**
   * Check training readiness and trigger micro-tuning.
   *
   * Called periodically to check if any domain buffers are ready for training.
   * When threshold reached, automatically triggers genome/train for that domain.
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

          const provider = 'peft'; // Local PEFT training
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

          // Execute local training via genome/train
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
   * Execute local LoRA fine-tuning via genome/train command.
   *
   * Flow: examples → JSONL file on disk → genome/train → PEFT adapter → activation
   *
   * After training, triggers genome reload via onTrainingComplete callback
   * so the new adapter is activated for the next inference request.
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

      // Execute local training via genome/train command
      // QLoRA enabled by default — quantize base model to 4-bit so we can train
      // the largest model that fits on hardware. LoRA weights stay full precision.
      const trainStart = Date.now();
      const result = await Commands.execute<GenomeTrainParams, GenomeTrainResult>('genome/train', {
        personaId: this.personaId,
        personaName: this.displayName ?? 'AI Assistant',
        traitType,
        datasetPath: jsonlPath,
        baseModel: this.baseModel,
        rank: 16,
        epochs: 3,
        learningRate: 0.0001,
        batchSize: 4,
        quantize: true,
        quantizeBits: 4,
      });
      const trainDuration = Date.now() - trainStart;

      if (result.success) {
        this.log(`✅ Training completed: ${result.adapterPath} (${trainDuration}ms, loss=${result.metrics.finalLoss})`);

        // Emit training complete event
        await Events.emit(AI_LEARNING_EVENTS.TRAINING_COMPLETE, {
          personaId: this.personaId,
          personaName: this.displayName ?? 'AI Assistant',
          domain: traitType,
          provider,
          examplesProcessed: result.metrics.examplesProcessed,
          trainingTime: trainDuration,
          finalLoss: result.metrics.finalLoss,
          adapterPath: result.adapterPath,
          layerId: result.layerId,
          timestamp: Date.now(),
        } satisfies AITrainingCompleteEventData);

        // Trigger genome reload — activates the new adapter for inference
        if (result.layerId && this._onTrainingComplete) {
          this.log(`🧬 Triggering genome reload for new adapter (layerId=${result.layerId})`);
          await this._onTrainingComplete(result.layerId, traitType);
        }
      } else {
        this.log(`❌ Training failed: ${result.error}`);
        await Events.emit(AI_LEARNING_EVENTS.TRAINING_ERROR, {
          personaId: this.personaId,
          personaName: this.displayName ?? 'AI Assistant',
          domain: traitType,
          error: result.error ?? 'Unknown training error',
          phase: 'training',
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
        phase: 'training',
        timestamp: Date.now(),
      } satisfies AITrainingErrorEventData);
    }
  }

  /**
   * Write JSONL training data to disk.
   * Returns the file path for genome/train.
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
