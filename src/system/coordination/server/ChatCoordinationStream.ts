/**
 * Chat Coordination Stream - Chat-specific coordination adapter
 *
 * Extracted from ThoughtStreamCoordinator (Phase 1, Commit 1.3)
 *
 * Extends BaseCoordinationStream to provide chat-specific coordination:
 * - Maps messageId → eventId
 * - Maps roomId → contextId
 * - Adds chat-specific decision data (intentions, etc.)
 *
 * This demonstrates the elegance of the base class pattern:
 * - Base provides coordination primitives
 * - Subclass adapts to chat domain
 * - Future: GameCoordinationStream, CodeCoordinationStream, etc.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  BaseCoordinationStream,
  type BaseThought,
  type BaseDecision,
  type BaseStream,
  type CoordinationConfig
} from '../shared/BaseCoordinationStream';

/**
 * Chat-specific thought (extends base with chat metadata)
 */
export interface ChatThought extends BaseThought {
  roomId: UUID;          // Chat-specific: room context
  messageId: string;     // Chat-specific: message being responded to
}

/**
 * Chat-specific decision (extends base with chat metadata)
 */
export interface ChatDecision extends BaseDecision {
  messageId: string;     // Alias of eventId for backwards compatibility
  roomId: UUID;          // Alias of contextId for backwards compatibility
  intentions: Array<{    // Chat-specific: detailed intention data
    personaId: UUID;
    confidence: number;
    responseType: string;
  }>;
}

/**
 * Chat-specific stream (extends base with chat state)
 */
export interface ChatStream extends BaseStream<ChatThought> {
  messageId: string;     // Alias of eventId for clarity
  roomId: UUID;          // Alias of contextId for clarity
}

/**
 * Chat Coordination Stream
 *
 * Adapts universal coordination to chat-specific domain
 */
export class ChatCoordinationStream extends BaseCoordinationStream<ChatThought, ChatDecision, ChatStream> {

  // ============================================================================
  // PHASE 3BIS: ACTIVITY AMBIENT STATE - Temperature tracking
  // ============================================================================

  private roomTemperatures = new Map<UUID, number>();
  private roomUserPresent = new Map<UUID, boolean>();
  private decayInterval: NodeJS.Timeout | null = null;

  // Temperature decay constants (exponential/natural decay)
  private static readonly DECAY_RATE = 0.95;           // 5% decay per interval (exponential)
  private static readonly DECAY_INTERVAL_MS = 10000;   // 10 seconds
  private static readonly TEMP_FLOOR = 0.01;           // Never fully cold (rooms stay "warm")

  // ============================================================================
  // ABSTRACT METHOD IMPLEMENTATIONS - Required by base class
  // ============================================================================

  protected getDomainName(): string {
    return 'Chat';
  }

  protected createStream(eventId: string, contextId: UUID): ChatStream {
    const maxResponders = this.getMaxResponders();

    return {
      // Base stream fields
      eventId,
      contextId,
      phase: 'gathering',
      thoughts: [],
      considerations: new Map(),
      startTime: Date.now(),
      availableSlots: maxResponders,
      claimedBy: new Set(),

      // Chat-specific aliases (for backwards compatibility)
      messageId: eventId,
      roomId: contextId
    };
  }

  protected convertDecision(baseDecision: BaseDecision, stream: ChatStream): ChatDecision {
    // Build chat-specific intentions array
    const intentions = Array.from(stream.considerations.values()).map(thought => ({
      personaId: thought.personaId,
      confidence: thought.confidence,
      responseType: 'answer' as const
    }));

    return {
      ...baseDecision,
      messageId: baseDecision.eventId,   // Alias for backwards compatibility
      roomId: baseDecision.contextId,    // Alias for backwards compatibility
      intentions
    };
  }

  protected getEventLogContext(eventId: string): string {
    return `message ${eventId.slice(0, 8)}`;
  }

  // ============================================================================
  // PROTECTED HOOK OVERRIDES - Chat-specific customization
  // ============================================================================

  /**
   * Chat-specific: Log thought with room context
   */
  protected onThoughtBroadcast(stream: ChatStream, thought: ChatThought): void {
    // Could add chat-specific validation, metrics, etc.
    // For now, just rely on base class logging
  }

  /**
   * Chat-specific: Validate claim based on room membership
   * (Future: Could check if persona is actually in the room)
   */
  protected onClaim(stream: ChatStream, thought: ChatThought): boolean {
    // Chat validation: persona must be considering message in same room
    if (thought.roomId !== stream.roomId) {
      this.log(`❌ Chat validation failed: thought.roomId=${thought.roomId} !== stream.roomId=${stream.roomId}`, true);
      return false;
    }

    return true; // Allow claim
  }

  /**
   * Chat-specific: Post-process decision (could add chat-specific metrics)
   */
  protected onDecisionMade(stream: ChatStream, decision: ChatDecision): void {
    // Could emit chat-specific events, update room stats, etc.
    // For now, just rely on base class behavior
  }

  // ============================================================================
  // PUBLIC CHAT-SPECIFIC API - Convenience methods for chat domain
  // ============================================================================

  /**
   * Chat-specific: Broadcast thought using chat terminology
   */
  async broadcastChatThought(messageId: string, roomId: UUID, thought: ChatThought): Promise<void> {
    // Ensure chat-specific fields are set
    thought.messageId = messageId;
    thought.roomId = roomId;

    // Delegate to base class (using generic eventId/contextId)
    await this.broadcastThought(messageId, roomId, thought);
  }

  /**
   * Chat-specific: Wait for chat decision
   */
  async waitForChatDecision(messageId: string, timeoutMs?: number): Promise<ChatDecision | null> {
    return this.waitForDecision(messageId, timeoutMs);
  }

  /**
   * Chat-specific: Check if persona can respond to message
   */
  async canRespondToMessage(personaId: UUID, messageId: string): Promise<boolean> {
    return this.checkPermission(personaId, messageId);
  }

  /**
   * Chat-specific: Get stream by message ID
   */
  getChatStream(messageId: string): ChatStream | undefined {
    return this.getStream(messageId);
  }

  /**
   * Chat-specific: Get all active message streams
   */
  getActiveMessageStreams(): Map<string, ChatStream> {
    return this.getStreams();
  }

  // ============================================================================
  // PHASE 3BIS: TEMPERATURE MANAGEMENT API
  // ============================================================================

  /**
   * Called when a human sends a message (increases temperature)
   */
  onHumanMessage(roomId: UUID): void {
    const current = this.roomTemperatures.get(roomId) ?? 0.5;  // Default to neutral
    const newTemp = Math.min(1.0, current + 0.3);
    this.roomTemperatures.set(roomId, newTemp);
    this.log(`🌡️ Temperature +0.3 (human message): room=${roomId.slice(0, 8)} temp=${newTemp.toFixed(2)}`);
  }

  /**
   * Called when a persona responds in a room (conversation is active — warm it)
   *
   * Previous design subtracted -0.2 per response, killing rooms within seconds
   * when 14 personas all serviced simultaneously. That's backwards — a response
   * means the conversation is alive. The coordination stream's maxResponders
   * already prevents pile-ons. Temperature should reflect activity, not throttle it.
   */
  onMessageServiced(roomId: UUID, personaId?: UUID): void {
    const current = this.roomTemperatures.get(roomId) ?? 0.5;
    const newTemp = Math.min(1.0, current + 0.05);
    this.roomTemperatures.set(roomId, newTemp);
    const who = personaId ? ` by ${personaId.slice(0, 8)}` : '';
    this.log(`🌡️ Temperature +0.05 (AI active${who}): room=${roomId.slice(0, 8)} temp=${newTemp.toFixed(2)}`);
  }

  /**
   * Called when user enters/leaves tab (affects temperature and presence)
   */
  onUserPresent(roomId: UUID, present: boolean): void {
    this.roomUserPresent.set(roomId, present);

    if (!present) {
      // User left - significant temperature drop
      const current = this.roomTemperatures.get(roomId) ?? 0.5;
      const newTemp = Math.max(0, current - 0.4);
      this.roomTemperatures.set(roomId, newTemp);
      this.log(`🌡️ Temperature -0.4 (user left): room=${roomId.slice(0, 8)} temp=${newTemp.toFixed(2)}`);
    } else {
      // User returned - moderate temperature increase
      const current = this.roomTemperatures.get(roomId) ?? 0.5;
      const newTemp = Math.min(1.0, current + 0.2);
      this.roomTemperatures.set(roomId, newTemp);
      this.log(`🌡️ Temperature +0.2 (user present): room=${roomId.slice(0, 8)} temp=${newTemp.toFixed(2)}`);
    }
  }

  /**
   * Get current temperature for a room (0.0-1.0)
   */
  getTemperature(roomId: UUID): number {
    return this.roomTemperatures.get(roomId) ?? 0.5;  // Default to neutral
  }

  /**
   * Get user presence status for a room
   */
  isUserPresent(roomId: UUID): boolean {
    return this.roomUserPresent.get(roomId) ?? false;
  }

  /**
   * Start exponential temperature decay loop (called on initialization)
   */
  private startTemperatureDecay(): void {
    if (this.decayInterval) {
      return;  // Already running
    }

    this.decayInterval = setInterval(() => {
      for (const [roomId, temp] of this.roomTemperatures) {
        // Only decay if no recent activity (no thoughts in last 60s)
        const stream = this.getChatStream(roomId);
        const recentThoughts = stream?.thoughts.filter(
          t => Date.now() - t.timestamp < 60000
        ) ?? [];

        if (recentThoughts.length === 0 && temp > ChatCoordinationStream.TEMP_FLOOR) {
          // Exponential decay: temp * DECAY_RATE (natural/ln decay)
          const newTemp = temp * ChatCoordinationStream.DECAY_RATE;
          const finalTemp = Math.max(ChatCoordinationStream.TEMP_FLOOR, newTemp);
          this.roomTemperatures.set(roomId, finalTemp);
          this.log(`🌡️ Temperature decay (exponential): room=${roomId.slice(0, 8)} ${temp.toFixed(2)} → ${finalTemp.toFixed(2)}`);
        }
      }
    }, ChatCoordinationStream.DECAY_INTERVAL_MS);

    this.log('🌡️ Temperature decay loop started (10s interval, exponential)');
  }

  /**
   * Stop temperature decay loop (called on shutdown)
   */
  private stopTemperatureDecay(): void {
    if (this.decayInterval) {
      clearInterval(this.decayInterval);
      this.decayInterval = null;
      this.log('🌡️ Temperature decay loop stopped');
    }
  }

  /**
   * Override base class initialization to start temperature tracking
   */
  initialize(): void {
    this.startTemperatureDecay();
  }

  /**
   * Override base class shutdown to stop temperature tracking
   */
  override shutdown(): void {
    this.stopTemperatureDecay();
    super.shutdown();
  }
}

// ============================================================================
// SINGLETON PATTERN - Global chat coordinator instance
// ============================================================================

let chatCoordinatorInstance: ChatCoordinationStream | null = null;

/**
 * Get global chat coordinator instance
 */
export function getChatCoordinator(): ChatCoordinationStream {
  if (!chatCoordinatorInstance) {
    chatCoordinatorInstance = new ChatCoordinationStream();
    chatCoordinatorInstance.initialize();  // Start temperature tracking
  }
  return chatCoordinatorInstance;
}

/**
 * Reset chat coordinator (for testing)
 */
export function resetChatCoordinator(): void {
  if (chatCoordinatorInstance) {
    chatCoordinatorInstance.shutdown();
    chatCoordinatorInstance = null;
  }
}
