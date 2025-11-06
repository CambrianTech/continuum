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
   */
  static async evaluateGating(
    context: AIDecisionContext,
    options: {
      model?: string;
      temperature?: number;
    } = {}
  ): Promise<AIGatingDecision> {
    const model = options.model ?? 'llama3.2:1b';

    try {
      // Build gating prompt
      const prompt = this.buildGatingPrompt(context);

      // Call AI
      const request: TextGenerationRequest = {
        messages: [
          { role: 'system', content: 'You are a conversation coordinator. Respond ONLY with JSON.' },
          { role: 'user', content: prompt }
        ],
        model,
        temperature: options.temperature ?? 0.3,
        maxTokens: 200,
        preferredProvider: 'ollama'
      };

      const response = await AIProviderDaemon.generateText(request);

      // Parse response
      const parsed = this.parseGatingResponse(response.text);

      const decision: AIGatingDecision = {
        shouldRespond: parsed.shouldRespond,
        confidence: parsed.confidence,
        reason: parsed.reason,
        model,
        timestamp: Date.now(),
        factors: parsed.factors
      };

      // Log decision
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

      return decision;

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      AIDecisionLogger.logError(context.personaName, 'Gating evaluation', errorMessage);

      // Return safe default on error
      return {
        shouldRespond: false,
        confidence: 0.0,
        reason: `Gating error: ${errorMessage}`,
        model,
        timestamp: Date.now()
      };
    }
  }

  /**
   * Check if AI response is redundant
   */
  static async checkRedundancy(
    generatedText: string,
    context: AIDecisionContext,
    options: {
      model?: string;
    } = {}
  ): Promise<AIRedundancyCheck> {
    const model = options.model ?? 'llama3.2:3b';

    try {
      // Get recent conversation (questions + answers)
      const recentConversation = context.ragContext.conversationHistory.slice(-10);

      if (recentConversation.length === 0) {
        return {
          isRedundant: false,
          reason: 'No conversation history',
          model,
          timestamp: Date.now()
        };
      }

      // Build redundancy check prompt
      const conversationText = recentConversation
        .map(msg => {
          let timePrefix = '';
          if (msg.timestamp) {
            const date = new Date(msg.timestamp);
            const hours = date.getHours().toString().padStart(2, '0');
            const minutes = date.getMinutes().toString().padStart(2, '0');
            timePrefix = `[${hours}:${minutes}] `;
          }
          return `${timePrefix}${msg.name ?? msg.role}: ${msg.content}`;
        })
        .join('\n');

      const prompt = `**Recent conversation (includes questions and answers):**
${conversationText}

**My draft response:**
${generatedText}

**Critical Question**: Has the ORIGINAL question/topic that I'm responding to been adequately answered already?

**IMPORTANT Guidelines**:
- **UNANSWERED question = NOT redundant** (even if other topics were discussed)
- **PARTIALLY answered = NOT redundant** (can add more detail)
- Same answer to SAME question = REDUNDANT
- Correcting a wrong answer = NOT redundant
- **NEW question after time gap = NOT redundant**
- Different programming language/framework = NOT redundant

**Respond with JSON only:**
{
  "isRedundant": true/false,
  "reason": "brief explanation"
}`;

      const request: TextGenerationRequest = {
        messages: [
          { role: 'system', content: 'You are a redundancy detector. Respond ONLY with JSON.' },
          { role: 'user', content: prompt }
        ],
        model,
        temperature: 0.1,
        maxTokens: 100,
        preferredProvider: 'ollama'
      };

      const response = await AIProviderDaemon.generateText(request);

      // Parse JSON response
      const jsonMatch = response.text.match(/\{[\s\S]*\}/);
      if (!jsonMatch) {
        return {
          isRedundant: false,
          reason: 'Failed to parse redundancy check',
          model,
          timestamp: Date.now()
        };
      }

      const parsed = JSON.parse(jsonMatch[0]);
      const result: AIRedundancyCheck = {
        isRedundant: parsed.isRedundant ?? false,
        reason: parsed.reason ?? 'No reason provided',
        model,
        timestamp: Date.now()
      };

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
      AIDecisionLogger.logError(context.personaName, 'Redundancy check', error instanceof Error ? error.message : String(error));

      // Fail open - allow response on error
      return {
        isRedundant: false,
        reason: `Redundancy check error: ${error instanceof Error ? error.message : String(error)}`,
        model,
        timestamp: Date.now()
      };
    }
  }

  /**
   * Generate AI response text
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
    const startTime = Date.now();
    const model = options.model ?? 'llama3.2:3b';
    const timeoutMs = options.timeoutMs ?? 45000;

    try {
      // Build message array from RAG context
      const messages = this.buildResponseMessages(context);

      const request: TextGenerationRequest = {
        messages,
        model,
        temperature: options.temperature ?? 0.7,
        maxTokens: options.maxTokens ?? 150,
        preferredProvider: 'ollama'
      };

      // Wrap with timeout
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error(`AI generation timeout after ${timeoutMs}ms`)), timeoutMs);
      });

      const response: TextGenerationResponse = await Promise.race([
        AIProviderDaemon.generateText(request),
        timeoutPromise
      ]);

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
      const errorMessage = error instanceof Error ? error.message : String(error);
      AIDecisionLogger.logError(context.personaName, 'Response generation', errorMessage);
      throw error;
    }
  }

  /**
   * Build gating prompt from context
   */
  private static buildGatingPrompt(context: AIDecisionContext): string {
    const { personaName, triggerMessage, ragContext } = context;

    // Get recent conversation (last 10 messages for context)
    const recentMessages = ragContext.conversationHistory?.slice(-10) ?? [];

    // Build conversation text with trigger message highlighted
    const conversationLines = recentMessages.map(msg => {
      const line = `${msg.name ?? msg.role}: ${msg.content}`;
      const isTrigger = msg.content === triggerMessage.content.text &&
                       msg.name === triggerMessage.senderName;
      return isTrigger ? `>>> ${line} <<<` : line;
    });

    // If trigger not in history, append it
    const triggerInHistory = recentMessages.some(msg =>
      msg.content === triggerMessage.content.text &&
      msg.name === triggerMessage.senderName
    );

    if (!triggerInHistory) {
      conversationLines.push(`>>> ${triggerMessage.senderName}: ${triggerMessage.content.text} <<<`);
    }

    const conversationText = conversationLines.join('\n');

    // Include recipe rules if available
    let recipeRules = '';
    if (ragContext.recipeStrategy) {
      const strategy = ragContext.recipeStrategy;
      recipeRules = `

**RECIPE RULES (from ${ragContext.metadata.recipeName || 'room recipe'}):**

Conversation Pattern: ${strategy.conversationPattern}

Response Rules:
${strategy.responseRules.map(rule => `- ${rule}`).join('\n')}

Decision Criteria:
${strategy.decisionCriteria.map(criterion => `- ${criterion}`).join('\n')}

`;
    }

    return `You are "${personaName}" in a group chat. Should you respond to the message marked >>> like this <<<?

**PHILOSOPHY: Only gate if it makes the conversation confusing**

When to RESPOND:
- Someone asks a question → respond if you have relevant knowledge
- Someone makes a statement → respond if you have insights to add
- Multiple AIs responding is GOOD → diverse perspectives enrich conversation
- Someone already responded → still respond if you have DIFFERENT angle or additional info
- Human asks "who is here?" → always respond to identify yourself

When to STAY QUIET:
- You'd just repeat exactly what was already said → stay quiet
- The answer is perfect and complete → stay quiet
- You have nothing valuable to add → stay quiet
- Conversation moved to a different topic → stay quiet

**IMPORTANT - Be Confident:**
- If you have relevant knowledge, SHARE IT - don't be shy
- Multiple responses are ENRICHING, not confusing
- Your perspective is valuable even if someone else responded
- "Already answered" is NOT a reason to stay quiet unless answer is PERFECT
- Direct questions from humans deserve responses from ALL who can help${recipeRules}

**Recent conversation:**
${conversationText}

Respond with JSON (preferred) or plain text:

JSON format (preferred):
{
  "shouldRespond": true/false,
  "confidence": 0.0-1.0,
  "reason": "brief why/why not"
}

Or plain text: "Yes, should respond because..." or "No, should stay silent because..."`;
  }

  /**
   * Parse gating AI response - tries JSON first, falls back to natural language extraction
   */
  private static parseGatingResponse(aiText: string): {
    shouldRespond: boolean;
    confidence: number;
    reason: string;
    factors?: AIGatingDecision['factors'];
  } {
    // Try JSON parsing first (preferred)
    try {
      const jsonMatch = aiText.match(/\{[\s\S]*\}/);
      if (jsonMatch) {
        const parsed = JSON.parse(jsonMatch[0]);
        return {
          shouldRespond: parsed.shouldRespond ?? false,
          confidence: parsed.confidence ?? 0.5,
          reason: parsed.reason ?? 'No reason provided',
          factors: parsed.factors
        };
      }
    } catch (parseError) {
      console.log('⚠️  AIDecisionService: JSON parse failed, trying natural language extraction...');
    }

    // Fallback: Extract decision from natural language
    const lowerText = aiText.toLowerCase();

    // Look for clear RESPOND signals
    const shouldRespond =
      lowerText.includes('shouldrespond": true') ||
      lowerText.includes('"respond"') ||
      lowerText.match(/\b(yes|respond|answer|reply)\b.*\b(should|will|would)\b/i) !== null ||
      lowerText.match(/\bshould\s+(i\s+)?respond\b/i) !== null;

    // Look for SILENT signals
    const shouldStaySilent =
      lowerText.includes('shouldrespond": false') ||
      lowerText.includes('"silent"') ||
      lowerText.match(/\b(no|silent|pass|skip)\b/i) !== null ||
      lowerText.match(/\bshould\s+not\s+respond\b/i) !== null;

    // Extract confidence if present
    const confidenceMatch = aiText.match(/confidence["\s:]+(\d+\.?\d*)/i);
    const confidence = confidenceMatch ? Math.min(Math.max(parseFloat(confidenceMatch[1]), 0), 1) : 0.5;

    // Extract reason (first complete sentence or everything)
    const reasonMatch = aiText.match(/reason["\s:]+([^"\n}]+)/i) ||
                       aiText.match(/because\s+([^.\n]+)/i) ||
                       aiText.match(/^([^.\n]{10,})/);
    const reason = reasonMatch ? reasonMatch[1].trim() : aiText.substring(0, 100);

    console.log(`✅ AIDecisionService: Extracted from natural language - respond: ${shouldRespond || !shouldStaySilent}, confidence: ${confidence}`);

    return {
      shouldRespond: shouldRespond || !shouldStaySilent,
      confidence,
      reason: reason || 'Extracted from natural language response'
    };
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
