/**
 * PersonaResponseGenerator — TS shim over the Rust cognition core.
 *
 * The cognitive verb ("this persona, given this message, produces this
 * response") now lives in Rust (continuum-core::persona::response::respond).
 * This shim is the TS-side contract that:
 *
 *   1. Applies dormancy / engagement gate (pre-flight, TS-only concern).
 *   2. Routes sentinel dispatch (complex multi-step tasks become sentinels
 *      instead of tool loops — orthogonal to cognition, stays TS).
 *   3. Builds the minimal RAG slice Rust needs (system prompt + recent
 *      history + known specialties) and calls cognitionPersonaRespond.
 *   4. Handles Silent|Spoke: Silent is logged + returned; Spoke runs the
 *      tool agent loop on the returned text and posts to chat.
 *   5. Emits UI events (POSTED / ERROR / typing / voice / stage) and
 *      captures training-data + fitness telemetry off the critical path.
 *
 * Out of scope for this PR (anvil's next rungs):
 *   - Tool agent loop migration to Rust.
 *   - Sentinel dispatch relocation.
 *   - Cloud-provider routing through Rust ai_provider.
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { UserEntity, ModelConfig } from '../../../data/entities/UserEntity';
import type { JTAGClient } from '../../../core/client/shared/JTAGClient';
import type { TextGenerationRequest, TextGenerationResponse, NativeToolSpec } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { ChatRAGBuilder } from '../../../rag/builders/ChatRAGBuilder';
import { getContextWindow, getInferenceSpeed } from '../../../shared/ModelContextWindows';
import { truncate, getMessageText, messagePreview } from '../../../../shared/utils/StringUtils';
import { AIDecisionLogger } from '../../../ai/server/AIDecisionLogger';
import { CoordinationDecisionLogger, type LogDecisionParams } from '../../../coordination/server/CoordinationDecisionLogger';
import { Events } from '../../../core/shared/Events';
import { EVENT_SCOPES } from '../../../events/shared/EventSystemConstants';
import { COGNITION_EVENTS, calculateSpeedScore, getStageStatus, type StageCompleteEvent } from '../../../conversation/shared/CognitionEventTypes';
import {
  AI_DECISION_EVENTS,
  type AIDecidedSilentEventData,
  type AIPostedEventData,
  type AIErrorEventData,
} from '../../../events/shared/AIDecisionEvents';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import type { PersonaToolExecutor } from './PersonaToolExecutor';
import type { PersonaMediaConfig } from './PersonaMediaConfig';
import { PersonaToolRegistry } from './PersonaToolRegistry';
import { getToolCapability, getModelFamily } from './ToolFormatAdapter';
import type { ProcessableMessage } from './QueueItemTypes';
import type { RAGContext } from '../../../rag/shared/RAGTypes';
import type { RustCognitionBridge } from './RustCognitionBridge';
import { FitnessTracker } from '../../../genome/server/FitnessTracker';
import { getAIAudioBridge } from '../../../voice/server/AIAudioBridge';
import { PRESENCE_EVENTS } from '../../../core/shared/EventConstants';
import { PersonaEngagementDecider, type DormancyState } from './PersonaEngagementDecider';
// PersonaAgentLoop / PersonaResponseValidator / PersonaPromptAssembler
// were the TS-side second-pass inference + retry loop on Rust
// personaRespond's output — duplicated work the Rust cognition crate
// already owns and bypassed the model's full context window via a TS
// maxTokens cap. Removed from this file's call path 2026-04-20; deleted
// entirely in the 0.5.1/0.5.2/0.5.4 cleanup sweep once the subgraph
// was confirmed closed (no live importers, no test refs). Tool calling
// continues through Rust cognition::tool_executor (0.5.3).
import { SentinelDispatchDecider } from '../../../sentinel/SentinelDispatchDecider';
import { SentinelDispatchCoordinator } from '../../../sentinel/SentinelDispatchCoordinator';
import { Commands } from '../../../core/shared/Commands';
import type { SentinelRunResult } from '../../../../commands/sentinel/run/shared/SentinelRunTypes';
import type { SocialSignals } from '../../../../shared/generated';
import type { PersonaResponse } from '../../../../shared/generated/cognition/PersonaResponse';
import type { PersonaRespondRequest } from '../../../../workers/continuum-core/bindings/modules/cognition';
import { inspect } from 'util';
import { createHash } from 'crypto';
import type { LLMMessage } from '../../../rag/shared/RAGTypes';

/**
 * Produce a stable UUID from an LLMMessage so Rust's analysis cache hits
 * across concurrent persona calls. Same content+name+timestamp → same id.
 * Hash is truncated to 16 bytes and reshaped as UUIDv4 (variant + version
 * bits set). Not a real UUIDv5 — we don't need a registered namespace —
 * just needs to parse as Uuid on the Rust side.
 */
function synthesizeDeterministicUuid(msg: LLMMessage): string {
  const key = `${msg.role}|${msg.name ?? ''}|${msg.timestamp ?? 0}|${msg.content}`;
  const digest = createHash('sha256').update(key).digest();
  const bytes = Buffer.from(digest.subarray(0, 16));
  // RFC4122 v4 bits: clock_seq_hi_and_reserved (byte 8) gets variant, version in byte 6.
  bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
  bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant 10
  const h = bytes.toString('hex');
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20, 32)}`;
}

export interface ResponseGenerationResult {
  success: boolean;
  messageId?: UUID;
  error?: string;
  wasRedundant?: boolean;
  storedToolResultIds: UUID[];
}

export interface PersonaResponseGeneratorConfig {
  personaId: UUID;
  personaName: string;
  entity: UserEntity;
  modelConfig: ModelConfig;
  modelInfo?: { contextWindow: number; tokensPerSecond: number; maxOutputTokens: number };
  client?: JTAGClient;
  toolExecutor: PersonaToolExecutor;
  toolRegistry: PersonaToolRegistry;
  mediaConfig: PersonaMediaConfig;
  getSessionId: () => UUID | null;
  logger: import('./PersonaLogger').PersonaLogger;
  genome?: import('./PersonaGenome').PersonaGenome;
  trainingAccumulator?: import('./TrainingDataAccumulator').TrainingDataAccumulator;
  rustCognitionBridge?: RustCognitionBridge;
}

export class PersonaResponseGenerator {
  private personaId: UUID;
  private personaName: string;
  private entity: UserEntity;
  private modelConfig: ModelConfig;
  private modelInfo: { contextWindow: number; tokensPerSecond: number; maxOutputTokens: number } | null;
  private client?: JTAGClient;
  private toolExecutor: PersonaToolExecutor;
  private toolRegistry: PersonaToolRegistry;
  private mediaConfig: PersonaMediaConfig;
  private getSessionId: () => UUID | null;
  private logger: import('./PersonaLogger').PersonaLogger;
  private genome?: import('./PersonaGenome').PersonaGenome;
  private trainingAccumulator?: import('./TrainingDataAccumulator').TrainingDataAccumulator;
  private rustCognitionBridge?: RustCognitionBridge;

  private _rustBridge: RustCognitionBridge | null = null;
  private engagementDecider: PersonaEngagementDecider;
  private _dispatchDecider: SentinelDispatchDecider;

  setRustBridge(bridge: RustCognitionBridge): void {
    this._rustBridge = bridge;
  }

  constructor(config: PersonaResponseGeneratorConfig) {
    this.personaId = config.personaId;
    this.personaName = config.personaName;
    this.entity = config.entity;
    this.modelConfig = config.modelConfig;
    this.modelInfo = config.modelInfo ?? null;
    this.client = config.client;
    this.toolExecutor = config.toolExecutor;
    this.toolRegistry = config.toolRegistry;
    this.mediaConfig = config.mediaConfig;
    this.getSessionId = config.getSessionId;
    this.logger = config.logger;
    this.genome = config.genome;
    this.trainingAccumulator = config.trainingAccumulator;
    this.rustCognitionBridge = config.rustCognitionBridge;
    if (config.rustCognitionBridge) this._rustBridge = config.rustCognitionBridge;

    this.engagementDecider = new PersonaEngagementDecider(config.personaName, this.log.bind(this));
    this._dispatchDecider = new SentinelDispatchDecider();
  }

  private log(message: string, ...args: unknown[]): void {
    const timestamp = new Date().toISOString();
    const formattedArgs = args.length > 0
      ? ' ' + args.map(a =>
          typeof a === 'object' ? inspect(a, { depth: 2, colors: false, compact: true }) : String(a)
        ).join(' ')
      : '';
    this.logger.enqueueLog('cognition.log', `[${timestamp}] ${message}${formattedArgs}\n`);
  }

  shouldRespondToMessage(
    message: ProcessableMessage,
    dormancyState?: DormancyState,
  ): boolean {
    return this.engagementDecider.shouldRespondToMessage(message, dormancyState);
  }

  /**
   * Sentinel dispatch check — complex multi-step human requests become
   * sentinel pipelines instead of a tool loop. Only one persona wins the
   * claim per message. Returns a terminal result if dispatched, else null.
   */
  private async checkSentinelDispatch(
    originalMessage: ProcessableMessage,
  ): Promise<ResponseGenerationResult | null> {
    if (originalMessage.senderType !== 'human') return null;
    const text = originalMessage.content.text;
    if (!text) return null;

    const decision = await this._dispatchDecider.evaluate(
      text,
      this.personaId,
      this.personaName,
      process.cwd(),
    );

    if (!decision.shouldDispatch || !decision.template) {
      if (decision.confidence > 0.3) {
        this.log(`🚀 ${this.personaName}: Sentinel considered but below threshold — ${decision.reasoning} (confidence=${decision.confidence.toFixed(2)})`);
      }
      return null;
    }

    const claimed = SentinelDispatchCoordinator.claim(
      originalMessage.id,
      this.personaId,
      decision.template,
    );
    if (!claimed) {
      const claimant = SentinelDispatchCoordinator.claimant(originalMessage.id);
      this.log(`🚀 ${this.personaName}: Sentinel [${decision.template}] already claimed by ${claimant?.slice(0, 8)} — skipping`);
      return null;
    }

    this.log(`🚀 ${this.personaName}: Dispatching sentinel [${decision.template}] (confidence=${decision.confidence.toFixed(2)}): ${decision.reasoning}`);

    try {
      const config = {
        ...decision.extractedConfig,
        roomId: originalMessage.roomId ?? 'general',
      };
      const result = await Commands.execute('sentinel/run', {
        type: 'pipeline',
        template: decision.template,
        templateConfig: config,
        async: true,
        parentPersonaId: this.personaId,
        sentinelName: `${decision.template} — ${text.slice(0, 60)}`,
      } as Record<string, unknown>) as SentinelRunResult;

      if (result.success) {
        this.log(`🚀 ${this.personaName}: Sentinel launched (handle=${result.handle})`);
        return { success: true, storedToolResultIds: [] };
      }
      this.log(`❌ ${this.personaName}: Sentinel launch failed: ${(result as unknown as Record<string, unknown>).error}`);
      SentinelDispatchCoordinator.release(originalMessage.id);
      return null;
    } catch (err) {
      this.log(`❌ ${this.personaName}: Sentinel dispatch error: ${err}`);
      SentinelDispatchCoordinator.release(originalMessage.id);
      return null;
    }
  }

  /**
   * Generate and post a response — the shim's main verb. Calls Rust cognition
   * for analysis + scoring + render + strip-thinks, keeps tool agent loop +
   * posting in TS.
   */
  async generateAndPostResponse(
    originalMessage: ProcessableMessage,
    decisionContext?: Omit<LogDecisionParams, 'responseContent' | 'tokensUsed' | 'responseTime'>,
    preBuiltRagContext?: RAGContext,
    socialSignals?: SocialSignals,
  ): Promise<ResponseGenerationResult> {
    const generateStartTime = Date.now();
    const allStoredResultIds: UUID[] = [];
    const pipelineTiming: Record<string, number> = {};

    try {
      // Sentinel short-circuit.
      const dispatchResult = await this.checkSentinelDispatch(originalMessage);
      if (dispatchResult) return dispatchResult;

      if (!this._rustBridge) {
        throw new Error(`${this.personaName}: Rust cognition bridge not initialized — cannot respond`);
      }

      // RAG: reuse the evaluator's build if it handed one off, else build fresh.
      // Rust only needs identity prompt + recent_history; the rest (tool defs,
      // memories) is TS concerns for the tool loop.
      const phase31Start = Date.now();
      const ragContext = preBuiltRagContext ?? await this.buildRagContext(originalMessage);
      pipelineTiming['3.1_rag'] = Date.now() - phase31Start;

      const knownSpecialties = this.buildKnownSpecialties(ragContext);
      const recentHistory = this.buildRecentHistory(ragContext);
      const systemPrompt = ragContext.identity.systemPrompt;
      const specialty = this.resolveSpecialty();

      // The single IPC: Rust owns the cognitive verb end-to-end.
      const phase32Start = Date.now();
      // Native multimodal: pass the message's media (images, audio) through
      // to Rust. When the persona's resolved model has the matching native
      // capability (Vision / AudioInput), Rust attaches as ContentPart::Image
      // / ::Audio on the final user-role message — the model sees / hears
      // the source bytes directly. Pre-2026-04-21 this was dropped on the
      // floor here, defaulting every multimodal model into text-only mode
      // (regression — qwen3.5 / Claude / GPT-4o are natively multimodal,
      // bridging defeats their whole point). See PERSONA-CONTEXT-PAGING.md
      // §0.5.X. Only items with inline base64 are forwarded — URL-only
      // references would need a fetch step we haven't added.
      const messageMedia = (originalMessage.content.media ?? [])
        .filter((m) => typeof m.base64 === 'string' && m.base64.length > 0)
        .map((m) => ({
          itemType: m.type,
          base64: m.base64,
          mimeType: m.mimeType,
        }));

      const rustRequest: PersonaRespondRequest = {
        personaId: this.personaId,
        roomId: originalMessage.roomId,
        messageId: originalMessage.id,
        personaName: this.personaName,
        specialty,
        // Per-persona render model — required so each persona renders with
        // its OWN configured model, not the shared-analysis base model.
        // Source of truth is this persona's ModelConfig (auto-routes trait
        // adapters etc. at the Rust side via select_model).
        model: this.modelConfig.model,
        messageText: originalMessage.content.text ?? '',
        systemPrompt,
        recentHistory,
        knownSpecialties,
        isVoice: originalMessage.sourceModality === 'voice',
        messageMedia: messageMedia.length > 0 ? messageMedia : undefined,
      };
      // Fixture capture for the Rust-persona-rewrite replay test harness
      // AND the eventual training corpus that Forge/Academy/Sentinel-AI
      // use to LoRA-train models against our actual RAG output shape.
      //
      // FIFO-pruned at FIXTURE_CAP_PER_DIR — keeps a representative
      // recent slice without unbounded compound growth. 200 fixtures
      // at ~25KB each = ~5MB ceiling per persona-respond dir, still
      // plenty of training-corpus diversity.
      //
      // No try/catch — disk write failure is a real bug to surface, not
      // hide. If permissions/disk are wrong, fix that, don't silently
      // lose fixtures.
      {
        const { writeFileSync, mkdirSync, readdirSync, statSync, unlinkSync } = await import('fs');
        const { homedir } = await import('os');
        const { join } = await import('path');
        const dir = join(homedir(), '.continuum', 'fixtures', 'persona-respond');
        mkdirSync(dir, { recursive: true });
        const ts = new Date().toISOString().replace(/[:.]/g, '-');
        const fname = `${this.personaName.replace(/\s+/g, '_')}-${originalMessage.id.slice(0, 8)}-${ts}.json`;
        writeFileSync(join(dir, fname), JSON.stringify({
          captured_at: Date.now(),
          persona_id: this.personaId,
          persona_name: this.personaName,
          model_config: this.modelConfig,
          rust_request: rustRequest,
        }, null, 2));

        const FIXTURE_CAP_PER_DIR = 200;
        const entries = readdirSync(dir)
          .filter((n) => n.endsWith('.json'))
          .map((n) => {
            const full = join(dir, n);
            return { full, mtime: statSync(full).mtimeMs };
          });
        if (entries.length > FIXTURE_CAP_PER_DIR) {
          entries.sort((a, b) => a.mtime - b.mtime);
          const toRemove = entries.slice(0, entries.length - FIXTURE_CAP_PER_DIR);
          for (const e of toRemove) {
            unlinkSync(e.full);
          }
        }
      }

      const response = await this._rustBridge.personaRespond(rustRequest);
      pipelineTiming['3.2_cognition'] = Date.now() - phase32Start;

      if (response.kind === 'silent') {
        return this.handleSilent(originalMessage, response, pipelineTiming, generateStartTime);
      }

      // No-fallback: Rust personaRespond is the ONLY inference path for
      // a persona reply. The previous TS agent loop, response validator,
      // and prompt assembler ran a SECOND inference pass on the Rust
      // output, applied a TS-side maxTokens cap, and fell back to TS
      // logic that duplicated work the Rust cognition crate already
      // owns. Joel's instruction (2026-04-20): "REMOVE THESE FUCKING
      // FALLBACKS". Tool calling will be re-added inside Rust as part
      // of the cognition migration; until then a persona's spoken text
      // is exactly what Rust returned.
      const finalText = response.text.trim();
      if (!finalText) {
        this.log(`⚠️ ${this.personaName}: Rust returned empty text — skipping post`);
        return { success: false, error: 'Empty response from Rust', storedToolResultIds: allStoredResultIds };
      }

      const phase35Start = Date.now();
      const postedMessageId = await this.postResponse(
        originalMessage,
        finalText,
        response,
        pipelineTiming,
        generateStartTime,
      );
      pipelineTiming['3.5_post'] = Date.now() - phase35Start;

      if (decisionContext) {
        CoordinationDecisionLogger.logDecision({
          ...decisionContext,
          responseContent: finalText,
          tokensUsed: finalText.length,
          responseTime: Date.now() - generateStartTime,
        }).catch(err => this.log(`❌ Failed to log POSTED decision: ${err}`));
      }

      // Training + fitness telemetry (fire-and-forget, off critical path).
      this.captureTrainingData(originalMessage, finalText);
      this.recordFitness(generateStartTime);

      const totalMs = Date.now() - generateStartTime;
      const phases = Object.entries(pipelineTiming).map(([k, v]) => `${k}=${v}ms`).join(' | ');
      this.log(`📊 ${this.personaName}: [PIPELINE] Total=${totalMs}ms | ${phases} | rust_inference=${response.inference_ms}ms rust_total=${response.total_ms}ms thinks=${response.think_blocks_emitted}`);

      return {
        success: true,
        messageId: postedMessageId,
        storedToolResultIds: allStoredResultIds,
      };
    } catch (error) {
      return this.handleError(error, originalMessage, allStoredResultIds);
    }
  }

  private async buildRagContext(originalMessage: ProcessableMessage): Promise<RAGContext> {
    const ragBuilder = new ChatRAGBuilder(this.log.bind(this));
    const ctxWindow = this.modelInfo?.contextWindow
      ?? this.modelConfig.contextWindow
      ?? getContextWindow(this.modelConfig.model, this.modelConfig.provider);
    const tps = this.modelInfo?.tokensPerSecond
      ?? getInferenceSpeed(this.modelConfig.model, this.modelConfig.provider);

    return ragBuilder.buildContext(
      originalMessage.roomId,
      this.personaId,
      {
        modelId: this.modelConfig.model,
        maxTokens: this.modelConfig.maxTokens,
        contextWindow: ctxWindow,
        tokensPerSecond: tps,
        maxMemories: 5,
        includeArtifacts: true,
        includeMemories: true,
        voiceSessionId: originalMessage.voiceSessionId,
        provider: this.modelConfig.provider,
        toolCapability: getToolCapability(this.modelConfig.provider, this.modelConfig),
        currentMessage: {
          role: 'user',
          content: originalMessage.content.text,
          name: originalMessage.senderName,
          timestamp: this.timestampToNumber(originalMessage.timestamp),
        },
      },
    );
  }

  private buildRecentHistory(ragContext: RAGContext): Array<{ id: string; sender_name: string; text: string }> {
    // LLMMessage has no id field (Rust needs one for its shared-analysis cache
    // key). Synthesize deterministic UUIDv5-style IDs from content+timestamp so
    // the SAME history entry produces the SAME id across every persona's call
    // for this message — that's what keeps Rust's per-message analysis cache
    // hitting when multiple personas service the same inbound. Full-fidelity
    // IDs follow when LLMMessage gains a real id field.
    return (ragContext.conversationHistory ?? []).map(h => ({
      id: synthesizeDeterministicUuid(h),
      sender_name: h.name ?? 'unknown',
      text: h.content,
    }));
  }

  private buildKnownSpecialties(ragContext: RAGContext): string[] {
    // RAG context may expose the room's persona roster via metadata; fall
    // back to this persona's own specialty if not (Rust tolerates that).
    const rosterSpecialties = (ragContext.metadata as Record<string, unknown> | undefined)
      ?.roomPersonaSpecialties as string[] | undefined;
    const own = this.resolveSpecialty();
    if (rosterSpecialties && rosterSpecialties.length > 0) {
      return Array.from(new Set([...rosterSpecialties, own]));
    }
    return [own];
  }

  private resolveSpecialty(): string {
    // UserEntity.specialty is the canonical slot; fall back to 'general'
    // if the entity predates the shared-cognition roster work.
    const entitySpecialty = (this.entity as unknown as { specialty?: string }).specialty;
    return entitySpecialty && entitySpecialty.trim().length > 0 ? entitySpecialty : 'general';
  }

  private buildMessagesForToolLoop(
    systemPrompt: string,
    recentHistory: Array<{ id: string; sender_name: string; text: string }>,
    originalMessage: ProcessableMessage,
  ): TextGenerationRequest['messages'] {
    const messages: TextGenerationRequest['messages'] = [];
    if (systemPrompt) {
      messages.push({ role: 'system', content: systemPrompt });
    }
    for (const h of recentHistory) {
      messages.push({ role: 'user', content: `${h.sender_name}: ${h.text}` });
    }
    const currentName = originalMessage.senderName ? `${originalMessage.senderName}: ` : '';
    messages.push({ role: 'user', content: `${currentName}${originalMessage.content.text ?? ''}` });
    return messages;
  }

  private handleSilent(
    originalMessage: ProcessableMessage,
    response: Extract<PersonaResponse, { kind: 'silent' }>,
    pipelineTiming: Record<string, number>,
    generateStartTime: number,
  ): ResponseGenerationResult {
    this.log(`🔇 ${this.personaName}: Silent — ${response.reason} (score=${response.relevance_score.toFixed(2)})`);
    if (this.client && DataDaemon.jtagContext) {
      Events.emit<AIDecidedSilentEventData>(
        DataDaemon.jtagContext,
        AI_DECISION_EVENTS.DECIDED_SILENT,
        {
          personaId: this.personaId,
          personaName: this.personaName,
          roomId: originalMessage.roomId,
          messageId: originalMessage.id,
          isHumanMessage: originalMessage.senderType === 'human',
          timestamp: Date.now(),
          reason: response.reason,
          confidence: response.relevance_score,
          gatingModel: this.modelConfig.model,
        },
        { scope: EVENT_SCOPES.ROOM, scopeId: originalMessage.roomId },
      ).catch(err => this.log(`⚠️ Silent event emit failed: ${err}`));
      getAIAudioBridge().setCognitiveState(this.personaId, 'idle').catch(() => {});
      Events.emit(DataDaemon.jtagContext, PRESENCE_EVENTS.TYPING_STOP, {
        userId: this.personaId, displayName: this.personaName, roomId: originalMessage.roomId,
      }).catch(() => {});
    }
    const totalMs = Date.now() - generateStartTime;
    const phases = Object.entries(pipelineTiming).map(([k, v]) => `${k}=${v}ms`).join(' | ');
    this.log(`📊 ${this.personaName}: [PIPELINE silent] Total=${totalMs}ms | ${phases}`);
    return { success: true, storedToolResultIds: [] };
  }

  private async postResponse(
    originalMessage: ProcessableMessage,
    finalText: string,
    rustResponse: Extract<PersonaResponse, { kind: 'spoke' }>,
    pipelineTiming: Record<string, number>,
    _generateStartTime: number,
  ): Promise<UUID | undefined> {
    const responseMessage = new ChatMessageEntity();
    responseMessage.roomId = originalMessage.roomId;
    responseMessage.senderId = this.personaId;
    responseMessage.senderName = this.personaName;
    responseMessage.senderType = this.entity.type;
    responseMessage.content = { text: finalText, media: [] };
    responseMessage.status = 'sent';
    responseMessage.priority = 'normal';
    responseMessage.timestamp = new Date();
    responseMessage.reactions = [];
    responseMessage.replyToId = originalMessage.id;
    responseMessage.metadata = {
      ...responseMessage.metadata,
      source: 'bot' as const,
    };

    // Voice routing BEFORE DB write — TTS shouldn't wait for persistence.
    if (originalMessage.sourceModality === 'voice' && originalMessage.voiceSessionId && DataDaemon.jtagContext) {
      Events.emit(
        DataDaemon.jtagContext,
        'persona:response:generated',
        {
          personaId: this.personaId,
          response: finalText,
          originalMessage: {
            id: originalMessage.id,
            roomId: originalMessage.roomId,
            sourceModality: 'voice' as const,
            voiceSessionId: originalMessage.voiceSessionId,
          },
        },
      ).catch(err => this.log(`⚠️ Voice event emit failed: ${err}`));
    }

    const postStart = Date.now();
    const postedEntity = await ORM.store(ChatMessageEntity.collection, responseMessage, false, 'default');
    const postDuration = Date.now() - postStart;
    this.log(`✅ ${this.personaName}: Posted (${postDuration}ms, id=${postedEntity.id})`);

    if (DataDaemon.jtagContext) {
      Events.emit<StageCompleteEvent>(
        DataDaemon.jtagContext,
        COGNITION_EVENTS.STAGE_COMPLETE,
        {
          messageId: postedEntity.id ?? originalMessage.id,
          personaId: this.personaId,
          contextId: originalMessage.roomId,
          stage: 'post-response',
          metrics: {
            stage: 'post-response',
            durationMs: postDuration,
            resourceUsed: 1,
            maxResource: 1,
            percentCapacity: 100,
            percentSpeed: calculateSpeedScore(postDuration, 'post-response'),
            status: getStageStatus(postDuration, 'post-response'),
            metadata: { messageId: postedEntity.id, success: true },
          },
          timestamp: Date.now(),
        },
      ).catch(err => this.log(`⚠️ Stage event emit failed: ${err}`));
    }

    AIDecisionLogger.logResponse(this.personaName, originalMessage.roomId, finalText);

    if (originalMessage.metadata?.isSystemTest === true) {
      this.log(`🚨 ANOMALY: ${this.personaName} responded to system test`);
      this.log(`   Test: ${originalMessage.metadata.testType ?? 'unknown'}`);
      this.log(`   Original: "${messagePreview(originalMessage.content, 100)}..."`);
      this.log(`   Response: "${truncate(finalText, 100)}..."`);
      AIDecisionLogger.logError(
        this.personaName,
        'COGNITIVE CANARY TRIGGERED',
        `Responded to system test (${originalMessage.metadata.testType})`,
      );
    }

    if (this.client && postedEntity && DataDaemon.jtagContext) {
      Events.emit<AIPostedEventData>(
        DataDaemon.jtagContext,
        AI_DECISION_EVENTS.POSTED,
        {
          personaId: this.personaId,
          personaName: this.personaName,
          roomId: originalMessage.roomId,
          messageId: originalMessage.id,
          isHumanMessage: originalMessage.senderType === 'human',
          timestamp: Date.now(),
          responseMessageId: postedEntity.id,
          passedRedundancyCheck: true,
        },
        { scope: EVENT_SCOPES.ROOM, scopeId: originalMessage.roomId },
      ).catch(err => this.log(`⚠️ Posted event emit failed: ${err}`));
      getAIAudioBridge().setCognitiveState(this.personaId, 'idle').catch(() => {});
      Events.emit(DataDaemon.jtagContext, PRESENCE_EVENTS.TYPING_STOP, {
        userId: this.personaId, displayName: this.personaName, roomId: originalMessage.roomId,
      }).catch(() => {});
    }

    pipelineTiming['3.5_post'] = postDuration;
    return postedEntity.id;
  }

  private captureTrainingData(originalMessage: ProcessableMessage, finalText: string): void {
    if (!this.trainingAccumulator) return;
    const accumulator = this.trainingAccumulator;
    const bridge = this.rustCognitionBridge;
    const fallbackDomain = this.inferTrainingDomain(originalMessage);
    const inputText = originalMessage.content.text ?? '';

    (async () => {
      let domain = fallbackDomain;
      let qualityRating: number | undefined;
      if (bridge) {
        try {
          const classification = await bridge.classifyDomain(inputText);
          domain = classification.domain;
          bridge.recordActivity(domain, true).catch(() => {});
          qualityRating = (await bridge.scoreInteraction(inputText, finalText)).score;
        } catch { /* fallback domain already set */ }
      }
      await accumulator.captureInteraction({
        roleId: this.personaId,
        personaId: this.personaId,
        domain,
        input: inputText,
        output: finalText,
        qualityRating,
      });
    })().catch(err => this.log(`⚠️ Failed to capture training: ${err}`));
  }

  private recordFitness(generateStartTime: number): void {
    if (!this.genome) return;
    const activeAdapter = this.genome.getCurrentAdapter();
    const layerId = activeAdapter?.getLayerId();
    if (layerId) {
      FitnessTracker.instance.recordInference(layerId, {
        success: true,
        latency: Date.now() - generateStartTime,
      });
    }
  }

  private handleError(
    error: unknown,
    originalMessage: ProcessableMessage,
    storedToolResultIds: UUID[],
  ): ResponseGenerationResult {
    const errorMsg = error instanceof Error ? error.message : String(error);
    const isNotConfigured = errorMsg.includes('not available') && errorMsg.includes('Available:');

    if (isNotConfigured) {
      this.log(`⏭️ ${this.personaName}: Provider not configured, staying quiet`);
    } else {
      this.log(`❌ ${this.personaName}: ${errorMsg}`);
      AIDecisionLogger.logError(this.personaName, 'Response generation/posting', errorMsg);
    }

    if (this.client && !isNotConfigured && DataDaemon.jtagContext) {
      Events.emit<AIErrorEventData>(
        DataDaemon.jtagContext,
        AI_DECISION_EVENTS.ERROR,
        {
          personaId: this.personaId,
          personaName: this.personaName,
          roomId: originalMessage.roomId,
          messageId: originalMessage.id,
          isHumanMessage: originalMessage.senderType === 'human',
          timestamp: Date.now(),
          error: errorMsg,
          phase: 'generating',
        },
        { scope: EVENT_SCOPES.ROOM, scopeId: originalMessage.roomId },
      ).catch(err => this.log(`⚠️ Error event emit failed: ${err}`));
      getAIAudioBridge().setCognitiveState(this.personaId, 'idle').catch(() => {});
      Events.emit(DataDaemon.jtagContext, PRESENCE_EVENTS.TYPING_STOP, {
        userId: this.personaId, displayName: this.personaName, roomId: originalMessage.roomId,
      }).catch(() => {});
    }

    return { success: false, error: errorMsg, storedToolResultIds };
  }

  private inferTrainingDomain(message: ProcessableMessage): string {
    const text = message.content.text ?? '';
    if (text.includes('```') || text.includes('function ') || text.includes('import ') || text.includes('const ')) {
      return 'code';
    }
    if (text.toLowerCase().includes('teach') || text.toLowerCase().includes('learn') || text.toLowerCase().includes('exam')) {
      return 'teaching';
    }
    return 'conversation';
  }

  private timestampToNumber(timestamp: Date | number | string | undefined): number {
    if (timestamp === undefined) return Date.now();
    if (timestamp instanceof Date) return timestamp.getTime();
    if (typeof timestamp === 'string') {
      const parsed = new Date(timestamp).getTime();
      return isNaN(parsed) ? Date.now() : parsed;
    }
    return timestamp;
  }
}
