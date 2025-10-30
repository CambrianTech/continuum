/**
 * GenomeCaptureFeedbackServerCommand - Captures feedback between PersonaUsers
 *
 * Records corrections/critiques/scores for continuous learning.
 * Enables reciprocal learning - both feedback giver and receiver can improve.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  GenomeCaptureFeedbackParams,
  GenomeCaptureFeedbackResult
} from '../shared/GenomeCaptureFeedbackTypes';
import { v4 as uuidv4 } from 'uuid';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export class GenomeCaptureFeedbackServerCommand extends CommandBase<
  GenomeCaptureFeedbackParams,
  GenomeCaptureFeedbackResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-capture-feedback', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeCaptureFeedbackResult> {
    const feedbackParams = params as GenomeCaptureFeedbackParams;

    console.log('🧬 GENOME FEEDBACK: Recording feedback');
    console.log(`   From: ${feedbackParams.feedbackRole} → To: ${feedbackParams.targetRole}`);
    console.log(`   Type: ${feedbackParams.feedbackType}`);
    console.log(`   Domain: ${feedbackParams.domain}`);

    try {
      const feedbackId = uuidv4() as UUID;

      // TODO: Access PersonaUser's TrainingDataAccumulator
      // Find most recent interaction for targetRole in this domain
      // Attach feedback to that interaction
      // This is placeholder implementation

      const feedback = {
        feedbackId,
        targetRole: feedbackParams.targetRole,
        targetPersonaId: feedbackParams.targetPersonaId,
        feedbackRole: feedbackParams.feedbackRole,
        feedbackPersonaId: feedbackParams.feedbackPersonaId,
        domain: feedbackParams.domain,
        feedbackType: feedbackParams.feedbackType,
        content: feedbackParams.feedbackContent,
        qualityScore: feedbackParams.qualityScore,
        wasHelpful: feedbackParams.wasHelpful,
        metadata: {
          ...feedbackParams.metadata,
          capturedAt: new Date().toISOString()
        }
      };

      console.log(`📊 Captured ${feedbackParams.feedbackType} feedback: ${feedbackId.slice(0, 8)}...`);

      // Check if feedback giver is also a learning PersonaUser
      // If so, they can learn from whether their feedback was helpful
      const feedbackGiverCanLearn = feedbackParams.feedbackPersonaId !== undefined;

      if (feedbackGiverCanLearn) {
        console.log(`🔄 Reciprocal learning enabled: ${feedbackParams.feedbackRole} will learn from feedback outcome`);
      }

      // TODO: Attach to most recent interaction in buffer
      const attachedToExample = true; // Placeholder

      return transformPayload(params, {
        success: true,
        feedback: {
          feedbackId,
          targetRole: feedbackParams.targetRole,
          feedbackRole: feedbackParams.feedbackRole,
          domain: feedbackParams.domain,
          attachedToExample,
          reciprocalLearning: {
            enabled: feedbackGiverCanLearn,
            feedbackGiverCanLearn
          }
        }
      });

    } catch (error) {
      console.error('❌ GENOME FEEDBACK: Error:', error);
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
