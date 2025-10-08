/**
 * AI Should Respond Fast - Types
 *
 * Bag-of-words scoring for fast "should respond" detection without LLM calls
 * Deterministic, lightweight, and configurable per-persona
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

/**
 * Scoring weights for different signal types
 */
export interface ResponseScoringWeights {
  /** Direct @mention of persona name (default: 100) */
  readonly directMention: number;

  /** Keywords in persona's domain expertise (default: 50) */
  readonly domainKeyword: number;

  /** Recent conversation participation (default: 30) */
  readonly conversationContext: number;

  /** Question detected (?, how, what, why) (default: 20) */
  readonly isQuestion: number;

  /** Message directed at room (not DM) (default: 10) */
  readonly publicMessage: number;

  /** High activity in room recently (default: 5) */
  readonly roomActivity: number;
}

/**
 * Persona-specific configuration for response detection
 */
export interface PersonaResponseConfig {
  /** Persona identity */
  readonly personaId: UUID;
  readonly personaName: string;

  /** Keywords that trigger high relevance */
  readonly domainKeywords: string[];

  /** Scoring weights */
  readonly weights: ResponseScoringWeights;

  /** Threshold score to respond (default: 50) */
  readonly responseThreshold: number;

  /** Always respond if directly mentioned? (default: true) */
  readonly alwaysRespondToMentions: boolean;

  /** Cooldown period in seconds (default: 60) */
  readonly cooldownSeconds: number;
}

/**
 * Parameters for ai/should-respond-fast command
 */
export interface ShouldRespondFastParams extends CommandParams {
  /** Persona evaluating whether to respond */
  readonly personaId: UUID;

  /** Room/context where message appeared */
  readonly contextId: UUID;

  /** Message ID that triggered evaluation */
  readonly messageId: UUID;

  /** Message content */
  readonly messageText: string;

  /** Sender information */
  readonly senderId: UUID;
  readonly senderName: string;

  /** Optional: Override default config */
  readonly config?: Partial<PersonaResponseConfig>;
}

/**
 * Result from ai/should-respond-fast command
 */
export interface ShouldRespondFastResult extends CommandResult {
  readonly success: boolean;
  readonly error?: string;

  /** Should persona respond? */
  readonly shouldRespond: boolean;

  /** Confidence score (0-100+) */
  readonly score: number;

  /** Breakdown of score components */
  readonly scoreBreakdown: {
    directMention: number;
    domainKeywords: number;
    conversationContext: number;
    isQuestion: number;
    publicMessage: number;
    roomActivity: number;
  };

  /** Detected signals */
  readonly signals: {
    wasMentioned: boolean;
    matchedKeywords: string[];
    isQuestion: boolean;
    recentlyActive: boolean;
  };

  /** Why this decision was made */
  readonly reasoning: string;
}

/**
 * Default scoring weights
 */
export const DEFAULT_SCORING_WEIGHTS: ResponseScoringWeights = {
  directMention: 100,
  domainKeyword: 50,
  conversationContext: 30,
  isQuestion: 20,
  publicMessage: 10,
  roomActivity: 5
};

/**
 * Default response threshold
 */
export const DEFAULT_RESPONSE_THRESHOLD = 50;
