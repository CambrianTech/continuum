/**
 * Working Memory Manager
 *
 * Manages short-term contextual thoughts for a persona
 * Handles storage, retrieval, and eviction of working memories
 */

import type { UUID } from '../../../../../core/types/CrossPlatformUUID';
import { cognitionStorage, type WorkingMemoryEntry } from './InMemoryCognitionStorage';

export interface RecallQuery {
  domain: string;
  contextId?: UUID;
  limit?: number;
  thoughtTypes?: string[];
}

export class WorkingMemoryManager {
  private readonly MAX_CAPACITY = 100;

  constructor(private personaId: UUID) {}

  /**
   * Store a thought in working memory
   */
  async store(memory: Omit<WorkingMemoryEntry, 'id' | 'personaId' | 'createdAt' | 'lastAccessedAt'>): Promise<void> {
    const entry: WorkingMemoryEntry = {
      ...memory,
      id: this.generateId(),
      personaId: this.personaId,
      createdAt: Date.now(),
      lastAccessedAt: Date.now()
    };

    cognitionStorage.addWorkingMemory(entry);

    // Check capacity and evict if needed
    await this.evictIfNeeded(memory.domain);
  }

  /**
   * Recall relevant thoughts from working memory
   */
  async recall(query: RecallQuery): Promise<WorkingMemoryEntry[]> {
    const allMemories = cognitionStorage.getWorkingMemory(this.personaId);

    // Filter by domain
    let filtered = allMemories.filter(m => m.domain === query.domain);

    // Filter by context if specified
    if (query.contextId) {
      filtered = filtered.filter(m => m.contextId === query.contextId);
    }

    // Filter by thought types if specified
    if (query.thoughtTypes && query.thoughtTypes.length > 0) {
      filtered = filtered.filter(m => query.thoughtTypes!.includes(m.thoughtType));
    }

    // Sort by importance and recency
    filtered.sort((a, b) => {
      const importanceScore = b.importance - a.importance;
      if (Math.abs(importanceScore) > 0.1) return importanceScore;
      return b.createdAt - a.createdAt;
    });

    // Update last accessed time
    const now = Date.now();
    filtered.forEach(m => m.lastAccessedAt = now);

    // Limit results
    const limit = query.limit || 20;
    return filtered.slice(0, limit);
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
    console.log(`🗑️ [WorkingMemory] Evicted ${evictedCount} memories for ${domain}`);
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

  private generateId(): UUID {
    return `${Date.now()}-${Math.random().toString(36).slice(2, 11)}` as UUID;
  }
}
