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

import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type {
  InboxMessageRequest,
  CognitionDecision,
  PriorityScore,
  PersonaState,
  SenderType,
} from '../../../../shared/generated';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { SubsystemLogger } from './being/logging/SubsystemLogger';

const SOCKET_PATH = '/tmp/continuum-core.sock';

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
