/**
 * Training Example Entity
 *
 * Stores individual training examples for MLX fine-tuning.
 * Uses standard entity pattern for consistency with rest of system.
 */

import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { TextField, NumberField, JsonField } from '../../../../system/data/decorators/FieldDecorators';

/**
 * Training message in conversation
 */
export interface TrainingMessage {
  readonly role: 'system' | 'user' | 'assistant';
  readonly content: string;
}

export class TrainingExampleEntity extends BaseEntity {
  static readonly collection = 'training_examples';

  get collection(): string {
    return TrainingExampleEntity.collection;
  }

  /** JSON array of conversation messages */
  @JsonField()
  messages!: readonly TrainingMessage[];

  /** Number of turns in conversation */
  @NumberField()
  messageCount!: number;

  /** Approximate token count */
  @NumberField()
  totalTokens!: number;

  /** Optional metadata (source, quality score, etc.) */
  @JsonField()
  metadata!: Record<string, unknown>;

  validate(): { success: boolean; error?: string } {
    if (!this.id) {
      return { success: false, error: 'Missing required field: id' };
    }

    if (!this.messages || !Array.isArray(this.messages)) {
      return { success: false, error: 'Missing or invalid field: messages' };
    }

    if (this.messages.length === 0) {
      return { success: false, error: 'messages array cannot be empty' };
    }

    // Validate each message has role and content
    for (let i = 0; i < this.messages.length; i++) {
      const msg = this.messages[i];
      if (!msg.role || !msg.content) {
        return { success: false, error: `Invalid message at index ${i}: missing role or content` };
      }
      if (!['system', 'user', 'assistant'].includes(msg.role)) {
        return { success: false, error: `Invalid message at index ${i}: role must be system, user, or assistant` };
      }
    }

    if (typeof this.messageCount !== 'number' || this.messageCount < 1) {
      return { success: false, error: 'Invalid messageCount' };
    }

    if (typeof this.totalTokens !== 'number' || this.totalTokens < 0) {
      return { success: false, error: 'Invalid totalTokens' };
    }

    return { success: true };
  }
}
