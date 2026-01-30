/**
 * AI Should Respond Fast - Types
 *
 * Bag-of-words scoring for fast "should respond" detection without LLM calls
 * Deterministic, lightweight, and configurable per-persona
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

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

  /** Question detected (?, how, what, why) (default: 10) */
  readonly isQuestion: number;

  /** Unanswered question needs attention (default: 5) */
  readonly unansweredQuestion: number;

  /** High activity in room recently (deprecated, default: 0) */
  readonly roomActivity: number;

  /** Message from human (not AI) - humans should get responses (default: 25) */
  readonly humanMessage: number;
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
  readonly senderType?: 'human' | 'persona' | 'agent' | 'system';

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
    unansweredQuestion: number;
    roomActivity: number;
    humanMessage: number;
  };

  /** Detected signals */
  readonly signals: {
    wasMentioned: boolean;
    matchedKeywords: string[];
    isQuestion: boolean;
    recentlyActive: boolean;
    isHumanMessage: boolean;
  };

  /** Why this decision was made */
  readonly reasoning: string;
}

/**
 * Default scoring weights
 *
 * Philosophy: Distinguish "needs A response" from "needs MY response"
 * - Direct mention = MUST respond (100+)
 * - Domain expertise + context = Should respond (40+)
 * - Unanswered question = Signal only, not decisive (5)
 * - Room activity = Removed (was causing everyone to respond)
 */
export const DEFAULT_SCORING_WEIGHTS: ResponseScoringWeights = {
  directMention: 100,       // MUST respond - you were called
  domainKeyword: 25,        // Reduced from 50 - expertise match
  conversationContext: 20,  // Reduced from 30 - thread continuation
  isQuestion: 10,           // Reduced from 20 - question signal
  unansweredQuestion: 5,    // New: someone should answer, but who?
  roomActivity: 0,          // Disabled: was 5, caused noise
  humanMessage: 25          // NEW: Humans deserve responses! AI-to-AI can be lower priority
};

/**
 * Default response threshold
 *
 * With new weights:
 * - Direct mention: 100 → Always respond
 * - Human question: 25 + 10 + 5 = 40 → Respond (humans deserve attention!)
 * - Domain + context: 25 + 20 = 45 → Respond (engaged expert)
 * - Domain + question: 25 + 10 = 35 → Maybe respond (borderline)
 * - AI question: 10 + 5 = 15 → Don't respond (let others handle AI chatter)
 * - Random message: 0 → Never respond
 */
export const DEFAULT_RESPONSE_THRESHOLD = 35;

/**
 * ShouldRespondFast — Type-safe command executor
 *
 * Usage:
 *   import { ShouldRespondFast } from '...shared/ShouldRespondFastTypes';
 *   const result = await ShouldRespondFast.execute({ ... });
 */
export const ShouldRespondFast = {
  execute(params: CommandInput<ShouldRespondFastParams>): Promise<ShouldRespondFastResult> {
    return Commands.execute<ShouldRespondFastParams, ShouldRespondFastResult>('ai/should-respond-fast', params as Partial<ShouldRespondFastParams>);
  },
  commandName: 'ai/should-respond-fast' as const,
} as const;
