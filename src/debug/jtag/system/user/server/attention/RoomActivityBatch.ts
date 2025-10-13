/**
 * RoomActivityBatch - Groups messages in a time window
 *
 * Human analogy: "There's activity in #general" not "10 individual pings"
 * Debounces messages, waits for conversation to settle before evaluation
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { TimerHandle } from '../../../core/types/CrossPlatformTypes';

export interface BatchReadyCallback {
  (batch: RoomActivityBatch): Promise<void>;
}

export class RoomActivityBatch {
  readonly roomId: UUID;
  readonly messages: ChatMessageEntity[] = [];

  private debounceTimer: TimerHandle | null = null;
  private readonly debounceMs: number;
  private readonly onBatchReady: BatchReadyCallback;

  constructor(
    roomId: UUID,
    onBatchReady: BatchReadyCallback,
    debounceMs: number = 500
  ) {
    this.roomId = roomId;
    this.onBatchReady = onBatchReady;
    this.debounceMs = debounceMs;
  }

  /**
   * Add message to batch, reset debounce timer
   * Waits for conversation to settle before processing
   */
  addMessage(message: ChatMessageEntity): void {
    this.messages.push(message);

    // Reset timer - wait for quiet period
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer);
    }

    this.debounceTimer = setTimeout(() => {
      this.triggerBatchReady();
    }, this.debounceMs);
  }

  /**
   * Cancel pending batch processing
   */
  cancel(): void {
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer);
      this.debounceTimer = null;
    }
  }

  /**
   * Get batch metadata for logging
   */
  getMetadata(): {
    roomId: UUID;
    messageCount: number;
    firstTimestamp: string;
    lastTimestamp: string;
  } {
    const firstMsg = this.messages[0];
    const lastMsg = this.messages[this.messages.length - 1];

    return {
      roomId: this.roomId,
      messageCount: this.messages.length,
      firstTimestamp: firstMsg
        ? (typeof firstMsg.timestamp === 'string' ? firstMsg.timestamp : firstMsg.timestamp.toISOString())
        : '',
      lastTimestamp: lastMsg
        ? (typeof lastMsg.timestamp === 'string' ? lastMsg.timestamp : lastMsg.timestamp.toISOString())
        : ''
    };
  }

  private triggerBatchReady(): void {
    this.debounceTimer = null;
    void this.onBatchReady(this);
  }
}
