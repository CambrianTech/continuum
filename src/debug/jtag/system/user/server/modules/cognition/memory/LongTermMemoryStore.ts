/**
 * LongTermMemoryStore - Persistent storage for consolidated memories
 *
 * Per-persona SQLite database: .continuum/personas/{id}/memory.sqlite
 * Stores embeddings for cosine similarity search
 * Append-only, no complex graph (simple and fast)
 */

import type { UUID } from '../../../../../core/types/CrossPlatformUUID';
import * as fs from 'fs';
import * as fsPromises from 'fs/promises';
import * as path from 'path';

type LogFn = (message: string) => void;

export interface LongTermMemoryEntry {
  id: UUID;
  personaId: UUID;
  domain: string | null;
  contextId: UUID | null;
  thoughtType: string;
  thoughtContent: string;
  importance: number;
  embedding: number[]; // Vector for similarity search
  metadata?: Record<string, unknown>;
  createdAt: number;
  consolidatedAt: number;
}

export interface SimilarityQuery {
  limit: number;
  threshold: number; // Minimum cosine similarity (0-1)
}

/**
 * Long-Term Memory Store
 *
 * TODO: Implement actual SQLite persistence
 * For now, in-memory with file-based fallback
 */
export class LongTermMemoryStore {
  private readonly personaId: UUID;
  private readonly storePath: string;
  private memories: LongTermMemoryEntry[] = [];
  private loaded: boolean = false;
  private readonly log: LogFn;

  constructor(personaId: UUID, logger?: LogFn) {
    this.personaId = personaId;
    this.log = logger || (() => {});

    // Per-persona storage path
    // TODO: Use PATHS constant from system/shared/Constants.ts
    const baseDir = process.env.CONTINUUM_DIR || '.continuum';
    this.storePath = path.join(
      baseDir,
      'personas',
      personaId,
      'memory.json' // TODO: Change to memory.sqlite
    );

    // Directory creation happens lazily in ensureDirectoryExists()
  }

  /**
   * Ensure directory exists before write (async - called before persist)
   */
  private async ensureDirectoryExists(): Promise<void> {
    const dir = path.dirname(this.storePath);
    await fsPromises.mkdir(dir, { recursive: true });
  }

  /**
   * Append batch of memories (atomic)
   */
  async appendBatch(batch: LongTermMemoryEntry[]): Promise<void> {
    await this.ensureLoaded();

    // Add to in-memory store
    this.memories.push(...batch);

    // Persist to disk
    await this.persist();

    this.log(`💾 Appended ${batch.length} memories for ${this.personaId}`);
  }

  /**
   * Find similar memories using cosine similarity
   */
  async findSimilar(
    queryEmbedding: number[],
    options: SimilarityQuery
  ): Promise<LongTermMemoryEntry[]> {
    await this.ensureLoaded();

    if (this.memories.length === 0) {
      return [];
    }

    // Compute similarities
    const withScores = this.memories.map(memory => ({
      memory,
      similarity: this.cosineSimilarity(queryEmbedding, memory.embedding)
    }));

    // Filter by threshold
    const filtered = withScores.filter(
      item => item.similarity >= options.threshold
    );

    // Sort by similarity (descending)
    filtered.sort((a, b) => b.similarity - a.similarity);

    // Limit results
    const results = filtered.slice(0, options.limit);

    this.log(
      `🔍 Found ${results.length} similar memories (threshold: ${options.threshold})`
    );

    return results.map(item => item.memory);
  }

  /**
   * Get all memories for a domain
   */
  async getByDomain(
    domain: string | null,
    limit?: number
  ): Promise<LongTermMemoryEntry[]> {
    await this.ensureLoaded();

    const filtered = this.memories.filter(m => m.domain === domain);

    // Sort by consolidation time (newest first)
    filtered.sort((a, b) => b.consolidatedAt - a.consolidatedAt);

    return limit ? filtered.slice(0, limit) : filtered;
  }

  /**
   * Get memory count
   */
  async getCount(): Promise<number> {
    await this.ensureLoaded();
    return this.memories.length;
  }

  /**
   * Get storage stats
   */
  async getStats(): Promise<{
    totalMemories: number;
    byDomain: Record<string, number>;
    oldestMemory: number | null;
    newestMemory: number | null;
  }> {
    await this.ensureLoaded();

    const byDomain: Record<string, number> = {};
    let oldestMemory: number | null = null;
    let newestMemory: number | null = null;

    for (const memory of this.memories) {
      const domain = memory.domain || 'global';
      byDomain[domain] = (byDomain[domain] || 0) + 1;

      if (oldestMemory === null || memory.consolidatedAt < oldestMemory) {
        oldestMemory = memory.consolidatedAt;
      }
      if (newestMemory === null || memory.consolidatedAt > newestMemory) {
        newestMemory = memory.consolidatedAt;
      }
    }

    return {
      totalMemories: this.memories.length,
      byDomain,
      oldestMemory,
      newestMemory
    };
  }

  /**
   * Clear all memories (for testing)
   */
  async clear(): Promise<void> {
    this.memories = [];
    await this.persist();
    this.log(`🗑️ Cleared all memories for ${this.personaId}`);
  }

  // ==================== Private Methods ====================

  /**
   * Ensure memories are loaded from disk (async - non-blocking)
   */
  private async ensureLoaded(): Promise<void> {
    if (this.loaded) return;

    try {
      const exists = await fsPromises.access(this.storePath).then(() => true).catch(() => false);
      if (exists) {
        const data = await fsPromises.readFile(this.storePath, 'utf-8');
        this.memories = JSON.parse(data);
        this.log(
          `📖 Loaded ${this.memories.length} memories for ${this.personaId}`
        );
      }
    } catch (error) {
      this.log(`❌ Error loading memories: ${error}`);
      this.memories = [];
    }

    this.loaded = true;
  }

  /**
   * Persist memories to disk (async - non-blocking)
   */
  private async persist(): Promise<void> {
    try {
      await this.ensureDirectoryExists();
      const data = JSON.stringify(this.memories, null, 2);
      await fsPromises.writeFile(this.storePath, data, 'utf-8');
    } catch (error) {
      this.log(`❌ Error persisting memories: ${error}`);
    }
  }

  /**
   * Cosine similarity between two vectors
   */
  private cosineSimilarity(a: number[], b: number[]): number {
    if (a.length !== b.length) {
      throw new Error('Vectors must have same length');
    }

    const dotProduct = a.reduce((sum, val, i) => sum + val * b[i], 0);
    const magnitudeA = Math.sqrt(a.reduce((sum, val) => sum + val * val, 0));
    const magnitudeB = Math.sqrt(b.reduce((sum, val) => sum + val * val, 0));

    if (magnitudeA === 0 || magnitudeB === 0) {
      return 0;
    }

    return dotProduct / (magnitudeA * magnitudeB);
  }
}
