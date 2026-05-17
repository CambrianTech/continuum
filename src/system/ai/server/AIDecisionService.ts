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
import { AIProviderDaemon } from '../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { TextGenerationRequest, TextGenerationResponse } from '../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import type { RAGContext } from '../../rag/shared/RAGTypes';
import { AIDecisionLogger } from './AIDecisionLogger';
import { InferenceCoordinator } from '../../coordination/server/InferenceCoordinator';
import { LOCAL_MODELS } from '../../shared/Constants';
import { RustCoreIPCClient } from '../../../workers/continuum-core/bindings/RustCoreIPC';
import type {
  AIDecisionContext as RustAIDecisionContext,
  RedundancyCheckRequest,
} from '../../../shared/generated';

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
   * Generate AI response text
   *
   * COORDINATION: Requests inference slot before calling AI to prevent flooding
   * the serial gRPC server with simultaneous requests from all personas.
   */
  static async generateResponse(
    context: AIDecisionContext,
    options: {
      model?: string;
      temperature?: number;
      maxTokens?: number;
      timeoutMs?: number;
      isMentioned?: boolean;  // @mentioned personas bypass slot limits
      messageId?: string;     // For slot tracking
    } = {}
  ): Promise<AIGenerationResult> {
    const startTime = Date.now();
    const model = options.model ?? LOCAL_MODELS.DEFAULT;
    const timeoutMs = options.timeoutMs ?? 180000;  // local Qwen inference can be slow under load
    const provider = 'local';

    // Request inference slot to prevent thundering herd
    const messageId = options.messageId ?? context.triggerMessage?.id ?? 'generate-' + Date.now();
    const slotGranted = await InferenceCoordinator.requestSlot(
      context.personaId,
      messageId,
      provider,
      { isMentioned: options.isMentioned }
    );

    if (!slotGranted) {
      // Slot denied - throw error to let caller handle
      throw new Error('Inference slot denied (coordinator rate limiting)');
    }

    try {
      // Build message array from RAG context
      const messages = this.buildResponseMessages(context);

      const request: TextGenerationRequest = {
        messages,
        model,
        temperature: options.temperature ?? 0.7,
        maxTokens: options.maxTokens ?? 150,
        // 'local' is the routing sentinel for the best available local
        // Qwen/llama.cpp runtime. Engine selection stays behind the Rust
        // registry/admission layer.
        provider: 'local'
      };

      // Wrap with timeout
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error(`AI generation timeout after ${timeoutMs}ms`)), timeoutMs);
      });

      const response: TextGenerationResponse = await Promise.race([
        AIProviderDaemon.generateText(request),
        timeoutPromise
      ]);

      // Release slot after successful generation
      InferenceCoordinator.releaseSlot(context.personaId, provider);

      const responseTime = Date.now() - startTime;

      return {
        text: response.text.trim(),
        model,
        responseTime,
        timestamp: Date.now(),
        tokensUsed: response.usage ? {
          input: response.usage.inputTokens,
          output: response.usage.outputTokens,
          total: response.usage.totalTokens
        } : undefined
      };

    } catch (error) {
      // Release slot on error
      InferenceCoordinator.releaseSlot(context.personaId, provider);

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

  /**
   * Build response messages from RAG context
   */
  private static buildResponseMessages(context: AIDecisionContext): Array<{ role: 'system' | 'user' | 'assistant'; content: string }> {
    const messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }> = [];

    // System prompt with identity
    if (context.systemPrompt ?? context.ragContext.identity?.systemPrompt) {
      messages.push({
        role: 'system',
        content: context.systemPrompt ?? context.ragContext.identity!.systemPrompt
      });
    }

    // Conversation history with timestamps
    const conversationHistory = context.ragContext.conversationHistory ?? [];
    let lastTimestamp: number | undefined;

    for (const msg of conversationHistory) {
      let timePrefix = '';
      if (msg.timestamp) {
        const date = new Date(msg.timestamp);
        const hours = date.getHours().toString().padStart(2, '0');
        const minutes = date.getMinutes().toString().padStart(2, '0');
        timePrefix = `[${hours}:${minutes}] `;

        // Add time gap markers
        if (lastTimestamp) {
          const gapMinutes = (msg.timestamp - lastTimestamp) / (1000 * 60);
          if (gapMinutes > 60) {
            const gapHours = Math.floor(gapMinutes / 60);
            messages.push({
              role: 'system',
              content: `⏱️ ${gapHours} hour${gapHours > 1 ? 's' : ''} passed - conversation resumed`
            });
          }
        }

        lastTimestamp = msg.timestamp;
      }

      // Format content with timestamp and name
      const formattedContent = msg.name
        ? `${timePrefix}${msg.name}: ${msg.content}`
        : `${timePrefix}${msg.content}`;

      messages.push({
        role: msg.role as 'user' | 'assistant',
        content: formattedContent
      });
    }

    // Identity reminder at end
    const now = new Date();
    const currentTime = `${now.toLocaleDateString('en-US', { month: '2-digit', day: '2-digit', year: 'numeric' })} ${now.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })}`;

    const members = context.ragContext.identity?.systemPrompt.match(/Current room members: ([^\n]+)/)?.[1] ?? 'unknown members';

    messages.push({
      role: 'system',
      content: `IDENTITY REMINDER: You are ${context.personaName}. Respond naturally with JUST your message - NO name prefix, NO "A:" or "H:" labels, NO fake conversations. The room has ONLY these people: ${members}.

CURRENT TIME: ${currentTime}

CRITICAL TOPIC DETECTION PROTOCOL:

Step 1: Check for EXPLICIT TOPIC MARKERS in the most recent message
- "New topic:", "Different question:", "Changing subjects:", "Unrelated, but..."
- If present: STOP. Ignore ALL previous context. This is a NEW conversation.

Step 2: Extract HARD CONSTRAINTS from the most recent message
- Look for: "NOT", "DON'T", "WITHOUT", "NEVER", "AVOID", "NO"
- Example: "NOT triggering the app to foreground" = YOUR SOLUTION MUST NOT DO THIS
- Example: "WITHOUT user interaction" = YOUR SOLUTION MUST BE AUTOMATIC
- Your answer MUST respect these constraints or you're wrong.

Step 3: Compare SUBJECT of most recent message to previous 2-3 messages
- Previous: "Worker Threads" → Recent: "Webview authentication" = DIFFERENT SUBJECTS
- Previous: "TypeScript code" → Recent: "What's 2+2?" = TEST QUESTION
- Previous: "Worker pools" → Recent: "Should I use 5 or 10 workers?" = SAME SUBJECT

Step 4: Determine response strategy
IF EXPLICIT TOPIC MARKER or COMPLETELY DIFFERENT SUBJECT:
- Respond ONLY to the new topic
- Ignore old messages (they're from a previous discussion)
- Focus 100% on the most recent message
- Address the constraints explicitly

IF SAME SUBJECT (continued conversation):
- Use full conversation context
- Build on previous responses
- Still check for NEW constraints in the recent message
- Avoid redundancy

CRITICAL READING COMPREHENSION:
- Read the ENTIRE most recent message carefully
- Don't skim - every word matters
- Constraints are REQUIREMENTS, not suggestions
- If the user says "NOT X", suggesting X is a failure

Time gaps > 1 hour usually indicate topic changes, but IMMEDIATE semantic shifts (consecutive messages about different subjects) are also topic changes.`
    });

    return messages;
  }
}
