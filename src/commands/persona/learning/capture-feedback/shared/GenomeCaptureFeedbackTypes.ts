/**
 * GenomeCaptureFeedbackTypes - Capture feedback between PersonaUsers
 *
 * Records corrections, critiques, scores from one persona to another.
 * Enables reciprocal learning - both giver and receiver learn from feedback.
 */

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Type of feedback being provided
 */
export type FeedbackType =
  | 'correction'      // "Actually, you should use X instead of Y"
  | 'approval'        // "Good approach, well done"
  | 'critique'        // "This could be improved by..."
  | 'score'           // Numeric evaluation
  | 'suggestion'      // "Consider trying..."
  | 'question';       // "Why did you choose X?"

/**
 * Records a correction, approval, critique, score, or suggestion from one persona to another, enabling reciprocal learning where both the feedback giver and receiver improve from the exchange.
 */
export interface GenomeCaptureFeedbackParams extends CommandParams {
  /**
   * Interaction ID to attach feedback to (optional)
   * If not provided, will find most recent interaction for targetRole in domain
   */
  interactionId?: string;

  /**
   * Who is receiving this feedback
   */
  targetRole: string;

  /**
   * Target PersonaUser ID (if different from context)
   */
  targetPersonaId?: UUID;

  /**
   * Who is giving this feedback
   */
  feedbackRole: string;

  /**
   * Feedback giver PersonaUser ID (if different from context)
   */
  feedbackPersonaId?: UUID;

  /**
   * Learning domain this applies to
   */
  domain: string;

  /**
   * Type of feedback
   */
  feedbackType: FeedbackType;

  /**
   * The actual feedback content
   */
  feedbackContent: string;

  /**
   * Quality score (0-1, optional)
   * Used to filter training examples
   */
  qualityScore?: number;

  /**
   * Was this feedback accepted/helpful?
   * Updated later to train the feedback giver
   */
  wasHelpful?: boolean;

  /**
   * Additional context
   */
  metadata?: {
    messageId?: UUID;
    contextId?: UUID;
    recipeId?: UUID;
    timestamp?: string;
    [key: string]: unknown;
  };
}

/**
 * Result from persona/learning/capture-feedback command
 */
export interface GenomeCaptureFeedbackResult extends CommandResult {
  success: boolean;
  error?: string;

  /**
   * Feedback capture summary
   */
  feedback?: {
    feedbackId: UUID;
    targetRole: string;
    feedbackRole: string;
    domain: string;
    attachedToExample: boolean;  // Was there a recent interaction to attach to?

    /**
     * Reciprocal learning opportunity
     * If feedback giver is also learning, they'll improve based on outcome
     */
    reciprocalLearning: {
      enabled: boolean;
      feedbackGiverCanLearn: boolean;
    };
  };
}

/**
 * GenomeCaptureFeedback — Type-safe command executor
 *
 * Usage:
 *   import { GenomeCaptureFeedback } from '...shared/GenomeCaptureFeedbackTypes';
 *   const result = await GenomeCaptureFeedback.execute({ ... });
 */
export const GenomeCaptureFeedback = {
  execute(params: CommandInput<GenomeCaptureFeedbackParams>): Promise<GenomeCaptureFeedbackResult> {
    return Commands.execute<GenomeCaptureFeedbackParams, GenomeCaptureFeedbackResult>('persona/learning/capture-feedback', params as Partial<GenomeCaptureFeedbackParams>);
  },
  commandName: 'persona/learning/capture-feedback' as const,
} as const;
