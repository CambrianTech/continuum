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
