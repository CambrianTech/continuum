/**
 * Process-Based Daemon - Foundation for OS process-isolated daemons
 * Extends BaseDaemon with async queue processing and IPC communication
 */

import { BaseDaemon } from './BaseDaemon';
import type { DaemonMessage, DaemonResponse } from './DaemonProtocol';
import type { ContinuumContext } from '../../types/shared/core/ContinuumTypes';
import { AsyncQueue } from './AsyncQueue';

export interface ProcessBasedDaemonConfig {
  queueSize?: number;
  batchSize?: number;
  processTimeoutMs?: number;
  resourceLimits?: {
    memory?: string;
    cpu?: string;
  };
}

export abstract class ProcessBasedDaemon<TMessage = unknown> extends BaseDaemon {
  protected queue: AsyncQueue<DaemonMessage<TMessage>>;
  protected config: ProcessBasedDaemonConfig;
  protected isProcessing = false;
  protected shutdownSignal = false;

  constructor(context?: ContinuumContext, config: ProcessBasedDaemonConfig = {}) {
    super(context);
    this.config = {
      queueSize: 10000,
      batchSize: 100,
      processTimeoutMs: 30000,
      ...config
    };
    this.queue = new AsyncQueue<DaemonMessage<TMessage>>(this.config.queueSize);
  }

  /**
   * Process a single message from the queue
   * Override this in derived classes for specific message handling
   */
  protected abstract processMessage(message: DaemonMessage<TMessage>): Promise<DaemonResponse>;

  /**
   * Process messages in batches for efficiency
   * Override this for batch processing (e.g., bulk file writes)
   */
  protected async processBatch(messages: DaemonMessage<TMessage>[]): Promise<DaemonResponse[]> {
    const results: DaemonResponse[] = [];
    for (const message of messages) {
      try {
        const response = await this.processMessage(message);
        results.push(response);
      } catch (error) {
        results.push({
          success: false,
          error: error instanceof Error ? error.message : String(error),
          messageId: message.id
        });
      }
    }
    return results;
  }

  /**
   * Enqueue message for async processing
   * Returns immediately without blocking
   */
  async enqueueMessage(message: DaemonMessage<TMessage>): Promise<void> {
    try {
      await this.queue.enqueue(message);
    } catch (error) {
      // Queue is full or daemon is shutting down
      throw new Error(`Failed to enqueue message: ${error}`);
    }
  }

  /**
   * Handle message from BaseDaemon - routes to queue
   */
  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    await this.enqueueMessage(message as DaemonMessage<TMessage>);
    return { success: true };
  }

  /**
   * Start the daemon and begin processing queue
   */
  async start(): Promise<void> {
    await super.start();
    this.startQueueProcessor();
  }

  /**
   * Stop the daemon and drain the queue
   */
  async stop(): Promise<void> {
    this.shutdownSignal = true;
    await this.queue.drain(); // Process remaining messages
    await super.stop();
  }

  /**
   * Called when daemon starts - override in derived classes
   */
  protected async onStart(): Promise<void> {
    // Default implementation - derived classes can override
  }

  /**
   * Called when daemon stops - override in derived classes
   */
  protected async onStop(): Promise<void> {
    // Default implementation - derived classes can override
  }

  /**
   * Main queue processing loop
   */
  private startQueueProcessor(): void {
    if (this.isProcessing) return;
    this.isProcessing = true;

    // Start background processing
    this.processQueueLoop().catch(error => {
      console.error(`Queue processor error in ${this.name}:`, error);
      this.emit('error', error);
    });
  }

  /**
   * Background queue processing loop
   */
  private async processQueueLoop(): Promise<void> {
    while (!this.shutdownSignal) {
      try {
        const messages = await this.queue.dequeueBatch(this.config.batchSize);
        if (messages.length > 0) {
          await this.processBatch(messages);
        }
      } catch (error) {
        console.error(`Queue processing error in ${this.name}:`, error);
        // Continue processing despite errors
      }
    }
    this.isProcessing = false;
  }

  /**
   * Get queue status for monitoring
   */
  getQueueStatus(): {
    size: number;
    isProcessing: boolean;
    maxSize: number;
  } {
    return {
      size: this.queue.size(),
      isProcessing: this.isProcessing,
      maxSize: this.config.queueSize ?? 10000
    };
  }
}