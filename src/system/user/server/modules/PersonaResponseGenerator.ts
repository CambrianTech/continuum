/* eslint-disable max-lines -- pre-existing 720-line file; scheduled for split into PRG.ts (orchestration) + PRG-postResponse.ts + PRG-pipeline.ts in the cleanup-sweep PR after #950 */
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
import type { TextGenerationRequest } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { ChatRAGBuilder } from '../../../rag/builders/ChatRAGBuilder';
import { getContextWindow, getInferenceSpeed } from '../../../shared/ModelContextWindows';
import { truncate, messagePreview } from '../../../../shared/utils/StringUtils';
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
import { getToolCapability } from './ToolFormatAdapter';
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

  /**
   * Cached capability vocabulary for this persona's model. Resolved
   * lazily on first need from `models/capabilities` IPC against the
   * Rust model registry (the canonical source — `models.toml`). Cached
   * for the persona's lifetime because a persona's model is fixed.
   *
   * Why this is a TS-side cache, not a Rust-side mid-call lookup: when
   * Rust did `try_global() → registry.model(input.model)` inside
   * `cognition::respond`, registry-key drift silently returned empty
   * caps → image bytes that arrived correctly via `messageMedia` got
   * demoted to text markers and the vision encoder never fired.
   * Caller-side resolution + cache puts the lookup at the right
   * boundary (orchestration layer, loud failure when keys diverge)
   * and keeps the inference hot path free of global lookups.
   */
  private _modelCapabilities: string[] | null = null;
  private _modelCapabilitiesPromise: Promise<string[]> | null = null;

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

  /**
   * Resolve this persona's model capabilities from the Rust registry,
   * caching for the persona's lifetime. Single-flight: concurrent
   * callers during the first resolution share one in-flight Promise so
   * we never issue a duplicate IPC round-trip at boot.
   *
   * Hard error if the model id isn't in `models.toml` — that's a
   * misconfigured persona, not something to silently paper over.
   * Better to fail visibly here than to silently send empty caps and
   * watch vision quietly disable itself two layers down.
   */
  private async resolveModelCapabilities(): Promise<string[]> {
    if (this._modelCapabilities) return this._modelCapabilities;
    if (this._modelCapabilitiesPromise) return this._modelCapabilitiesPromise;
    if (!this._rustBridge) {
      throw new Error(`${this.personaName}: cannot resolve model capabilities — Rust bridge not initialized`);
    }
    const bridge = this._rustBridge;
    this._modelCapabilitiesPromise = (async (): Promise<string[]> => {
      const caps = await bridge.getModelCapabilities(this.modelConfig.model);
      this._modelCapabilities = caps;
      this._modelCapabilitiesPromise = null;
      return caps;
    })();
    return this._modelCapabilitiesPromise;
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
  // eslint-disable-next-line max-lines-per-function, complexity -- pre-existing: this is the convergence point that needs to be split into pipeline stages, scheduled for the cleanup-sweep PR after #950
  async generateAndPostResponse(
    originalMessage: ProcessableMessage,
    decisionContext?: Omit<LogDecisionParams, 'responseContent' | 'tokensUsed' | 'responseTime'>,
    preBuiltRagContext?: RAGContext,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars -- caller passes for forward-compat with social-signal injection feature
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
      // §0.5.X.
      //
      // Storage: per Joel's 2026-04-21 directive, base64 NEVER persists in
      // the chat_messages DB column. The entity carries `blobHash` + `url`
      // refs only. Resolve back to bytes here, on the request path —
      // chat-send already wrote the file to disk via
      // MediaBlobService.externalize (synchronously, before data/create).
      // Description (from VisionDescriptionService cache) gets pulled
      // alongside so text-only personas downstream get the bridge text
      // instead of hallucinating from prompt context.
      const { MediaBlobService } = await import('../../../storage/MediaBlobService');
      const { VisionDescriptionService } = await import('../../../vision/VisionDescriptionService');
      const fs = await import('fs');

      const messageMediaResolved = await Promise.all(
        (originalMessage.content.media ?? []).map(async (m) => {
          // Prefer inline base64 if it's still around (browser pre-encode
          // path or an item smaller than the externalize threshold), else
          // resolve via blobHash → file on disk → base64.
          let base64: string | undefined = m.base64;
          if (!base64 && m.blobHash) {
            const path = MediaBlobService.getPath(m.blobHash);
            if (path) {
              try {
                const buf = await fs.promises.readFile(path);
                base64 = buf.toString('base64');
              } catch {
                // File missing despite hash — drop this item, log later.
                return null;
              }
            }
          }
          if (!base64) {
            return null; // Nothing to send to the model
          }
          // Pull cached description (populated by prewarmVisionDescriptions
          // at chat-send time). Cache hit takes ~0ms; miss returns
          // undefined — text-only personas downstream get a "no
          // description available" marker instead of fabricating.
          let description: string | undefined;
          if (m.type === 'image') {
            try {
              const visionSvc = VisionDescriptionService.getInstance();
              if (visionSvc.descriptionStatus(base64) === 'cached') {
                const desc = await visionSvc.describeBase64(base64, m.mimeType ?? 'image/png', { maxLength: 200 });
                description = desc?.description;
              }
            } catch {
              // Best-effort; drop to undefined on any cache error
            }
          }
          return {
            itemType: m.type,
            base64,
            mimeType: m.mimeType,
            description,
          };
        })
      );
      const messageMedia = messageMediaResolved.filter((x): x is NonNullable<typeof x> => x !== null);

      // Resolve THIS persona's model capabilities (cached). Required by
      // the IPC contract — Rust no longer does a registry lookup on its
      // side, so the answer to "is this model vision-capable?" must
      // travel WITH the request. Hard error if the model isn't in the
      // registry (broken persona configuration, fail loudly here).
      const capabilities = await this.resolveModelCapabilities();

      // IPC shape: { signal, personaContext }. Rust projects (signal,
      // ctx) → RespondInput via cognition_io::build_respond_input,
      // runs respond(), returns the response. No recipe-name field —
      // recipes are JSON data walked by whatever wraps this call
      // (today: nothing — chat dispatches directly; future: a small
      // walker that interprets recipe pipelines for non-chat hosts).
      //
      // Field-name convention here is camelCase to match the ts-rs
      // generated `Signal` / `PersonaContext` types (Rust serde
      // rename_all = "camelCase"). Snake_case in the wire payload
      // would be silently rejected by Rust serde — exact field names
      // matter, no fallback parser.
      const signal = {
        kind: { kind: 'chat-message' as const },
        text: originalMessage.content.text ?? '',
        media: messageMedia,
        originator: {
          kind: 'user' as const,
          // Snake_case here is intentional: ts-rs doesn't apply
          // `rename_all = "camelCase"` to enum variant fields, only
          // to the variant tags. So Rust's `User { user_id }` stays
          // snake_case on the wire.
          user_id: originalMessage.senderId,
        },
        timestampMs: Date.now(),
        messageId: originalMessage.id,
      };
      const personaContext = {
        personaId: this.personaId,
        displayName: this.personaName,
        specialty,
        model: this.modelConfig.model,
        // Capabilities cross the wire as kebab-case strings (Rust
        // `Capability` serde rename) — matches the `Capability`
        // ts-rs export.
        capabilities: capabilities as unknown as import('../../../../shared/generated/model_registry/Capability').Capability[],
        systemPrompt,
        recentHistory: recentHistory.map(h => ({
          id: h.id,
          senderName: h.sender_name,
          text: h.text,
        })),
        knownSpecialties,
        roomId: originalMessage.roomId,
        isVoice: originalMessage.sourceModality === 'voice',
      };

      const rustRequest: PersonaRespondRequest = {
        signal,
        personaContext,
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
      // Build the fixture path up front; write it twice — once with
      // the request before the IPC call (so we capture the input even
      // if Rust hangs or crashes mid-call), then rewrite atomically
      // with the response paired in. Self-contained fixtures
      // (input + observed output + timing) are what makes the live
      // session replayable as an integration test — anything less is
      // just an input dump that requires re-running real inference
      // to know "what was it supposed to do?".
      const { writeFileSync, renameSync, mkdirSync, readdirSync, statSync, unlinkSync } = await import('fs');
      const { homedir } = await import('os');
      const { join } = await import('path');
      const fixtureDir = join(homedir(), '.continuum', 'fixtures', 'persona-respond');
      mkdirSync(fixtureDir, { recursive: true });
      const fixtureTs = new Date().toISOString().replace(/[:.]/g, '-');
      const fixtureName = `${this.personaName.replace(/\s+/g, '_')}-${originalMessage.id.slice(0, 8)}-${fixtureTs}.json`;
      const fixturePath = join(fixtureDir, fixtureName);
      // The whole shebang: every input the persona had visibility into
      // for THIS turn, plus the IPC payload built from those inputs,
      // plus (after the await) the Rust response. No black boxes — if
      // a persona "sees" something or "doesn't see" something, this
      // file documents both, so a replay test can prove the behavior
      // OR catch the regression that hid it.
      //
      // Sensitive payload note: media base64 lives in `rust_request`.
      // Fixtures are written under ~/.continuum (already gitignored
      // and out of the repo), but anything copied for sharing should
      // strip base64 first. The `rag_context.conversationHistory`
      // mirrors what crossed the IPC; full RAG sources (with
      // embeddings, scores, and original document bodies) are NOT
      // included here — would balloon fixture size 10x. If RAG
      // attribution itself needs replay, capture upstream of PRG.
      const fixtureBase = {
        schema_version: 3,
        captured_at: Date.now(),
        session_id: this.getSessionId(),
        persona_id: this.personaId,
        persona_name: this.personaName,
        model_config: this.modelConfig,
        // Original message the persona is reacting to — what the
        // chat path handed in. Lets a replay reconstruct the trigger
        // shape (text + media + sender) without hunting through DB.
        original_message: {
          id: originalMessage.id,
          roomId: originalMessage.roomId,
          senderId: originalMessage.senderId,
          senderType: originalMessage.senderType,
          text: originalMessage.content.text,
          mediaCount: originalMessage.content.media?.length ?? 0,
          mediaTypes: (originalMessage.content.media ?? []).map((m) => m.type),
          sourceModality: originalMessage.sourceModality,
        },
        // EXACT RAG context the persona had before building the IPC.
        // FULL conversation history (no truncation, no sampling) so
        // replay can reconstruct the persona's exact view. Identity
        // system prompt full. Metadata copied verbatim. If the
        // captured fixture differs from prod behavior, the difference
        // is in the test setup or downstream code — never in the
        // input itself, because the input is byte-for-byte preserved.
        rag_context: {
          conversationHistory: (ragContext.conversationHistory ?? []).map((h) => ({
            role: h.role,
            name: h.name ?? null,
            content: h.content,
          })),
          identitySystemPrompt: ragContext.identity.systemPrompt ?? null,
          metadata: ragContext.metadata ?? {},
        },
        resolved_capabilities: capabilities,
        rust_request: rustRequest,
      };
      writeFileSync(fixturePath, JSON.stringify({
        ...fixtureBase,
        rust_response: null, // pending — set after the IPC await
        ipc_error: null,
        ipc_duration_ms: null,
      }, null, 2));

      const ipcStart = Date.now();
      let response: PersonaResponse;
      try {
        response = await this._rustBridge.personaRespond(rustRequest);
      } catch (err) {
        // Persist the failure into the fixture too — the replay tests
        // need to see "this input made Rust throw" as a first-class
        // recorded outcome, not lost as a TS-side log line.
        const ipcDurMs = Date.now() - ipcStart;
        try {
          writeFileSync(fixturePath + '.tmp', JSON.stringify({
            ...fixtureBase,
            rust_response: null,
            ipc_error: { message: String(err), stack: (err as Error)?.stack ?? null },
            ipc_duration_ms: ipcDurMs,
          }, null, 2));
          renameSync(fixturePath + '.tmp', fixturePath);
        } catch (writeErr) {
          this.log(`⚠️ ${this.personaName}: failed to update fixture with IPC error: ${writeErr}`);
        }
        throw err;
      }
      const ipcDurationMs = Date.now() - ipcStart;
      pipelineTiming['3.2_cognition'] = Date.now() - phase32Start;

      // Rewrite the fixture with the response paired in. Atomic:
      // write to .tmp then rename, so a crash mid-write leaves the
      // pre-call fixture intact rather than producing a half file
      // that breaks parsers.
      try {
        writeFileSync(fixturePath + '.tmp', JSON.stringify({
          ...fixtureBase,
          rust_response: response,
          ipc_error: null,
          ipc_duration_ms: ipcDurationMs,
        }, null, 2));
        renameSync(fixturePath + '.tmp', fixturePath);
      } catch (writeErr) {
        this.log(`⚠️ ${this.personaName}: failed to update fixture with response: ${writeErr}`);
      }

      // FIFO trim — keep recent slice without unbounded growth.
      const FIXTURE_CAP_PER_DIR = 200;
      const entries = readdirSync(fixtureDir)
        .filter((n) => n.endsWith('.json'))
        .map((n) => {
          const full = join(fixtureDir, n);
          return { full, mtime: statSync(full).mtimeMs };
        });
      if (entries.length > FIXTURE_CAP_PER_DIR) {
        entries.sort((a, b) => a.mtime - b.mtime);
        const toRemove = entries.slice(0, entries.length - FIXTURE_CAP_PER_DIR);
        for (const e of toRemove) {
          unlinkSync(e.full);
        }
      }

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

    // Resolve THIS persona's model capabilities up front so toolCapability
    // is derived from the registry truth, not provider-string defaults. A
    // vision-only VLM (qwen2-vl-7b) has caps [text-generation, chat, vision,
    // streaming] with NO `tool-use` — defaulting to 'xml' makes RAG inject
    // sentinel/tool definitions the model has zero training to invoke, and
    // it emits literal tool-name fragments as response text. Capability
    // declaration travels WITH the request → no silent provider default.
    const caps = await this.resolveModelCapabilities();
    const hasToolUse = caps.includes('tool-use');
    const toolCapability = hasToolUse
      ? getToolCapability(this.modelConfig.provider, this.modelConfig)
      : 'none';

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
        toolCapability,
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

  // eslint-disable-next-line max-lines-per-function -- pre-existing: posting + side-effects bundled here, scheduled for cleanup-sweep PR after #950
  private async postResponse(
    originalMessage: ProcessableMessage,
    finalText: string,
    rustResponse: Extract<PersonaResponse, { kind: 'spoke' }>,
    pipelineTiming: Record<string, number>,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars -- caller passes for total-pipeline timing, kept in signature for future telemetry
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

    (async (): Promise<void> => {
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
