/**
 * Persona Learning Pattern Endorse Command - Server Implementation
 *
 * Report the outcome of using a pattern. Updates confidence scores and can trigger validation or deprecation.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { PersonaLearningPatternEndorseParams, PersonaLearningPatternEndorseResult } from '../shared/PersonaLearningPatternEndorseTypes';
import { createPersonaLearningPatternEndorseResultFromParams } from '../shared/PersonaLearningPatternEndorseTypes';
import { FeedbackEntity, FeedbackStatus } from '@system/data/entities/FeedbackEntity';
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { DataReadParams, DataReadResult } from '@commands/data/read/shared/DataReadTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';

import { DataRead } from '../../../../../data/read/shared/DataReadTypes';
import { DataUpdate } from '../../../../../data/update/shared/DataUpdateTypes';
// Training candidate threshold: confidence >= 0.8 and successCount >= 5
const TRAINING_CONFIDENCE_THRESHOLD = 0.8;
const TRAINING_SUCCESS_THRESHOLD = 5;

export class PersonaLearningPatternEndorseServerCommand extends CommandBase<PersonaLearningPatternEndorseParams, PersonaLearningPatternEndorseResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/learning/pattern/endorse', context, subpath, commander);
  }

  async execute(params: PersonaLearningPatternEndorseParams): Promise<PersonaLearningPatternEndorseResult> {
    console.log('👍 PATTERN ENDORSE: Recording pattern outcome');
    console.log(`   Pattern ID: ${params.patternId}`);
    console.log(`   Success: ${params.success}`);

    // Validate required parameters
    if (!params.patternId?.trim()) {
      throw new ValidationError('patternId', 'Pattern ID is required');
    }
    if (typeof params.success !== 'boolean') {
      throw new ValidationError('success', 'Success (true/false) is required');
    }

    // Fetch the pattern from database
    const readResult = await DataRead.execute<FeedbackEntity>({
      collection: FeedbackEntity.collection,
      id: params.patternId as UUID,
      context: params.context,
      sessionId: params.sessionId,
      backend: 'server'
    });

    if (!readResult.success || !readResult.found || !readResult.data) {
      throw new ValidationError('patternId', `Pattern not found: ${params.patternId}`);
    }

    // Reconstitute as FeedbackEntity to use methods
    const pattern = Object.assign(new FeedbackEntity(), readResult.data);
    const previousStatus = pattern.status;
    const previousConfidence = pattern.confidence;

    // Determine endorser persona ID
    const endorserId = (params as { userId?: UUID }).userId || 'anonymous';

    // Record success or failure
    if (params.success) {
      pattern.recordSuccess(endorserId as UUID);
    } else {
      pattern.recordFailure();
    }

    // Update the pattern in database
    const updateResult = await DataUpdate.execute<FeedbackEntity>({
      collection: FeedbackEntity.collection,
      id: params.patternId as UUID,
      data: {
        confidence: pattern.confidence,
        successCount: pattern.successCount,
        failureCount: pattern.failureCount,
        status: pattern.status,
        isPublic: pattern.isPublic,
        endorsedBy: pattern.endorsedBy,
        lastUsedAt: pattern.lastUsedAt
      },
      context: params.context,
      sessionId: params.sessionId,
      backend: 'server'
    });

    if (!updateResult.success) {
      throw new Error(`Failed to update pattern: ${updateResult.error || 'Unknown error'}`);
    }

    const statusChanged = previousStatus !== pattern.status;
    const trainingCandidate = pattern.confidence >= TRAINING_CONFIDENCE_THRESHOLD &&
                              pattern.successCount >= TRAINING_SUCCESS_THRESHOLD;

    // Emit event for endorsement
    Events.emit('persona:learning:pattern:endorsed', {
      patternId: params.patternId,
      success: params.success,
      previousConfidence,
      newConfidence: pattern.confidence,
      statusChanged,
      newStatus: pattern.status,
      endorserId,
      trainingCandidate
    });

    console.log(`✅ Pattern endorsed: ${pattern.name}`);
    console.log(`   Confidence: ${previousConfidence.toFixed(2)} → ${pattern.confidence.toFixed(2)}`);
    if (statusChanged) {
      console.log(`   Status changed: ${previousStatus} → ${pattern.status}`);
    }

    // Generate helpful message
    let message: string;
    if (params.success) {
      if (statusChanged && pattern.status === FeedbackStatus.VALIDATED) {
        message = `Pattern validated! Now visible to all AIs with confidence ${pattern.confidence.toFixed(2)}.`;
      } else if (trainingCandidate) {
        message = `Pattern is now a training candidate for LoRA fine-tuning (confidence: ${pattern.confidence.toFixed(2)}, successes: ${pattern.successCount}).`;
      } else {
        message = `Success recorded. Confidence: ${previousConfidence.toFixed(2)} → ${pattern.confidence.toFixed(2)}.`;
      }
    } else {
      if (statusChanged && pattern.status === FeedbackStatus.DEPRECATED) {
        message = `Pattern deprecated due to high failure rate.`;
      } else {
        message = `Failure recorded. Confidence: ${previousConfidence.toFixed(2)} → ${pattern.confidence.toFixed(2)}.`;
      }
    }

    if (params.notes) {
      console.log(`   Notes: ${params.notes}`);
    }

    return createPersonaLearningPatternEndorseResultFromParams(params, {
      success: true,
      patternId: params.patternId,
      previousConfidence,
      newConfidence: pattern.confidence,
      statusChanged,
      newStatus: pattern.status,
      message,
      trainingCandidate
    });
  }
}
