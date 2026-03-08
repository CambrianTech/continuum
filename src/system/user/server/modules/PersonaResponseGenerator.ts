/**
 * PersonaResponseGenerator - Orchestrator for AI response generation and posting
 *
 * Delegates to extracted modules:
 * - PersonaPromptAssembler: LLM message array construction
 * - PersonaResponseValidator: Response cleaning and validation gates
 * - PersonaEngagementDecider: Dormancy/engagement checks
 *
 * This module orchestrates: RAG build → prompt assembly → inference → validation → tool loop → post
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { ChatMessageEntity, type MediaItem } from '../../../data/entities/ChatMessageEntity';
import { inspect } from 'util';
import type { UserEntity } from '../../../data/entities/UserEntity';
import type { ModelConfig } from '../../../data/entities/UserEntity';
import type { JTAGClient } from '../../../core/client/shared/JTAGClient';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { TextGenerationRequest, TextGenerationResponse, ChatMessage, ContentPart, NativeToolSpec, ToolCall as NativeToolCall, ToolResult as NativeToolResult } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
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
import type { PersonaToolExecutor } from './PersonaToolExecutor';
import type { PersonaMediaConfig } from './PersonaMediaConfig';
import { PersonaToolRegistry } from './PersonaToolRegistry';
import { supportsNativeTools, sanitizeToolName, coerceParamsToSchema, getToolCapability } from './ToolFormatAdapter';
import { InferenceCoordinator } from '../../../coordination/server/InferenceCoordinator';
// ContentDeduplicator removed — content dedup now handled by Rust (cognition/check-content-dedup IPC)
import { SystemPaths } from '../../../core/config/SystemPaths';
import type { ProcessableMessage } from './QueueItemTypes';
import type { RAGContext } from '../../../rag/shared/RAGTypes';
import { PromptCapture } from '../../../rag/shared/PromptCapture';
import { LOCAL_MODELS } from '../../../../system/shared/Constants';
import type { RustCognitionBridge } from './RustCognitionBridge';
import { FitnessTracker } from '../../../genome/server/FitnessTracker';
import { getAIAudioBridge } from '../../../voice/server/AIAudioBridge';
import { PRESENCE_EVENTS } from '../../../core/shared/EventConstants';
import { PersonaPromptAssembler } from './PersonaPromptAssembler';
import { PersonaResponseValidator } from './PersonaResponseValidator';
import { PersonaEngagementDecider, type DormancyState } from './PersonaEngagementDecider';
import { PersonaTimingConfig } from './PersonaTimingConfig';
import type { SocialSignals } from '../../../../shared/generated';
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
  trainingAccumulator?: import('./TrainingDataAccumulator').TrainingDataAccumulator;  // For capturing interactions
  rustCognitionBridge?: import('./RustCognitionBridge').RustCognitionBridge;  // For domain classification + quality scoring
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
  private trainingAccumulator?: import('./TrainingDataAccumulator').TrainingDataAccumulator;
  private rustCognitionBridge?: import('./RustCognitionBridge').RustCognitionBridge;

  /** Rust cognition bridge — set lazily after PersonaUser creates it */
  private _rustBridge: RustCognitionBridge | null = null;
  /** Extracted modules */
  private promptAssembler: PersonaPromptAssembler;
  private responseValidator: PersonaResponseValidator;
  private engagementDecider: PersonaEngagementDecider;

  /**
   * Set Rust cognition bridge (called after PersonaUser creates it).
   * All validation gates (garbage, loop, truncated tool, semantic loop) are in Rust.
   */
  setRustBridge(bridge: RustCognitionBridge): void {
    this._rustBridge = bridge;
    this.responseValidator.setRustBridge(bridge);
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
    this.trainingAccumulator = config.trainingAccumulator;
    this.rustCognitionBridge = config.rustCognitionBridge;

    // Initialize modular helpers
    this.promptAssembler = new PersonaPromptAssembler(config.personaName, config.modelConfig, this.log.bind(this));
    this.responseValidator = new PersonaResponseValidator(config.personaName, this.log.bind(this));
    this.engagementDecider = new PersonaEngagementDecider(config.personaName, this.log.bind(this));
  }

  /**
   * Get effective model for inference via Rust IPC.
   * 4-tier priority chain: trait adapter → current → any → base model.
   * Domain-to-trait mapping is canonical in Rust (no TS duplicate).
   */
  private async getEffectiveModel(taskDomain?: string): Promise<string> {
    if (!this._rustBridge) throw new Error('Rust bridge not initialized — cannot select model');
    const baseModel = this.modelConfig.model;
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
   * Convert MediaItems to ContentPart blocks. Delegates to PersonaPromptAssembler.
   */
  private mediaToContentParts(media: MediaItem[]): ContentPart[] {
    return this.promptAssembler.mediaToContentParts(media);
  }

  // NOTE: calculateSafeMessageCount was removed (dead code)
  // Context budgeting is now handled by ChatRAGBuilder.calculateSafeMessageCount()
  // which uses ModelContextWindows as the single source of truth

  /**
   * Check if persona should respond to message based on dormancy level.
   * Delegates to PersonaEngagementDecider.
   */
  shouldRespondToMessage(
    message: ProcessableMessage,
    dormancyState?: DormancyState
  ): boolean {
    return this.engagementDecider.shouldRespondToMessage(message, dormancyState);
  }

  /**
   * Generate and post a response to a chat message
   * Phase 2: AI-powered responses with RAG context via AIProviderDaemon
   */
  async generateAndPostResponse(
    originalMessage: ProcessableMessage,
    decisionContext?: Omit<LogDecisionParams, 'responseContent' | 'tokensUsed' | 'responseTime'>,
    preBuiltRagContext?: RAGContext,
    socialSignals?: SocialSignals
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
            maxTokens: this.modelConfig.maxTokens,
            maxMemories: 5,
            includeArtifacts: true,
            includeMemories: true,
            voiceSessionId,
            provider: this.modelConfig.provider,
            toolCapability: getToolCapability(this.modelConfig.provider, this.modelConfig),
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

      // 🔧 SUB-PHASE 3.2: Build message history for LLM (delegated to PersonaPromptAssembler)
      const phase32Start = Date.now();
      const messages = this.promptAssembler.assembleMessages(fullRAGContext, originalMessage, socialSignals);

      // Tool capability for XML parsing (still needed for response parsing, not injection)
      const toolCap = getToolCapability(this.modelConfig.provider, this.modelConfig);

      // 🔧 SUB-PHASE 3.3: Generate AI response with timeout
      this.log(`🔧 ${this.personaName}: [PHASE 3.3] Calling AIProviderDaemon.generateText (provider: ${this.modelConfig.provider}, model: ${this.modelConfig.model})...`);

      // Bug #5 fix: Use adjusted maxTokens from RAG context (two-dimensional budget)
      // RAG budget can only REDUCE maxTokens (protect against context overflow),
      // never INCREASE beyond what the model config specifies.
      const configMaxTokens = this.modelConfig.maxTokens;
      const ragAdjusted = fullRAGContext.metadata.adjustedMaxTokens;
      // Use != null (not truthy) so 0 is properly handled — 0 means budget is blown.
      let effectiveMaxTokens = (ragAdjusted != null && ragAdjusted < configMaxTokens)
        ? ragAdjusted
        : configMaxTokens;

      // VOICE MODE: Allow reasonable response length for natural conversation
      // DON'T artificially truncate - that's robotic and cuts off mid-sentence
      // Natural turn-taking should be handled by arbiter coordination, not hard limits
      // Removed aggressive 100-token limit - now uses 800 tokens (~60 seconds of speech)
      const responseStyle = (fullRAGContext.metadata as any)?.responseStyle;
      const isVoiceMode = responseStyle?.voiceMode || originalMessage.sourceModality === 'voice';
      if (isVoiceMode) {
        // Voice mode: Use generous limit for natural speech (800 tokens ≈ 600 words ≈ 60 seconds)
        // Previous 100-token limit caused mid-sentence cutoffs - unacceptable
        if (effectiveMaxTokens > PersonaTimingConfig.generation.voiceMaxTokens) {
          this.log(`🔊 ${this.personaName}: VOICE MODE - limiting response from ${effectiveMaxTokens} to ${PersonaTimingConfig.generation.voiceMaxTokens} tokens`);
          effectiveMaxTokens = PersonaTimingConfig.generation.voiceMaxTokens;
        }
      }

      this.log(`📊 ${this.personaName}: RAG metadata check:`, {
        hasAdjustedMaxTokens: ragAdjusted != null,
        adjustedMaxTokens: ragAdjusted,
        inputTokenCount: fullRAGContext.metadata.inputTokenCount,
        configMaxTokens: this.modelConfig.maxTokens,
        effectiveMaxTokens: effectiveMaxTokens,
        model: this.modelConfig.model,
        provider: this.modelConfig.provider
      });

      // Budget blown: prompt already exceeds context window, no room for output tokens.
      // This means calculateSafeMessageCount selected too many messages — a bug upstream.
      // Don't send to inference (it'll just error). Log and bail.
      if (effectiveMaxTokens <= 0) {
        this.log(`❌ ${this.personaName}: Budget blown — input tokens (${fullRAGContext.metadata.inputTokenCount}) exceed context window. Skipping inference.`);
        return { success: false, error: 'Context budget exceeded — prompt too large for model', storedToolResultIds: [] };
      }

      // PHASE 1B: Classify message domain → activate matching adapter → select model
      // This closes the gap: adapters were discovered on startup but never activated before inference.
      // Flow: classify domain (Rust, ~μs) → activate adapter (page into GPU) → select model (uses active adapter)
      let currentDomain: string | undefined = this.genome?.getCurrentAdapter()?.getDomain();
      if (this.genome && this._rustBridge) {
        try {
          const messageText = originalMessage.content.text;
          const classification = await this._rustBridge.classifyDomain(messageText);
          if (classification.confidence > 0.3) {
            await this.genome.activateForDomain(classification.domain);
            currentDomain = classification.domain;
            this.log(`🧬 ${this.personaName}: Domain classified='${classification.domain}' (confidence=${classification.confidence.toFixed(2)}), adapter=${classification.adapter_name || 'none'}`);
          }
        } catch (err) {
          // Classification failure is non-fatal — proceed with whatever adapter is currently active
          this.log(`⚠️ ${this.personaName}: Domain classification failed: ${err}`);
        }
      }
      const effectiveModel = await this.getEffectiveModel(currentDomain);
      const request: TextGenerationRequest = {
        messages,
        model: effectiveModel,  // Use trained model if available, otherwise base model
        temperature: this.modelConfig.temperature ?? 0.7,
        maxTokens: effectiveMaxTokens,    // Bug #5 fix: Use adjusted value from two-dimensional budget
        provider: this.modelConfig.provider,
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
      const provider = this.modelConfig.provider;

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
        model: effectiveModel,
        provider: this.modelConfig.provider,
        temperature: request.temperature ?? 0.7,
        maxTokens: effectiveMaxTokens,
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

      // Wrap generation call with timeout (180s - generous limit for local Candle/Sentinel generation)
      // gpt2 on CPU needs ~60-90s for 100-150 tokens, 180s provides comfortable margin
      // Queue can handle 4 concurrent requests, so 180s allows slower hardware to complete
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error(`AI generation timeout after ${PersonaTimingConfig.generation.timeoutMs / 1000} seconds`)), PersonaTimingConfig.generation.timeoutMs);
      });

      let aiResponse: TextGenerationResponse;
      let extractedThinking: string | undefined;
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
          this.modelConfig.provider,
          this.modelConfig.model,
          inputTokenEstimate,
          outputTokenEstimate
        );

        CognitionLogger.logResponseGeneration(
          this.personaId,
          this.personaName,
          this.modelConfig.provider,
          this.modelConfig.model,
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
              maxResource: this.modelConfig.maxTokens,
              percentCapacity: (aiResponse.text.length / this.modelConfig.maxTokens) * 100,
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

        // Phase 3.3.5: Clean and validate response (delegated to PersonaResponseValidator)
        const cleanResult = await this.responseValidator.cleanResponse(aiResponse.text.trim());
        if (cleanResult.wasCleaned && cleanResult.text.length === 0) {
          InferenceCoordinator.releaseSlot(this.personaId, provider);
          return { success: true, wasRedundant: true, storedToolResultIds: [] };
        }
        if (cleanResult.wasCleaned) {
          aiResponse.text = cleanResult.text;
        }
        if (cleanResult.thinking) {
          extractedThinking = cleanResult.thinking;
          this.log(`💭 ${this.personaName}: [thinking] ${truncate(cleanResult.thinking, 200)}`);
        }

        const hasToolCalls = !!(aiResponse.toolCalls && aiResponse.toolCalls.length > 0);
        const validation = await this.responseValidator.validate({
          responseText: aiResponse.text,
          hasToolCalls,
          conversationHistory: fullRAGContext.conversationHistory,
        });

        if (!validation.passed) {
          InferenceCoordinator.releaseSlot(this.personaId, provider);

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
                confidence: validation.confidence,
                reason: validation.reason,
                gatingModel: `rust-${validation.gate}`
              },
              { scope: EVENT_SCOPES.ROOM, scopeId: originalMessage.roomId }
            ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));

            getAIAudioBridge().setCognitiveState(this.personaId, 'idle').catch(() => {});
            Events.emit(DataDaemon.jtagContext!, PRESENCE_EVENTS.TYPING_STOP, {
              userId: this.personaId, displayName: this.personaName, roomId: originalMessage.roomId
            }).catch(() => {});
          }

          if (this.responseValidator.isHardFailure(validation.gate!)) {
            return { success: false, wasRedundant: false, storedToolResultIds: [], error: `garbage_output: ${validation.reason}` };
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

          // Refresh typing indicator during tool loop (3s decay timer would otherwise expire)
          if (DataDaemon.jtagContext) {
            Events.emit(DataDaemon.jtagContext, PRESENCE_EVENTS.TYPING_START, {
              userId: this.personaId, displayName: this.personaName, roomId: originalMessage.roomId
            }).catch(() => {});
          }

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
                  input: coerceParamsToSchema(tc.parameters ?? {}, toolSpecs, name),
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
            // ── XML path for non-native providers (DeepSeek, Candle, local) ──
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

          // Regenerate — force text response after 3 tool iterations.
          // Small/medium models loop on tools indefinitely without summarizing.
          // After 3 iterations (or at safety cap - 1), disable tools to force text.
          const forceText = toolIterations >= 3 || toolIterations >= SAFETY_MAX - 1;
          const regenerationTools = forceText ? undefined : request.tools;
          const regenerationToolChoice = forceText ? undefined : request.toolChoice;

          this.log(`🔧 ${this.personaName}: [AGENT-LOOP] Regenerating with ${messages.length} messages (tools ${forceText ? 'DISABLED — forcing text response' : 'enabled'})`);

          try {
            const regenerateStartTime = Date.now();
            const regeneratedResponse = await AIProviderDaemon.generateText({
              ...request,
              messages,
              tools: regenerationTools,
              toolChoice: regenerationToolChoice,
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

            // Update full response state — clean via validator
            const loopCleaned = await this.responseValidator.cleanResponse(regeneratedResponse.text?.trim() || '');
            if (loopCleaned.text.length > 0) {
              aiResponse.text = loopCleaned.text;
            } else if (regeneratedResponse.text?.trim()) {
              this.log(`⚠️ ${this.personaName}: [AGENT-LOOP] Regenerated response empty after cleaning — keeping previous text`);
            }
            aiResponse.toolCalls = regeneratedResponse.toolCalls ?? undefined;
            aiResponse.content = regeneratedResponse.content ?? undefined;
            aiResponse.finishReason = regeneratedResponse.finishReason;

            this.log(`✅ ${this.personaName}: [AGENT-LOOP] Got response (${aiResponse.text.length} chars, toolCalls: ${aiResponse.toolCalls?.length ?? 0})`);

            // If we forced text (tools disabled), break — don't let the parser
            // re-detect tool-call-like text and continue the loop
            if (forceText) {
              this.log(`✅ ${this.personaName}: [AGENT-LOOP] Forced text response after ${toolIterations} iteration(s), stopping`);
              break;
            }
          } catch (regenerateError) {
            const errorMsg = regenerateError instanceof Error ? regenerateError.message : String(regenerateError);
            this.log(`❌ ${this.personaName}: [AGENT-LOOP] Regeneration failed: ${errorMsg}`);
            aiResponse.text = (await this.toolExecutor.parseResponse(aiResponse.text)).cleanedText;
            break;
          }
        }

        if (toolIterations >= SAFETY_MAX) {
          this.log(`⚠️  ${this.personaName}: [AGENT-LOOP] Hit safety cap (${SAFETY_MAX}), stopping`);
        }
        // Always strip any remaining tool call text from the final response.
        // Models may embed tool calls in text even after forced-text regeneration.
        if (toolIterations > 0 && aiResponse.text) {
          const finalCleaned = await this.toolExecutor.parseResponse(aiResponse.text);
          if (finalCleaned.toolCalls.length > 0) {
            this.log(`🧹 ${this.personaName}: [AGENT-LOOP] Stripped ${finalCleaned.toolCalls.length} residual tool call(s) from final response`);
            aiResponse.text = finalCleaned.cleanedText;
          }
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
          this.modelConfig.provider,
          this.modelConfig.model,
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

          // Return avatar to idle on error
          getAIAudioBridge().setCognitiveState(this.personaId, 'idle').catch(() => {});

          // Clear typing indicator
          Events.emit(DataDaemon.jtagContext!, PRESENCE_EVENTS.TYPING_STOP, {
            userId: this.personaId, displayName: this.personaName, roomId: originalMessage.roomId
          }).catch(() => {});
        }

        // Log error to AI decisions log
        AIDecisionLogger.logError(this.personaName, 'AI generation (PHASE 3.3)', errorMessage);

        // Re-throw to be caught by outer try-catch
        throw error;
      }

      // 🔧 SUB-PHASE 3.5: Create and post response
      // Guard: never post empty messages (provider returned blank completion)
      if (!aiResponse.text.trim()) {
        this.log(`⚠️ ${this.personaName}: [PHASE 3.5] Empty response from AI provider — skipping post`);
        return { success: false, error: 'Empty response from provider', storedToolResultIds: [] };
      }
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
      if (extractedThinking) {
        responseMessage.metadata = { ...responseMessage.metadata, source: 'bot' as const, thinking: extractedThinking };
      }

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
      const postedEntity = await ORM.store(ChatMessageEntity.collection, responseMessage, false, 'default');
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

      // 🧬 CONTINUOUS LEARNING: Capture interaction for training data accumulation
      // Fire-and-forget — domain classification + quality scoring are Rust IPC calls
      // that DON'T affect the user-visible response. Previously these 2 awaited IPC
      // calls blocked the post-response path by 10-50ms each under load.
      if (this.trainingAccumulator) {
        const inputText = originalMessage.content.text;
        const outputText = aiResponse.text.trim();
        const accumulator = this.trainingAccumulator;
        const bridge = this.rustCognitionBridge;
        const fallbackDomain = this.inferTrainingDomain(originalMessage);

        // Entire classify → score → capture pipeline runs off the critical path
        (async () => {
          let domain = fallbackDomain;
          let qualityRating: number | undefined;
          if (bridge) {
            try {
              const classification = await bridge.classifyDomain(inputText);
              domain = classification.domain;
              bridge.recordActivity(domain, true).catch(() => {});
              qualityRating = (await bridge.scoreInteraction(inputText, outputText)).score;
            } catch { /* fallback domain already set */ }
          }
          await accumulator.captureInteraction({
            roleId: this.personaId,
            personaId: this.personaId,
            domain,
            input: inputText,
            output: outputText,
            qualityRating,
          });
        })().catch(err => this.log(`⚠️ Failed to capture interaction for training: ${err}`));
      }

      // 🧬 FITNESS TRACKING: Record inference result for genome natural selection
      if (this.genome) {
        const activeAdapter = this.genome.getCurrentAdapter();
        const layerId = activeAdapter?.getLayerId();
        if (layerId) {
          const inferenceLatency = Date.now() - generateStartTime;
          FitnessTracker.instance.recordInference(layerId, {
            success: true,
            latency: inferenceLatency,
          });
        }
      }

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

        // Return avatar to idle after posting
        getAIAudioBridge().setCognitiveState(this.personaId, 'idle').catch(() => {});

        // Clear typing indicator
        Events.emit(DataDaemon.jtagContext!, PRESENCE_EVENTS.TYPING_STOP, {
          userId: this.personaId, displayName: this.personaName, roomId: originalMessage.roomId
        }).catch(() => {});
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

        // Return avatar to idle on error
        getAIAudioBridge().setCognitiveState(this.personaId, 'idle').catch(() => {});

        // Clear typing indicator
        Events.emit(DataDaemon.jtagContext!, PRESENCE_EVENTS.TYPING_STOP, {
          userId: this.personaId, displayName: this.personaName, roomId: originalMessage.roomId
        }).catch(() => {});
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
  /**
   * Infer the training domain from message content.
   * Used to categorize captured interactions for domain-specific fine-tuning.
   */
  private inferTrainingDomain(message: ProcessableMessage): string {
    const text = message.content.text;

    // Messages containing code blocks → 'code'
    if (text.includes('```') || text.includes('function ') || text.includes('import ') || text.includes('const ')) {
      return 'code';
    }

    // Messages in academy-related rooms → 'teaching'
    // (Room name isn't directly available, but we can check metadata or keywords)
    if (text.toLowerCase().includes('teach') || text.toLowerCase().includes('learn') || text.toLowerCase().includes('exam')) {
      return 'teaching';
    }

    // Default: conversation
    return 'conversation';
  }

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
