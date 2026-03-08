/**
 * PersonaMessageGate - Echo chamber prevention and post-inference validation
 *
 * Extracted from PersonaMessageEvaluator to isolate gating logic.
 * Handles:
 * - Echo chamber detection (AI-to-AI loop prevention)
 * - Post-inference adequacy checks (skip if humans or adequate AIs already responded)
 * - Recent message cache management
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { Events } from '../../../core/shared/Events';
import { COLLECTIONS } from '../../../shared/Constants';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { ProcessableMessage } from './QueueItemTypes';
import type { RustCognitionBridge } from './RustCognitionBridge';
import { PersonaTimingConfig } from './PersonaTimingConfig';

export class PersonaMessageGate {
  // In-memory recent message cache — eliminates SQLite queries for post-inference validation.
  private static _recentMessages: Map<string, ChatMessageEntity[]> = new Map();
  private static _cacheInitialized = false;
  private static readonly MAX_CACHED_PER_ROOM = PersonaTimingConfig.messageCache.maxPerRoom;

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

  private static initMessageCache(): void {
    if (PersonaMessageGate._cacheInitialized) return;
    PersonaMessageGate._cacheInitialized = true;

    Events.subscribe(`data:${COLLECTIONS.CHAT_MESSAGES}:created`, (entity: any) => {
      const msg = entity as ChatMessageEntity;
      if (!msg.roomId) return;
      const roomId = msg.roomId;
      let messages = PersonaMessageGate._recentMessages.get(roomId);
      if (!messages) {
        messages = [];
        PersonaMessageGate._recentMessages.set(roomId, messages);
      }
      messages.push(msg);
      if (messages.length > PersonaMessageGate.MAX_CACHED_PER_ROOM) {
        messages.shift();
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
   * Detect echo chamber: AI-to-AI conversations without human participation.
   * Returns true if echo chamber is detected (should NOT respond).
   */
  isEchoChamber(
    messageEntity: ProcessableMessage,
    senderIsHuman: boolean,
    isMentioned: boolean,
  ): boolean {
    if (senderIsHuman || isMentioned) return false;

    const twoMinutesAgo = new Date(Date.now() - PersonaTimingConfig.echoChamber.windowMs);
    const recentMessages = this.getRecentMessagesSince(messageEntity.roomId, twoMinutesAgo);

    const hasHumanRecently = recentMessages.some(m => m.senderType === 'human');
    const aiMessageCount = recentMessages.filter(m => m.senderType !== 'human').length;

    if (!hasHumanRecently && aiMessageCount >= PersonaTimingConfig.echoChamber.aiMessageThreshold) {
      this.log(`🔇 ${this.personaName}: Echo chamber detected (${aiMessageCount} AI messages, no human in 2min)`);
      return true;
    }
    return false;
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

    // Check if adequate AI responses exist
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
