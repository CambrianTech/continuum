/**
 * PersonaTrainingManager - Handles continuous learning for PersonaUser
 *
 * REFACTORING: Extracted from PersonaUser.ts (lines 1918-2004)
 * Pure function extraction - no behavioral changes
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { Events } from '../../../core/shared/Events';
import type { TrainingDataAccumulator } from './TrainingDataAccumulator';
import type { UserStateEntity } from '../../../data/entities/UserStateEntity';
import {
  AI_LEARNING_EVENTS,
  type AITrainingStartedEventData,
  type AITrainingCompleteEventData
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
  async checkTrainingReadiness(): Promise<void> {
    try {
      const domains = this.trainingAccumulator.getDomains();

      if (domains.length === 0) {
        return; // No accumulated training data
      }

      for (const domain of domains) {
        if (this.trainingAccumulator.shouldMicroTune(domain)) {
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

          this.log(`📊 Consumed ${examples.length} examples for ${domain} training`);

          // TODO Phase 7.5.1: Trigger genome/train command
          // For now, just log that we would train
          this.log(`🚀 Would train ${domain} adapter with ${examples.length} examples`);

          // Clear learning state
          state.learningState.isLearning = false;
          state.learningState.domain = undefined;
          state.learningState.provider = undefined;
          state.learningState.startedAt = undefined;
          state.learningState.exampleCount = undefined;
          state.learningState.estimatedCompletion = undefined;
          await this.saveState(); // Persist state to database

          // Simulate training completion for UI feedback
          const trainingCompleteData: AITrainingCompleteEventData = {
            personaId: this.personaId,
            personaName: this.displayName ?? 'AI Assistant',
            domain,
            provider,
            examplesProcessed: examples.length,
            trainingTime: examples.length * 25,
            finalLoss: 0.5,
            timestamp: Date.now()
          };
          await Events.emit(AI_LEARNING_EVENTS.TRAINING_COMPLETE, trainingCompleteData);

          // Future implementation:
          // await Commands.execute('genome/train', {
          //   personaId: this.personaId,
          //   provider: 'unsloth',
          //   domain,
          //   trainingExamples: examples,
          //   dryRun: false
          // });
        }
      }
    } catch (error) {
      this.log(`❌ Error checking training readiness: ${error}`);
    }
  }
}
