/**
 * PersonaMessageGate - Echo chamber prevention and post-inference validation
 *
 * Echo chamber detection is now in Rust (Gate 6 of full_evaluate).
 * This module handles:
 * - Feeding the Rust message cache (via IPC on new messages)
 * - Post-inference adequacy checks (uses TS cache for ChatMessageEntity fields + Rust IPC for similarity)
 * - Recent message cache for post-inference validation
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { Events } from '../../../core/shared/Events';
import { COLLECTIONS } from '../../../shared/Constants';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { ProcessableMessage } from './QueueItemTypes';
import type { RustCognitionBridge } from './RustCognitionBridge';
import { PersonaTimingConfig } from './PersonaTimingConfig';

export class PersonaMessageGate {
  // In-memory recent message cache — used for post-inference adequacy (needs ChatMessageEntity fields).
  // Echo chamber detection is now Rust-side (Gate 6 of full_evaluate).
  private static _recentMessages: Map<string, ChatMessageEntity[]> = new Map();
  private static _cacheInitialized = false;
  private static readonly MAX_CACHED_PER_ROOM = PersonaTimingConfig.messageCache.maxPerRoom;

  // Rust bridges to feed — all personas' bridges get message cache updates
  private static _rustBridges: Set<RustCognitionBridge> = new Set();

  private readonly personaId: UUID;
  private readonly personaName: string;
  private readonly log: (message: string, ...args: any[]) => void;

  constructor(
    personaId: UUID,
    personaName: string,
    log: (message: string, ...args: any[]) => void,
  ) {
    this.personaId = personaId;
    this.personaName = personaName;
    this.log = log;
    PersonaMessageGate.initMessageCache();
  }

  /**
   * Register a Rust bridge so it receives message cache updates.
   * Called once per persona after bridge initialization.
   */
  registerRustBridge(bridge: RustCognitionBridge): void {
    PersonaMessageGate._rustBridges.add(bridge);
  }

  /**
   * Unregister a Rust bridge during persona shutdown to prevent leaks.
   */
  static unregisterRustBridge(bridge: RustCognitionBridge | null): void {
    if (bridge) {
      PersonaMessageGate._rustBridges.delete(bridge);
    }
  }

  private static initMessageCache(): void {
    if (PersonaMessageGate._cacheInitialized) return;
    PersonaMessageGate._cacheInitialized = true;

    Events.subscribe(`data:${COLLECTIONS.CHAT_MESSAGES}:created`, (entity: any) => {
      const msg = entity as ChatMessageEntity;
      if (!msg.roomId) return;
      const roomId = msg.roomId;

      // TS-side cache (for post-inference adequacy)
      let messages = PersonaMessageGate._recentMessages.get(roomId);
      if (!messages) {
        messages = [];
        PersonaMessageGate._recentMessages.set(roomId, messages);
      }
      messages.push(msg);
      if (messages.length > PersonaMessageGate.MAX_CACHED_PER_ROOM) {
        messages.shift();
      }

      // Feed Rust-side cache (for echo chamber — Gate 6 of full_evaluate)
      const timestamp = msg.timestamp instanceof Date ? msg.timestamp.getTime() : new Date(msg.timestamp).getTime();
      for (const bridge of PersonaMessageGate._rustBridges) {
        bridge.cacheMessage(
          roomId,
          msg.id,
          msg.senderId,
          msg.senderType ?? 'human',
          msg.senderName ?? 'Unknown',
          msg.content?.text ?? '',
          timestamp,
        ).catch(() => { /* non-fatal */ });
      }
    });
  }

  /**
   * Get recent messages for a room from in-memory cache, filtered by timestamp.
   */
  getRecentMessagesSince(roomId: UUID, since: Date): ChatMessageEntity[] {
    const messages = PersonaMessageGate._recentMessages.get(roomId);
    if (!messages) return [];
    const sinceTime = since.getTime();
    return messages.filter(m => {
      const ts = m.timestamp instanceof Date ? m.timestamp.getTime() : new Date(m.timestamp).getTime();
      return ts > sinceTime;
    });
  }

  /**
   * Post-inference validation: check if context changed since evaluation started.
   * Returns { shouldSkip, reason } if a human already answered or adequate AI responses exist.
   */
  async checkPostInferenceAdequacy(
    messageEntity: ProcessableMessage,
    rustCognition: RustCognitionBridge,
  ): Promise<{ shouldSkip: boolean; reason?: string }> {
    const messageTimestamp = new Date(messageEntity.timestamp);
    const recentAfter = this.getRecentMessagesSince(messageEntity.roomId, messageTimestamp);

    // Filter to messages from OTHER senders
    const otherResponses = recentAfter.filter(m =>
      m.senderId !== this.personaId && m.id !== messageEntity.id
    );

    if (otherResponses.length === 0) {
      return { shouldSkip: false };
    }

    // Check if a human already answered substantively
    const humanResponses = otherResponses.filter(m => m.senderType === 'human');
    if (humanResponses.some(m => (m.content?.text?.length ?? 0) > 50)) {
      return { shouldSkip: true, reason: 'Human already answered substantively' };
    }

    // Check if adequate AI responses exist (Rust IPC — batch similarity check)
    const aiResponses = otherResponses.filter(m => m.senderType !== 'human');
    if (aiResponses.length > 0) {
      const originalText = messageEntity.content?.text || '';
      const responses = aiResponses.map(r => ({
        sender_name: r.senderName ?? 'Unknown',
        text: r.content?.text || '',
      }));

      const result = await rustCognition.checkAdequacy(originalText, responses);
      if (result.is_adequate) {
        return {
          shouldSkip: true,
          reason: `Adequate AI response exists: ${result.reason} (confidence: ${(result.confidence * 100).toFixed(0)}%)`,
        };
      }
    }

    return { shouldSkip: false };
  }
}
