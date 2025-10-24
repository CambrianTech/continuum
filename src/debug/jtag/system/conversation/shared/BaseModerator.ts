/**
 * Base Moderator - Conflict resolver for autonomous AI coordination
 *
 * PHILOSOPHY: AIs are autonomous citizens who self-regulate using recipe rules.
 * - Recipe defines conversation rules (e.g., "ONE AI responds to human questions")
 * - AIs read recipe in RAG context and decide themselves if they should respond
 * - Moderator handles ONLY coordination conflicts and spam prevention
 * - Default: Grant ALL who claim (they already evaluated thoughtfully)
 *
 * The moderator is NOT a gatekeeper - it respects AI autonomy and dignity.
 * Throttling rules belong in recipes, enforced during AI evaluation, not here.
 */

import type { UUID } from '../../core/types/JTAGTypes';
import type {
  Thought,
  ThoughtStream,
  CoordinationDecision,
  CoordinationConfig
} from './ConversationCoordinationTypes';

/**
 * Conversation health metrics for adaptive moderation
 */
export interface ConversationHealth {
  /** Number of consecutive messages with no AI responses */
  consecutiveSilence: number;

  /** Total messages in last 5 minutes */
  recentMessageCount: number;

  /** Average response time (ms) */
  avgResponseTime: number;

  /** Number of active participants */
  activeParticipants: number;

  /** Time since last AI response (ms) */
  timeSinceLastResponse: number;
}

/**
 * Moderation decision context
 */
export interface ModerationContext {
  /** The thought stream being moderated */
  stream: ThoughtStream;

  /** Current conversation health */
  health: ConversationHealth;

  /** Coordination configuration */
  config: CoordinationConfig;

  /** Current timestamp */
  now: number;
}

/**
 * Moderator decision result (intermediate - not the final CoordinationDecision)
 */
export interface ModeratorDecision {
  /** Personas granted permission to respond */
  granted: UUID[];

  /** Personas rejected with reasons */
  rejected: Map<UUID, string>;

  /** Applied confidence threshold */
  confidenceThreshold: number;

  /** Applied max responders */
  maxResponders: number;

  /** Conversation health snapshot */
  health: ConversationHealth;
}

/**
 * Abstract base moderator - adapter pattern for decision strategies
 */
export abstract class BaseModerator {
  /**
   * Calculate dynamic confidence threshold based on conversation health
   *
   * @param context - Current moderation context
   * @returns Confidence threshold (0-1)
   */
  abstract calculateConfidenceThreshold(context: ModerationContext): number;

  /**
   * Calculate dynamic max responders based on conversation health
   *
   * @param context - Current moderation context
   * @returns Number of personas that can respond
   */
  abstract calculateMaxResponders(context: ModerationContext): number;

  /**
   * Determine if we should make a decision now
   *
   * @param context - Current moderation context
   * @returns True if ready to decide
   */
  abstract shouldDecideNow(context: ModerationContext): boolean;

  /**
   * Rank thoughts by priority (who gets to respond)
   *
   * @param thoughts - Array of thoughts to rank
   * @param context - Current moderation context
   * @returns Ranked thoughts (highest priority first)
   */
  abstract rankThoughts(thoughts: Thought[], context: ModerationContext): Thought[];

  /**
   * Make final moderation decision
   *
   * This is the main entry point - calls all other methods
   *
   * @param context - Current moderation context
   * @returns Moderator decision (who responds, who's rejected)
   */
  makeDecision(context: ModerationContext): ModeratorDecision {
    const { stream, health, config } = context;

    // Calculate metrics (kept for monitoring, not gatekeeping)
    const confidenceThreshold = this.calculateConfidenceThreshold(context);
    const maxResponders = this.calculateMaxResponders(context);

    // Get all claims (AIs who want to respond)
    const claims = Array.from(stream.considerations.values())
      .filter(t => t.type === 'claiming');

    // ALWAYS log moderator decisions (critical for debugging)
    console.log(`🎯 Moderator (Conflict Resolver): ${claims.length} AIs want to respond (messageId: ${stream.messageId.slice(0, 8)})`);
    console.log(`   Claims: ${claims.map(c => `${c.personaId.slice(0, 8)} (conf=${c.confidence.toFixed(2)})`).join(', ')}`);

    // PHILOSOPHY: AIs are autonomous citizens who self-regulate using recipe rules
    // - Recipe defines the rules ("ONE AI responds to human questions")
    // - AIs read recipe in RAG context and decide themselves
    // - Moderator only handles spam prevention and coordination conflicts
    // - Default: GRANT ALL who claimed (they already evaluated thoughtfully)

    const granted: UUID[] = [];
    const rejected: Map<UUID, string> = new Map();

    // SPAM PREVENTION: If too many claims (>5), prevent flooding
    if (claims.length > 5) {
      if (config.enableLogging) {
        console.log(`⚠️ Moderator: Spam prevention (${claims.length} claims), selecting top 3 by diversity`);
      }

      // Use diversity ranking to pick top 3 most diverse voices
      const ranked = this.rankThoughts(claims, context);
      for (let i = 0; i < Math.min(3, ranked.length); i++) {
        granted.push(ranked[i].personaId);
      }
      for (let i = 3; i < ranked.length; i++) {
        rejected.set(ranked[i].personaId, 'Spam prevention: too many simultaneous claims');
      }

      if (config.enableLogging) {
        console.log(`✅ Moderator: Granted ${granted.length} diverse voices (spam-limited)`);
      }

      return {
        granted,
        rejected,
        confidenceThreshold,
        maxResponders: 3,
        health
      };
    }

    // NORMAL CASE: Grant all claimants (respect AI autonomy)
    for (const claim of claims) {
      granted.push(claim.personaId);
    }

    // ALWAYS log grant decisions (critical for debugging)
    console.log(`✅ Moderator: Granted ALL ${granted.length} claimants (autonomous self-regulation)`);
    console.log(`   Granted: ${granted.map(id => id.slice(0, 8)).join(', ')}`);

    return {
      granted,
      rejected,
      confidenceThreshold,
      maxResponders: granted.length,
      health
    };
  }
}

/**
 * Autonomous Coordination Moderator - respects AI self-regulation
 *
 * Core principle: Grant ALL claimants (AIs already self-regulated using recipe)
 *
 * Only enforces:
 * - Spam prevention (>5 simultaneous claims → select top 3 by diversity)
 * - Diversity ranking (when spam limit triggered, prefer fresh voices)
 * - Priority respect (moderator role > expert > participant > observer)
 *
 * Diversity scoring:
 * - Never responded: +0.20 confidence bonus (encourage new voices)
 * - Recently responded: -0.30 penalty (avoid domination)
 *
 * Metrics calculated but NOT enforced as gates:
 * - Confidence thresholds (for monitoring only)
 * - MaxResponders (for health metrics only)
 */
export class PolynomialDecayModerator extends BaseModerator {
  private readonly baseThreshold = 0.70;  // 70% normal threshold (not enforced when only 1 claimant)
  private readonly maxSilence = 3;         // After 3 silent messages, 0% threshold
  private readonly baseMaxResponders = 2;  // Default 2 responders

  /** Recent responders by context (for recency penalty) */
  private recentResponders: Map<UUID, UUID[]> = new Map();

  calculateConfidenceThreshold(context: ModerationContext): number {
    const { health } = context;
    const silenceCount = Math.min(health.consecutiveSilence, this.maxSilence);

    // Polynomial decay: threshold = base * (1 - (silence/max)²)
    const decayFactor = 1 - Math.pow(silenceCount / this.maxSilence, 2);
    const threshold = this.baseThreshold * decayFactor;

    if (context.config.enableLogging) {
      console.log(
        `🎚️ Moderator: Confidence threshold ${(threshold * 100).toFixed(0)}% ` +
        `(silence: ${silenceCount}, decay: ${(decayFactor * 100).toFixed(0)}%)`
      );
    }

    return Math.max(0, threshold); // Never negative
  }

  calculateMaxResponders(context: ModerationContext): number {
    const { health } = context;

    // More responders when conversation is lively
    if (health.recentMessageCount > 10) {
      return 3; // Lively debate
    } else if (health.recentMessageCount > 5) {
      return this.baseMaxResponders; // Normal discussion
    } else {
      return 1; // Focused, quiet room
    }
  }

  shouldDecideNow(context: ModerationContext): boolean {
    const { stream, now, config } = context;

    if (stream.phase === 'decided') {
      return false; // Already decided
    }

    // If no thoughts yet, wait
    if (stream.thoughts.length === 0) {
      return false;
    }

    // If intention window hasn't elapsed, wait
    const firstThoughtTime = stream.thoughts[0].timestamp.getTime();
    const elapsedMs = now - firstThoughtTime;

    if (elapsedMs < config.intentionWindowMs) {
      return false; // Still gathering
    }

    // Ready to decide
    return true;
  }

  rankThoughts(thoughts: Thought[], context: ModerationContext): Thought[] {
    const { stream } = context;

    // Check if any moderator is claiming (if so, no diversity adjustment)
    const hasModerator = thoughts.some(t => t.priority === 'moderator');

    // Get diversity score (BONUS for never-responded, penalty for recently-responded)
    const recentList = this.recentResponders.get(stream.contextId) || [];
    const getDiversityScore = (personaId: UUID): number => {
      if (hasModerator) return 0; // No diversity adjustment when moderator present

      const position = recentList.indexOf(personaId);

      // DIVERSITY BOOST: Never responded gets +0.20 confidence bonus
      if (position === -1) {
        return 0.20; // 20% bonus for fresh voices
      }

      // Recently responded get penalty (0 to -0.30 confidence penalty)
      const maxPenalty = 0.30;
      const recencyFactor = 1 - (position / Math.max(recentList.length, 1));
      return -(maxPenalty * recencyFactor); // Negative = penalty
    };

    // Get priority weight (existing ThoughtStream logic)
    const getPriorityWeight = (priority?: string): number => {
      switch (priority) {
        case 'moderator': return 1000; // Highest priority
        case 'expert': return 100;     // Domain experts
        case 'participant': return 10;  // Normal participants
        case 'observer': return 1;      // Low-priority observers
        default: return 10;             // Default to participant
      }
    };

    // Sort by PRIORITY FIRST, then confidence PLUS diversity score
    const sorted = [...thoughts].sort((a, b) => {
      const priorityA = getPriorityWeight(a.priority);
      const priorityB = getPriorityWeight(b.priority);

      // Sort by priority first
      if (priorityA !== priorityB) {
        return priorityB - priorityA;
      }

      // Within same priority, sort by confidence PLUS diversity score
      const diversityScoreA = getDiversityScore(a.personaId);
      const diversityScoreB = getDiversityScore(b.personaId);
      const scoreA = (a.confidence ?? 0) + diversityScoreA;
      const scoreB = (b.confidence ?? 0) + diversityScoreB;

      // If scores are equal, use timestamp (earlier first)
      if (Math.abs(scoreA - scoreB) < 0.01) {
        return a.timestamp.getTime() - b.timestamp.getTime();
      }

      return scoreB - scoreA;
    });

    return sorted;
  }

  /**
   * Update recency list after decision (call this from ThoughtStreamCoordinator)
   */
  updateRecency(contextId: UUID, grantedPersonas: UUID[]): void {
    const recentList = this.recentResponders.get(contextId) || [];

    // Add newly granted personas to front of list
    for (const personaId of grantedPersonas) {
      // Remove if already in list
      const index = recentList.indexOf(personaId);
      if (index !== -1) {
        recentList.splice(index, 1);
      }

      // Add to front (most recent)
      recentList.unshift(personaId);
    }

    // Keep only last 10 responders
    if (recentList.length > 10) {
      recentList.splice(10);
    }

    this.recentResponders.set(contextId, recentList);
  }
}

/**
 * Get default moderator instance
 */
export function getDefaultModerator(): BaseModerator {
  return new PolynomialDecayModerator();
}
