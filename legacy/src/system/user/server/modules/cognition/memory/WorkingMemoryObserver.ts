/**
 * WorkingMemoryObserver - Non-blocking observer for WorkingMemory
 *
 * Observes working memory changes without blocking
 * Used by MemoryConsolidationWorker to detect patterns
 */

import type { WorkingMemoryEntry } from './InMemoryCognitionStorage';
import { WorkingMemoryManager } from './WorkingMemoryManager';

export class WorkingMemoryObserver {
  private log: (message: string) => void;

  constructor(private workingMemory: WorkingMemoryManager, logger?: (message: string) => void) {
    this.log = logger || (() => {});
  }

  /**
   * Get recent thoughts (non-blocking)
   *
   * @param limit - Maximum number of thoughts to retrieve
   * @returns Array of recent working memory entries
   */
  async getRecent(limit: number): Promise<WorkingMemoryEntry[]> {
    try {
      return await this.workingMemory.recall({
        sortBy: 'recent',
        limit,
        includePrivate: true // Observer can see private thoughts
      });
    } catch (error) {
      this.log(`❌ Error getting recent thoughts: ${error}`);
      return [];
    }
  }

  /**
   * Get high-importance thoughts
   *
   * @param minImportance - Minimum importance threshold (0-1)
   * @param limit - Maximum number of thoughts to retrieve
   * @returns Array of important working memory entries
   */
  async getImportant(minImportance: number, limit: number): Promise<WorkingMemoryEntry[]> {
    try {
      return await this.workingMemory.recall({
        minImportance,
        sortBy: 'important',
        limit,
        includePrivate: true
      });
    } catch (error) {
      this.log(`❌ Error getting important thoughts: ${error}`);
      return [];
    }
  }

  /**
   * Get thoughts for a specific domain
   *
   * @param domain - Domain to filter by (null = global)
   * @param limit - Maximum number of thoughts to retrieve
   * @returns Array of domain-specific working memory entries
   */
  async getByDomain(domain: string | null, limit: number): Promise<WorkingMemoryEntry[]> {
    try {
      return await this.workingMemory.recall({
        domain,
        sortBy: 'recent',
        limit,
        includePrivate: true
      });
    } catch (error) {
      this.log(`❌ Error getting domain thoughts: ${error}`);
      return [];
    }
  }

  /**
   * Get current memory pressure (0-1)
   * 0 = empty, 1 = at capacity
   */
  async getMemoryPressure(domain: string): Promise<number> {
    try {
      const capacity = await this.workingMemory.getCapacity(domain);
      return capacity.used / capacity.max;
    } catch (error) {
      this.log(`❌ Error getting memory pressure: ${error}`);
      return 0;
    }
  }
}
