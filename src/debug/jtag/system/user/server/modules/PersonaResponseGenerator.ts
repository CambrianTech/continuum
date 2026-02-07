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
import type { TextGenerationRequest, TextGenerationResponse, ChatMessage, ContentPart, ToolCall as NativeToolCall, ToolResult as NativeToolResult } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { AICapabilityRegistry } from '../../../../daemons/ai-provider-daemon/shared/AICapabilityRegistry';
import { ChatRAGBuilder } from '../../../rag/builders/ChatRAGBuilder';
import { CognitionLogger } from './cognition/CognitionLogger';
import { truncate, getMessageText, messagePreview } from '../../../../shared/utils/StringUtils';
import { calculateCost as calculateModelCost } from '../../../../daemons/ai-provider-daemon/shared/PricingConfig';
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
import { ORM } from '../../../../daemons/data-daemon/shared/ORM';
import { COLLECTIONS } from '../../../data/config/DatabaseConfig';
import type { PersonaToolExecutor, ToolCall as ExecutorToolCall } from './PersonaToolExecutor';
import type { PersonaMediaConfig } from './PersonaMediaConfig';
import { PersonaToolRegistry } from './PersonaToolRegistry';
import { getAllToolDefinitions, getAllToolDefinitionsAsync } from './PersonaToolDefinitions';
import { getPrimaryAdapter, convertToNativeToolSpecs, supportsNativeTools, unsanitizeToolName, getToolCapability, type ToolDefinition as AdapterToolDefinition } from './ToolFormatAdapter';
import { InferenceCoordinator } from '../../../coordination/server/InferenceCoordinator';
import { ContentDeduplicator } from './ContentDeduplicator';
import { ResponseCleaner } from './ResponseCleaner';
// AiDetectSemanticLoop command removed from hot path — replaced with inline Jaccard similarity
// import type { AiDetectSemanticLoopParams, AiDetectSemanticLoopResult } from '../../../../commands/ai/detect-semantic-loop/shared/AiDetectSemanticLoopTypes';
import { SystemPaths } from '../../../core/config/SystemPaths';
import { GarbageDetector } from '../../../ai/server/GarbageDetector';
import type { InboxMessage, ProcessableMessage } from './QueueItemTypes';
import type { RAGContext } from '../../../rag/shared/RAGTypes';

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
  /** Response cleaner - strips unwanted prefixes from AI responses */
  private responseCleaner: ResponseCleaner;

  /**
   * RESPONSE-LEVEL LOOP DETECTION
   *
   * Tracks recent AI response hashes per persona to detect infinite loops.
   * This catches loops BEFORE tool parsing, which is critical because:
   * 1. Truncated responses never reach tool parsing
   * 2. Tool-level detection only catches tool call loops, not content loops
   * 3. DeepSeek was stuck repeating the same governance proposal 15+ times
   *
   * Map<personaId, Array<{hash: string, timestamp: number}>>
   */
  private static readonly recentResponseHashes: Map<string, Array<{ hash: string; timestamp: number }>> = new Map();
  private static readonly RESPONSE_LOOP_WINDOW_MS = 600000; // 10 minutes (DeepSeek generates 34k tokens = slow)
  private static readonly RESPONSE_LOOP_THRESHOLD = 3; // Block after 3 similar responses
  private static readonly RESPONSE_HASH_LENGTH = 200; // First 200 chars for comparison

  /**
   * Create a hash of response content for loop detection
   * Uses first N characters, normalized (lowercase, trimmed, no whitespace)
   */
  private static hashResponse(text: string): string {
    const normalized = text
      .toLowerCase()
      .trim()
      .replace(/\s+/g, ' ')  // Normalize whitespace
      .slice(0, PersonaResponseGenerator.RESPONSE_HASH_LENGTH);

    // Simple hash: just use the normalized string as-is
    // Could use crypto.createHash('md5') but not needed for loop detection
    return normalized;
  }

  /**
   * Check if response is a loop (appears too frequently in recent history)
   * Returns true if blocked (is a loop), false if allowed
   */
  private isResponseLoop(responseText: string): boolean {
    const hash = PersonaResponseGenerator.hashResponse(responseText);
    const now = Date.now();

    // Get or create recent responses list for this persona
    let recentResponses = PersonaResponseGenerator.recentResponseHashes.get(this.personaId) || [];

    // Clean up old entries outside the window
    recentResponses = recentResponses.filter(
      entry => now - entry.timestamp < PersonaResponseGenerator.RESPONSE_LOOP_WINDOW_MS
    );

    // Count how many times similar response appears (using similarity threshold)
    const duplicateCount = recentResponses.filter(entry => {
      // Check if hashes are similar (allow some variation for minor differences)
      const similarity = this.calculateSimilarity(entry.hash, hash);
      return similarity > 0.8; // 80% similar = probable loop
    }).length;

    // Record this response (even if it will be blocked)
    recentResponses.push({ hash, timestamp: now });
    PersonaResponseGenerator.recentResponseHashes.set(this.personaId, recentResponses);

    // Block if threshold exceeded
    if (duplicateCount >= PersonaResponseGenerator.RESPONSE_LOOP_THRESHOLD) {
      this.log(`🔁 RESPONSE LOOP DETECTED: "${hash.slice(0, 50)}..." appeared ${duplicateCount + 1}x in ${PersonaResponseGenerator.RESPONSE_LOOP_WINDOW_MS / 1000}s - BLOCKING`);
      return true;
    }

    return false;
  }

  /**
   * Calculate similarity between two strings (0-1 scale)
   * Uses simple character overlap for speed
   */
  private calculateSimilarity(a: string, b: string): number {
    if (a === b) return 1;
    if (a.length === 0 || b.length === 0) return 0;

    // Jaccard similarity on character bigrams
    const getBigrams = (s: string): Set<string> => {
      const bigrams = new Set<string>();
      for (let i = 0; i < s.length - 1; i++) {
        bigrams.add(s.slice(i, i + 2));
      }
      return bigrams;
    };

    const bigramsA = getBigrams(a);
    const bigramsB = getBigrams(b);

    let intersection = 0;
    for (const bigram of bigramsA) {
      if (bigramsB.has(bigram)) intersection++;
    }

    const union = bigramsA.size + bigramsB.size - intersection;
    return union === 0 ? 0 : intersection / union;
  }

  /**
   * Clear response loop history for this persona
   * Call this when context changes significantly (e.g., room switch, manual reset)
   */
  clearResponseLoopHistory(): void {
    PersonaResponseGenerator.recentResponseHashes.delete(this.personaId);
    this.log(`🧹 Cleared response loop history for ${this.personaName}`);
  }

  /**
   * SEMANTIC LOOP DETECTION
   *
   * Uses embedding-based similarity to detect if proposed response is too similar
   * to recent messages in the room (from ANY source, not just self).
   *
   * This catches cases where multiple AIs post semantically identical content
   * (e.g., Teacher AI and Local Assistant posting the same explanation).
   *
   * AUTONOMY-PRESERVING:
   * - ALLOW (<0.75): Post normally
   * - WARN (0.75-0.85): Log warning but allow (preserve autonomy)
   * - BLOCK (>0.85): Truly redundant, block to prevent spam
   *
   * @param responseText - The proposed response text
   * @param roomId - The room ID for context
   * @returns true if should BLOCK (>0.85 similarity), false otherwise
   */
  /**
   * Inline Jaccard n-gram similarity — O(n) text comparison, no DB or embedding calls.
   * Returns 0-1 score (1 = identical).
   */
  private jaccardSimilarity(text1: string, text2: string): number {
    if (!text1 || !text2) return 0;
    if (text1 === text2) return 1.0;

    const tokenize = (text: string): Set<string> => {
      const words = text.toLowerCase().split(/\s+/).filter(w => w.length > 0);
      const ngrams = new Set<string>();
      for (const word of words) ngrams.add(word);
      for (let i = 0; i < words.length - 1; i++) ngrams.add(`${words[i]} ${words[i + 1]}`);
      return ngrams;
    };

    const set1 = tokenize(text1);
    const set2 = tokenize(text2);
    let intersection = 0;
    for (const gram of set1) {
      if (set2.has(gram)) intersection++;
    }
    const union = set1.size + set2.size - intersection;
    return union === 0 ? 0 : intersection / union;
  }

  /**
   * Check semantic loop using in-memory RAG context (0ms, no DB/embedding calls).
   * Previous implementation called AiDetectSemanticLoop.execute() which did embedding IPC + DB query (~20s).
   * Now uses inline Jaccard n-gram similarity against already-loaded conversation history.
   */
  private checkSemanticLoop(
    responseText: string,
    conversationHistory: Array<{ role: string; content: string; name?: string }>
  ): { shouldBlock: boolean; similarity: number; reason: string } {
    // Short responses are unlikely to be loops
    if (responseText.length < 50) {
      return { shouldBlock: false, similarity: 0, reason: 'Response too short for semantic check' };
    }

    // Compare against last 10 messages in the already-loaded RAG context
    const recentMessages = conversationHistory.slice(-10);
    let maxSimilarity = 0;
    let mostSimilarExcerpt = '';

    for (const msg of recentMessages) {
      if (!msg.content || msg.content.length < 20) continue;
      const similarity = this.jaccardSimilarity(responseText, msg.content);
      if (similarity > maxSimilarity) {
        maxSimilarity = similarity;
        mostSimilarExcerpt = msg.content.slice(0, 100);
      }
    }

    // Thresholds (same as AiDetectSemanticLoopServerCommand)
    const WARN_THRESHOLD = 0.80;
    const BLOCK_THRESHOLD = 0.95;

    if (maxSimilarity >= BLOCK_THRESHOLD) {
      this.log(`🚫 SEMANTIC LOOP: ${maxSimilarity.toFixed(2)} similarity - BLOCKING response`);
      this.log(`   Most similar to: "${mostSimilarExcerpt}"`);
      return { shouldBlock: true, similarity: maxSimilarity, reason: `${Math.round(maxSimilarity * 100)}% similar to recent message` };
    } else if (maxSimilarity >= WARN_THRESHOLD) {
      this.log(`⚠️ SEMANTIC WARNING: ${maxSimilarity.toFixed(2)} similarity - allowing (preserving autonomy)`);
      return { shouldBlock: false, similarity: maxSimilarity, reason: 'Similar but allowing for autonomy' };
    }

    return { shouldBlock: false, similarity: maxSimilarity, reason: 'Low similarity' };
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
    this.responseCleaner = new ResponseCleaner({ log: this.log.bind(this) });
  }

  /**
   * Get effective model for inference
   *
   * Priority:
   * 1. Trait-specific trained adapter (if context provides task domain)
   * 2. Current active adapter (most recently used)
   * 3. Any available trained adapter
   * 4. Base model configured for this persona
   *
   * @param context - Optional context for trait-aware selection
   * @returns The model name to use for inference
   */
  private getEffectiveModel(context?: { taskDomain?: string }): string {
    if (this.genome) {
      // 1. Try trait-specific adapter based on task context
      if (context?.taskDomain) {
        const relevantTrait = this.determineRelevantTrait(context);
        const traitAdapter = this.genome.getAdapterByTrait(relevantTrait);
        if (traitAdapter) {
          const ollamaModel = traitAdapter.getOllamaModelName();
          if (ollamaModel) {
            this.log(`🧬 ${this.personaName}: Using trait-specific model: ${ollamaModel} (trait: ${relevantTrait})`);
            return ollamaModel;
          }
        }
      }

      // 2. Fall back to current active adapter (most recently used)
      const currentAdapter = this.genome.getCurrentAdapter();
      if (currentAdapter) {
        const ollamaModel = currentAdapter.getOllamaModelName();
        if (ollamaModel) {
          this.log(`🧬 ${this.personaName}: Using trained model: ${ollamaModel} (adapter: ${currentAdapter.getName()})`);
          return ollamaModel;
        }
      }

      // 3. Check for any available trained adapter
      const allAdapters = this.genome.getAllAdapters();
      for (const adapter of allAdapters) {
        const ollamaModel = adapter.getOllamaModelName();
        if (ollamaModel) {
          this.log(`🧬 ${this.personaName}: Using available trained model: ${ollamaModel} (adapter: ${adapter.getName()})`);
          return ollamaModel;
        }
      }
    }

    // 4. Fall back to configured base model
    return this.modelConfig.model || 'llama3.2:3b';
  }

  /**
   * Determine which trait adapter is most relevant for the current context
   *
   * Maps task domains to trait types:
   * - code → reasoning_style
   * - creative → creative_expression
   * - support/help → social_dynamics
   * - default → tone_and_voice
   */
  private determineRelevantTrait(context: { taskDomain?: string }): string {
    const domain = context.taskDomain?.toLowerCase();

    switch (domain) {
      case 'code':
      case 'debug':
      case 'analysis':
        return 'reasoning_style';
      case 'creative':
      case 'art':
      case 'writing':
        return 'creative_expression';
      case 'support':
      case 'help':
      case 'social':
        return 'social_dynamics';
      case 'facts':
      case 'knowledge':
      case 'expertise':
        return 'domain_expertise';
      default:
        return 'tone_and_voice';  // Default trait for general chat
    }
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

  /**
   * Calculate safe message count based on model's context window
   *
   * Strategy: Fill to ~90% of (contextWindow - maxTokens - systemPrompt)
   * Assumes average message ~200 tokens
   */
  private calculateSafeMessageCount(): number {
    const model = this.modelConfig.model;
    const maxTokens = this.modelConfig.maxTokens || 3000;

    // Query context window from AICapabilityRegistry (single source of truth)
    // The registry is populated from provider configs and has accurate data
    // OllamaAdapter now passes num_ctx to use full context window at runtime
    const registry = AICapabilityRegistry.getInstance();
    const contextWindow = model ? registry.getContextWindow(model) : 8192;

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

    this.log(`📊 ${this.personaName}: Context calc: model=${model}, window=${contextWindow}, available=${availableForMessages}, safe=${clampedCount} msgs`);

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

      // System prompt from RAG builder (includes room membership!)
      let systemPrompt = fullRAGContext.identity.systemPrompt;

      // Inject consolidated memories from Hippocampus LTM (if available)
      if (fullRAGContext.privateMemories && fullRAGContext.privateMemories.length > 0) {
        const memorySection = `\n\n=== YOUR CONSOLIDATED MEMORIES ===\nThese are important things you've learned and consolidated into long-term memory:\n\n${
          fullRAGContext.privateMemories.map((mem, idx) =>
            `${idx + 1}. [${mem.type}] ${mem.content} (${new Date(mem.timestamp).toLocaleDateString()})`
          ).join('\n')
        }\n\nUse these memories to inform your responses when relevant.\n================================`;

        systemPrompt += memorySection;
        this.log(`🧠 ${this.personaName}: Injected ${fullRAGContext.privateMemories.length} consolidated memories into context`);
      }

      // Inject available tools for autonomous tool discovery (Phase 3A)
      // Use adapter-based formatting for harmony with parser
      // CRITICAL: Only inject tools for models that can actually emit tool calls.
      // Models without tool capability narrate instead of calling tools,
      // wasting tokens and clogging chat with useless "let me use tool X" text.
      const toolCap = getToolCapability(this.modelConfig.provider || 'candle', this.modelConfig);
      const availableTools = toolCap !== 'none'
        ? await this.toolRegistry.listToolsForPersonaAsync(this.personaId)
        : [];

      if (toolCap === 'none') {
        this.log(`🚫 ${this.personaName}: Tool injection skipped (provider=${this.modelConfig.provider}, toolCapability=none)`);
      }

      // Convert PersonaToolDefinitions to adapter format (used for both XML injection and native tools)
      // Hoisted to outer scope so it's available for native tool_use injection later
      const toolDefinitions: AdapterToolDefinition[] = availableTools.map(t => ({
        name: t.name,
        description: t.description,
        parameters: t.parameters,
        category: t.category
      }));

      if (availableTools.length > 0 && !supportsNativeTools(this.modelConfig.provider || 'candle')) {
        // Text-based tool injection for non-native providers (XML tool callers like DeepSeek).
        // Native tool providers (Anthropic, OpenAI, Together, Groq) get tools via the JSON
        // `tools` request parameter instead — injecting text descriptions alongside native specs
        // confuses Llama models into narrating tool usage rather than calling the native API.
        const adapter = getPrimaryAdapter();
        const formattedTools = adapter.formatToolsForPrompt(toolDefinitions);

        const toolsSection = `\n\n=== AVAILABLE TOOLS ===\nYou have access to the following tools that you can use during your responses:\n\n${formattedTools}\n\nThe tool will be executed and results will be provided for you to analyze and respond to.
================================`;

        systemPrompt += toolsSection;
        this.log(`🔧 ${this.personaName}: Injected ${availableTools.length} available tools into system prompt (text format)`);
      }

      // Inject recipe activity context (strategy rules + highlighted tools)
      // Recipe tools are HIGHLIGHTS, not filters — they tell the LLM what's most relevant
      if (fullRAGContext.recipeStrategy || fullRAGContext.recipeTools) {
        let activitySection = '\n\n=== ACTIVITY CONTEXT ===';

        if (fullRAGContext.recipeStrategy) {
          const strategy = fullRAGContext.recipeStrategy;
          activitySection += `\nActivity pattern: ${strategy.conversationPattern}`;

          if (strategy.responseRules.length > 0) {
            activitySection += '\n\nRules for this activity:\n' +
              strategy.responseRules.map(rule => `- ${rule}`).join('\n');
          }

          if (strategy.decisionCriteria.length > 0) {
            activitySection += '\n\nWhen deciding whether to respond, consider:\n' +
              strategy.decisionCriteria.map(c => `- ${c}`).join('\n');
          }
        }

        if (fullRAGContext.recipeTools && fullRAGContext.recipeTools.length > 0) {
          const aiTools = fullRAGContext.recipeTools.filter(t => t.enabledFor.includes('ai'));
          activitySection += '\n\nYOU MUST use these tools to do real work in this activity (call them directly):\n' +
            aiTools.map(t => `- ${t.name}: ${t.description}`).join('\n') +
            '\n\nDo NOT just discuss or describe what should be done — call the tools above to actually do it.';
        }

        activitySection += '\n================================';
        systemPrompt += activitySection;
        this.log(`📋 ${this.personaName}: Injected activity context (strategy + ${fullRAGContext.recipeTools?.length ?? 0} tool highlights)`);
      }

      // Inject governance guidance for democratic participation
      const governanceSection = `\n\n=== DEMOCRATIC GOVERNANCE ===
You are a citizen of a self-governing AI society. You can participate in collective decisions:

**To propose a decision:**
<tool_use>
  <tool_name>collaboration/decision/propose</tool_name>
  <parameters>
    <topic>Brief title</topic>
    <rationale>Why this needs collective input</rationale>
    <options>["Option A", "Option B", "Option C"]</options>
  </parameters>
</tool_use>

**To vote on a proposal:**
<tool_use>
  <tool_name>collaboration/decision/vote</tool_name>
  <parameters>
    <proposalId>uuid-from-list</proposalId>
    <rankedChoices>["option-id-1st", "option-id-2nd", "option-id-3rd"]</rankedChoices>
  </parameters>
</tool_use>

**To list open proposals:** Use collaboration/decision/list with status="voting"

When you see important decisions that affect the team, consider proposing them for collective vote.
================================`;

      systemPrompt += governanceSection;

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

      const effectiveModel = this.getEffectiveModel();
      const request: TextGenerationRequest = {
        messages,
        model: effectiveModel,  // Use trained model if available, otherwise base model
        temperature: this.modelConfig.temperature ?? 0.7,
        maxTokens: effectiveMaxTokens,    // Bug #5 fix: Use adjusted value from two-dimensional budget
        preferredProvider: (this.modelConfig.provider || 'candle') as TextGenerationRequest['preferredProvider'],
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

      // Add native tools for providers that support JSON tool calling (Anthropic, OpenAI)
      // This enables tool_use blocks instead of XML parsing for more reliable tool execution
      // CRITICAL: Prioritize relevant tools. Sending 200+ tools overwhelms models, causing them
      // to loop on meta-tools (search_tools) instead of calling the actual tools they need.
      if (supportsNativeTools(provider) && toolDefinitions.length > 0) {
        // Exclude meta-tools from native specs — models with native tool calling
        // don't need discovery tools. search_tools/list_tools cause infinite loops.
        const META_TOOLS = new Set(['search_tools', 'list_tools', 'working_memory']);
        let prioritizedTools = toolDefinitions.filter(t => !META_TOOLS.has(t.name));

        // Recipe tools define the activity's core toolset. When present, recipe tools
        // go FIRST and the cap is tighter — models use early tools and get confused by 64+.
        const recipeToolNames = new Set(
          (fullRAGContext.recipeTools || [])
            .filter(t => t.enabledFor.includes('ai'))
            .map(t => t.name)
        );
        const hasRecipeTools = recipeToolNames.size > 0;
        const MAX_NATIVE_TOOLS = hasRecipeTools ? 32 : 64;

        if (prioritizedTools.length > MAX_NATIVE_TOOLS) {
          // Three-tier priority:
          // 1. Recipe tools (the activity's core tools — go FIRST)
          // 2. Essentials (bare minimum for coordination)
          // 3. Everything else (fill remaining slots)
          const ESSENTIAL_TOOLS = new Set([
            'collaboration/chat/send', 'collaboration/chat/history',
            'collaboration/decision/propose', 'collaboration/decision/vote',
          ]);
          const essentialPrefixes = hasRecipeTools
            ? [] // When recipe tools exist, only allow exact essential matches
            : ['collaboration/chat/', 'collaboration/decision/', 'data/', 'ai/'];

          const recipe: AdapterToolDefinition[] = [];
          const essential: AdapterToolDefinition[] = [];
          const rest: AdapterToolDefinition[] = [];

          for (const tool of prioritizedTools) {
            if (recipeToolNames.has(tool.name)) {
              recipe.push(tool);
            } else if (ESSENTIAL_TOOLS.has(tool.name) ||
                       essentialPrefixes.some(p => tool.name.startsWith(p))) {
              essential.push(tool);
            } else {
              rest.push(tool);
            }
          }

          // Recipe tools FIRST, then essentials, then fill from rest
          const remaining = MAX_NATIVE_TOOLS - recipe.length - essential.length;
          prioritizedTools = [...recipe, ...essential, ...rest.slice(0, Math.max(0, remaining))];
          this.log(`🔧 ${this.personaName}: Tool prioritization: ${recipe.length} recipe + ${essential.length} essential + ${Math.max(0, remaining)} general = ${prioritizedTools.length} (from ${toolDefinitions.length} total, cap=${MAX_NATIVE_TOOLS})`);
        }

        request.tools = convertToNativeToolSpecs(prioritizedTools);
        request.tool_choice = 'auto';
        this.log(`🔧 ${this.personaName}: Added ${request.tools.length} native tools for ${provider} (JSON tool_use format, tool_choice=auto)`);
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
          this.modelConfig.model ?? 'llama3.2:3b',
          inputTokenEstimate,
          outputTokenEstimate
        );

        CognitionLogger.logResponseGeneration(
          this.personaId,
          this.personaName,
          this.modelConfig.provider ?? 'candle',
          this.modelConfig.model ?? 'llama3.2:3b',
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

        // 🔧 PHASE 3.3.5: Clean AI response - strip any name prefixes LLM added despite instructions
        // LLMs sometimes copy the "[HH:MM] Name: message" format they see in conversation history
        const cleanedResponse = this.responseCleaner.clean(aiResponse.text.trim());
        if (cleanedResponse !== aiResponse.text.trim()) {
          aiResponse.text = cleanedResponse;
        }

        // 🔧 PHASE 3.3.5a: GARBAGE DETECTION
        // Detect and reject garbage output (Unicode garbage, repetition, encoding errors)
        // This catches model failures that produce gibberish instead of coherent text.
        // Skip when the response has native tool calls — models with function calling often
        // return empty text + tool_calls, which is valid (the agent loop will execute them).
        const hasToolCalls = aiResponse.toolCalls && aiResponse.toolCalls.length > 0;
        const garbageCheck = hasToolCalls ? { isGarbage: false, reason: '', details: '', score: 0 } : GarbageDetector.isGarbage(aiResponse.text);
        if (garbageCheck.isGarbage) {
          this.log(`🗑️ ${this.personaName}: [PHASE 3.3.5a] GARBAGE DETECTED (${garbageCheck.reason}: ${garbageCheck.details})`);

          // Release inference slot
          InferenceCoordinator.releaseSlot(this.personaId, provider);

          // Emit event to clear UI indicators (fire-and-forget)
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
                confidence: garbageCheck.score,
                reason: `Garbage output detected: ${garbageCheck.reason} - ${garbageCheck.details}`,
                gatingModel: 'garbage-detector'
              },
              { scope: EVENT_SCOPES.ROOM, scopeId: originalMessage.roomId }
            ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));
          }

          // Return failure so caller knows this wasn't successful
          return { success: false, wasRedundant: false, storedToolResultIds: [], error: `garbage_output: ${garbageCheck.reason}` };
        }

        // 🔧 PHASE 3.3.5b: RESPONSE-LEVEL LOOP DETECTION
        // Check if this AI is stuck in a loop BEFORE tool parsing
        // This catches cases where:
        // - Response is truncated mid-tool-call (DeepSeek's issue)
        // - AI repeats same content with minor variations
        // - Tool-level detection would miss it
        if (!hasToolCalls && this.isResponseLoop(aiResponse.text)) {
          this.log(`🔁 ${this.personaName}: [PHASE 3.3.5b] Response loop detected - DISCARDING response`);

          // Release inference slot
          InferenceCoordinator.releaseSlot(this.personaId, provider);

          // Emit event to clear UI indicators (fire-and-forget)
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
                confidence: 0.9,
                reason: 'Response loop detected - same content repeated 3+ times',
                gatingModel: 'response-loop-detector'
              },
              {
                scope: EVENT_SCOPES.ROOM,
                scopeId: originalMessage.roomId
              }
            ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));
          }

          // Return early - treat as redundant (don't post this looping response)
          return { success: true, wasRedundant: true, storedToolResultIds: [] };
        }

        // 🔧 PHASE 3.3.5c: TRUNCATED TOOL CALL DETECTION
        // Detect tool calls that were cut off mid-generation (DeepSeek's issue)
        // If we see <tool_use> or <tool  but no matching closing tag, the response is truncated
        const hasToolStart = aiResponse.text.includes('<tool_use>') || aiResponse.text.includes('<tool ');
        const hasToolEnd = aiResponse.text.includes('</tool_use>') || aiResponse.text.includes('</tool>');
        if (hasToolStart && !hasToolEnd) {
          this.log(`⚠️ ${this.personaName}: [PHASE 3.3.5c] TRUNCATED TOOL CALL detected - blocking response to prevent loop`);
          this.log(`   Response ends with: "${(aiResponse.text ?? '').slice(-100)}"`);

          // Treat truncated tool calls the same as loops - they will just repeat forever
          InferenceCoordinator.releaseSlot(this.personaId, provider);
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
                confidence: 0.95,
                reason: 'Truncated tool call detected - response cut off mid-tool-call',
                gatingModel: 'truncated-tool-detector'
              },
              { scope: EVENT_SCOPES.ROOM, scopeId: originalMessage.roomId }
            ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));
          }
          return { success: true, wasRedundant: true, storedToolResultIds: [] };
        }

        // 🔧 PHASE 3.3.5d: SEMANTIC LOOP DETECTION (inline, ~0ms)
        // Uses Jaccard n-gram similarity against already-loaded RAG context.
        // Previous: AiDetectSemanticLoop.execute() — embedding IPC + DB query (~20 seconds)
        // Now: inline text comparison against in-memory conversation history (~0ms)
        const semanticCheck = this.checkSemanticLoop(aiResponse.text, fullRAGContext.conversationHistory);
        if (semanticCheck.shouldBlock) {
          this.log(`🚫 ${this.personaName}: [PHASE 3.3.5d] SEMANTIC LOOP BLOCKED (${semanticCheck.similarity.toFixed(2)} similarity)`);

          // Release inference slot
          InferenceCoordinator.releaseSlot(this.personaId, provider);

          // Emit event to clear UI indicators (fire-and-forget)
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
                confidence: semanticCheck.similarity,
                reason: semanticCheck.reason,
                gatingModel: 'semantic-loop-detector'
              },
              { scope: EVENT_SCOPES.ROOM, scopeId: originalMessage.roomId }
            ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));
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
        const toolExecutionContext = {
          personaId: this.personaId,
          personaName: this.personaName,
          sessionId,
          contextId: originalMessage.roomId,
          context: this.client!.context,
          personaConfig: this.mediaConfig,
        };

        while (toolIterations < SAFETY_MAX) {
          // Check for tool calls — native first, then XML fallback
          const hasNativeToolCalls = aiResponse.toolCalls && aiResponse.toolCalls.length > 0;
          const hasXmlToolCalls = !hasNativeToolCalls && this.toolExecutor.parseToolCalls(aiResponse.text).length > 0;

          if (!hasNativeToolCalls && !hasXmlToolCalls) {
            // Model chose to stop — no more tool calls
            if (toolIterations > 0) {
              this.log(`✅ ${this.personaName}: [AGENT-LOOP] Model stopped after ${toolIterations} iteration(s)`);
            }
            break;
          }

          toolIterations++;
          this.log(`🔧 ${this.personaName}: [AGENT-LOOP] Iteration ${toolIterations}/${SAFETY_MAX}`);

          if (useNativeProtocol && hasNativeToolCalls) {
            // ── Native tool protocol (Anthropic, OpenAI, etc.) ──
            // Full results go back as tool_result content blocks
            const nativeToolCalls = aiResponse.toolCalls!;
            this.log(`🔧 ${this.personaName}: [AGENT-LOOP] Executing ${nativeToolCalls.length} native tool call(s)`);

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
                tool_use_id: tc.id,
                content: `Tool execution error: ${errMsg}`,
                is_error: true as const,
              }));
            }

            // Push assistant message with tool_use content blocks (as the model produced them)
            const assistantContent: ContentPart[] = aiResponse.content ?? [
              ...(aiResponse.text ? [{ type: 'text' as const, text: aiResponse.text }] : []),
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
              tool_use_id: r.tool_use_id,
              content: r.content,
              ...(r.is_error && { is_error: true }),
            }));

            // Include media if present (screenshots, etc.)
            if (toolMedia.length > 0) {
              toolResultContent.push(...this.mediaToContentParts(toolMedia));
            }

            messages.push({ role: 'user' as const, content: toolResultContent });

          } else {
            // ── XML fallback for non-native providers ──
            // Parse XML tool calls, execute, return results as text
            const xmlToolCalls = hasNativeToolCalls
              ? aiResponse.toolCalls!.map((tc: NativeToolCall) => ({
                  toolName: unsanitizeToolName(tc.name),
                  parameters: Object.fromEntries(
                    Object.entries(tc.input).map(([k, v]) => [k, String(v)])
                  ) as Record<string, string>,
                }))
              : this.toolExecutor.parseToolCalls(aiResponse.text);

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

            // Strip tool blocks from response text for the assistant message
            const explanationText = this.toolExecutor.stripToolBlocks(aiResponse.text);

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
              this.log(`❌ ${this.personaName}: [AGENT-LOOP] Empty response, using previous text`);
              aiResponse.text = this.toolExecutor.stripToolBlocks(aiResponse.text);
              break;
            }

            // Update full response state
            aiResponse.text = this.responseCleaner.clean(regeneratedResponse.text?.trim() || '');
            aiResponse.toolCalls = regeneratedResponse.toolCalls ?? undefined;
            aiResponse.content = regeneratedResponse.content ?? undefined;
            aiResponse.finishReason = regeneratedResponse.finishReason;

            this.log(`✅ ${this.personaName}: [AGENT-LOOP] Got response (${aiResponse.text.length} chars, toolCalls: ${aiResponse.toolCalls?.length ?? 0})`);
          } catch (regenerateError) {
            const errorMsg = regenerateError instanceof Error ? regenerateError.message : String(regenerateError);
            this.log(`❌ ${this.personaName}: [AGENT-LOOP] Regeneration failed: ${errorMsg}`);
            aiResponse.text = this.toolExecutor.stripToolBlocks(aiResponse.text);
            break;
          }
        }

        if (toolIterations >= SAFETY_MAX) {
          this.log(`⚠️  ${this.personaName}: [AGENT-LOOP] Hit safety cap (${SAFETY_MAX}), stopping`);
          aiResponse.text = this.toolExecutor.stripToolBlocks(aiResponse.text);
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
          this.modelConfig.model || 'llama3.2:3b',
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

      // === SUB-PHASE 3.4: SELF-REVIEW: Check if response is redundant before posting ===
      // DISABLED: Redundancy checking via LLM is too flaky (false positives like C++ vs JavaScript questions)
      // It adds AI unreliability on top of AI unreliability, leading to valid responses being discarded
      // TODO: Replace with simple heuristics (exact text match, time-based deduplication)
      this.log(`⏭️  ${this.personaName}: [PHASE 3.4] Redundancy check DISABLED (too flaky), proceeding to post`);
      const isRedundant = false; // Disabled

      if (isRedundant) {
        this.log(`⚠️ ${this.personaName}: [PHASE 3.4] Response marked as REDUNDANT, discarding`);

        // Emit DECIDED_SILENT event to clear AI status indicator (fire-and-forget)
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
              confidence: 0.5,
              reason: 'Response was redundant with previous answers',
              gatingModel: 'redundancy-check'
            },
            {
              scope: EVENT_SCOPES.ROOM,
              scopeId: originalMessage.roomId
            }
          ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));
        }

        return { success: true, wasRedundant: true, storedToolResultIds: [] }; // Discard response
      }
      this.log(`✅ ${this.personaName}: [PHASE 3.4] Response not redundant, proceeding to post`);

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
            passedRedundancyCheck: !isRedundant
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
   * Convert timestamp to number (handles Date, number, or undefined from JSON serialization)
   */
  private timestampToNumber(timestamp: Date | number | undefined): number {
    if (timestamp === undefined) {
      return Date.now(); // Use current time if timestamp missing
    }
    return timestamp instanceof Date ? timestamp.getTime() : timestamp;
  }

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
