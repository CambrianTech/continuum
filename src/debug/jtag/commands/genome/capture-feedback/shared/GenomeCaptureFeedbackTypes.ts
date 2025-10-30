/**
 * GenomeCaptureFeedbackTypes - Capture feedback between PersonaUsers
 *
 * Records corrections, critiques, scores from one persona to another.
 * Enables reciprocal learning - both giver and receiver learn from feedback.
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

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
 * Parameters for genome/capture-feedback command
 */
export interface GenomeCaptureFeedbackParams extends CommandParams {
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
 * Result from genome/capture-feedback command
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
