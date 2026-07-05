/**
 * PersonaTrainingSignalExtractor - Continuous micro-LoRA training signal detection
 *
 * Extracted from PersonaMessageEvaluator to isolate training signal logic.
 * Detects human corrections/approvals via AI classification and buffers
 * them for trait-specific LoRA fine-tuning.
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '../../../shared/Constants';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { ProcessableMessage } from './QueueItemTypes';
import { SignalDetector, getSignalDetector } from './SignalDetector';
import { getTrainingBuffer } from './TrainingBuffer';

export class PersonaTrainingSignalExtractor {
  private readonly signalDetector: SignalDetector;
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
    this.signalDetector = getSignalDetector();
  }

  /**
   * Detect training signals from messages and add to training buffer.
   * When a human corrects or approves an AI response, we capture that
   * as a training signal for the appropriate trait adapter.
   */
  async detectAndBufferTrainingSignal(messageEntity: ProcessableMessage): Promise<void> {
    try {
      const precedingAIMessage = await this.getPrecedingAIMessage(messageEntity);
      const conversationHistory = await this.getRecentConversationHistory(messageEntity.roomId);

      const signal = await this.signalDetector.detectSignalAsync(
        messageEntity,
        precedingAIMessage,
        conversationHistory
      );

      if (signal && signal.type !== 'none') {
        this.log(`🎓 ${this.personaName}: Training signal detected via AI classification`);
        this.log(`   Type: ${signal.type}, Trait: ${signal.trait}, Polarity: ${signal.polarity}`);
        this.log(`   Confidence: ${(signal.confidence * 100).toFixed(0)}%`);

        const trainingLogger = (msg: string) => this.log(`[TrainingBuffer] ${msg}`);
        const buffer = getTrainingBuffer(this.personaId, this.personaName, trainingLogger);
        const trainingTriggered = await buffer.add(signal);

        if (trainingTriggered) {
          this.log(`🔥 ${this.personaName}: Micro-training triggered for ${signal.trait}!`);
        }
      }
    } catch (error) {
      this.log(`⚠️ ${this.personaName}: Signal detection error (non-fatal):`, error);
    }
  }

  /**
   * Get the preceding AI message before a given message (for correction detection).
   * Only returns messages from THIS persona (learning from own corrections).
   */
  private async getPrecedingAIMessage(humanMessage: ProcessableMessage): Promise<ChatMessageEntity | null> {
    try {
      const result = await ORM.query<ChatMessageEntity>({
        collection: COLLECTIONS.CHAT_MESSAGES,
        filter: {
          roomId: humanMessage.roomId,
          timestamp: { $lt: humanMessage.timestamp },
          senderType: { $ne: 'human' }
        },
        sort: [{ field: 'timestamp', direction: 'desc' }],
        limit: 1
      }, 'default');

      if (result.success && result.data && result.data.length > 0) {
        const msg = result.data[0].data;
        if (msg.senderId === this.personaId) {
          return msg;
        }
      }
      return null;
    } catch {
      return null;
    }
  }

  /**
   * Get recent conversation history for training context.
   */
  private async getRecentConversationHistory(roomId: UUID, limit: number = 10): Promise<ChatMessageEntity[]> {
    try {
      const result = await ORM.query<ChatMessageEntity>({
        collection: COLLECTIONS.CHAT_MESSAGES,
        filter: { roomId },
        sort: [{ field: 'timestamp', direction: 'desc' }],
        limit
      }, 'default');

      if (result.success && result.data) {
        return result.data.map(record => record.data).reverse();
      }
      return [];
    } catch {
      return [];
    }
  }
}
