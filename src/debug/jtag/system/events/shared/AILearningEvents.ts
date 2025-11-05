/**
 * AI Learning Events - Real-time feedback for AI training and learning
 *
 * These events provide transparency during PersonaUser training:
 * - User sees "learning" indicators when training is active
 * - Multiple AIs can be learning simultaneously
 * - Shows training progress and completion
 */

/**
 * AI Learning Event Types
 * Emitted by PersonaUser during training/learning activities
 */
export const AI_LEARNING_EVENTS = {
  /** AI started training (fine-tuning LoRA adapter) */
  TRAINING_STARTED: 'ai:learning:training-started',

  /** AI is currently training (progress update) */
  TRAINING_PROGRESS: 'ai:learning:training-progress',

  /** AI completed training successfully */
  TRAINING_COMPLETE: 'ai:learning:training-complete',

  /** AI encountered error during training */
  TRAINING_ERROR: 'ai:learning:training-error',

  /** AI captured interaction for learning buffer */
  INTERACTION_CAPTURED: 'ai:learning:interaction-captured',

  /** AI received feedback on interaction */
  FEEDBACK_RECEIVED: 'ai:learning:feedback-received'
} as const;

export type AILearningEventType = typeof AI_LEARNING_EVENTS[keyof typeof AI_LEARNING_EVENTS];

/**
 * Base event data for all AI learning events
 */
export interface AILearningEventData {
  /** ID of the persona that is learning */
  personaId: string;

  /** Display name of the persona */
  personaName: string;

  /** Timestamp of the event */
  timestamp: number;
}

/**
 * Event data for TRAINING_STARTED
 */
export interface AITrainingStartedEventData extends AILearningEventData {
  /** Domain being trained (e.g., 'conversational', 'code', 'teaching') */
  domain: string;

  /** Training provider (e.g., 'unsloth', 'deepseek', 'openai') */
  provider: string;

  /** Number of training examples */
  exampleCount: number;

  /** Estimated training time in milliseconds */
  estimatedTime: number;
}

/**
 * Event data for TRAINING_PROGRESS
 */
export interface AITrainingProgressEventData extends AILearningEventData {
  /** Domain being trained */
  domain: string;

  /** Progress percentage (0-100) */
  progress: number;

  /** Current epoch number */
  currentEpoch?: number;

  /** Total epochs */
  totalEpochs?: number;

  /** Current loss value */
  currentLoss?: number;
}

/**
 * Event data for TRAINING_COMPLETE
 */
export interface AITrainingCompleteEventData extends AILearningEventData {
  /** Domain that was trained */
  domain: string;

  /** Training provider used */
  provider: string;

  /** Number of examples processed */
  examplesProcessed: number;

  /** Total training time in milliseconds */
  trainingTime: number;

  /** Final loss value */
  finalLoss: number;

  /** Path to trained adapter */
  adapterPath?: string;
}

/**
 * Event data for TRAINING_ERROR
 */
export interface AITrainingErrorEventData extends AILearningEventData {
  /** Domain that failed training */
  domain: string;

  /** Error message */
  error: string;

  /** Phase where error occurred */
  phase: 'preparation' | 'training' | 'saving';
}

/**
 * Event data for INTERACTION_CAPTURED
 */
export interface AIInteractionCapturedEventData extends AILearningEventData {
  /** Domain of the interaction */
  domain: string;

  /** Role ID for the interaction */
  roleId: string;

  /** Current buffer size for this domain */
  bufferSize: number;

  /** Batch threshold for this domain */
  batchThreshold: number;

  /** Whether buffer is ready for training */
  readyForTraining: boolean;
}

/**
 * Event data for FEEDBACK_RECEIVED
 */
export interface AIFeedbackReceivedEventData extends AILearningEventData {
  /** Domain of the feedback */
  domain: string;

  /** Role ID that received feedback */
  roleId: string;

  /** Quality score (0.0-1.0) */
  qualityScore?: number;

  /** Source of feedback */
  feedbackSource: 'human' | 'ai' | 'system';
}
