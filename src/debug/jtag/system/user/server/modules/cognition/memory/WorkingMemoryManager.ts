/**
 * Working Memory Manager
 *
 * Manages short-term contextual thoughts for a persona
 * Handles storage, retrieval, and eviction of working memories
 */

import type { UUID } from '../../../../../core/types/CrossPlatformUUID';
import { cognitionStorage, type WorkingMemoryEntry } from './InMemoryCognitionStorage';
import { randomUUID } from 'crypto';

export interface RecallQuery {
  domain?: string | null;  // null = global
  contextId?: UUID | null | '*';  // '*' = all contexts, null = domain-wide
  limit?: number;
  thoughtTypes?: string[];
  minImportance?: number;
  sortBy?: 'recent' | 'important' | 'relevance';
  includePrivate?: boolean;
}

export class WorkingMemoryManager {
  private readonly MAX_CAPACITY = 100;
  private log: (message: string) => void;

  constructor(private personaId: UUID, logger?: (message: string) => void) {
    this.log = logger || console.log.bind(console);
  }

  /**
   * Store a thought in working memory
   */
  async store(memory: Omit<WorkingMemoryEntry, 'id' | 'personaId' | 'createdAt' | 'lastAccessedAt'>): Promise<UUID> {
    // Validate thoughtType length
    if (memory.thoughtType.length > 50) {
      throw new Error('thoughtType must be 50 characters or less');
    }

    // Validate importance range
    if (memory.importance < 0 || memory.importance > 1) {
      throw new Error('importance must be between 0.0 and 1.0');
    }

    const entry: WorkingMemoryEntry = {
      ...memory,
      id: randomUUID() as UUID,
      personaId: this.personaId,
      createdAt: Date.now(),
      lastAccessedAt: Date.now()
    };

    cognitionStorage.addWorkingMemory(entry);

    // Check capacity and evict if needed
    if (memory.domain !== null) {
      await this.evictIfNeeded(memory.domain);
    }

    return entry.id;
  }

  /**
   * Recall relevant thoughts from working memory
   */
  async recall(query: RecallQuery): Promise<WorkingMemoryEntry[]> {
    const allMemories = cognitionStorage.getWorkingMemory(this.personaId);

    // Filter by domain
    let filtered = allMemories;
    if (query.domain !== undefined) {
      filtered = filtered.filter(m => m.domain === query.domain);
    }

    // Filter by context
    if (query.contextId !== undefined && query.contextId !== '*') {
      filtered = filtered.filter(m => m.contextId === query.contextId);
    }

    // Filter by thought types
    if (query.thoughtTypes && query.thoughtTypes.length > 0) {
      filtered = filtered.filter(m => query.thoughtTypes!.includes(m.thoughtType));
    }

    // Filter by minimum importance
    if (query.minImportance !== undefined) {
      filtered = filtered.filter(m => m.importance >= query.minImportance!);
    }

    // Filter out non-shareable if not including private
    if (!query.includePrivate) {
      filtered = filtered.filter(m => m.shareable);
    }

    // Sort by specified criteria
    const sortBy = query.sortBy || 'important';
    filtered.sort((a, b) => {
      if (sortBy === 'recent') {
        return b.createdAt - a.createdAt;
      } else if (sortBy === 'important') {
        const importanceScore = b.importance - a.importance;
        if (Math.abs(importanceScore) > 0.1) return importanceScore;
        return b.createdAt - a.createdAt;  // Tiebreaker: recency
      }
      // 'relevance' - for now same as important
      return b.importance - a.importance;
    });

    // Update last accessed time (immutably)
    const now = Date.now();
    const updated = filtered.map(m => ({ ...m, lastAccessedAt: now }));

    // Write back updated access times
    updated.forEach(m => {
      const memories = cognitionStorage.getWorkingMemory(this.personaId);
      const updatedMemories = memories.map(mem =>
        mem.id === m.id ? m : mem
      );
      cognitionStorage.clearWorkingMemory(this.personaId);
      updatedMemories.forEach(mem => cognitionStorage.addWorkingMemory(mem));
    });

    // Limit results
    const limit = query.limit || 20;
    return updated.slice(0, limit);
  }

  /**
   * Evict old/low-importance memories if over capacity
   */
  private async evictIfNeeded(domain: string): Promise<void> {
    const memories = cognitionStorage.getWorkingMemory(this.personaId);
    const domainMemories = memories.filter(m => m.domain === domain);

    if (domainMemories.length <= this.MAX_CAPACITY) {
      return; // Under capacity
    }

    // Score each memory for retention
    const scored = domainMemories.map(m => ({
      memory: m,
      score: this.calculateRetentionScore(m)
    }));

    // Sort by score (keep highest scores)
    scored.sort((a, b) => b.score - a.score);

    // Keep top N, remove rest
    const toKeep = new Set(scored.slice(0, this.MAX_CAPACITY).map(s => s.memory.id));

    // Filter out evicted memories
    const allFiltered = memories.filter(m =>
      m.domain !== domain || toKeep.has(m.id)
    );

    cognitionStorage.clearWorkingMemory(this.personaId);
    allFiltered.forEach(m => cognitionStorage.addWorkingMemory(m));

    const evictedCount = domainMemories.length - this.MAX_CAPACITY;
    this.log(`🗑️ [WorkingMemory] Evicted ${evictedCount} memories for ${domain}`);
  }

  /**
   * Calculate retention score for a memory
   * Higher score = keep longer
   */
  private calculateRetentionScore(memory: WorkingMemoryEntry): number {
    let score = memory.importance;

    // Boost recent memories
    const age = Date.now() - memory.createdAt;
    const dayInMs = 24 * 60 * 60 * 1000;
    const recencyBoost = Math.exp(-age / (7 * dayInMs)); // Decay over 7 days
    score += recencyBoost * 0.3;

    // Boost recently accessed
    const timeSinceAccess = Date.now() - memory.lastAccessedAt;
    const accessRecency = Math.exp(-timeSinceAccess / dayInMs);
    score += accessRecency * 0.2;

    return score;
  }

  /**
   * Get current capacity usage for a domain
   */
  async getCapacity(domain: string): Promise<{ used: number; max: number }> {
    const memories = cognitionStorage.getWorkingMemory(this.personaId);
    const used = memories.filter(m => m.domain === domain).length;
    return { used, max: this.MAX_CAPACITY };
  }

  /**
   * Clear all working memory for a domain
   */
  async clear(domain?: string): Promise<void> {
    cognitionStorage.clearWorkingMemory(this.personaId, domain);
  }

  /**
   * Clear specific thoughts by ID (batch)
   * Used by memory consolidation to remove thoughts after consolidating
   */
  async clearBatch(thoughtIds: UUID[]): Promise<void> {
    if (thoughtIds.length === 0) return;

    const memories = cognitionStorage.getWorkingMemory(this.personaId);
    const idsToRemove = new Set(thoughtIds);
    const filtered = memories.filter(m => !idsToRemove.has(m.id));

    cognitionStorage.clearWorkingMemory(this.personaId);
    filtered.forEach(m => cognitionStorage.addWorkingMemory(m));

    this.log(`🗑️ [WorkingMemory] Cleared ${thoughtIds.length} thoughts`);
  }
}
