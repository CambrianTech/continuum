/**
 * AttentionManager - Batches room activity for intelligent catch-up
 *
 * Human analogy: Peripheral awareness that decides "do I need to pay attention?"
 * Computing analogy: Interrupt coalescing + batch processing
 *
 * Instead of processing every message individually:
 * - Batches messages by room
 * - Waits for conversation to settle (debouncing)
 * - ONE evaluation per batch instead of N evaluations
 *
 * Result: 10 messages → 1 batch → 1 LLM call (10x efficiency)
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import { RoomActivityBatch } from './RoomActivityBatch';

export interface BatchProcessor {
  processBatch(batch: RoomActivityBatch): Promise<void>;
}

export class AttentionManager {
  private activeBatches = new Map<UUID, RoomActivityBatch>();
  private readonly processor: BatchProcessor;
  private readonly myPersonaId: UUID;
  private readonly debounceMs: number;

  constructor(
    myPersonaId: UUID,
    processor: BatchProcessor,
    debounceMs: number = 500
  ) {
    this.myPersonaId = myPersonaId;
    this.processor = processor;
    this.debounceMs = debounceMs;
  }

  /**
   * Handle incoming message - add to batch instead of processing immediately
   */
  onMessageCreated(message: ChatMessageEntity): void {
    // Ignore own messages immediately
    if (message.senderId === this.myPersonaId) {
      return;
    }

    // Get or create batch for this room
    let batch = this.activeBatches.get(message.roomId);
    if (!batch) {
      batch = new RoomActivityBatch(
        message.roomId,
        this.handleBatchReady.bind(this),
        this.debounceMs
      );
      this.activeBatches.set(message.roomId, batch);
    }

    // Add message to batch (resets debounce timer)
    batch.addMessage(message);
  }

  /**
   * Called when a batch is ready for processing (after quiet period)
   */
  private async handleBatchReady(batch: RoomActivityBatch): Promise<void> {
    // Remove from active batches
    this.activeBatches.delete(batch.roomId);

    // Process batch (ONE evaluation for all messages)
    await this.processor.processBatch(batch);
  }

  /**
   * Shutdown - cancel all pending batches
   */
  shutdown(): void {
    for (const batch of this.activeBatches.values()) {
      batch.cancel();
    }
    this.activeBatches.clear();
  }

  /**
   * Get statistics for monitoring
   */
  getStats(): {
    activeBatches: number;
    batchDetails: Array<{
      roomId: UUID;
      messageCount: number;
    }>;
  } {
    return {
      activeBatches: this.activeBatches.size,
      batchDetails: Array.from(this.activeBatches.values()).map(batch => ({
        roomId: batch.roomId,
        messageCount: batch.messages.length
      }))
    };
  }
}
