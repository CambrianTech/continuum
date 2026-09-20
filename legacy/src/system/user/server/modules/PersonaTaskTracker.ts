/**
 * PersonaTaskTracker - Tracks which tool results have been processed
 *
 * PROBLEM: Tool results are stored as ChatMessageEntity and remain in context.
 * When an AI responds to tool results, those results are still in context,
 * causing the AI to see them as "new information" and respond again infinitely.
 *
 * SOLUTION: Track which tool result messages have been processed (AI has already
 * responded to them) so we can filter them out before evaluation.
 *
 * This prevents the infinite loop where:
 * 1. AI executes tool (e.g., decision/list)
 * 2. Tool results stored as ChatMessageEntity
 * 3. AI responds to tool results
 * 4. Next evaluation: AI sees same tool results again
 * 5. AI decides to respond again (infinite loop)
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * Tracks which tool result messages have been processed by the AI
 */
export class PersonaTaskTracker {
  /** Set of tool result message IDs that have been processed */
  private processedToolResults: Set<UUID> = new Set();

  /**
   * Check if a tool result message has already been processed
   * @param messageId - The UUID of the ChatMessageEntity containing tool results
   * @returns true if the AI has already responded to this tool result
   */
  hasProcessedToolResult(messageId: UUID): boolean {
    return this.processedToolResults.has(messageId);
  }

  /**
   * Mark a tool result message as processed
   * Call this AFTER the AI has responded to the tool results
   * @param messageId - The UUID of the ChatMessageEntity containing tool results
   */
  markToolResultProcessed(messageId: UUID): void {
    this.processedToolResults.add(messageId);
  }

  /**
   * Mark multiple tool result messages as processed
   * @param messageIds - Array of ChatMessageEntity UUIDs containing tool results
   */
  markMultipleProcessed(messageIds: UUID[]): void {
    for (const messageId of messageIds) {
      this.processedToolResults.add(messageId);
    }
  }

  /**
   * Get all processed tool result IDs (for debugging)
   * @returns Array of processed tool result message IDs
   */
  getProcessedToolResults(): UUID[] {
    return Array.from(this.processedToolResults);
  }

  /**
   * Clear all tracked tool results (for testing or reset)
   */
  clear(): void {
    this.processedToolResults.clear();
  }

  /**
   * Get count of tracked tool results
   * @returns Number of tool results that have been processed
   */
  getCount(): number {
    return this.processedToolResults.size;
  }
}
