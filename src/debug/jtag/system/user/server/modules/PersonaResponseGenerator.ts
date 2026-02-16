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
// DATA_COMMANDS import removed — response posting now uses ORM.store() directly
import { ChatMessageEntity, type MediaItem } from '../../../data/entities/ChatMessageEntity';
import { inspect } from 'util';
import type { UserEntity } from '../../../data/entities/UserEntity';
import type { ModelConfig } from '../../../../commands/user/create/shared/UserCreateTypes';
import type { JTAGClient } from '../../../core/client/shared/JTAGClient';
import { Commands } from '../../../core/shared/Commands';
// DataCreateParams/DataCreateResult imports removed — response posting now uses ORM.store() directly
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { TextGenerationRequest, TextGenerationResponse, ChatMessage, ContentPart, NativeToolSpec, ToolCall as NativeToolCall, ToolResult as NativeToolResult } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { AICapabilityRegistry } from '../../../../daemons/ai-provider-daemon/shared/AICapabilityRegistry';
import { ChatRAGBuilder } from '../../../rag/builders/ChatRAGBuilder';
import { CognitionLogger } from './cognition/CognitionLogger';
import { truncate, getMessageText, messagePreview } from '../../../../shared/utils/StringUtils';
import { calculateCost as calculateModelCost } from '../../../../daemons/ai-provider-daemon/shared/PricingConfig';
import { AIDecisionLogger } from '../../../ai/server/AIDecisionLogger';
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
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '../../../data/config/DatabaseConfig';
import type { PersonaToolExecutor, ToolCall as ExecutorToolCall } from './PersonaToolExecutor';
import type { PersonaMediaConfig } from './PersonaMediaConfig';
import { PersonaToolRegistry } from './PersonaToolRegistry';
import { supportsNativeTools, unsanitizeToolName, sanitizeToolName, coerceParamsToSchema, getToolCapability } from './ToolFormatAdapter';
import { InferenceCoordinator } from '../../../coordination/server/InferenceCoordinator';
import { ContentDeduplicator } from './ContentDeduplicator';
// AiDetectSemanticLoop command removed from hot path — replaced with inline Jaccard similarity
// import type { AiDetectSemanticLoopParams, AiDetectSemanticLoopResult } from '../../../../commands/ai/detect-semantic-loop/shared/AiDetectSemanticLoopTypes';
import { SystemPaths } from '../../../core/config/SystemPaths';
// GarbageDetector — moved to Rust (persona/text_analysis/garbage_detection.rs)
import type { InboxMessage, ProcessableMessage } from './QueueItemTypes';
import type { RAGContext } from '../../../rag/shared/RAGTypes';
import { PromptCapture } from '../../../rag/shared/PromptCapture';
import { LOCAL_MODELS } from '../../../../system/shared/Constants';
import type { RustCognitionBridge } from './RustCognitionBridge';
// SemanticLoopResult — now inside ValidationResult, accessed via Rust IPC

// import { AiDetectSemanticLoop } from '../../../../commands/ai/detect-semantic-loop/shared/AiDetectSemanticLoopTypes';
// DataCreate import removed — response posting now uses ORM.store() directly
/**
 * Response generation result
 */
export interface ResponseGenerationResult {
  success: boolean;
  messageId?: UUID;
  error?: string;
  wasRedundant?: boolean;
  storedToolResultIds: UUID[];  // IDs of tool result messages that were processed (always present, may be empty)
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
  toolRegistry: PersonaToolRegistry;
  mediaConfig: PersonaMediaConfig;
  getSessionId: () => UUID | null;  // Function to get PersonaUser's current sessionId
  logger: import('./PersonaLogger').PersonaLogger;  // For persona-specific logging
  genome?: import('./PersonaGenome').PersonaGenome;  // For accessing trained LoRA adapters
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
  private toolRegistry: PersonaToolRegistry;
  private mediaConfig: PersonaMediaConfig;
  private getSessionId: () => UUID | null;
  private logger: import('./PersonaLogger').PersonaLogger;
  private genome?: import('./PersonaGenome').PersonaGenome;

  /** Content deduplicator - prevents same content from being posted within time window */
  private contentDeduplicator: ContentDeduplicator;
  /** Rust cognition bridge — set lazily after PersonaUser creates it */
  private _rustBridge: RustCognitionBridge | null = null;

  /**
   * Set Rust cognition bridge (called after PersonaUser creates it).
   * All validation gates (garbage, loop, truncated tool, semantic loop) are in Rust.
   */
  setRustBridge(bridge: RustCognitionBridge): void {
    this._rustBridge = bridge;
  }

  constructor(config: PersonaResponseGeneratorConfig) {
    this.personaId = config.personaId;
    this.personaName = config.personaName;
    this.entity = config.entity;
    this.logger = config.logger;
    this.modelConfig = config.modelConfig;
    this.client = config.client;
    this.toolExecutor = config.toolExecutor;
    this.toolRegistry = config.toolRegistry;
    this.mediaConfig = config.mediaConfig;
    this.getSessionId = config.getSessionId;
    this.genome = config.genome;

    // Initialize modular helpers
    this.contentDeduplicator = new ContentDeduplicator({ log: this.log.bind(this) });
  }

  /**
   * Get effective model for inference via Rust IPC.
   * 4-tier priority chain: trait adapter → current → any → base model.
   * Domain-to-trait mapping is canonical in Rust (no TS duplicate).
   */
  private async getEffectiveModel(taskDomain?: string): Promise<string> {
    if (!this._rustBridge) throw new Error('Rust bridge not initialized — cannot select model');
    const baseModel = this.modelConfig.model || LOCAL_MODELS.DEFAULT;
    const result = await this._rustBridge.selectModel(baseModel, taskDomain);
    return result.model;
  }

  /**
   * Log to persona's cognition.log file
   */
  private log(message: string, ...args: any[]): void {
    const timestamp = new Date().toISOString();
    const formattedArgs = args.length > 0
      ? ' ' + args.map(a =>
          typeof a === 'object' ? inspect(a, { depth: 2, colors: false, compact: true }) : String(a)
        ).join(' ')
      : '';
    this.logger.enqueueLog('cognition.log', `[${timestamp}] ${message}${formattedArgs}\n`);
  }

  /**
   * Safety cap for agent tool loop iterations, tiered by model capability.
   * Frontier models (Anthropic, OpenAI) are trusted to self-terminate via finishReason.
   * Mid-tier models with native tool support get moderate cap.
   * XML-based / local models get tight leash since they can't signal "I'm done" via finishReason.
   */
  private getSafetyMaxIterations(provider: string): number {
    if (['anthropic', 'openai', 'azure'].includes(provider)) return 25;
    if (supportsNativeTools(provider)) return 10;
    return 5;
  }

  /**
   * Convert MediaItems to ContentPart blocks for inclusion in model messages.
   */
  private mediaToContentParts(media: MediaItem[]): ContentPart[] {
    return media.map(m => {
      if (m.type === 'image') return { type: 'image' as const, image: m };
      if (m.type === 'audio') return { type: 'audio' as const, audio: m };
      if (m.type === 'video') return { type: 'video' as const, video: m };
      return { type: 'image' as const, image: m }; // Default fallback
    });
  }

  // NOTE: calculateSafeMessageCount was removed (dead code)
  // Context budgeting is now handled by ChatRAGBuilder.calculateSafeMessageCount()
  // which uses ModelContextWindows as the single source of truth

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
    message: ProcessableMessage,
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
        this.log(`✨ ${this.personaName}: @mention detected, waking from ${dormancyLevel} mode`);
      }
      return true;
    }

    // Level 0: Active - respond to everything
    if (dormancyLevel === 'active') {
      return true;
    }

    // Level 1: Mention-Only - only respond to @mentions (already handled above)
    if (dormancyLevel === 'mention-only') {
      this.log(`💤 ${this.personaName}: Dormant (mention-only), skipping message`);
      return false;
    }

    // Level 2: Human-Only - respond to humans OR @mentions (mentions already handled)
    if (dormancyLevel === 'human-only') {
      const isHumanSender = message.senderType === 'human';

      if (isHumanSender) {
        return true;
      }

      this.log(`💤 ${this.personaName}: Dormant (human-only), skipping AI message`);
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
    originalMessage: ProcessableMessage,
    decisionContext?: Omit<LogDecisionParams, 'responseContent' | 'tokensUsed' | 'responseTime'>,
    preBuiltRagContext?: RAGContext
  ): Promise<ResponseGenerationResult> {
    this.log(`🔧 TRACE-POINT-D: Entered respondToMessage (timestamp=${Date.now()})`);
    // Voice modality is a typed field — no cast needed
    this.log(`🔧 ${this.personaName}: Voice check - sourceModality=${originalMessage.sourceModality}, voiceSessionId=${originalMessage.voiceSessionId?.slice(0, 8) ?? 'none'}`);
    const generateStartTime = Date.now();  // Track total response time for decision logging
    const allStoredResultIds: UUID[] = [];  // Collect all tool result message IDs for task tracking
    try {
      // Pipeline timing tracker — filled as each phase completes
      const pipelineTiming: Record<string, number> = {};

      // 🔧 SUB-PHASE 3.1: Build RAG context (or use pre-built from evaluator)
      const phase31Start = Date.now();
      let fullRAGContext: RAGContext;

      if (preBuiltRagContext) {
        // OPTIMIZATION: Evaluator already built full RAG context — reuse it, skip redundant build
        fullRAGContext = preBuiltRagContext;
        pipelineTiming['3.1_rag'] = Date.now() - phase31Start;
        this.log(`⚡ ${this.personaName}: [PHASE 3.1] Using pre-built RAG context (${fullRAGContext.conversationHistory.length} messages, ${pipelineTiming['3.1_rag']}ms)`);
      } else {
        // Fallback: Build RAG context from scratch (for code paths that don't go through evaluator)
        this.log(`🔧 ${this.personaName}: [PHASE 3.1] Building RAG context with model=${this.modelConfig.model}...`);
        const ragBuilder = new ChatRAGBuilder(this.log.bind(this));
        const voiceSessionId = originalMessage.voiceSessionId;
        fullRAGContext = await ragBuilder.buildContext(
          originalMessage.roomId,
          this.personaId,
          {
            modelId: this.modelConfig.model,
            maxMemories: 5,
            includeArtifacts: true,
            includeMemories: true,
            voiceSessionId,
            provider: this.modelConfig.provider || 'candle',
            toolCapability: getToolCapability(this.modelConfig.provider || 'candle', this.modelConfig),
            currentMessage: {
              role: 'user',
              content: originalMessage.content.text,
              name: originalMessage.senderName,
              timestamp: this.timestampToNumber(originalMessage.timestamp)
            }
          }
        );
        pipelineTiming['3.1_rag'] = Date.now() - phase31Start;
        this.log(`✅ ${this.personaName}: [PHASE 3.1] RAG context built (${fullRAGContext.conversationHistory.length} messages, ${pipelineTiming['3.1_rag']}ms)`);
      }

      // 🔧 SUB-PHASE 3.2: Build message history for LLM
      const phase32Start = Date.now();
      this.log(`🔧 ${this.personaName}: [PHASE 3.2] Building LLM message array...`);
      // ✅ Support multimodal content (images, audio, video) for vision-capable models
      // Adapters will transform based on model capability (raw images vs text descriptions)
      const messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string | ChatMessage['content'] }> = [];

      // System prompt from RAG builder — includes room membership, memories, tool definitions,
      // widget context, global awareness, etc. ALL injected by budget-aware RAG sources.
      // No bypasses here — everything flows through the RAG budget system.
      let systemPrompt = fullRAGContext.identity.systemPrompt;

      // Tool capability for XML parsing (still needed for response parsing, not injection)
      const toolCap = getToolCapability(this.modelConfig.provider || 'candle', this.modelConfig);

      // Log system prompt size for monitoring
      this.log(`📋 ${this.personaName}: [RAG] ${systemPrompt.length} chars (~${Math.ceil(systemPrompt.length / 4)} tokens), toolCap=${toolCap}, provider=${this.modelConfig.provider}`);

      messages.push({
        role: 'system',
        content: systemPrompt
      });

      // Build artifact lookup map (messageId → artifacts) for multimodal support
      // This enables vision models to see images from messages with media
      type RAGArtifact = typeof fullRAGContext.artifacts[number];
      const artifactsByMessageId = new Map<string, RAGArtifact[]>();
      for (const artifact of fullRAGContext.artifacts) {
        const messageId = artifact.metadata?.messageId as string | undefined;
        if (messageId) {
          if (!artifactsByMessageId.has(messageId)) {
            artifactsByMessageId.set(messageId, []);
          }
          artifactsByMessageId.get(messageId)!.push(artifact);
        }
      }

      // Also create a timestamp+name lookup for matching LLMMessages to artifacts
      // LLMMessages don't have IDs, so we match by timestamp+sender combination
      const artifactsByTimestampName = new Map<string, RAGArtifact[]>();
      for (const artifact of fullRAGContext.artifacts) {
        const timestamp = artifact.metadata?.timestamp as Date | number | undefined;
        const senderName = artifact.metadata?.senderName as string | undefined;
        if (timestamp && senderName) {
          const timestampMs = timestamp instanceof Date ? timestamp.getTime() : typeof timestamp === 'number' ? timestamp : 0;
          const key = `${timestampMs}_${senderName}`;
          if (!artifactsByTimestampName.has(key)) {
            artifactsByTimestampName.set(key, []);
          }
          artifactsByTimestampName.get(key)!.push(artifact);
        }
      }

      this.log(`🖼️  ${this.personaName}: Loaded ${fullRAGContext.artifacts.length} artifacts for ${artifactsByMessageId.size} messages`);

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

          // Check if this message has associated artifacts (images, audio, video)
          // Match by timestamp+name since LLMMessages don't have IDs
          const lookupKey = msg.timestamp && msg.name ? `${msg.timestamp}_${msg.name}` : null;
          const messageArtifacts = lookupKey ? artifactsByTimestampName.get(lookupKey) : undefined;

          if (messageArtifacts && messageArtifacts.length > 0) {
            // Multimodal message: Convert to ContentPart[] format
            const contentParts: ContentPart[] = [
              {
                type: 'text',
                text: formattedContent
              }
            ];

            // Add artifacts as image/audio/video parts
            for (const artifact of messageArtifacts) {
              const mimeType = artifact.metadata?.mimeType as string | undefined;

              if (artifact.type === 'image' && artifact.base64) {
                contentParts.push({
                  type: 'image',
                  image: {
                    base64: artifact.base64,
                    mimeType
                  }
                });
              } else if (artifact.type === 'audio' && artifact.base64) {
                contentParts.push({
                  type: 'audio',
                  audio: {
                    base64: artifact.base64,
                    mimeType
                  }
                });
              } else if (artifact.type === 'video' && artifact.base64) {
                contentParts.push({
                  type: 'video',
                  video: {
                    base64: artifact.base64,
                    mimeType
                  }
                });
              }
            }

            messages.push({
              role: msg.role,
              content: contentParts  // Multimodal content
            });

            this.log(`🖼️  ${this.personaName}: Added ${messageArtifacts.length} artifact(s) to message from ${msg.name}`);
          } else {
            // Text-only message
            messages.push({
              role: msg.role,
              content: formattedContent
            });
          }
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

      // VOICE MODE: Add conversational brevity instruction (only if not already in RAG context)
      // VoiceConversationSource injects these via systemPromptSection when active
      // This is a fallback for cases where sourceModality is set but VoiceConversationSource wasn't used
      const hasVoiceRAGContext = fullRAGContext.metadata && (fullRAGContext.metadata as any).responseStyle?.voiceMode;
      if (originalMessage.sourceModality === 'voice' && !hasVoiceRAGContext) {
        messages.push({
          role: 'system',
          content: `🎙️ VOICE CONVERSATION MODE:
This is a SPOKEN conversation. Your response will be converted to speech.

CRITICAL: Keep responses SHORT and CONVERSATIONAL:
- Maximum 2-3 sentences
- No bullet points, lists, or formatting
- Speak naturally, as if talking face-to-face
- Ask clarifying questions instead of long explanations
- If the topic is complex, give a brief answer and offer to elaborate

BAD (too long): "There are several approaches to this problem. First, you could... Second, another option is... Third, additionally you might consider..."
GOOD (conversational): "The simplest approach would be X. Want me to explain the alternatives?"

Remember: This is voice chat, not a written essay. Be brief, be natural, be human.`
        });
        this.log(`🔊 ${this.personaName}: Added voice conversation mode instructions (fallback - VoiceConversationSource not active)`);
      } else if (hasVoiceRAGContext) {
        this.log(`🔊 ${this.personaName}: Voice instructions provided by VoiceConversationSource`);
      }

      this.log(`✅ ${this.personaName}: [PHASE 3.2] LLM message array built (${messages.length} messages)`);

      // 🔧 SUB-PHASE 3.3: Generate AI response with timeout
      this.log(`🔧 ${this.personaName}: [PHASE 3.3] Calling AIProviderDaemon.generateText (provider: ${this.modelConfig.provider}, model: ${this.modelConfig.model})...`);

      // Bug #5 fix: Use adjusted maxTokens from RAG context (two-dimensional budget)
      // If ChatRAGBuilder calculated an adjusted value, use it. Otherwise fall back to config.
      let effectiveMaxTokens = fullRAGContext.metadata.adjustedMaxTokens ?? this.modelConfig.maxTokens ?? 150;

      // VOICE MODE: Allow reasonable response length for natural conversation
      // DON'T artificially truncate - that's robotic and cuts off mid-sentence
      // Natural turn-taking should be handled by arbiter coordination, not hard limits
      // Removed aggressive 100-token limit - now uses 800 tokens (~60 seconds of speech)
      const responseStyle = (fullRAGContext.metadata as any)?.responseStyle;
      const isVoiceMode = responseStyle?.voiceMode || originalMessage.sourceModality === 'voice';
      if (isVoiceMode) {
        // Voice mode: Use generous limit for natural speech (800 tokens ≈ 600 words ≈ 60 seconds)
        // Previous 100-token limit caused mid-sentence cutoffs - unacceptable
        const VOICE_MAX_TOKENS = 800;
        if (effectiveMaxTokens > VOICE_MAX_TOKENS) {
          this.log(`🔊 ${this.personaName}: VOICE MODE - limiting response from ${effectiveMaxTokens} to ${VOICE_MAX_TOKENS} tokens`);
          effectiveMaxTokens = VOICE_MAX_TOKENS;
        }
      }

      this.log(`📊 ${this.personaName}: RAG metadata check:`, {
        hasAdjustedMaxTokens: !!fullRAGContext.metadata.adjustedMaxTokens,
        adjustedMaxTokens: fullRAGContext.metadata.adjustedMaxTokens,
        inputTokenCount: fullRAGContext.metadata.inputTokenCount,
        configMaxTokens: this.modelConfig.maxTokens,
        effectiveMaxTokens: effectiveMaxTokens,
        model: this.modelConfig.model,
        provider: this.modelConfig.provider
      });

      const effectiveModel = await this.getEffectiveModel();
      const request: TextGenerationRequest = {
        messages,
        model: effectiveModel,  // Use trained model if available, otherwise base model
        temperature: this.modelConfig.temperature ?? 0.7,
        maxTokens: effectiveMaxTokens,    // Bug #5 fix: Use adjusted value from two-dimensional budget
        provider: this.modelConfig.provider || 'candle',
        intelligenceLevel: this.entity.intelligenceLevel,  // Pass PersonaUser intelligence level to adapter
        // CRITICAL: personaContext enables per-persona logging and prevents "unknown" rejections
        personaContext: {
          uniqueId: this.personaId,
          displayName: this.personaName,
          logDir: SystemPaths.personas.dir(this.personaId)
        }
      };

      // GENOME INTEGRATION: Add active LoRA adapters from PersonaGenome
      // This enables personas to use skill-specific fine-tuned weights during generation
      if (this.genome) {
        const activeAdapters = this.genome.getActiveAdaptersForRequest();
        if (activeAdapters.length > 0) {
          request.activeAdapters = activeAdapters;
          this.log(`🧬 ${this.personaName}: [PHASE 3.3] Genome providing ${activeAdapters.length} active adapters: [${activeAdapters.map(a => a.name).join(', ')}]`);
        }
      }

      // 🎰 PHASE 3.3a: Request inference slot from coordinator
      // This prevents thundering herd - only N personas can generate simultaneously per provider
      const provider = this.modelConfig.provider || 'candle';

      // Native tools from RAG budget (ToolDefinitionsSource handles prioritization + budget)
      const toolMeta = fullRAGContext.metadata?.toolDefinitions;
      if (toolMeta?.nativeToolSpecs && (toolMeta.nativeToolSpecs as unknown[]).length > 0) {
        request.tools = toolMeta.nativeToolSpecs as any;
        request.toolChoice = (toolMeta.toolChoice as string) || 'auto';
        this.log(`🔧 ${this.personaName}: Added ${(toolMeta.nativeToolSpecs as unknown[]).length} native tools from RAG budget (toolChoice=${request.toolChoice})`);
      }
      pipelineTiming['3.2_format'] = Date.now() - phase32Start;
      this.log(`✅ ${this.personaName}: [PHASE 3.2] LLM messages built (${messages.length} messages, ${pipelineTiming['3.2_format']}ms)`);

      // Check for mentions by both uniqueId (@helper) and displayName (@Helper AI)
      const messageText = originalMessage.content.text.toLowerCase();
      const isMentioned =
        messageText.includes(`@${this.entity.uniqueId.toLowerCase()}`) ||
        messageText.includes(`@${this.personaName.toLowerCase()}`);

      const phase33aStart = Date.now();
      const slotGranted = await InferenceCoordinator.requestSlot(
        this.personaId,
        originalMessage.id,
        provider,
        { isMentioned }
      );

      pipelineTiming['3.3a_slot'] = Date.now() - phase33aStart;

      if (!slotGranted) {
        this.log(`🎰 ${this.personaName}: [PHASE 3.3a] Inference slot denied (${pipelineTiming['3.3a_slot']}ms) - skipping response`);
        return { success: true, wasRedundant: true, storedToolResultIds: [] }; // Treat as redundant (another AI will respond)
      }
      this.log(`🎰 ${this.personaName}: [PHASE 3.3a] Inference slot granted (${pipelineTiming['3.3a_slot']}ms)`);

      // ── Prompt capture for replay/debugging ──
      // Captures the complete prompt (system + messages + tools) in JSONL format.
      // Read with: PromptCapture.load({ personaName: 'Helper AI', limit: 5 })
      // Replay with: AIProviderDaemon.generateText(PromptCapture.toReplayRequest(capture))
      PromptCapture.capture({
        personaId: this.personaId,
        personaName: this.personaName,
        model: request.model || this.modelConfig.model || 'unknown',
        provider: request.provider || 'candle',
        temperature: request.temperature ?? 0.7,
        maxTokens: request.maxTokens ?? 256,
        messages: messages.map(m => ({
          role: m.role,
          content: m.content,
          name: undefined  // name is embedded in content as "[HH:MM] Name: text"
        })),
        tools: request.tools as unknown[] | undefined,
        toolChoice: typeof request.toolChoice === 'string' ? request.toolChoice : request.toolChoice ? JSON.stringify(request.toolChoice) : undefined,
        triggerMessageId: originalMessage.id,
        triggerMessagePreview: originalMessage.content?.text?.slice(0, 100),
        ragSourceCount: fullRAGContext.metadata?.messageCount,
        ragTotalTokens: fullRAGContext.metadata?.inputTokenCount as number | undefined,
        activeAdapters: request.activeAdapters?.map(a => ({ name: a.name, path: a.path }))
      });

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
        // Wait for AIProviderDaemon to initialize (max 30 seconds)
        // This handles race condition where PersonaUser tries to respond before daemon is ready
        const phase33bStart = Date.now();
        const MAX_WAIT_MS = 30000;
        const POLL_INTERVAL_MS = 100;
        let waitedMs = 0;
        while (!AIProviderDaemon.isInitialized() && waitedMs < MAX_WAIT_MS) {
          await new Promise(resolve => setTimeout(resolve, POLL_INTERVAL_MS));
          waitedMs += POLL_INTERVAL_MS;
        }
        pipelineTiming['3.3b_daemon_init'] = Date.now() - phase33bStart;
        if (pipelineTiming['3.3b_daemon_init'] > 50) {
          this.log(`⏳ ${this.personaName}: [PHASE 3.3b] AIProviderDaemon init wait: ${pipelineTiming['3.3b_daemon_init']}ms`);
        }
        if (!AIProviderDaemon.isInitialized()) {
          throw new Error(`AIProviderDaemon not initialized after ${MAX_WAIT_MS}ms`);
        }

        const inferenceStart = Date.now();
        aiResponse = await Promise.race([
          AIProviderDaemon.generateText(request),
          timeoutPromise
        ]);
        pipelineTiming['3.3_inference'] = Date.now() - inferenceStart;

        // 🎰 Release slot on success
        InferenceCoordinator.releaseSlot(this.personaId, provider);
        const generateDuration = Date.now() - generateStartTime;
        this.log(`✅ ${this.personaName}: [PHASE 3.3] AI response generated (${aiResponse.text.trim().length} chars)`);

        // Fire-and-forget: Log AI response generation to cognition database (non-blocking telemetry)
        const inputTokenEstimate = messages.reduce((sum, m) => sum + Math.ceil(getMessageText(m.content).length / 4), 0);  // ~4 chars/token
        const outputTokenEstimate = Math.ceil(aiResponse.text.length / 4);
        const cost = calculateModelCost(
          this.modelConfig.provider ?? 'candle',
          this.modelConfig.model ?? LOCAL_MODELS.DEFAULT,
          inputTokenEstimate,
          outputTokenEstimate
        );

        CognitionLogger.logResponseGeneration(
          this.personaId,
          this.personaName,
          this.modelConfig.provider ?? 'candle',
          this.modelConfig.model ?? LOCAL_MODELS.DEFAULT,
          `${messages.slice(0, 2).map(m => `[${m.role}] ${messagePreview(m.content, 100)}`).join('\\n')}...`,  // First 2 messages as prompt summary
          inputTokenEstimate,
          outputTokenEstimate,
          cost,  // Calculated cost based on provider/model pricing
          truncate(aiResponse.text, 500),  // First 500 chars of response
          generateDuration,
          'success',
          this.modelConfig.temperature ?? 0.7,
          'chat',  // Domain
          originalMessage.roomId  // Context ID
        ).catch(err => this.log(`⚠️ Failed to log response generation: ${err}`));

        // Fire-and-forget: Emit cognition event for generate stage (non-blocking telemetry)
        Events.emit<StageCompleteEvent>(
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
                model: effectiveModel,  // Use the actual model used (may be trained LoRA adapter)
                provider: this.modelConfig.provider,
                tokensUsed: aiResponse.text.length
              }
            },
            timestamp: Date.now()
          }
        ).catch(err => this.log(`⚠️ Failed to emit stage complete event: ${err}`));

        // Clean AI response via Rust IPC — strip name prefixes LLMs add
        if (!this._rustBridge) {
          throw new Error('Rust bridge not initialized — cannot validate response');
        }

        const cleaned = await this._rustBridge.cleanResponse(aiResponse.text.trim());
        if (cleaned.was_cleaned) {
          aiResponse.text = cleaned.text;
        }

        // Combined validation gates (1 Rust IPC call)
        // Runs 4 gates in Rust: garbage detection, response loop, truncated tool call, semantic loop.
        const hasToolCalls = !!(aiResponse.toolCalls && aiResponse.toolCalls.length > 0);

        const validation = await this._rustBridge.validateResponse(
          aiResponse.text,
          hasToolCalls,
          fullRAGContext.conversationHistory
        );

        if (!validation.passed) {
          const gate = validation.gate_failed ?? 'unknown';
          this.log(`🚫 ${this.personaName}: [PHASE 3.3.5] Validation gate FAILED: ${gate} (${validation.total_time_us}us)`);

          // Release inference slot
          InferenceCoordinator.releaseSlot(this.personaId, provider);

          // Build gate-specific event data
          const gateConfidence = gate === 'garbage' ? validation.garbage_result.score
            : gate === 'response_loop' ? 0.9
            : gate === 'truncated_tool_call' ? 0.95
            : gate === 'semantic_loop' ? validation.semantic_result.similarity
            : 0.8;

          const gateReason = gate === 'garbage' ? `Garbage output: ${validation.garbage_result.reason} - ${validation.garbage_result.details}`
            : gate === 'response_loop' ? `Response loop detected - ${validation.loop_duplicate_count} duplicates`
            : gate === 'truncated_tool_call' ? 'Truncated tool call detected - response cut off mid-tool-call'
            : gate === 'semantic_loop' ? validation.semantic_result.reason
            : `Validation failed: ${gate}`;

          // Emit DECIDED_SILENT event (fire-and-forget)
          if (this.client) {
            Events.emit<AIDecidedSilentEventData>(
              DataDaemon.jtagContext!,
              AI_DECISION_EVENTS.DECIDED_SILENT,
              {
                personaId: this.personaId,
                personaName: this.personaName,
                roomId: originalMessage.roomId,
                messageId: originalMessage.id,
                isHumanMessage: originalMessage.senderType === 'human',
                timestamp: Date.now(),
                confidence: gateConfidence,
                reason: gateReason,
                gatingModel: `rust-${gate}`
              },
              { scope: EVENT_SCOPES.ROOM, scopeId: originalMessage.roomId }
            ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));
          }

          // Garbage returns failure; loops/truncated return redundant
          if (gate === 'garbage') {
            return { success: false, wasRedundant: false, storedToolResultIds: [], error: `garbage_output: ${validation.garbage_result.reason}` };
          }
          return { success: true, wasRedundant: true, storedToolResultIds: [] };
        }

        // 🔧 CANONICAL AGENT LOOP — model decides when to stop
        // Pattern: while (finishReason === 'tool_use') { execute → full results → regenerate }
        // Full tool results go back to the model (not summaries). Tools stay enabled.
        // The model signals completion by returning text without tool_use.
        // Safety cap prevents infinite loops for dumber models.
        const agentLoopStart = Date.now();
        const SAFETY_MAX = this.getSafetyMaxIterations(provider);
        let toolIterations = 0;
        const useNativeProtocol = supportsNativeTools(provider);

        // Build execution context once (loop-invariant — persona, session, room don't change)
        const sessionId = this.getSessionId();
        if (!sessionId) {
          throw new Error(`${this.personaName}: Cannot execute tools without sessionId`);
        }
        // Enrich context with userId so commands know the caller's identity
        const enrichedContext = { ...this.client!.context, userId: this.personaId };
        const toolExecutionContext = {
          personaId: this.personaId,
          personaName: this.personaName,
          sessionId,
          contextId: originalMessage.roomId,
          context: enrichedContext,
          personaConfig: this.mediaConfig,
        };

        while (toolIterations < SAFETY_MAX) {
          // Check for tool calls — native first, then XML fallback
          // ONE Rust IPC call replaces 3 separate sync TS calls (parse + correct + strip)
          const hasNativeToolCalls = aiResponse.toolCalls && aiResponse.toolCalls.length > 0;
          const parsed = !hasNativeToolCalls ? await this.toolExecutor.parseResponse(aiResponse.text) : null;
          const hasXmlToolCalls = parsed !== null && parsed.toolCalls.length > 0;

          if (!hasNativeToolCalls && !hasXmlToolCalls) {
            // Model chose to stop — no more tool calls
            if (toolIterations > 0) {
              this.log(`✅ ${this.personaName}: [AGENT-LOOP] Model stopped after ${toolIterations} iteration(s)`);
            }
            break;
          }

          toolIterations++;
          this.log(`🔧 ${this.personaName}: [AGENT-LOOP] Iteration ${toolIterations}/${SAFETY_MAX}`);

          if (hasNativeToolCalls || (useNativeProtocol && hasXmlToolCalls)) {
            // ── Native tool protocol (Anthropic, OpenAI, Groq, Together, etc.) ──
            // Handles both:
            //   1. Adapter returned structured tool_calls (normal case)
            //   2. Model output tool calls in text, Rust parsed them (Groq/Llama case)
            let nativeToolCalls: NativeToolCall[];
            if (hasNativeToolCalls) {
              nativeToolCalls = aiResponse.toolCalls!;
            } else {
              // Synthesize native format from text-parsed calls
              // Coerce params to match schema types (e.g. string "true" → boolean true)
              // so the API doesn't reject tool_use blocks on regeneration
              const toolSpecs = (request.tools as NativeToolSpec[]) ?? [];
              nativeToolCalls = parsed!.toolCalls.map((tc, i) => {
                const name = sanitizeToolName(tc.toolName);
                return {
                  id: `synth_${Date.now()}_${i}`,
                  name,
                  input: coerceParamsToSchema(tc.parameters, toolSpecs, name),
                };
              });
            }
            this.log(`🔧 ${this.personaName}: [AGENT-LOOP] Executing ${nativeToolCalls.length} native tool call(s)${!hasNativeToolCalls ? ' (synthesized from text)' : ''}`);

            let toolResults: NativeToolResult[];
            let toolMedia: MediaItem[] = [];
            try {
              const execResult = await this.toolExecutor.executeNativeToolCalls(
                nativeToolCalls,
                toolExecutionContext,
              );
              toolResults = execResult.results;
              toolMedia = execResult.media;
              allStoredResultIds.push(...execResult.storedIds);
            } catch (toolExecError) {
              // Tool execution batch failed — return error results for all tool calls
              // so the model can see what happened and decide what to do
              const errMsg = toolExecError instanceof Error ? toolExecError.message : String(toolExecError);
              this.log(`❌ ${this.personaName}: [AGENT-LOOP] Tool execution failed: ${errMsg}`);
              toolResults = nativeToolCalls.map(tc => ({
                toolUseId: tc.id,
                content: `Tool execution error: ${errMsg}`,
                isError: true as const,
              }));
            }

            // Push assistant message with tool_use content blocks
            // Use adapter's content if native tool calls, synthesize if text-parsed
            const assistantContent: ContentPart[] = hasNativeToolCalls
              ? (aiResponse.content ?? [
                  ...(aiResponse.text ? [{ type: 'text' as const, text: aiResponse.text }] : []),
                  ...nativeToolCalls.map(tc => ({
                    type: 'tool_use' as const,
                    id: tc.id,
                    name: tc.name,
                    input: tc.input,
                  })),
                ])
              : [
                  ...(parsed!.cleanedText ? [{ type: 'text' as const, text: parsed!.cleanedText }] : []),
                  ...nativeToolCalls.map(tc => ({
                    type: 'tool_use' as const,
                    id: tc.id,
                    name: tc.name,
                    input: tc.input,
                  })),
                ];
            messages.push({ role: 'assistant' as const, content: assistantContent });

            // Push tool results as user message with tool_result content blocks (FULL results)
            const toolResultContent: ContentPart[] = toolResults.map(r => ({
              type: 'tool_result' as const,
              tool_use_id: r.toolUseId,
              content: r.content,
              is_error: r.isError ?? null,
            }));

            // Include media if present (screenshots, etc.)
            if (toolMedia.length > 0) {
              toolResultContent.push(...this.mediaToContentParts(toolMedia));
            }

            messages.push({ role: 'user' as const, content: toolResultContent });

          } else if (hasXmlToolCalls) {
            // ── XML path for non-native providers (DeepSeek, Ollama, Candle) ──
            // Parse XML tool calls, execute, return results as text
            const xmlToolCalls = parsed!.toolCalls;

            this.log(`🔧 ${this.personaName}: [AGENT-LOOP] Executing ${xmlToolCalls.length} XML tool call(s)`);

            let formattedResults: string;
            let xmlToolMedia: MediaItem[] = [];
            try {
              const xmlExecResult = await this.toolExecutor.executeToolCalls(
                xmlToolCalls,
                toolExecutionContext,
              );
              formattedResults = xmlExecResult.formattedResults;
              xmlToolMedia = xmlExecResult.media ?? [];
              allStoredResultIds.push(...xmlExecResult.storedResultIds);
            } catch (toolExecError) {
              const errMsg = toolExecError instanceof Error ? toolExecError.message : String(toolExecError);
              this.log(`❌ ${this.personaName}: [AGENT-LOOP] XML tool execution failed: ${errMsg}`);
              formattedResults = `<tool_result>\n<status>error</status>\n<error>\n\`\`\`\nTool execution error: ${errMsg}\n\`\`\`\n</error>\n</tool_result>`;
            }

            // Use pre-parsed cleaned text from Rust IPC (already stripped)
            const explanationText = parsed!.cleanedText;

            messages.push({ role: 'assistant' as const, content: explanationText });

            // Full tool results as user message (NOT summarized)
            const toolResultContent: (ContentPart | { type: 'text'; text: string })[] = [
              { type: 'text' as const, text: formattedResults },
            ];
            if (xmlToolMedia.length > 0) {
              toolResultContent.push(...this.mediaToContentParts(xmlToolMedia));
            }
            messages.push({ role: 'user' as const, content: toolResultContent });
          }

          // Regenerate — tools stay enabled, model decides when to stop
          this.log(`🔧 ${this.personaName}: [AGENT-LOOP] Regenerating with ${messages.length} messages (tools enabled)`);

          try {
            const regenerateStartTime = Date.now();
            const regeneratedResponse = await AIProviderDaemon.generateText({
              ...request,
              messages, // Tools NOT stripped — model decides when to stop
            });
            const regenerateDuration = Date.now() - regenerateStartTime;

            this.log(`⏱️  ${this.personaName}: [AGENT-LOOP] Regeneration took ${regenerateDuration}ms, finishReason: ${regeneratedResponse.finishReason}`);

            if (!regeneratedResponse.text && !regeneratedResponse.toolCalls?.length) {
              this.log(`⚠️  ${this.personaName}: [AGENT-LOOP] Empty response from ${provider}/${effectiveModel} after ${toolIterations} tool iteration(s), using cleaned previous text`);
              // Regeneration returned nothing — use the model's explanation text from before tool calls
              // parseResponse strips tool blocks, leaving the natural language (e.g. "Let me check that...")
              const fallback = await this.toolExecutor.parseResponse(aiResponse.text);
              aiResponse.text = fallback.cleanedText;
              break;
            }

            // Update full response state — clean via Rust IPC
            const loopCleaned = await this._rustBridge!.cleanResponse(regeneratedResponse.text?.trim() || '');
            aiResponse.text = loopCleaned.text;
            aiResponse.toolCalls = regeneratedResponse.toolCalls ?? undefined;
            aiResponse.content = regeneratedResponse.content ?? undefined;
            aiResponse.finishReason = regeneratedResponse.finishReason;

            this.log(`✅ ${this.personaName}: [AGENT-LOOP] Got response (${aiResponse.text.length} chars, toolCalls: ${aiResponse.toolCalls?.length ?? 0})`);
          } catch (regenerateError) {
            const errorMsg = regenerateError instanceof Error ? regenerateError.message : String(regenerateError);
            this.log(`❌ ${this.personaName}: [AGENT-LOOP] Regeneration failed: ${errorMsg}`);
            aiResponse.text = (await this.toolExecutor.parseResponse(aiResponse.text)).cleanedText;
            break;
          }
        }

        if (toolIterations >= SAFETY_MAX) {
          this.log(`⚠️  ${this.personaName}: [AGENT-LOOP] Hit safety cap (${SAFETY_MAX}), stopping`);
          aiResponse.text = (await this.toolExecutor.parseResponse(aiResponse.text)).cleanedText;
        }
        pipelineTiming['3.4_agent_loop'] = Date.now() - agentLoopStart;
        if (toolIterations > 0) {
          this.log(`⏱️ ${this.personaName}: [AGENT-LOOP] Total: ${pipelineTiming['3.4_agent_loop']}ms (${toolIterations} iterations)`);
        }

        // PHASE 5C: Log coordination decision to database WITH complete response content
        // This captures the complete decision pipeline: context → decision → actual response
        this.log(`🔍 ${this.personaName}: [PHASE 5C DEBUG] decisionContext exists: ${!!decisionContext}, responseContent: "${truncate(aiResponse.text, 50)}..."`);
        if (decisionContext) {
          this.log(`🔧 ${this.personaName}: [PHASE 5C] Logging decision with response content (${aiResponse.text.length} chars)...`);
          CoordinationDecisionLogger.logDecision({
            ...decisionContext,
            responseContent: aiResponse.text,  // ✅ FIX: Now includes actual response!
            tokensUsed: aiResponse.text.length,  // Estimate based on character count
            responseTime: Date.now() - generateStartTime
          }).catch(error => {
            this.log(`❌ ${this.personaName}: Failed to log POSTED decision:`, error);
          });
          this.log(`✅ ${this.personaName}: [PHASE 5C] Decision logged with responseContent successfully`);
        } else {
          this.log(`❌ ${this.personaName}: [PHASE 5C] decisionContext is undefined - cannot log response!`);
        }
      } catch (error) {
        // 🎰 Release slot on error - CRITICAL to prevent slot leaks
        InferenceCoordinator.releaseSlot(this.personaId, provider);

        const errorMessage = error instanceof Error ? error.message : String(error);
        this.log(`❌ ${this.personaName}: [PHASE 3.3] AI generation failed:`, errorMessage);

        // Fire-and-forget: Log failed AI response generation to cognition database (non-blocking telemetry)
        const generateDuration = Date.now() - generateStartTime;
        CognitionLogger.logResponseGeneration(
          this.personaId,
          this.personaName,
          this.modelConfig.provider || 'candle',
          this.modelConfig.model || LOCAL_MODELS.DEFAULT,
          messages ? `${messages.slice(0, 2).map(m => `[${m.role}] ${messagePreview(m.content, 100)}`).join('\\n')}...` : '[messages unavailable]',
          messages ? messages.reduce((sum, m) => sum + getMessageText(m.content).length, 0) : 0,
          0,  // No completion tokens on error
          0.0,  // No cost
          `[GENERATION FAILED: ${errorMessage}]`,  // Error as response summary
          generateDuration,
          'error',  // Status
          this.modelConfig.temperature ?? 0.7,
          'chat',
          originalMessage.roomId,
          { errorMessage }  // Include error details
        ).catch(err => this.log(`⚠️ Failed to log error response: ${err}`));

        // Fire-and-forget: Emit ERROR event for UI display (non-blocking)
        if (this.client) {
          Events.emit<AIErrorEventData>(
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
          ).catch(err => this.log(`⚠️ Failed to emit error event: ${err}`));
        }

        // Log error to AI decisions log
        AIDecisionLogger.logError(this.personaName, 'AI generation (PHASE 3.3)', errorMessage);

        // Re-throw to be caught by outer try-catch
        throw error;
      }

      // 🔧 SUB-PHASE 3.5: Create and post response
      this.log(`🔧 ${this.personaName}: [PHASE 3.5] Creating response message entity...`);
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

      // 🔊 VOICE ROUTING: Emit BEFORE DB write — voice gets response text instantly.
      // The DB write (500ms-1.5s under contention) should NOT delay TTS.
      // Voice event only needs the response text and message metadata, not the persisted entity.
      if (originalMessage.sourceModality === 'voice' && originalMessage.voiceSessionId) {
        this.log(`🔊 ${this.personaName}: Voice message - emitting for TTS routing BEFORE DB write (sessionId=${originalMessage.voiceSessionId.slice(0, 8)})`);

        Events.emit(
          DataDaemon.jtagContext!,
          'persona:response:generated',
          {
            personaId: this.personaId,
            response: aiResponse.text.trim(),
            originalMessage: {
              id: originalMessage.id,
              roomId: originalMessage.roomId,
              sourceModality: 'voice' as const,
              voiceSessionId: originalMessage.voiceSessionId,
            }
          }
        ).catch(err => this.log(`⚠️ Voice event emit failed: ${err}`));
      }

      // ✅ Post response via ORM.store() — direct path, no command routing overhead.
      // Previously went through JTAGClient → CommandDaemon → DataCreateServerCommand → ORM.store().
      const postStartTime = Date.now();
      const postedEntity = await ORM.store(ChatMessageEntity.collection, responseMessage);
      pipelineTiming['3.5_post'] = Date.now() - postStartTime;
      const postDuration = pipelineTiming['3.5_post'];
      this.log(`✅ ${this.personaName}: [PHASE 3.5] Message posted (${postDuration}ms, ID: ${postedEntity.id})`);

      // Emit cognition event for post-response stage (fire-and-forget — telemetry)
      Events.emit<StageCompleteEvent>(
        DataDaemon.jtagContext!,
        COGNITION_EVENTS.STAGE_COMPLETE,
        {
          messageId: postedEntity.id ?? originalMessage.id,
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
              messageId: postedEntity.id,
              success: true
            }
          },
          timestamp: Date.now()
        }
      ).catch(err => this.log(`⚠️ Stage event emit failed: ${err}`));

      // ✅ Log successful response posting
      AIDecisionLogger.logResponse(
        this.personaName,
        originalMessage.roomId,
        aiResponse.text.trim()
      );

      // 🐦 COGNITIVE CANARY: Log anomaly if AI responded to system test message
      if (originalMessage.metadata?.isSystemTest === true) {
        const anomalyMessage = `🚨 ANOMALY DETECTED: ${this.personaName} responded to system test message`;
        this.log(anomalyMessage);
        this.log(`   Test Type: ${originalMessage.metadata.testType ?? 'unknown'}`);
        this.log(`   Original Message: "${messagePreview(originalMessage.content, 100)}..."`);
        this.log(`   AI Response: "${truncate(aiResponse.text?.trim(), 100)}..."`);
        this.log(`   Room ID: ${originalMessage.roomId}`);
        this.log(`   Message ID: ${originalMessage.id}`);

        AIDecisionLogger.logError(
          this.personaName,
          'COGNITIVE CANARY TRIGGERED',
          `Responded to system test (${originalMessage.metadata.testType}) - this should never happen`
        );
      }

      // Emit POSTED event (fire-and-forget — UI update, not critical path)
      if (this.client && postedEntity) {
        Events.emit<AIPostedEventData>(
          DataDaemon.jtagContext!,
          AI_DECISION_EVENTS.POSTED,
          {
            personaId: this.personaId,
            personaName: this.personaName,
            roomId: originalMessage.roomId,
            messageId: originalMessage.id,
            isHumanMessage: originalMessage.senderType === 'human',
            timestamp: Date.now(),
            responseMessageId: postedEntity.id,
            passedRedundancyCheck: true
          },
          {
            scope: EVENT_SCOPES.ROOM,
            scopeId: originalMessage.roomId
          }
        ).catch(err => this.log(`⚠️ Posted event emit failed: ${err}`));
      }

      // 📊 PIPELINE SUMMARY — single line with all phase timings
      const totalPipeline = Date.now() - generateStartTime;
      const phases = Object.entries(pipelineTiming)
        .map(([k, v]) => `${k}=${v}ms`)
        .join(' | ');
      this.log(`📊 ${this.personaName}: [PIPELINE] Total=${totalPipeline}ms | ${phases}`);

      return {
        success: true,
        messageId: postedEntity.id,
        storedToolResultIds: allStoredResultIds  // Always return array, even if empty
      };
    } catch (error) {
      // Fail silently - real people don't send canned error messages, they just stay quiet
      AIDecisionLogger.logError(
        this.personaName,
        'Response generation/posting',
        error instanceof Error ? error.message : String(error)
      );

      // Emit ERROR event (fire-and-forget — UI indicator)
      if (this.client) {
        Events.emit<AIErrorEventData>(
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
        ).catch(err => this.log(`⚠️ Error event emit failed: ${err}`));
      }

      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        storedToolResultIds: []
      };
    }
  }

  /**
   * Convert timestamp to number (handles Date, number, string, or undefined from JSON serialization)
   *
   * NOTE: Rust ORM returns dates as ISO strings (e.g., "2026-02-07T18:17:56.886Z").
   * Must handle all formats to prevent type mismatch errors when passing to Rust IPC.
   */
  private timestampToNumber(timestamp: Date | number | string | undefined): number {
    if (timestamp === undefined) {
      return Date.now(); // Use current time if timestamp missing
    }
    if (timestamp instanceof Date) {
      return timestamp.getTime();
    }
    if (typeof timestamp === 'string') {
      // Parse ISO string from Rust ORM (e.g., "2026-02-07T18:17:56.886Z")
      const parsed = new Date(timestamp).getTime();
      return isNaN(parsed) ? Date.now() : parsed;
    }
    return timestamp; // Already a number
  }

}
