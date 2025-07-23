/**
 * JTAG Message Queue - Composes generic components for JTAG-specific needs
 */

import { JTAGMessage, JTAGContext } from '../JTAGTypes';
import { PriorityQueue, Priority, QueuedItem } from './PriorityQueue';
import { DeduplicationService } from './DeduplicationService';

// Re-export for convenience
export { Priority as MessagePriority };

export interface JTAGQueueConfig {
  maxSize?: number;
  maxRetries?: number;
  flushInterval?: number;
  enableDeduplication?: boolean;
  deduplicationWindow?: number;
  persistenceKey?: string;
}

export class JTAGMessageQueue {
  private priorityQueue: PriorityQueue<JTAGMessage>;
  private deduplicationService: DeduplicationService<JTAGMessage>;
  private context: JTAGContext;

  constructor(context: JTAGContext, config: JTAGQueueConfig = {}) {
    this.context = context;

    // Initialize priority queue
    this.priorityQueue = new PriorityQueue<JTAGMessage>({
      maxSize: config.maxSize || 1000,
      maxRetries: config.maxRetries || 3,
      flushInterval: config.flushInterval || 500,
      persistenceKey: config.persistenceKey || 
        (context.environment === 'browser' ? 'jtag-message-queue' : undefined)
    });

    // Initialize deduplication service
    this.deduplicationService = new DeduplicationService<JTAGMessage>({
      enabled: config.enableDeduplication ?? true,
      windowMs: config.deduplicationWindow || 60000
    });

    console.log(`📦 JTAGMessageQueue[${context.environment}]: Initialized with modular components`);
  }

  /**
   * Enqueue message with deduplication check
   */
  enqueue(message: JTAGMessage, priority: Priority = Priority.NORMAL): boolean {
    // Check for duplicates first
    if (!this.deduplicationService.shouldProcess(message)) {
      console.log(`🚫 JTAGMessageQueue[${this.context.environment}]: Prevented duplicate message`);
      return false; // Message was deduplicated
    }

    // Add to priority queue
    const id = this.priorityQueue.enqueue(message, priority);
    console.log(`📥 JTAGMessageQueue[${this.context.environment}]: Queued message ${id} (priority: ${Priority[priority]})`);
    return true; // Message was queued
  }

  /**
   * Start processing queue
   */
  startProcessing(flushHandler: (messages: QueuedItem<JTAGMessage>[]) => Promise<QueuedItem<JTAGMessage>[]>): void {
    this.priorityQueue.startProcessing(flushHandler);
    console.log(`🌊 JTAGMessageQueue[${this.context.environment}]: Started processing`);
  }

  /**
   * Stop processing queue
   */
  stopProcessing(): void {
    this.priorityQueue.stopProcessing();
    console.log(`⏹️ JTAGMessageQueue[${this.context.environment}]: Stopped processing`);
  }

  /**
   * Get comprehensive status
   */
  getStatus() {
    return {
      ...this.priorityQueue.status,
      deduplication: this.deduplicationService.statistics
    };
  }

  /**
   * Clear queue and deduplication history
   */
  clear(): void {
    this.priorityQueue.clear();
    this.deduplicationService.clear();
    console.log(`🧹 JTAGMessageQueue[${this.context.environment}]: Cleared`);
  }
}