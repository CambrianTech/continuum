/**
 * MessageProcessor - Dedicated message routing and processing logic
 * 
 * Extracted from JTAGRouter to improve testability and separation of concerns.
 * Handles the core message routing decisions without transport concerns.
 */

import type { JTAGMessage, JTAGEnvironment } from '../../types/JTAGTypes';
import { JTAGMessageTypes } from '../../types/JTAGTypes';
import { CorrelationManager } from './CorrelationManager';
import { RouterUtilities } from './RouterUtilities';
import { 
  REQUEST_TOKEN_PREFIX,
  RESPONSE_TOKEN_PREFIX,
  TIMEOUTS
} from './RouterConstants';
import type { RouterResult } from './RouterTypes';

export interface IMessageProcessor {
  processMessage<T extends RouterResult>(message: JTAGMessage): Promise<ProcessingResult<T>>;
  cleanup(): void;
}

export interface ProcessingResult<T extends RouterResult> {
  readonly action: 'route_local' | 'route_remote' | 'deduplicated' | 'cached';
  readonly targetEnvironment: JTAGEnvironment;
  readonly processingToken?: string;
  readonly result?: T;
}

/**
 * MessageProcessor - Clean separation of message processing logic
 */
export class MessageProcessor implements IMessageProcessor {
  private readonly correlationManager = new CorrelationManager();
  private readonly processedMessages = new Set<string>();
  private readonly currentEnvironment: JTAGEnvironment;

  constructor(currentEnvironment: JTAGEnvironment) {
    this.currentEnvironment = currentEnvironment;
  }

  async processMessage<T extends RouterResult>(message: JTAGMessage): Promise<ProcessingResult<T>> {
    // 1. Create processing token for deduplication
    const processingToken = this.createProcessingToken(message);
    
    // 2. Check for duplicates
    if (processingToken && this.processedMessages.has(processingToken)) {
      console.warn(`🔄 MessageProcessor: Skipping duplicate message ${processingToken}`);
      return {
        action: 'deduplicated',
        targetEnvironment: this.currentEnvironment,
        processingToken,
        result: { success: true, deduplicated: true } as T
      };
    }

    // 3. Mark as processing
    if (processingToken) {
      this.processedMessages.add(processingToken);
      this.scheduleTokenCleanup(processingToken);
    }

    // 4. Determine target environment
    const targetEnvironment = this.determineTargetEnvironment(message);

    // 5. Return routing decision
    const action = targetEnvironment === this.currentEnvironment ? 'route_local' : 'route_remote';
    
    return {
      action,
      targetEnvironment,
      processingToken
    };
  }

  /**
   * Create processing token for deduplication
   */
  private createProcessingToken(message: JTAGMessage): string | undefined {
    if (JTAGMessageTypes.isRequest(message)) {
      this.correlationManager.registerRequest(message.correlationId);
      return `${REQUEST_TOKEN_PREFIX}${message.correlationId}`;
    } else if (JTAGMessageTypes.isResponse(message)) {
      return `${RESPONSE_TOKEN_PREFIX}${message.correlationId}`;
    }
    // Events don't need deduplication tokens
    return undefined;
  }

  /**
   * Determine target environment for message
   */
  private determineTargetEnvironment(message: JTAGMessage): JTAGEnvironment {
    return RouterUtilities.extractEnvironment(message.endpoint, this.currentEnvironment);
  }

  /**
   * Schedule cleanup of processing token
   */
  private scheduleTokenCleanup(processingToken: string): void {
    setTimeout(() => {
      this.processedMessages.delete(processingToken);
    }, TIMEOUTS.MESSAGE_PROCESSING);
  }

  /**
   * Remove processing token on error for retry capability
   */
  removeProcessingToken(processingToken: string): void {
    this.processedMessages.delete(processingToken);
  }

  /**
   * Get correlation manager for external access
   */
  get correlations(): CorrelationManager {
    return this.correlationManager;
  }

  /**
   * Clean up resources
   */
  cleanup(): void {
    this.processedMessages.clear();
    this.correlationManager.clear();
  }

  /**
   * Get processing statistics for debugging
   */
  getStats(): {
    activeTokens: number;
    correlations: { reqMappings: number; resMappings: number };
  } {
    return {
      activeTokens: this.processedMessages.size,
      correlations: this.correlationManager.getStats()
    };
  }
}