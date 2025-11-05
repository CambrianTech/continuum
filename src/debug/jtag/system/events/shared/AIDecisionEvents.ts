/**
 * AI Decision Events - Real-time feedback for AI evaluation and response generation
 *
 * These events provide transparency and feedback during AI decision-making:
 * - User sees "thinking" indicators during 20+ second evaluations
 * - Multiple AIs can be tracked simultaneously per room
 * - Enables coordination (e.g., ensure at least one AI responds)
 */

/**
 * AI Decision Event Types
 * Emitted by PersonaUser during message evaluation and response generation
 */
export const AI_DECISION_EVENTS = {
  /** AI started evaluating whether to respond (gating phase) */
  EVALUATING: 'ai:decision:evaluating',

  /** AI decided to respond (will generate response) */
  DECIDED_RESPOND: 'ai:decision:respond',

  /** AI decided to stay silent (will not respond) */
  DECIDED_SILENT: 'ai:decision:silent',

  /** AI started generating response text */
  GENERATING: 'ai:response:generating',

  /** AI checking if response is redundant */
  CHECKING_REDUNDANCY: 'ai:response:checking-redundancy',

  /** AI successfully posted response */
  POSTED: 'ai:response:posted',

  /** AI encountered error during response */
  ERROR: 'ai:response:error'
} as const;

export type AIDecisionEventType = typeof AI_DECISION_EVENTS[keyof typeof AI_DECISION_EVENTS];

/**
 * Base event data for all AI decision events
 */
export interface AIDecisionEventData {
  /** ID of the persona making the decision */
  personaId: string;

  /** Display name of the persona */
  personaName: string;

  /** Room where decision is being made */
  roomId: string;

  /** Message being evaluated */
  messageId: string;

  /** Whether message is from a human user */
  isHumanMessage: boolean;

  /** Timestamp of the event */
  timestamp: number;
}

/**
 * Event data for EVALUATING
 */
export interface AIEvaluatingEventData extends AIDecisionEventData {
  /** Preview of message content */
  messagePreview: string;

  /** Sender of the message */
  senderName: string;
}

/**
 * Event data for DECIDED_RESPOND
 */
export interface AIDecidedRespondEventData extends AIDecisionEventData {
  /** Confidence level (0.0-1.0) */
  confidence: number;

  /** Reason for deciding to respond */
  reason: string;

  /** Model used for gating decision */
  gatingModel: string;
}

/**
 * Event data for DECIDED_SILENT
 */
export interface AIDecidedSilentEventData extends AIDecisionEventData {
  /** Confidence level (0.0-1.0) */
  confidence: number;

  /** Reason for staying silent */
  reason: string;

  /** Model used for gating decision */
  gatingModel: string;
}

/**
 * Event data for GENERATING
 */
export interface AIGeneratingEventData extends AIDecisionEventData {
  /** Model being used for response generation */
  responseModel: string;
}

/**
 * Event data for CHECKING_REDUNDANCY
 */
export interface AICheckingRedundancyEventData extends AIDecisionEventData {
  /** Length of generated response */
  responseLength: number;
}

/**
 * Event data for POSTED
 */
export interface AIPostedEventData extends AIDecisionEventData {
  /** ID of the posted response message */
  responseMessageId: string;

  /** Whether redundancy check allowed posting */
  passedRedundancyCheck: boolean;
}

/**
 * Event data for ERROR
 */
export interface AIErrorEventData extends AIDecisionEventData {
  /** Error message */
  error: string;

  /** Phase where error occurred */
  phase: 'evaluating' | 'generating' | 'checking' | 'posting';
}
