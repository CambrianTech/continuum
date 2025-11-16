/**
 * Sentinel Heuristic Adapter - Phase 1 Implementation
 *
 * Uses smart heuristics (not dumb bag-of-words) to decide if persona should respond.
 * This is a placeholder until we train a real Sentinel-AI model.
 *
 * Philosophy: "Needs A response" vs "Needs MY response"
 * - Direct mention → MUST respond (100% confidence)
 * - Domain expertise + context → Should respond (80% confidence)
 * - Unanswered question + expertise → Maybe respond (50% confidence)
 * - Random message → Don't respond (0% confidence)
 *
 * **IMPORTANT**: This adapter wraps the existing ShouldRespondFastServerCommand logic
 * but provides a cleaner interface for PersonaUser cognition.
 */

import type {
  ISentinelResponseAdapter,
  SentinelResponseInput,
  SentinelResponseDecision
} from '../shared/SentinelResponseTypes';
import type { UUID } from '../../../../../core/types/CrossPlatformUUID';
import { Commands } from '../../../../../core/shared/Commands';
import type {
  ShouldRespondFastParams,
  ShouldRespondFastResult
} from '../../../../../../commands/ai/should-respond-fast/shared/ShouldRespondFastTypes';

/**
 * Phase 1: Heuristic-based Sentinel (wraps ShouldRespondFast command)
 */
export class SentinelHeuristicAdapter implements ISentinelResponseAdapter {
  /**
   * Decide if persona should respond (delegates to ShouldRespondFast command)
   */
  async shouldRespond(input: SentinelResponseInput): Promise<SentinelResponseDecision> {
    // Call the existing ShouldRespondFast command
    const result = await Commands.execute<ShouldRespondFastParams, ShouldRespondFastResult>(
      'ai/should-respond-fast',
      {
        personaId: input.personaId,
        contextId: input.contextId,
        messageId: input.messageId,
        messageText: input.messageText,
        senderId: input.senderId,
        senderName: input.senderName,
        config: {
          personaId: input.personaId,
          personaName: input.personaName,
          domainKeywords: input.personaDomainKeywords,
          weights: {
            directMention: 100,
            domainKeyword: 25,
            conversationContext: 20,
            isQuestion: 10,
            unansweredQuestion: 5,
            roomActivity: 0
          },
          responseThreshold: 35,
          alwaysRespondToMentions: true,
          cooldownSeconds: 60
        }
      }
    );

    if (!result.success) {
      // Fallback: Don't respond on error
      return {
        shouldRespond: false,
        confidence: 0,
        reasoning: `Error evaluating response: ${result.error}`,
        factors: {
          directMention: false,
          domainMatch: 0,
          conversationThread: false,
          unansweredQuestion: false,
          recentlyActive: false,
          competitionLevel: 0
        },
        scoreBreakdown: {
          heuristic: 0
        }
      };
    }

    // Map ShouldRespondFast result to Sentinel decision
    const {
      shouldRespond,
      score,
      scoreBreakdown: breakdown,
      signals
    } = result;

    // Calculate factors
    const factors = {
      directMention: signals.wasMentioned,
      domainMatch: this.calculateDomainMatch(breakdown.domainKeywords, signals.matchedKeywords),
      conversationThread: signals.recentlyActive,
      unansweredQuestion: breakdown.unansweredQuestion > 0,
      recentlyActive: signals.recentlyActive,
      competitionLevel: this.estimateCompetition(breakdown)
    };

    // Calculate confidence based on score strength
    // Direct mention = 100 = 100% confidence
    // Domain + context = 45 = 80% confidence
    // Just question = 10 = 20% confidence
    const confidence = Math.min(1.0, score / 100);

    // Build reasoning string
    const reasoning = this.buildReasoning(shouldRespond, factors, score);

    return {
      shouldRespond,
      confidence,
      reasoning,
      factors,
      scoreBreakdown: {
        heuristic: score
      }
    };
  }

  /**
   * Record actual response (Phase 1: no-op, will be used for training in Phase 2)
   */
  async recordResponse(
    _input: SentinelResponseInput,
    _actualResponse: {
      readonly didRespond: boolean;
      readonly responseId?: UUID;
      readonly userFeedback?: 'too-eager' | 'too-quiet' | 'appropriate';
    }
  ): Promise<void> {
    // Phase 1: Don't record anything (heuristic-only)
    // Phase 2: Will save to training_examples collection
    // Phase 3: Will trigger continuous learning
  }

  /**
   * Get adapter info
   */
  getAdapterInfo() {
    return {
      phase: 'heuristic' as const,
      modelName: 'heuristic-v1',
      trainingExamples: 0,
      lastTrainedAt: undefined
    };
  }

  // ============================================================================
  // PRIVATE HELPERS
  // ============================================================================

  /**
   * Calculate domain match (0-1) based on keyword matches
   */
  private calculateDomainMatch(keywordScore: number, matchedKeywords: string[]): number {
    // keywordScore is weight * matches
    // Each keyword match adds 25 points
    // Normalize to 0-1 range
    if (matchedKeywords.length === 0) return 0;
    if (matchedKeywords.length === 1) return 0.5;
    if (matchedKeywords.length === 2) return 0.75;
    return 1.0;  // 3+ keywords = perfect match
  }

  /**
   * Estimate competition level (how many other personas could respond?)
   * Higher competition = lower urgency for THIS persona to respond
   */
  private estimateCompetition(breakdown: ShouldRespondFastResult['scoreBreakdown']): number {
    // If direct mention: no competition (0)
    if (breakdown.directMention > 0) return 0;

    // If domain match: some competition (0.3)
    if (breakdown.domainKeywords > 0) return 0.3;

    // If just general question: high competition (0.8)
    if (breakdown.isQuestion > 0) return 0.8;

    // Random message: extreme competition (1.0)
    return 1.0;
  }

  /**
   * Build human-readable reasoning string
   */
  private buildReasoning(
    shouldRespond: boolean,
    factors: SentinelResponseDecision['factors'],
    score: number
  ): string {
    if (factors.directMention) {
      return `Direct mention detected - must respond (score: ${score})`;
    }

    if (shouldRespond) {
      const reasons: string[] = [];
      if (factors.domainMatch > 0.5) reasons.push('domain expertise match');
      if (factors.conversationThread) reasons.push('active in conversation');
      if (factors.unansweredQuestion) reasons.push('unanswered question');

      return `Should respond: ${reasons.join(', ')} (score: ${score})`;
    }

    // Not responding
    if (factors.unansweredQuestion) {
      return `Question needs attention but other personas better suited (competition: ${factors.competitionLevel.toFixed(1)})`;
    }

    return `No strong signal to respond (score: ${score})`;
  }
}
