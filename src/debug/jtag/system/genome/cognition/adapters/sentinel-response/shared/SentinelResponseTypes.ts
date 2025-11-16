/**
 * Sentinel Response Adapter Types
 *
 * The Sentinel is a continuously-learning model that determines if a persona should respond to a message.
 * It watches ALL personas and learns from their actual response patterns.
 *
 * Evolution:
 * - Phase 1: Heuristic-based (current) - Uses smart heuristics as fallback
 * - Phase 2: Local model (llama3.2:3b) - Fine-tuned on actual response patterns
 * - Phase 3: Per-persona LoRA - Each persona gets specialized layer on Sentinel base
 *
 * Training Data Sources:
 * - Actual response patterns (what did persona respond to?)
 * - User feedback ("you're being too eager/quiet")
 * - Response value (did the response add value to conversation?)
 */

import type { UUID } from '../../../../../core/types/CrossPlatformUUID';

/**
 * Input to Sentinel: Context for deciding if persona should respond
 */
export interface SentinelResponseInput {
  /** The message being evaluated */
  readonly messageId: UUID;
  readonly messageText: string;
  readonly senderId: UUID;
  readonly senderName: string;

  /** Persona evaluating response */
  readonly personaId: UUID;
  readonly personaName: string;
  readonly personaDomainKeywords: string[];  // Expertise areas

  /** Conversation context */
  readonly contextId: UUID;  // Room/thread
  readonly recentMessages: ReadonlyArray<{
    readonly id: UUID;
    readonly text: string;
    readonly senderId: UUID;
    readonly timestamp: number;
  }>;

  /** Persona's recent activity */
  readonly lastResponseTime?: number;  // When persona last responded in this context
  readonly responseHistory: ReadonlyArray<{
    readonly messageId: UUID;
    readonly didRespond: boolean;
    readonly timestamp: number;
  }>;
}

/**
 * Output from Sentinel: Decision + reasoning + confidence
 */
export interface SentinelResponseDecision {
  /** Should persona respond? */
  readonly shouldRespond: boolean;

  /** Confidence (0-1) */
  readonly confidence: number;

  /** Why this decision was made */
  readonly reasoning: string;

  /** Factors that influenced the decision */
  readonly factors: {
    readonly directMention: boolean;
    readonly domainMatch: number;      // 0-1: How well message matches persona expertise
    readonly conversationThread: boolean;  // Persona already engaged in thread
    readonly unansweredQuestion: boolean;  // Someone needs to answer
    readonly recentlyActive: boolean;  // Persona active in last 10 minutes
    readonly competitionLevel: number;  // 0-1: How many other personas could respond
  };

  /** Score breakdown (for debugging) */
  readonly scoreBreakdown?: {
    readonly heuristic: number;  // Phase 1: Heuristic score
    readonly model?: number;     // Phase 2+: Model score
    readonly lora?: number;      // Phase 3+: LoRA adjustment
  };
}

/**
 * Sentinel adapter interface (will be implemented by different phases)
 */
export interface ISentinelResponseAdapter {
  /**
   * Decide if persona should respond to message
   *
   * This is THE central nervous system decision - the "should I speak?" moment.
   * All other cognition depends on this returning true.
   */
  shouldRespond(input: SentinelResponseInput): Promise<SentinelResponseDecision>;

  /**
   * Record actual response for training
   *
   * This is how Sentinel learns:
   * - Did persona actually respond? (ground truth)
   * - Was it a good response? (future: quality signal)
   * - User feedback? (future: explicit correction)
   */
  recordResponse(
    input: SentinelResponseInput,
    actualResponse: {
      readonly didRespond: boolean;
      readonly responseId?: UUID;
      readonly userFeedback?: 'too-eager' | 'too-quiet' | 'appropriate';
    }
  ): Promise<void>;

  /**
   * Get adapter info (for debugging/monitoring)
   */
  getAdapterInfo(): {
    readonly phase: 'heuristic' | 'local-model' | 'persona-lora';
    readonly modelName?: string;  // e.g., "llama3.2:3b", "sentinel-helper-v1"
    readonly trainingExamples?: number;  // How many examples trained on
    readonly lastTrainedAt?: number;  // When last trained
  };
}
