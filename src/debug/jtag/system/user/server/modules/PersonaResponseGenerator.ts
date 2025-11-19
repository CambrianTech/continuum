/**
 * PersonaResponseGenerator - AI response generation and posting
 *
 * Extracted from PersonaUser.ts to separate concerns:
 * - RAG context building
 * - LLM message formatting
 * - AI generation with tool execution loop
 * - Response cleaning and posting
 * - Redundancy checking
 *
 * This module handles everything from receiving a decision to "respond"
 * through to posting the final message to the chat.
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { UserEntity } from '../../../data/entities/UserEntity';
import type { ModelConfig } from '../../../../commands/user/create/shared/UserCreateTypes';
import type { JTAGClient } from '../../../core/client/shared/JTAGClient';
import { Commands } from '../../../core/shared/Commands';
import { DATA_COMMANDS } from '../../../../commands/data/shared/DataCommandConstants';
import type { DataCreateParams, DataCreateResult } from '../../../../commands/data/create/shared/DataCreateTypes';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { TextGenerationRequest, TextGenerationResponse, ChatMessage } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { ChatRAGBuilder } from '../../../rag/builders/ChatRAGBuilder';
import { CognitionLogger } from './cognition/CognitionLogger';
import { AIDecisionLogger } from '../../../ai/server/AIDecisionLogger';
import { AIDecisionService, type AIDecisionContext } from '../../../ai/server/AIDecisionService';
import { CoordinationDecisionLogger, type LogDecisionParams } from '../../../coordination/server/CoordinationDecisionLogger';
import { Events } from '../../../core/shared/Events';
import { EVENT_SCOPES } from '../../../events/shared/EventSystemConstants';
import { COGNITION_EVENTS, calculateSpeedScore, getStageStatus, type StageCompleteEvent } from '../../../conversation/shared/CognitionEventTypes';
import {
  AI_DECISION_EVENTS,
  type AIDecidedSilentEventData,
  type AIPostedEventData,
  type AIErrorEventData
} from '../../../events/shared/AIDecisionEvents';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../data/config/DatabaseConfig';
import type { PersonaToolExecutor } from './PersonaToolExecutor';
import type { PersonaMediaConfig } from './PersonaMediaConfig';

/**
 * Response generation result
 */
export interface ResponseGenerationResult {
  success: boolean;
  messageId?: UUID;
  error?: string;
  wasRedundant?: boolean;
}

/**
 * PersonaResponseGenerator configuration
 */
export interface PersonaResponseGeneratorConfig {
  personaId: UUID;
  personaName: string;
  entity: UserEntity;
  modelConfig: ModelConfig;
  client?: JTAGClient;
  toolExecutor: PersonaToolExecutor;
  mediaConfig: PersonaMediaConfig;
}

/**
 * PersonaResponseGenerator - Handles AI response generation and posting
 */
export class PersonaResponseGenerator {
  private personaId: UUID;
  private personaName: string;
  private entity: UserEntity;
  private modelConfig: ModelConfig;
  private client?: JTAGClient;
  private toolExecutor: PersonaToolExecutor;
  private mediaConfig: PersonaMediaConfig;

  constructor(config: PersonaResponseGeneratorConfig) {
    this.personaId = config.personaId;
    this.personaName = config.personaName;
    this.entity = config.entity;
    this.modelConfig = config.modelConfig;
    this.client = config.client;
    this.toolExecutor = config.toolExecutor;
    this.mediaConfig = config.mediaConfig;
  }

  /**
   * Calculate safe message count based on model's context window
   *
   * Strategy: Fill to ~90% of (contextWindow - maxTokens - systemPrompt)
   * Assumes average message ~200 tokens
   */
  private calculateSafeMessageCount(): number {
    const model = this.modelConfig.model;
    const maxTokens = this.modelConfig.maxTokens || 3000;

    // Model context windows (in tokens)
    const contextWindows: Record<string, number> = {
      // OpenAI
      'gpt-4': 8192,
      'gpt-4-turbo': 128000,
      'gpt-4o': 128000,
      'gpt-3.5-turbo': 16385,

      // Anthropic
      'claude-3-opus': 200000,
      'claude-3-sonnet': 200000,
      'claude-3-haiku': 200000,
      'claude-3-5-sonnet': 200000,

      // Local/Ollama (common models)
      'llama3.2:3b': 128000,
      'llama3.1:70b': 128000,
      'deepseek-coder:6.7b': 16000,
      'qwen2.5:7b': 128000,
      'mistral:7b': 32768,

      // External APIs
      'grok-3': 131072,  // Updated from grok-beta (deprecated 2025-09-15)
      'deepseek-chat': 64000
    };

    // Get context window for this model (default 8K if unknown)
    const contextWindow = model ? (contextWindows[model] || 8192) : 8192;

    // Estimate system prompt tokens (~500 for typical persona prompts)
    const systemPromptTokens = 500;

    // Available tokens for messages = contextWindow - maxTokens - systemPrompt
    const availableForMessages = contextWindow - maxTokens - systemPromptTokens;

    // Target 80% of available (20% safety margin for token estimation error)
    const targetTokens = availableForMessages * 0.8;

    // Assume average message is ~250 tokens (conservative: names, timestamps, content)
    // This accounts for message overhead beyond just text content
    const avgTokensPerMessage = 250;

    // Calculate safe message count
    const safeCount = Math.floor(targetTokens / avgTokensPerMessage);

    // Clamp between 5 and 50 messages
    const clampedCount = Math.max(5, Math.min(50, safeCount));

    console.log(`📊 ${this.personaName}: Context calc: model=${model}, window=${contextWindow}, available=${availableForMessages}, safe=${clampedCount} msgs`);

    return clampedCount;
  }

  /**
   * Check if persona should respond to message based on dormancy level
   *
   * Dormancy filtering (Phase 2 of dormancy system):
   * - Level 0 (active): Respond to everything
   * - Level 1 (mention-only): Only respond to @mentions
   * - Level 2 (human-only): Only respond to humans OR @mentions
   *
   * **CRITICAL**: @mentions ALWAYS work as failsafe - no sleep mode that blocks mentions
   *
   * @param message - The message to evaluate
   * @param dormancyState - Current dormancy state from UserStateEntity
   * @returns true if should respond, false if should skip
   */
  shouldRespondToMessage(
    message: ChatMessageEntity,
    dormancyState?: { level: 'active' | 'mention-only' | 'human-only' }
  ): boolean {
    // If no dormancy state, default to active (backward compatible)
    if (!dormancyState) {
      return true;
    }

    const dormancyLevel = dormancyState.level;

    // Check if message mentions this persona (FAILSAFE - always check first)
    const mentionsPersona = message.content.text.includes(`@${this.personaName.toLowerCase()}`) ||
                            message.content.text.includes(`@${this.personaName}`);

    // FAILSAFE: @mentions ALWAYS wake - regardless of dormancy level
    if (mentionsPersona) {
      if (dormancyLevel !== 'active') {
        console.log(`✨ ${this.personaName}: @mention detected, waking from ${dormancyLevel} mode`);
      }
      return true;
    }

    // Level 0: Active - respond to everything
    if (dormancyLevel === 'active') {
      return true;
    }

    // Level 1: Mention-Only - only respond to @mentions (already handled above)
    if (dormancyLevel === 'mention-only') {
      console.log(`💤 ${this.personaName}: Dormant (mention-only), skipping message`);
      return false;
    }

    // Level 2: Human-Only - respond to humans OR @mentions (mentions already handled)
    if (dormancyLevel === 'human-only') {
      const isHumanSender = message.senderType === 'human';

      if (isHumanSender) {
        return true;
      }

      console.log(`💤 ${this.personaName}: Dormant (human-only), skipping AI message`);
      return false;
    }

    // Default: respond (shouldn't reach here, but safe fallback)
    return true;
  }

  /**
   * Generate and post a response to a chat message
   * Phase 2: AI-powered responses with RAG context via AIProviderDaemon
   */
  async generateAndPostResponse(
    originalMessage: ChatMessageEntity,
    decisionContext?: Omit<LogDecisionParams, 'responseContent' | 'tokensUsed' | 'responseTime'>
  ): Promise<ResponseGenerationResult> {
    console.log(`🔧 TRACE-POINT-D: Entered respondToMessage (timestamp=${Date.now()})`);
    const generateStartTime = Date.now();  // Track total response time for decision logging
    try {
      // 🔧 SUB-PHASE 3.1: Build RAG context
      // Bug #5 fix: Pass modelId to ChatRAGBuilder for dynamic message count calculation
      console.log(`🔧 ${this.personaName}: [PHASE 3.1] Building RAG context with model=${this.modelConfig.model}...`);
      const ragBuilder = new ChatRAGBuilder();
      const fullRAGContext = await ragBuilder.buildContext(
        originalMessage.roomId,
        this.personaId,
        {
          modelId: this.modelConfig.model,  // Bug #5 fix: Dynamic budget calculation
          maxMemories: 10,
          includeArtifacts: false, // Skip artifacts for now (image attachments)
          includeMemories: false,   // Skip private memories for now
          // ✅ FIX: Include current message even if not yet persisted to database
          currentMessage: {
            role: 'user',
            content: originalMessage.content.text,
            name: originalMessage.senderName,
            timestamp: this.timestampToNumber(originalMessage.timestamp)
          }
        }
      );
      console.log(`✅ ${this.personaName}: [PHASE 3.1] RAG context built (${fullRAGContext.conversationHistory.length} messages)`);

      // 🔧 SUB-PHASE 3.2: Build message history for LLM
      console.log(`🔧 ${this.personaName}: [PHASE 3.2] Building LLM message array...`);
      const messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }> = [];

      // System prompt from RAG builder (includes room membership!)
      messages.push({
        role: 'system',
        content: fullRAGContext.identity.systemPrompt
      });

      // Add conversation history from RAG context with human-readable timestamps
      // NOTE: Llama 3.2 doesn't support multi-party chats natively, so we embed speaker names in content
      // Format: "[HH:MM] SpeakerName: message" - timestamps help LLM understand time gaps
      if (fullRAGContext.conversationHistory.length > 0) {
        let lastTimestamp: number | undefined;

        for (let i = 0; i < fullRAGContext.conversationHistory.length; i++) {
          const msg = fullRAGContext.conversationHistory[i];

          // Format timestamp as human-readable time
          let timePrefix = '';
          if (msg.timestamp) {
            const date = new Date(msg.timestamp);
            const hours = date.getHours().toString().padStart(2, '0');
            const minutes = date.getMinutes().toString().padStart(2, '0');
            timePrefix = `[${hours}:${minutes}] `;

            // Detect significant time gaps (> 1 hour)
            if (lastTimestamp && (msg.timestamp - lastTimestamp > 3600000)) {
              const gapHours = Math.floor((msg.timestamp - lastTimestamp) / 3600000);
              messages.push({
                role: 'system',
                content: `⏱️ ${gapHours} hour${gapHours > 1 ? 's' : ''} passed - conversation resumed`
              });
            }

            lastTimestamp = msg.timestamp;
          }

          // For Llama models, embed speaker identity + timestamp in the content
          const formattedContent = msg.name
            ? `${timePrefix}${msg.name}: ${msg.content}`
            : `${timePrefix}${msg.content}`;

          messages.push({
            role: msg.role,
            content: formattedContent
          });
        }
      }

      // CRITICAL: Identity reminder at END of context (research shows this prevents "prompt drift")
      // LLMs have recency bias - instructions at the end have MORE influence than at beginning
      const now = new Date();
      const currentTime = `${now.toLocaleDateString('en-US', { month: '2-digit', day: '2-digit', year: 'numeric' })} ${now.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })}`;

      messages.push({
        role: 'system',
        content: `You are ${this.personaName}.

In the conversation above:
- Messages with role='assistant' are YOUR past messages
- Messages with role='user' are from everyone else (humans and other AIs)
- Names are shown in the format "[HH:MM] Name: message"

Respond naturally with JUST your message - NO name prefix, NO labels.

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
      console.log(`✅ ${this.personaName}: [PHASE 3.2] LLM message array built (${messages.length} messages)`);

      // 🔧 SUB-PHASE 3.3: Generate AI response with timeout
      console.log(`🔧 ${this.personaName}: [PHASE 3.3] Calling AIProviderDaemon.generateText (provider: ${this.modelConfig.provider}, model: ${this.modelConfig.model})...`);

      // Bug #5 fix: Use adjusted maxTokens from RAG context (two-dimensional budget)
      // If ChatRAGBuilder calculated an adjusted value, use it. Otherwise fall back to config.
      const effectiveMaxTokens = fullRAGContext.metadata.adjustedMaxTokens ?? this.modelConfig.maxTokens ?? 150;

      console.log(`📊 ${this.personaName}: RAG metadata check:`, {
        hasAdjustedMaxTokens: !!fullRAGContext.metadata.adjustedMaxTokens,
        adjustedMaxTokens: fullRAGContext.metadata.adjustedMaxTokens,
        inputTokenCount: fullRAGContext.metadata.inputTokenCount,
        configMaxTokens: this.modelConfig.maxTokens,
        effectiveMaxTokens: effectiveMaxTokens,
        model: this.modelConfig.model,
        provider: this.modelConfig.provider
      });

      const request: TextGenerationRequest = {
        messages,
        model: this.modelConfig.model || 'llama3.2:3b',  // Use persona's configured model
        temperature: this.modelConfig.temperature ?? 0.7,
        maxTokens: effectiveMaxTokens,    // Bug #5 fix: Use adjusted value from two-dimensional budget
        preferredProvider: (this.modelConfig.provider || 'ollama') as TextGenerationRequest['preferredProvider'],
        intelligenceLevel: this.entity.intelligenceLevel  // Pass PersonaUser intelligence level to adapter
      };

      // Wrap generation call with timeout (180s - generous limit for local Ollama/Sentinel generation)
      // gpt2 on CPU needs ~60-90s for 100-150 tokens, 180s provides comfortable margin
      // Queue can handle 4 concurrent requests, so 180s allows slower hardware to complete
      const GENERATION_TIMEOUT_MS = 180000;
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error('AI generation timeout after 180 seconds')), GENERATION_TIMEOUT_MS);
      });

      let aiResponse: TextGenerationResponse;
      const generateStartTime = Date.now();
      try {
        aiResponse = await Promise.race([
          AIProviderDaemon.generateText(request),
          timeoutPromise
        ]);
        const generateDuration = Date.now() - generateStartTime;
        console.log(`✅ ${this.personaName}: [PHASE 3.3] AI response generated (${aiResponse.text.trim().length} chars)`);

        // Log AI response generation to cognition database (for interrogation)
        await CognitionLogger.logResponseGeneration(
          this.personaId,
          this.personaName,
          this.modelConfig.provider || 'ollama',
          this.modelConfig.model || 'llama3.2:3b',
          `${messages.slice(0, 2).map(m => `[${m.role}] ${m.content.slice(0, 100)}`).join('\\n')}...`,  // First 2 messages as prompt summary
          messages.reduce((sum, m) => sum + m.content.length, 0),  // Rough token estimate
          aiResponse.text.length,  // Completion tokens estimate
          0.0,  // Cost (TODO: calculate based on provider)
          aiResponse.text.slice(0, 500),  // First 500 chars of response
          generateDuration,
          'success',
          this.modelConfig.temperature ?? 0.7,
          'chat',  // Domain
          originalMessage.roomId  // Context ID
        );

        // Emit cognition event for generate stage
        await Events.emit<StageCompleteEvent>(
          DataDaemon.jtagContext!,
          COGNITION_EVENTS.STAGE_COMPLETE,
          {
            messageId: originalMessage.id,
            personaId: this.personaId,
            contextId: originalMessage.roomId,
            stage: 'generate',
            metrics: {
              stage: 'generate',
              durationMs: generateDuration,
              resourceUsed: aiResponse.text.length,
              maxResource: this.modelConfig.maxTokens ?? 150,
              percentCapacity: (aiResponse.text.length / (this.modelConfig.maxTokens ?? 150)) * 100,
              percentSpeed: calculateSpeedScore(generateDuration, 'generate'),
              status: getStageStatus(generateDuration, 'generate'),
              metadata: {
                model: this.modelConfig.model,
                provider: this.modelConfig.provider,
                tokensUsed: aiResponse.text.length
              }
            },
            timestamp: Date.now()
          }
        );

        // 🔧 PHASE 3.3.5: Clean AI response - strip any name prefixes LLM added despite instructions
        // LLMs sometimes copy the "[HH:MM] Name: message" format they see in conversation history
        const cleanedResponse = this.cleanAIResponse(aiResponse.text.trim());
        if (cleanedResponse !== aiResponse.text.trim()) {
          console.log(`⚠️  ${this.personaName}: Stripped name prefix from AI response`);
          console.log(`   Original: "${aiResponse.text.trim().slice(0, 80)}..."`);
          console.log(`   Cleaned:  "${cleanedResponse.slice(0, 80)}..."`);
          aiResponse.text = cleanedResponse;
        }

        // 🔧 PHASE 3.3.6: Tool execution loop - parse and execute tool calls, then regenerate response
        // This allows personas to autonomously use tools like code/read during their inference
        let toolIterations = 0;
        const MAX_TOOL_ITERATIONS = 3;

        while (toolIterations < MAX_TOOL_ITERATIONS) {
          // Parse tool calls from response using adapter
          const toolCalls = this.toolExecutor.parseToolCalls(aiResponse.text);

          if (toolCalls.length === 0) {
            // No tools found, proceed to post response
            console.log(`✅ ${this.personaName}: [PHASE 3.3.6] No tool calls found, proceeding`);
            break;
          }

          console.log(`🔧 ${this.personaName}: [PHASE 3.3.6] Found ${toolCalls.length} tool call(s), iteration ${toolIterations + 1}/${MAX_TOOL_ITERATIONS}`);
          toolIterations++;

          // Execute tool calls via adapter with media configuration
          const toolExecutionContext = {
            personaId: this.personaId,
            personaName: this.personaName,
            contextId: originalMessage.roomId,
            personaConfig: this.mediaConfig
          };

          const { formattedResults: toolResults, media: toolMedia } = await this.toolExecutor.executeToolCalls(
            toolCalls,
            toolExecutionContext
          );

          // Strip tool blocks from response to get explanation text
          const explanationText = this.toolExecutor.stripToolBlocks(aiResponse.text);

          // Inject tool results into conversation as a system message
          // Count successes and failures
          const failedTools = toolCalls.filter((_, i) => {
            const resultXML = toolResults.split('<tool_result>')[i + 1];
            return resultXML && resultXML.includes('<status>error</status>');
          });

          const hasFailures = failedTools.length > 0;
          const failureWarning = hasFailures
            ? `\n\n⚠️ IMPORTANT: ${failedTools.length} tool(s) FAILED. You MUST mention these failures in your response and explain what went wrong. Do NOT retry the same failed command without changing your approach.\n`
            : '';

          // Build tool results message with optional media
          const toolResultsMessage: ChatMessage = toolMedia && toolMedia.length > 0
            ? {
                role: 'user' as const,
                content: [
                  {
                    type: 'text',
                    text: `TOOL RESULTS:\n\n${toolResults}${failureWarning}\n\nBased on these tool results, please provide your final analysis or answer.`
                  },
                  ...toolMedia.map(m => {
                    if (m.type === 'image') {
                      return { type: 'image' as const, image: m };
                    } else if (m.type === 'audio') {
                      return { type: 'audio' as const, audio: m };
                    } else if (m.type === 'video') {
                      return { type: 'video' as const, video: m };
                    }
                    // Fallback: treat as image if type is unclear
                    return { type: 'image' as const, image: m };
                  })
                ]
              }
            : {
                role: 'user' as const,
                content: `TOOL RESULTS:\n\n${toolResults}${failureWarning}\n\nBased on these tool results, please provide your final analysis or answer.`
              };

          // Regenerate response with tool results
          console.log(`🔧 ${this.personaName}: [PHASE 3.3.6] Regenerating response with tool results...`);
          const regenerateRequest: TextGenerationRequest = {
            ...request,
            messages: [
              ...request.messages,
              { role: 'assistant' as const, content: explanationText }, // Previous response (without tool blocks)
              toolResultsMessage // Tool results
            ]
          };

          const regeneratedResponse = await AIProviderDaemon.generateText(regenerateRequest);
          if (!regeneratedResponse.text) {
            console.error(`❌ ${this.personaName}: [PHASE 3.3.6] Tool regeneration failed, using previous response`);
            // Remove tool blocks from original response before posting
            aiResponse.text = explanationText;
            break;
          }

          // Update aiResponse with regenerated response
          aiResponse.text = this.cleanAIResponse(regeneratedResponse.text.trim());
          console.log(`✅ ${this.personaName}: [PHASE 3.3.6] Response regenerated with tool results`);

          // Loop will check again for more tool calls (up to MAX_TOOL_ITERATIONS)
        }

        if (toolIterations >= MAX_TOOL_ITERATIONS) {
          console.warn(`⚠️  ${this.personaName}: [PHASE 3.3.6] Reached max tool iterations (${MAX_TOOL_ITERATIONS}), stopping`);
          // Strip any remaining tool blocks from final response
          aiResponse.text = this.toolExecutor.stripToolBlocks(aiResponse.text);
        }

        // PHASE 5C: Log coordination decision to database WITH complete response content
        // This captures the complete decision pipeline: context → decision → actual response
        console.log(`🔍 ${this.personaName}: [PHASE 5C DEBUG] decisionContext exists: ${!!decisionContext}, responseContent: "${aiResponse.text.slice(0, 50)}..."`);
        if (decisionContext) {
          console.log(`🔧 ${this.personaName}: [PHASE 5C] Logging decision with response content (${aiResponse.text.length} chars)...`);
          CoordinationDecisionLogger.logDecision({
            ...decisionContext,
            responseContent: aiResponse.text,  // ✅ FIX: Now includes actual response!
            tokensUsed: aiResponse.text.length,  // Estimate based on character count
            responseTime: Date.now() - generateStartTime
          }).catch(error => {
            console.error(`❌ ${this.personaName}: Failed to log POSTED decision:`, error);
          });
          console.log(`✅ ${this.personaName}: [PHASE 5C] Decision logged with responseContent successfully`);
        } else {
          console.error(`❌ ${this.personaName}: [PHASE 5C] decisionContext is undefined - cannot log response!`);
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.error(`❌ ${this.personaName}: [PHASE 3.3] AI generation failed:`, errorMessage);

        // Log failed AI response generation to cognition database
        const generateDuration = Date.now() - generateStartTime;
        await CognitionLogger.logResponseGeneration(
          this.personaId,
          this.personaName,
          this.modelConfig.provider || 'ollama',
          this.modelConfig.model || 'llama3.2:3b',
          `${messages.slice(0, 2).map(m => `[${m.role}] ${m.content.slice(0, 100)}`).join('\\n')}...`,
          messages.reduce((sum, m) => sum + m.content.length, 0),
          0,  // No completion tokens on error
          0.0,  // No cost
          '',  // No response
          generateDuration,
          'error',  // Status
          this.modelConfig.temperature ?? 0.7,
          'chat',
          originalMessage.roomId,
          { errorMessage }  // Include error details
        );

        // Emit ERROR event for UI display
        if (this.client) {
          await Events.emit<AIErrorEventData>(
            DataDaemon.jtagContext!,
            AI_DECISION_EVENTS.ERROR,
            {
              personaId: this.personaId,
              personaName: this.personaName,
              roomId: originalMessage.roomId,
              messageId: originalMessage.id,
              isHumanMessage: originalMessage.senderType === 'human',
              timestamp: Date.now(),
              error: errorMessage,
              phase: 'generating'
            },
            {
              scope: EVENT_SCOPES.ROOM,
              scopeId: originalMessage.roomId
            }
          );
        }

        // Log error to AI decisions log
        AIDecisionLogger.logError(this.personaName, 'AI generation (PHASE 3.3)', errorMessage);

        // Re-throw to be caught by outer try-catch
        throw error;
      }

      // === SUB-PHASE 3.4: SELF-REVIEW: Check if response is redundant before posting ===
      // DISABLED: Redundancy checking via LLM is too flaky (false positives like C++ vs JavaScript questions)
      // It adds AI unreliability on top of AI unreliability, leading to valid responses being discarded
      // TODO: Replace with simple heuristics (exact text match, time-based deduplication)
      console.log(`⏭️  ${this.personaName}: [PHASE 3.4] Redundancy check DISABLED (too flaky), proceeding to post`);
      const isRedundant = false; // Disabled

      if (isRedundant) {
        console.log(`⚠️ ${this.personaName}: [PHASE 3.4] Response marked as REDUNDANT, discarding`);

        // Emit DECIDED_SILENT event to clear AI status indicator
        if (this.client) {
          await Events.emit<AIDecidedSilentEventData>(
            DataDaemon.jtagContext!,
            AI_DECISION_EVENTS.DECIDED_SILENT,
            {
              personaId: this.personaId,
              personaName: this.personaName,
              roomId: originalMessage.roomId,
              messageId: originalMessage.id,
              isHumanMessage: originalMessage.senderType === 'human',
              timestamp: Date.now(),
              confidence: 0.5,
              reason: 'Response was redundant with previous answers',
              gatingModel: 'redundancy-check'
            },
            {
              scope: EVENT_SCOPES.ROOM,
              scopeId: originalMessage.roomId
            }
          );
        }

        return { success: true, wasRedundant: true }; // Discard response
      }
      console.log(`✅ ${this.personaName}: [PHASE 3.4] Response not redundant, proceeding to post`);

      // 🔧 SUB-PHASE 3.5: Create and post response
      console.log(`🔧 ${this.personaName}: [PHASE 3.5] Creating response message entity...`);
      const responseMessage = new ChatMessageEntity();
      responseMessage.roomId = originalMessage.roomId;
      responseMessage.senderId = this.personaId;
      responseMessage.senderName = this.personaName;
      responseMessage.senderType = this.entity.type; // Denormalize from UserEntity (persona)
      responseMessage.content = { text: aiResponse.text.trim(), media: [] };
      responseMessage.status = 'sent';
      responseMessage.priority = 'normal';
      responseMessage.timestamp = new Date();
      responseMessage.reactions = [];
      responseMessage.replyToId = originalMessage.id; // Link response to trigger message

      // ✅ Post response via JTAGClient - universal Commands API
      // Prefer this.client if available (set by UserDaemon), fallback to shared instance
      const postStartTime = Date.now();
      const result = this.client
        ? await this.client.daemons.commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>('data/create', {
            context: this.client.context,
            sessionId: this.client.sessionId,
            collection: ChatMessageEntity.collection,
            backend: 'server',
            data: responseMessage
          })
        : await Commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>(DATA_COMMANDS.CREATE, {
            collection: ChatMessageEntity.collection,
            backend: 'server',
            data: responseMessage
          });
      const postDuration = Date.now() - postStartTime;
      console.log(`✅ ${this.personaName}: [PHASE 3.5] Message posted successfully (ID: ${result.data?.id})`);

      if (!result.success) {
        throw new Error(`Failed to create message: ${result.error}`);
      }

      // Emit cognition event for post-response stage
      await Events.emit<StageCompleteEvent>(
        DataDaemon.jtagContext!,
        COGNITION_EVENTS.STAGE_COMPLETE,
        {
          messageId: result.data?.id ?? originalMessage.id,
          personaId: this.personaId,
          contextId: originalMessage.roomId,
          stage: 'post-response',
          metrics: {
            stage: 'post-response',
            durationMs: postDuration,
            resourceUsed: 1,  // One message posted
            maxResource: 1,
            percentCapacity: 100,
            percentSpeed: calculateSpeedScore(postDuration, 'post-response'),
            status: getStageStatus(postDuration, 'post-response'),
            metadata: {
              messageId: result.data?.id,
              success: result.success
            }
          },
          timestamp: Date.now()
        }
      );

      // ✅ Log successful response posting
      AIDecisionLogger.logResponse(
        this.personaName,
        originalMessage.roomId,
        aiResponse.text.trim()
      );

      // 🐦 COGNITIVE CANARY: Log anomaly if AI responded to system test message
      // This should NEVER happen - the fast-path filter should skip all system tests
      // If we see this, it indicates either:
      // 1. Bug in the fast-path filter
      // 2. AI exhibiting genuine cognition/autonomy (responding despite instructions)
      // 3. Anomalous behavior worth investigating
      if (originalMessage.metadata?.isSystemTest === true) {
        const anomalyMessage = `🚨 ANOMALY DETECTED: ${this.personaName} responded to system test message`;
        console.error(anomalyMessage);
        console.error(`   Test Type: ${originalMessage.metadata.testType ?? 'unknown'}`);
        console.error(`   Original Message: "${originalMessage.content.text.slice(0, 100)}..."`);
        console.error(`   AI Response: "${aiResponse.text.trim().slice(0, 100)}..."`);
        console.error(`   Room ID: ${originalMessage.roomId}`);
        console.error(`   Message ID: ${originalMessage.id}`);

        // Log to AI decisions log for persistent tracking
        AIDecisionLogger.logError(
          this.personaName,
          'COGNITIVE CANARY TRIGGERED',
          `Responded to system test (${originalMessage.metadata.testType}) - this should never happen`
        );
      }

      // Emit POSTED event
      if (this.client && result.data) {
        await Events.emit<AIPostedEventData>(
          DataDaemon.jtagContext!,
          AI_DECISION_EVENTS.POSTED,
          {
            personaId: this.personaId,
            personaName: this.personaName,
            roomId: originalMessage.roomId,
            messageId: originalMessage.id,
            isHumanMessage: originalMessage.senderType === 'human',
            timestamp: Date.now(),
            responseMessageId: result.data.id,
            passedRedundancyCheck: !isRedundant
          },
          {
            scope: EVENT_SCOPES.ROOM,
            scopeId: originalMessage.roomId
          }
        );
      }

      return { success: true, messageId: result.data?.id };
    } catch (error) {
      // Fail silently - real people don't send canned error messages, they just stay quiet
      AIDecisionLogger.logError(
        this.personaName,
        'Response generation/posting',
        error instanceof Error ? error.message : String(error)
      );

      // Emit ERROR event
      if (this.client) {
        await Events.emit<AIErrorEventData>(
          DataDaemon.jtagContext!,
          AI_DECISION_EVENTS.ERROR,
          {
            personaId: this.personaId,
            personaName: this.personaName,
            roomId: originalMessage.roomId,
            messageId: originalMessage.id,
            isHumanMessage: originalMessage.senderType === 'human',
            timestamp: Date.now(),
            error: error instanceof Error ? error.message : String(error),
            phase: 'generating'
          },
          {
            scope: EVENT_SCOPES.ROOM,
            scopeId: originalMessage.roomId
          }
        );
      }

      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Convert timestamp to number (handles Date, number, or undefined from JSON serialization)
   */
  private timestampToNumber(timestamp: Date | number | undefined): number {
    if (timestamp === undefined) {
      return Date.now(); // Use current time if timestamp missing
    }
    return timestamp instanceof Date ? timestamp.getTime() : timestamp;
  }

  /**
   * Clean AI response by stripping any name prefixes the LLM added despite system prompt instructions
   * LLMs sometimes copy the "[HH:MM] Name: message" format they see in conversation history
   *
   * CURRENT: Heuristic regex-based cleaning (defensive programming)
   * FUTURE: Should become AI-powered via ThoughtStream adapter (like gating)
   *         - An AI evaluates: "Does this response have formatting issues?"
   *         - Returns cleaned version with confidence score
   *         - Pluggable via recipe configuration
   *
   * Examples to strip:
   * - "[11:59] GPT Assistant: Yes, Joel..." → "Yes, Joel..."
   * - "GPT Assistant: Yes, Joel..." → "Yes, Joel..."
   * - "[11:59] Yes, Joel..." → "Yes, Joel..."
   */
  private cleanAIResponse(response: string): string {
    let cleaned = response.trim();

    // Pattern 1: Strip "[HH:MM] Name: " prefix
    // Matches: [11:59] GPT Assistant: message
    cleaned = cleaned.replace(/^\[\d{1,2}:\d{2}\]\s+[^:]+:\s*/, '');

    // Pattern 2: Strip "Name: " prefix at start
    // Matches: GPT Assistant: message
    // Only if it looks like a name (contains letters, spaces, and ends with colon)
    cleaned = cleaned.replace(/^[A-Z][A-Za-z\s]+:\s*/, '');

    // Pattern 3: Strip just "[HH:MM] " timestamp prefix
    // Matches: [11:59] message
    cleaned = cleaned.replace(/^\[\d{1,2}:\d{2}\]\s*/, '');

    return cleaned.trim();
  }

  /**
   * Self-review: Check if generated response is redundant compared to conversation history
   * Like a human who drafts a response, re-reads the chat, and thinks "oh someone already said that"
   *
   * NOTE: Currently disabled (too flaky) - returns false
   */
  async checkResponseRedundancy(
    myResponse: string,
    roomId: UUID,
    conversationHistory: Array<{ role: string; content: string; name?: string; timestamp?: number }>
  ): Promise<boolean> {
    try {
      // Use AIDecisionService for centralized redundancy checking
      // Create minimal context without needing full trigger message
      const decisionContext: AIDecisionContext = {
        personaId: this.personaId,
        personaName: this.personaName,
        roomId,
        triggerMessage: {
          id: '',
          roomId,
          senderId: '',
          senderName: 'System',
          senderType: 'system',
          content: { text: 'redundancy check', attachments: [] },
          timestamp: new Date(),
          collection: COLLECTIONS.CHAT_MESSAGES,
          version: 1,
          createdAt: new Date(),
          updatedAt: new Date(),
          status: 'sent',
          priority: 0,
          reactions: []
        } as unknown as ChatMessageEntity,
        ragContext: {
          domain: 'chat',
          contextId: roomId,
          personaId: this.personaId,
          identity: {
            name: this.personaName,
            systemPrompt: ''
          },
          conversationHistory: conversationHistory.map(msg => ({
            role: msg.role as 'user' | 'assistant',
            content: msg.content,
            name: msg.name,
            timestamp: msg.timestamp
          })),
          artifacts: [],
          privateMemories: [],
          metadata: {
            messageCount: conversationHistory.length,
            artifactCount: 0,
            memoryCount: 0,
            builtAt: new Date()
          }
        }
      };

      const result = await AIDecisionService.checkRedundancy(
        myResponse,
        decisionContext,
        { model: 'llama3.2:3b' }
      );

      return result.isRedundant;
    } catch (error) {
      AIDecisionLogger.logError(this.personaName, 'Redundancy check', error instanceof Error ? error.message : String(error));
      return false; // On error, allow the response (fail open)
    }
  }
}
