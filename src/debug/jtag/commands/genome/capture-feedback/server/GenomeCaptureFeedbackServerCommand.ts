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
import { UserDaemonServer } from '../../../../daemons/user-daemon/server/UserDaemonServer';
import { PersonaUser } from '../../../../system/user/server/PersonaUser';
import type { FeedbackCapture } from '../../../../system/user/server/modules/TrainingDataAccumulator';

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
      // Get UserDaemon singleton
      const userDaemon = UserDaemonServer.getInstance();
      if (!userDaemon) {
        return transformPayload(params, {
          success: false,
          error: 'UserDaemon not initialized'
        });
      }

      // Get PersonaUser instance (feedback target)
      const targetPersonaId = feedbackParams.targetPersonaId;
      if (!targetPersonaId) {
        // No persona specified - just log and return success
        console.log(`ℹ️  No targetPersonaId specified, feedback not captured`);
        return transformPayload(params, {
          success: true,
          feedback: {
            feedbackId: 'no-target-persona',
            targetRole: feedbackParams.targetRole,
            feedbackRole: feedbackParams.feedbackRole,
            domain: feedbackParams.domain,
            attachedToExample: false,
            reciprocalLearning: {
              enabled: false,
              feedbackGiverCanLearn: false
            }
          }
        });
      }

      const baseUser = userDaemon.getPersonaUser(targetPersonaId);
      if (!baseUser || !(baseUser instanceof PersonaUser)) {
        return transformPayload(params, {
          success: false,
          error: `PersonaUser not found: ${targetPersonaId}`
        });
      }

      const personaUser = baseUser as PersonaUser;

      // Find interaction to attach feedback to
      let interactionId = feedbackParams.interactionId;

      if (!interactionId) {
        // Find most recent interaction for targetRole in domain
        const interaction = personaUser.trainingAccumulator.findMostRecentInteraction(
          feedbackParams.domain,
          feedbackParams.targetRole
        );

        if (!interaction) {
          return transformPayload(params, {
            success: false,
            error: `No recent interaction found for ${feedbackParams.targetRole} in domain ${feedbackParams.domain}`
          });
        }

        interactionId = interaction.id;
        console.log(`📌 Found most recent interaction: ${interactionId}`);
      }

      // Map feedbackType to feedback source
      const source: 'human' | 'ai' | 'system' =
        feedbackParams.feedbackPersonaId ? 'ai' : 'human';

      // Attach feedback to interaction
      const feedback: FeedbackCapture = {
        interactionId,
        source,
        rating: feedbackParams.qualityScore,
        comments: feedbackParams.feedbackContent
      };

      await personaUser.trainingAccumulator.captureFeedback(feedback);

      console.log(`📊 Attached ${feedbackParams.feedbackType} feedback to ${interactionId.slice(0, 8)}...`);

      // Check if feedback giver is also a learning PersonaUser
      const feedbackGiverCanLearn = feedbackParams.feedbackPersonaId !== undefined;

      if (feedbackGiverCanLearn) {
        console.log(`🔄 Reciprocal learning enabled: ${feedbackParams.feedbackRole} can learn from feedback outcome`);
      }

      return transformPayload(params, {
        success: true,
        feedback: {
          feedbackId: interactionId,
          targetRole: feedbackParams.targetRole,
          feedbackRole: feedbackParams.feedbackRole,
          domain: feedbackParams.domain,
          attachedToExample: true,
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
