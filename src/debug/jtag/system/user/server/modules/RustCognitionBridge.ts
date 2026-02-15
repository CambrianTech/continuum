/**
 * Rust Cognition Bridge
 *
 * Connects PersonaUser to the Rust cognition engine via IPC.
 * Provides fast-path decision making (<1ms) for:
 * - Priority calculation
 * - Should-respond decisions
 * - State management
 * - Message deduplication
 *
 * NO FALLBACKS. If Rust fails, we fail loudly. Fix the problem, don't mask it.
 * FORENSIC LOGGING. Every operation logged to persona's rust-cognition.log
 *
 * TypeScript handles: DB queries, AI provider calls, UI
 * Rust handles: Fast compute, state tracking, deduplication
 */

import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type {
  InboxMessageRequest,
  CognitionDecision,
  PriorityScore,
  PersonaState,
  SenderType,
  ActivityDomain,
  ChannelRegistryStatus,
  ChannelEnqueueRequest,
  TextSimilarityResult,
  SemanticLoopResult,
  ConversationMessage,
  ValidationResult,
} from '../../../../shared/generated';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { SubsystemLogger } from './being/logging/SubsystemLogger';

// Memory subsystem types (Hippocampus in Rust — corpus-based, no SQL)
import type { CorpusMemory } from '../../../../workers/continuum-core/bindings/CorpusMemory';
import type { CorpusTimelineEvent } from '../../../../workers/continuum-core/bindings/CorpusTimelineEvent';
import type { LoadCorpusResponse } from '../../../../workers/continuum-core/bindings/LoadCorpusResponse';
import type { MemoryRecallResponse } from '../../../../workers/continuum-core/bindings/MemoryRecallResponse';
import type { MultiLayerRecallRequest } from '../../../../workers/continuum-core/bindings/MultiLayerRecallRequest';
import type { ConsciousnessContextResponse } from '../../../../workers/continuum-core/bindings/ConsciousnessContextResponse';

const SOCKET_PATH = getContinuumCoreSocketPath();

/**
 * Interface for PersonaUser dependency injection
 * Matches pattern used by PrefrontalCortex, MotorCortex, LimbicSystem
 */
export interface PersonaUserForRustCognition {
  readonly id: UUID;
  readonly displayName: string;
  readonly entity: { uniqueId: string };
  readonly homeDirectory: string;
}

export class RustCognitionBridge {
  private readonly client: RustCoreIPCClient;
  private readonly personaId: UUID;
  private readonly personaName: string;
  private readonly logger: SubsystemLogger;
  private connected = false;
  private engineCreated = false;

  // Stats for forensics
  private stats = {
    connectAttempts: 0,
    connectSuccesses: 0,
    priorityCalcs: 0,
    priorityFailures: 0,
    fastPathCalls: 0,
    fastPathFailures: 0,
    enqueueCalls: 0,
    enqueueFailures: 0,
    totalLatencyMs: 0,
  };

  constructor(personaUser: PersonaUserForRustCognition) {
    this.personaId = personaUser.id;
    this.personaName = personaUser.displayName;
    this.client = new RustCoreIPCClient(SOCKET_PATH);

    // Logger writes to persona's logs directory: .continuum/personas/{uniqueId}/logs/rust-cognition.log
    this.logger = new SubsystemLogger('rust-cognition', personaUser.id, personaUser.entity.uniqueId, {
      logDir: `${personaUser.homeDirectory}/logs`
    });
    this.logger.info(`Created bridge for ${personaUser.displayName} (${personaUser.id})`);
  }

  /**
   * Initialize connection and create cognition engine
   * THROWS if Rust unavailable - no silent degradation
   */
  async initialize(): Promise<void> {
    const start = performance.now();
    this.stats.connectAttempts++;

    this.logger.info(`Connecting to ${SOCKET_PATH}...`);

    try {
      await this.client.connect();
      this.connected = true;
      const connectMs = performance.now() - start;
      this.logger.info(`Socket connected in ${connectMs.toFixed(2)}ms`);
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`CONNECT FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Socket path: ${SOCKET_PATH}`);
      this.logger.error(`Error: ${error}`);
      this.logger.error(`Is continuum-core-server running?`);
      throw error;
    }

    try {
      const engineStart = performance.now();
      await this.client.cognitionCreateEngine(this.personaId, this.personaName);
      this.engineCreated = true;
      this.stats.connectSuccesses++;
      const engineMs = performance.now() - engineStart;
      const totalMs = performance.now() - start;
      this.logger.info(`Engine created in ${engineMs.toFixed(2)}ms (total init: ${totalMs.toFixed(2)}ms)`);
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`ENGINE CREATE FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`PersonaId: ${this.personaId}`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  private assertReady(operation: string): void {
    if (!this.connected) {
      const msg = `${operation}: Not connected to Rust (socket: ${SOCKET_PATH})`;
      this.logger.error(msg);
      this.logger.error(`Stats: ${JSON.stringify(this.stats)}`);
      throw new Error(msg);
    }
    if (!this.engineCreated) {
      const msg = `${operation}: Engine not created (connected but no engine)`;
      this.logger.error(msg);
      this.logger.error(`Stats: ${JSON.stringify(this.stats)}`);
      throw new Error(msg);
    }
  }

  /**
   * Calculate message priority (sub-1ms in Rust)
   * THROWS on failure
   */
  async calculatePriority(
    content: string,
    senderType: SenderType,
    isVoice: boolean,
    roomId: UUID,
    timestamp: number
  ): Promise<PriorityScore> {
    this.assertReady('calculatePriority');
    this.stats.priorityCalcs++;
    const start = performance.now();

    try {
      const result = await this.client.cognitionCalculatePriority(
        this.personaId,
        content,
        senderType as 'human' | 'persona' | 'agent' | 'system',
        isVoice,
        roomId,
        timestamp
      );

      const elapsed = performance.now() - start;
      this.stats.totalLatencyMs += elapsed;

      if (elapsed > 5) {
        this.logger.warn(`calculatePriority SLOW: ${elapsed.toFixed(2)}ms (target <1ms)`);
      }

      this.logger.info(`Priority: ${result.score.toFixed(3)} (${elapsed.toFixed(2)}ms) content="${content.slice(0, 30)}..."`);
      return result;
    } catch (error) {
      this.stats.priorityFailures++;
      const elapsed = performance.now() - start;
      this.logger.error(`calculatePriority FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Input: senderType=${senderType}, isVoice=${isVoice}, roomId=${roomId}`);
      this.logger.error(`Error: ${error}`);
      this.logger.error(`Stats: ${JSON.stringify(this.stats)}`);
      throw error;
    }
  }

  /**
   * Fast-path decision: should we respond?
   * Handles deduplication, mention detection, state gating
   * THROWS on failure
   */
  async fastPathDecision(message: InboxMessageRequest): Promise<CognitionDecision> {
    this.assertReady('fastPathDecision');
    this.stats.fastPathCalls++;
    const start = performance.now();

    try {
      const result = await this.client.cognitionFastPathDecision(this.personaId, message);

      const elapsed = performance.now() - start;
      this.stats.totalLatencyMs += elapsed;

      if (elapsed > 5) {
        this.logger.warn(`fastPathDecision SLOW: ${elapsed.toFixed(2)}ms (target <1ms)`);
      }

      this.logger.info(`FastPath: respond=${result.should_respond}, confidence=${result.confidence.toFixed(2)}, reason="${result.reason}" (${elapsed.toFixed(2)}ms)`);
      return result;
    } catch (error) {
      this.stats.fastPathFailures++;
      const elapsed = performance.now() - start;
      this.logger.error(`fastPathDecision FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Message ID: ${message.id}, sender: ${message.sender_name}`);
      this.logger.error(`Error: ${error}`);
      this.logger.error(`Stats: ${JSON.stringify(this.stats)}`);
      throw error;
    }
  }

  /**
   * Get current persona state (energy, mood, attention)
   * THROWS on failure
   */
  async getState(): Promise<PersonaState & { service_cadence_ms: number }> {
    this.assertReady('getState');
    const start = performance.now();

    try {
      const result = await this.client.cognitionGetState(this.personaId);
      const elapsed = performance.now() - start;

      this.logger.info(`State: mood=${result.mood}, energy=${result.energy.toFixed(2)}, attention=${result.attention.toFixed(2)} (${elapsed.toFixed(2)}ms)`);
      return result;
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`getState FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  /**
   * Enqueue message to Rust priority inbox
   * THROWS on failure
   */
  async enqueueMessage(message: InboxMessageRequest): Promise<void> {
    this.assertReady('enqueueMessage');
    this.stats.enqueueCalls++;
    const start = performance.now();

    try {
      await this.client.cognitionEnqueueMessage(this.personaId, message);
      const elapsed = performance.now() - start;
      this.stats.totalLatencyMs += elapsed;

      this.logger.info(`Enqueued message ${message.id} with priority ${message.priority.toFixed(3)} (${elapsed.toFixed(2)}ms)`);
    } catch (error) {
      this.stats.enqueueFailures++;
      const elapsed = performance.now() - start;
      this.logger.error(`enqueueMessage FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Message: ${JSON.stringify(message)}`);
      this.logger.error(`Error: ${error}`);
      this.logger.error(`Stats: ${JSON.stringify(this.stats)}`);
      throw error;
    }
  }

  // ========================================================================
  // Channel System — Multi-domain queue management in Rust
  // ========================================================================

  /**
   * Enqueue an item into Rust's channel system.
   * Routes to correct domain (AUDIO/CHAT/BACKGROUND) based on item type.
   * THROWS on failure
   */
  async channelEnqueue(item: ChannelEnqueueRequest): Promise<{ routed_to: ActivityDomain; status: ChannelRegistryStatus }> {
    this.assertReady('channelEnqueue');
    const start = performance.now();

    try {
      const result = await this.client.channelEnqueue(this.personaId, item);
      const elapsed = performance.now() - start;

      this.logger.info(`Channel enqueue: routed to ${result.routed_to}, total=${result.status.total_size} (${elapsed.toFixed(2)}ms)`);
      return result;
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`channelEnqueue FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  /**
   * Run one service cycle: consolidate + dequeue next item to process.
   * This is the main scheduling entry point replacing TS-side channel iteration.
   * THROWS on failure
   */
  async serviceCycle(): Promise<{
    should_process: boolean;
    item: any | null;
    channel: ActivityDomain | null;
    wait_ms: number;
    stats: ChannelRegistryStatus;
  }> {
    this.assertReady('serviceCycle');
    const start = performance.now();

    try {
      const result = await this.client.channelServiceCycle(this.personaId);
      const elapsed = performance.now() - start;

      if (result.should_process) {
        this.logger.info(`Service cycle: process ${result.channel} item (${elapsed.toFixed(2)}ms) total=${result.stats.total_size}`);
      } else if (elapsed > 5) {
        this.logger.warn(`Service cycle SLOW idle: ${elapsed.toFixed(2)}ms (target <1ms) wait=${result.wait_ms}ms`);
      }

      return result;
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`serviceCycle FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  /**
   * Service cycle + fast-path decision in ONE IPC call.
   * Eliminates a separate IPC round-trip for fastPathDecision.
   * Returns scheduling result + cognition decision together.
   * THROWS on failure
   */
  async serviceCycleFull(): Promise<{
    should_process: boolean;
    item: any | null;
    channel: ActivityDomain | null;
    wait_ms: number;
    stats: ChannelRegistryStatus;
    decision: { should_respond: boolean; confidence: number; reason: string; decision_time_ms: number; fast_path_used: boolean } | null;
  }> {
    this.assertReady('serviceCycleFull');
    const start = performance.now();

    try {
      const result = await this.client.channelServiceCycleFull(this.personaId);
      const elapsed = performance.now() - start;

      if (result.should_process) {
        const decisionStr = result.decision
          ? `respond=${result.decision.should_respond}, reason="${result.decision.reason}"`
          : 'no-decision';
        this.logger.info(`Service cycle full: process ${result.channel} item [${decisionStr}] (${elapsed.toFixed(2)}ms) total=${result.stats.total_size}`);
      }

      return result;
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`serviceCycleFull FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  /**
   * Get per-channel status snapshot
   * THROWS on failure
   */
  async channelStatus(): Promise<ChannelRegistryStatus> {
    this.assertReady('channelStatus');
    const start = performance.now();

    try {
      const result = await this.client.channelStatus(this.personaId);
      const elapsed = performance.now() - start;

      this.logger.info(`Channel status: total=${result.total_size}, urgent=${result.has_urgent_work} (${elapsed.toFixed(2)}ms)`);
      return result;
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`channelStatus FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  /**
   * Clear all channel queues
   * THROWS on failure
   */
  async channelClear(): Promise<void> {
    this.assertReady('channelClear');
    const start = performance.now();

    try {
      await this.client.channelClear(this.personaId);
      const elapsed = performance.now() - start;
      this.logger.info(`Channels cleared (${elapsed.toFixed(2)}ms)`);
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`channelClear FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  // ========================================================================
  // Memory Subsystem (Hippocampus in Rust — corpus-based, no SQL)
  // Corpus loaded at startup, recall/consciousness bypass TS event loop
  // ========================================================================

  /**
   * Load a persona's full memory corpus into Rust's in-memory cache.
   * Called at persona startup — sends all memories + timeline events from TS ORM.
   * Subsequent recall/consciousness operations run on this cached corpus.
   * THROWS on failure
   */
  async memoryLoadCorpus(
    memories: CorpusMemory[],
    events: CorpusTimelineEvent[]
  ): Promise<LoadCorpusResponse> {
    this.assertReady('memoryLoadCorpus');
    const start = performance.now();

    try {
      const result = await this.client.memoryLoadCorpus(this.personaId, memories, events);
      const elapsed = performance.now() - start;

      this.logger.info(`Corpus loaded: ${result.memory_count} memories (${result.embedded_memory_count} embedded), ${result.timeline_event_count} events (${result.embedded_event_count} embedded) in ${result.load_time_ms.toFixed(1)}ms (ipc=${elapsed.toFixed(1)}ms)`);
      return result;
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`memoryLoadCorpus FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`memories=${memories.length}, events=${events.length}`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  /**
   * Append a single memory to the cached corpus (incremental update).
   * Called after Hippocampus stores a new memory to the DB.
   * Keeps Rust cache coherent with the ORM without full reload.
   * THROWS on failure
   */
  async memoryAppendMemory(memory: CorpusMemory): Promise<void> {
    this.assertReady('memoryAppendMemory');
    const start = performance.now();

    try {
      await this.client.memoryAppendMemory(this.personaId, memory);
      const elapsed = performance.now() - start;

      this.logger.info(`Memory appended: ${memory.record.id} type=${memory.record.memory_type} embedded=${!!memory.embedding} (${elapsed.toFixed(1)}ms)`);
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`memoryAppendMemory FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`memory_id=${memory.record.id}`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  /**
   * Append a single timeline event to the cached corpus (incremental update).
   * THROWS on failure
   */
  async memoryAppendEvent(event: CorpusTimelineEvent): Promise<void> {
    this.assertReady('memoryAppendEvent');
    const start = performance.now();

    try {
      await this.client.memoryAppendEvent(this.personaId, event);
      const elapsed = performance.now() - start;

      this.logger.info(`Event appended: ${event.event.id} type=${event.event.event_type} embedded=${!!event.embedding} (${elapsed.toFixed(1)}ms)`);
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`memoryAppendEvent FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`event_id=${event.event.id}`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  /**
   * 6-layer parallel multi-recall — the primary recall API
   * Runs Core, Semantic, Temporal, Associative, DecayResurface, CrossContext in parallel
   * THROWS on failure
   */
  async memoryMultiLayerRecall(params: MultiLayerRecallRequest): Promise<MemoryRecallResponse> {
    this.assertReady('memoryMultiLayerRecall');
    const start = performance.now();

    try {
      const result = await this.client.memoryMultiLayerRecall(this.personaId, params);
      const elapsed = performance.now() - start;

      const layerSummary = result.layer_timings
        .map(l => `${l.layer}(${l.results_found}/${l.time_ms.toFixed(1)}ms)`)
        .join(', ');
      this.logger.info(`Multi-layer recall: ${result.memories.length}/${result.total_candidates} memories, layers=[${layerSummary}] total=${result.recall_time_ms.toFixed(1)}ms (ipc=${elapsed.toFixed(1)}ms)`);

      if (elapsed > 100) {
        this.logger.warn(`Multi-layer recall SLOW: ${elapsed.toFixed(1)}ms (target <50ms)`);
      }

      return result;
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`memoryMultiLayerRecall FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`query="${params.query_text}", room=${params.room_id}`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  /**
   * Build consciousness context for RAG injection
   * Replaces UnifiedConsciousness.getContext() — temporal + cross-context + intentions
   * THROWS on failure
   */
  async memoryConsciousnessContext(
    roomId: string,
    currentMessage?: string,
    skipSemanticSearch?: boolean
  ): Promise<ConsciousnessContextResponse> {
    this.assertReady('memoryConsciousnessContext');
    const start = performance.now();

    try {
      const result = await this.client.memoryConsciousnessContext(
        this.personaId,
        roomId,
        currentMessage,
        skipSemanticSearch
      );
      const elapsed = performance.now() - start;

      this.logger.info(`Consciousness context: events=${result.cross_context_event_count}, intentions=${result.active_intention_count}, peripheral=${result.has_peripheral_activity} (build=${result.build_time_ms.toFixed(1)}ms, ipc=${elapsed.toFixed(1)}ms)`);

      if (elapsed > 100) {
        this.logger.warn(`Consciousness context SLOW: ${elapsed.toFixed(1)}ms (target <20ms)`);
      }

      return result;
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`memoryConsciousnessContext FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`roomId=${roomId}`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  // ========================================================================
  // Text Analysis — Unified similarity + semantic loop checking in Rust
  // Replaces 3 duplicate Jaccard implementations in TS
  // ========================================================================

  /**
   * Compute text similarity using Rust's unified Jaccard implementation.
   * Returns both character-bigram and word-ngram similarity in one IPC call.
   * THROWS on failure
   */
  async textSimilarity(text1: string, text2: string): Promise<TextSimilarityResult> {
    this.assertReady('textSimilarity');
    const start = performance.now();

    try {
      const result = await this.client.cognitionTextSimilarity(text1, text2);
      const elapsed = performance.now() - start;

      this.logger.info(`TextSimilarity: ngram=${result.ngram_similarity.toFixed(3)}, char=${result.char_similarity.toFixed(3)}, compute=${result.compute_time_us}us (ipc=${elapsed.toFixed(2)}ms)`);
      return result;
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`textSimilarity FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  /**
   * Check if a response is semantically looping against conversation history.
   * Uses word-ngram Jaccard: blocks at 95%, warns at 80%.
   * THROWS on failure
   */
  async checkSemanticLoop(
    responseText: string,
    history: ConversationMessage[],
    maxHistory?: number
  ): Promise<SemanticLoopResult> {
    this.assertReady('checkSemanticLoop');
    const start = performance.now();

    try {
      const result = await this.client.cognitionCheckSemanticLoop(responseText, history, maxHistory);
      const elapsed = performance.now() - start;

      if (result.should_block) {
        this.logger.warn(`SemanticLoop BLOCKED: ${result.reason} (similarity=${result.similarity.toFixed(3)}, ${elapsed.toFixed(2)}ms)`);
      } else {
        this.logger.info(`SemanticLoop: pass, similarity=${result.similarity.toFixed(3)} (${elapsed.toFixed(2)}ms)`);
      }
      return result;
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`checkSemanticLoop FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  // ========================================================================
  // Phase 2: Combined Validation — 4 gates in 1 IPC call
  // ========================================================================

  /**
   * Run ALL response validation gates in a single Rust IPC call:
   * 1. Garbage detection (8 checks)
   * 2. Response loop detection (per-persona DashMap state)
   * 3. Truncated tool call detection
   * 4. Semantic loop detection
   * THROWS on failure
   */
  async validateResponse(
    responseText: string,
    hasToolCalls: boolean,
    conversationHistory?: ConversationMessage[]
  ): Promise<ValidationResult> {
    this.assertReady('validateResponse');
    const start = performance.now();

    try {
      const result = await this.client.cognitionValidateResponse(
        this.personaId,
        responseText,
        hasToolCalls,
        conversationHistory
      );
      const elapsed = performance.now() - start;

      if (!result.passed) {
        this.logger.warn(`Validation FAILED: gate=${result.gate_failed}, compute=${result.total_time_us}us (ipc=${elapsed.toFixed(2)}ms)`);
      } else {
        this.logger.info(`Validation passed: compute=${result.total_time_us}us (ipc=${elapsed.toFixed(2)}ms)`);
      }
      return result;
    } catch (error) {
      const elapsed = performance.now() - start;
      this.logger.error(`validateResponse FAILED after ${elapsed.toFixed(2)}ms`);
      this.logger.error(`Error: ${error}`);
      throw error;
    }
  }

  /**
   * Get bridge stats for debugging
   */
  getStats(): typeof this.stats {
    return { ...this.stats };
  }

  /**
   * Cleanup on persona shutdown
   */
  disconnect(): void {
    this.logger.info(`Disconnecting. Final stats: ${JSON.stringify(this.stats)}`);
    if (this.connected) {
      this.client.disconnect();
      this.connected = false;
      this.engineCreated = false;
    }
  }
}

/**
 * Helper to convert ChatMessageEntity to InboxMessageRequest
 */
export function toInboxMessageRequest(
  message: {
    id: UUID;
    roomId: UUID;
    senderId: UUID;
    senderName?: string;
    content: string;
    timestamp: Date | number;
  },
  senderType: SenderType,
  priority: number,
  sourceModality?: 'chat' | 'voice',
  voiceSessionId?: UUID
): InboxMessageRequest {
  return {
    id: message.id,
    room_id: message.roomId,
    sender_id: message.senderId,
    sender_name: message.senderName || 'Unknown',
    sender_type: senderType,
    content: message.content,
    timestamp: typeof message.timestamp === 'number'
      ? message.timestamp
      : message.timestamp.getTime(),
    priority,
    source_modality: sourceModality,
    voice_session_id: voiceSessionId,
  };
}
