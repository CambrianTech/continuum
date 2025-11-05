/**
 * GenomeMultiAgentLearnServerCommand - Multi-agent collaborative learning
 *
 * All participants learn from shared outcome.
 * Enables GAN-like training where multiple PersonaUsers improve together.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  GenomeMultiAgentLearnParams,
  GenomeMultiAgentLearnResult,
  ParticipantLearningResult
} from '../shared/GenomeMultiAgentLearnTypes';

export class GenomeMultiAgentLearnServerCommand extends CommandBase<
  GenomeMultiAgentLearnParams,
  GenomeMultiAgentLearnResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-multi-agent-learn', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeMultiAgentLearnResult> {
    const learnParams = params as GenomeMultiAgentLearnParams;

    console.log('🧬 GENOME MULTI-AGENT: Collaborative learning');
    console.log(`   Domain: ${learnParams.domain}`);
    console.log(`   Outcome: ${learnParams.outcome.success ? 'SUCCESS' : 'FAILURE'}`);
    console.log(`   Participants: ${Object.keys(learnParams.participants).join(', ')}`);

    try {
      const participantResults: Record<string, ParticipantLearningResult> = {};
      let participantsLearned = 0;
      let participantsSkipped = 0;

      // Process each participant's learning
      for (const [roleId, learning] of Object.entries(learnParams.participants)) {
        console.log(`🎓 Processing learning for ${roleId}...`);

        // Get success level from outcome metrics
        const successMetricValue = learnParams.outcome.metrics[learning.successMetric];
        const successLevel = successMetricValue ?? 0.5;
        const successThreshold = learning.successThreshold ?? 0.7;

        // Determine learning type
        let learningType: 'reinforcement' | 'correction' | 'none';
        if (successLevel >= successThreshold) {
          learningType = 'reinforcement';  // They did well, reinforce pattern
        } else {
          learningType = 'correction';     // They need improvement, correct pattern
        }

        // TODO: Access PersonaUser's TrainingDataAccumulator
        // Generate training example based on:
        // - Their contribution
        // - The feedback they received
        // - The outcome
        // - Whether they succeeded or failed
        // This is placeholder implementation

        const trainingExample = {
          input: learning.contribution,
          expectedOutput: learningType === 'reinforcement'
            ? learning.reinforcePattern ?? learning.contribution
            : learning.correctPattern ?? learning.feedback,
          actualOutput: learning.contribution,
          feedback: learning.feedback,
          successLevel,
          outcome: learnParams.outcome
        };

        console.log(`   ${roleId}: ${learningType} (success: ${(successLevel * 100).toFixed(0)}%)`);

        // Store training example for this participant
        const trainingExamples = 1; // Placeholder
        const learned = true; // Placeholder

        participantResults[roleId] = {
          roleId,
          learned,
          trainingExamples,
          successLevel,
          learningType
        };

        if (learned) {
          participantsLearned++;
        } else {
          participantsSkipped++;
        }
      }

      // Calculate collaborative insights
      const successLevels = Object.values(participantResults).map(r => r.successLevel);
      const topPerformer = Object.entries(participantResults)
        .sort((a, b) => b[1].successLevel - a[1].successLevel)[0]?.[0];
      const needsImprovement = Object.entries(participantResults)
        .sort((a, b) => a[1].successLevel - b[1].successLevel)[0]?.[0];
      const teamSuccess = successLevels.reduce((sum, level) => sum + level, 0) / successLevels.length;

      console.log(`✅ GENOME MULTI-AGENT: ${participantsLearned} participants learned`);
      console.log(`   Team success: ${(teamSuccess * 100).toFixed(0)}%`);
      console.log(`   Top performer: ${topPerformer}`);

      return transformPayload(params, {
        success: true,
        learning: {
          domain: learnParams.domain,
          outcomeSuccess: learnParams.outcome.success,
          participantsLearned,
          participantsSkipped,
          participants: participantResults,
          insights: {
            topPerformer,
            needsImprovement,
            teamSuccess
          }
        }
      });

    } catch (error) {
      console.error('❌ GENOME MULTI-AGENT: Error:', error);
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
