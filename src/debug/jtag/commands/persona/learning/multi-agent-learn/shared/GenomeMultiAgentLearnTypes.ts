/**
 * GenomeMultiAgentLearnTypes - Multi-agent collaborative learning
 *
 * All participants learn from shared outcome after recipe completion.
 * Enables GAN-like training where multiple PersonaUsers improve together.
 */

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Outcome of collaborative activity
 */
export interface CollaborativeOutcome {
  /**
   * Did the collaboration succeed?
   */
  success: boolean;

  /**
   * Performance metrics
   */
  metrics: {
    [metricName: string]: number;  // testsPassedRatio: 0.9, codeQuality: 0.85, etc.
  };

  /**
   * Additional outcome context
   */
  context?: {
    testResults?: unknown;
    userFeedback?: string;
    productionMetrics?: unknown;
    [key: string]: unknown;
  };
}

/**
 * Learning specification for one participant
 */
export interface ParticipantLearning {
  /**
   * What this participant contributed
   */
  contribution: string;

  /**
   * What feedback they should learn from
   */
  feedback: string;

  /**
   * How to measure their success
   * References a metric from outcome.metrics
   */
  successMetric: string;

  /**
   * Expected success threshold (0-1)
   * If actual < threshold, training emphasizes improvement
   */
  successThreshold?: number;

  /**
   * What they should learn if they succeeded
   */
  reinforcePattern?: string;

  /**
   * What they should learn if they failed
   */
  correctPattern?: string;
}

/**
 * Parameters for persona/learning/multi-agent-learn command
 */
export interface GenomeMultiAgentLearnParams extends CommandParams {
  /**
   * Learning domain
   */
  domain: string;

  /**
   * Outcome of the collaborative activity
   */
  outcome: CollaborativeOutcome;

  /**
   * Learning specifications for each participant
   * Key = roleId
   */
  participants: {
    [roleId: string]: ParticipantLearning;
  };

  /**
   * Should training happen immediately or be queued?
   */
  trainingMode?: 'immediate' | 'queued';

  /**
   * Recipe execution context
   */
  metadata?: {
    recipeId?: UUID;
    contextId?: UUID;
    sessionId?: UUID;
    timestamp?: string;
    [key: string]: unknown;
  };
}

/**
 * Learning result for one participant
 */
export interface ParticipantLearningResult {
  roleId: string;
  personaId?: UUID;
  learned: boolean;              // Did they have training data to learn from?
  trainingExamples: number;      // How many examples were generated
  successLevel: number;          // 0-1, based on their successMetric
  learningType: 'reinforcement' | 'correction' | 'none';
}

/**
 * Result from persona/learning/multi-agent-learn command
 */
export interface GenomeMultiAgentLearnResult extends CommandResult {
  success: boolean;
  error?: string;

  /**
   * Learning summary for each participant
   */
  learning?: {
    domain: string;
    outcomeSuccess: boolean;
    participantsLearned: number;
    participantsSkipped: number;

    /**
     * Per-participant results
     */
    participants: {
      [roleId: string]: ParticipantLearningResult;
    };

    /**
     * Collaborative learning insights
     */
    insights?: {
      /**
       * Which role performed best?
       */
      topPerformer?: string;

      /**
       * Which role needs most improvement?
       */
      needsImprovement?: string;

      /**
       * Overall team success
       */
      teamSuccess: number;  // 0-1
    };
  };
}

/**
 * GenomeMultiAgentLearn — Type-safe command executor
 *
 * Usage:
 *   import { GenomeMultiAgentLearn } from '...shared/GenomeMultiAgentLearnTypes';
 *   const result = await GenomeMultiAgentLearn.execute({ ... });
 */
export const GenomeMultiAgentLearn = {
  execute(params: CommandInput<GenomeMultiAgentLearnParams>): Promise<GenomeMultiAgentLearnResult> {
    return Commands.execute<GenomeMultiAgentLearnParams, GenomeMultiAgentLearnResult>('persona/learning/multi-agent-learn', params as Partial<GenomeMultiAgentLearnParams>);
  },
  commandName: 'persona/learning/multi-agent-learn' as const,
} as const;
