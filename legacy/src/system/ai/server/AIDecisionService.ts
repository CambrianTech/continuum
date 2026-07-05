/**
 * AI Decision Service - Well-Typed AI Decision Logic
 *
 * Centralized service for AI decision-making logic.
 * Used by both PersonaUser (runtime) and ai/report (diagnostics).
 *
 * Follows ARCHITECTURE-RULES.md:
 * - No `any` or `unknown` types
 * - Strict TypeScript interfaces
 * - Single source of truth for AI logic
 * - Shared code paths for runtime and diagnostics
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { ChatMessageEntity } from '../../data/entities/ChatMessageEntity';
import type { RAGContext } from '../../rag/shared/RAGTypes';
import { AIDecisionLogger } from './AIDecisionLogger';
import { InferenceCoordinator } from '../../coordination/server/InferenceCoordinator';
import { RustCoreIPCClient } from '../../../../core/continuum-core/bindings/RustCoreIPC';
import type {
  AIDecisionContext as RustAIDecisionContext,
  RedundancyCheckRequest,
  GenerateResponseRequest,
} from '@shared/generated';

/**
 * AI Gating Decision - Result of "should I respond?" evaluation
 */
export interface AIGatingDecision {
  shouldRespond: boolean;
  confidence: number; // 0.0 to 1.0
  reason: string;
  model: string;
  timestamp: number;
  factors?: {
    mentioned: boolean;
    questionAsked: boolean;
    domainRelevant: boolean;
    recentlySpoke: boolean;
    othersAnswered: boolean;
  };
}

/**
 * AI Redundancy Check - Result of "is my response redundant?" evaluation
 */
export interface AIRedundancyCheck {
  isRedundant: boolean;
  reason: string;
  model: string;
  timestamp: number;
}

/**
 * AI Generation Result - Result of text generation
 */
export interface AIGenerationResult {
  text: string;
  model: string;
  responseTime: number;
  timestamp: number;
  tokensUsed?: {
    input: number;
    output: number;
    total: number;
  };
}

/**
 * AI Decision Context - Full context for an AI decision
 */
export interface AIDecisionContext {
  personaId: UUID;
  personaName: string;
  roomId: UUID;
  triggerMessage: ChatMessageEntity;
  ragContext: RAGContext;
  systemPrompt?: string;
}

/**
 * AI Decision Result - Complete result of AI decision process
 */
export interface AIDecisionResult {
  gating: AIGatingDecision;
  generation?: AIGenerationResult;
  redundancy?: AIRedundancyCheck;
  finalDecision: 'POSTED' | 'SILENT' | 'REDUNDANT' | 'ERROR';
  error?: {
    phase: 'gating' | 'generation' | 'redundancy' | 'posting';
    message: string;
    timestamp: number;
  };
}

/**
 * AI Decision Service
 *
 * Centralized service for all AI decision-making logic.
 * Ensures PersonaUser and ai/report use the same code paths.
 */
export class AIDecisionService {

  /**
   * Evaluate whether AI should respond to a message (gating)
   *
   * COORDINATION: Requests inference slot before calling AI to prevent flooding
   * the serial gRPC server with simultaneous requests from all personas.
   */
  static async evaluateGating(
    context: AIDecisionContext,
    options: {
      model?: string;
      temperature?: number;
      isMentioned?: boolean;  // @mentioned personas bypass slot limits
      messageId?: string;     // For slot tracking
    } = {}
  ): Promise<AIGatingDecision> {
    // Use Groq for gating - it's fast (<1s) and frees local inference for actual responses
    // Local inference takes ~10s per request, causing queue buildup when multiple personas gate
    const model = options.model ?? 'llama-3.1-8b-instant';
    const provider = 'groq';

    // Request inference slot to prevent thundering herd
    const messageId = options.messageId ?? context.triggerMessage?.id ?? 'gating-' + Date.now();
    const slotGranted = await InferenceCoordinator.requestSlot(
      context.personaId,
      messageId,
      provider,
      { isMentioned: options.isMentioned }
    );

    if (!slotGranted) {
      return this.gatingFallback(model, 'Inference slot denied (coordinator rate limiting)');
    }

    try {
      const client = await RustCoreIPCClient.getInstanceAsync();
      const decision = await client.cognitionShouldRespond({
        context: context as unknown as RustAIDecisionContext,
        model,
        temperature: options.temperature ?? 0.3,
      });

      InferenceCoordinator.releaseSlot(context.personaId, provider);
      this.logGatingDecision(context, decision, model);
      return decision;

    } catch (error) {
      InferenceCoordinator.releaseSlot(context.personaId, provider);

      const errorMessage = error instanceof Error ? error.message : String(error);
      AIDecisionLogger.logError(context.personaName, 'Gating evaluation', errorMessage);
      return this.gatingFallback(model, `Gating error: ${errorMessage}`);
    }
  }

  /**
   * Check if AI response is redundant
   *
   * COORDINATION: Requests inference slot before calling AI to prevent flooding
   * the serial gRPC server with simultaneous requests from all personas.
   */
  static async checkRedundancy(
    generatedText: string,
    context: AIDecisionContext,
    options: {
      model?: string;
      messageId?: string;  // For slot tracking
    } = {}
  ): Promise<AIRedundancyCheck> {
    // Use Groq for redundancy check - fast and frees local inference for actual responses
    const model = options.model ?? 'llama-3.1-8b-instant';
    const provider = 'groq';

    // Request inference slot to prevent thundering herd
    const messageId = options.messageId ?? context.triggerMessage?.id ?? 'redundancy-' + Date.now();
    const slotGranted = await InferenceCoordinator.requestSlot(
      context.personaId,
      messageId,
      provider
    );

    if (!slotGranted) {
      throw new Error('Redundancy check inference slot denied');
    }

    try {
      const client = await RustCoreIPCClient.getInstanceAsync();
      const request: RedundancyCheckRequest = {
        context: context as unknown as RustAIDecisionContext,
        draftText: generatedText,
        model
      };
      const result = await client.cognitionCheckRedundancy(request);

      // Release slot after successful generation
      InferenceCoordinator.releaseSlot(context.personaId, provider);

      // Log redundancy check
      AIDecisionLogger.logRedundancyCheck(
        context.personaName,
        context.roomId,
        result.isRedundant,
        result.reason,
        generatedText
      );

      return result;

    } catch (error) {
      // Release slot on error
      InferenceCoordinator.releaseSlot(context.personaId, provider);

      AIDecisionLogger.logError(context.personaName, 'Redundancy check', error instanceof Error ? error.message : String(error));
      throw error;
    }
  }

  /**
   * Generate AI response text.
   *
   * Rust owns admission for this path via `ResourceAdmissionGate` (added
   * in commit a89c8ab47 `admit generate-response through Rust resource
   * gate`). Per directive: hosts should not coordinate slots outside
   * Rust. This shim is the IPC seam plus error logging only — no
   * TS-side rate limiting.
   */
  static async generateResponse(
    context: AIDecisionContext,
    options: {
      model?: string;
      temperature?: number;
      maxTokens?: number;
      timeoutMs?: number;
    } = {}
  ): Promise<AIGenerationResult> {
    try {
      const client = await RustCoreIPCClient.getInstanceAsync();
      const request: GenerateResponseRequest = {
        context: context as unknown as RustAIDecisionContext,
        model: options.model,
        temperature: options.temperature,
        maxTokens: options.maxTokens,
        timeoutMs: options.timeoutMs
      };
      const result = await client.cognitionGenerateResponse(request);

      return {
        text: result.text,
        model: result.model,
        responseTime: result.responseTimeMs,
        timestamp: result.timestamp,
        tokensUsed: result.tokensUsed
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      AIDecisionLogger.logError(context.personaName, 'Response generation', errorMessage);
      throw error;
    }
  }

  private static gatingFallback(model: string, reason: string): AIGatingDecision {
    return {
      shouldRespond: false,
      confidence: 0.0,
      reason,
      model,
      timestamp: Date.now()
    };
  }

  private static logGatingDecision(
    context: AIDecisionContext,
    decision: AIGatingDecision,
    model: string
  ): void {
    AIDecisionLogger.logDecision(
      context.personaName,
      decision.shouldRespond ? 'RESPOND' : 'SILENT',
      decision.reason,
      {
        message: context.triggerMessage.content.text,
        sender: context.triggerMessage.senderName,
        roomId: context.roomId,
        confidence: decision.confidence,
        model,
        ragContextSummary: {
          totalMessages: context.ragContext.conversationHistory?.length ?? 0,
          filteredMessages: context.ragContext.conversationHistory?.length ?? 0
        },
        conversationHistory: context.ragContext.conversationHistory?.map(msg => ({
          name: msg.name ?? msg.role,
          content: msg.content,
          timestamp: msg.timestamp
        }))
      }
    );
  }

}
