/**
 * MemoryConsolidationSubprocess - Hippocampus-style memory management
 *
 * RTOS-style subprocess for automatic memory consolidation and activation
 * Extends PersonaContinuousSubprocess (no queue, continuous checking)
 *
 * Architecture:
 * - Separate thread (non-blocking)
 * - Pattern-driven (cosine similarity)
 * - Direct persona access (no events)
 * - Adaptive timing based on priority
 *
 * Implementation is TINY (~100 lines) because base class handles everything
 */

import { PersonaContinuousSubprocess } from '../../PersonaSubprocess';
import type { PersonaUser } from '../../../PersonaUser';
import type { WorkingMemoryEntry } from './InMemoryCognitionStorage';
import { LongTermMemoryStore, type LongTermMemoryEntry } from './LongTermMemoryStore';
import type { QueueItem } from '../../PersonaInbox';
import { RustEmbeddingClient } from '../../../../../core/services/RustEmbeddingClient';

export interface ConsolidationOptions {
  minSimilarity?: number;
  minClusterSize?: number;
  minClusterStrength?: number;
  minImportance?: number;
  activationThreshold?: number;
}

/**
 * Memory Consolidation Subprocess
 *
 * Like cbar's CBP_PlaneAnalyzer - tiny implementation, base handles threading
 */
export class MemoryConsolidationSubprocess extends PersonaContinuousSubprocess {
  private readonly longTermMemory: LongTermMemoryStore;
  private readonly options: Required<ConsolidationOptions>;

  constructor(persona: PersonaUser, options: ConsolidationOptions = {}) {
    // Low priority background process
    super(persona, { priority: 'low', name: 'MemoryConsolidation' });

    this.longTermMemory = new LongTermMemoryStore(persona.id, (msg) => this.log(msg));

    // Default options
    this.options = {
      minSimilarity: options.minSimilarity ?? 0.75,
      minClusterSize: options.minClusterSize ?? 3,
      minClusterStrength: options.minClusterStrength ?? 0.8,
      minImportance: options.minImportance ?? 0.6,
      activationThreshold: options.activationThreshold ?? 0.8
    };
  }

  /**
   * Continuous tick - check for consolidation/activation opportunities
   *
   * THIS IS THE ONLY METHOD WE IMPLEMENT
   */
  protected async tick(): Promise<void> {
    try {
      // Peek at persona's inbox (non-blocking, direct access)
      const inboxItems = await this.persona.inbox.peek(10);

      // Get recent thoughts from persona's working memory (direct access)
      const thoughts = await this.persona.workingMemory.recall({
        sortBy: 'recent',
        limit: 20,
        includePrivate: true
      });

      // No activity, nothing to do
      if (inboxItems.length === 0 && thoughts.length === 0) {
        return;
      }

      // Detect patterns via cosine similarity
      const patterns = await this.detectPatterns(inboxItems, thoughts);

      // Consolidate if triggered
      if (patterns.shouldConsolidate) {
        await this.consolidate(patterns.reason!);
      }

      // Activate if triggered
      if (patterns.shouldActivate) {
        await this.activate(patterns.context!);
      }
    } catch (error) {
      this.log(`❌ Tick error: ${error}`);
    }
  }

  // ==================== Private Methods ====================

  private async detectPatterns(
    inboxItems: QueueItem[],
    thoughts: WorkingMemoryEntry[]
  ): Promise<{
    shouldConsolidate: boolean;
    shouldActivate: boolean;
    reason: string | null;
    context: number[] | null;
  }> {
    // Extract embeddings
    const inboxEmbeddings = await Promise.all(
      inboxItems.map(item => this.embed(this.extractText(item)))
    );

    const thoughtEmbeddings = await Promise.all(
      thoughts.map(t => this.embed(t.thoughtContent))
    );

    const allEmbeddings = [...inboxEmbeddings, ...thoughtEmbeddings];

    if (allEmbeddings.length === 0) {
      return {
        shouldConsolidate: false,
        shouldActivate: false,
        reason: null,
        context: null
      };
    }

    // Compute pairwise similarities
    const similarities = this.computePairwiseSimilarities(allEmbeddings);

    // Detect clusters
    const clusters = this.detectClusters(similarities, {
      minSimilarity: this.options.minSimilarity,
      minClusterSize: this.options.minClusterSize
    });

    // Trigger consolidation if strong pattern emerges
    const shouldConsolidate = clusters.some(
      c => c.strength >= this.options.minClusterStrength
    );

    // Trigger activation if inbox matches long-term patterns
    const shouldActivate = inboxEmbeddings.length > 0
      ? await this.matchesLongTermPatterns(inboxEmbeddings)
      : false;

    return {
      shouldConsolidate,
      shouldActivate,
      reason: shouldConsolidate
        ? `Pattern cluster (strength: ${clusters[0]?.strength.toFixed(2)}, size: ${clusters[0]?.indices.length})`
        : null,
      context: shouldActivate ? inboxEmbeddings[0] : null
    };
  }

  private async consolidate(reason: string): Promise<void> {
    this.log(`💾 Consolidation triggered: ${reason}`);

    // Get high-importance memories from persona (direct access)
    const candidates = await this.persona.workingMemory.recall({
      minImportance: this.options.minImportance,
      limit: 50,
      sortBy: 'important'
    });

    if (candidates.length === 0) return;

    // Encode and store (batch)
    const batch: LongTermMemoryEntry[] = await Promise.all(
      candidates.map(async (memory) => ({
        id: memory.id,
        personaId: memory.personaId,
        domain: memory.domain,
        contextId: memory.contextId,
        thoughtType: memory.thoughtType,
        thoughtContent: memory.thoughtContent,
        importance: memory.importance,
        embedding: await this.embed(memory.thoughtContent),
        metadata: memory.metadata,
        createdAt: memory.createdAt,
        consolidatedAt: Date.now()
      }))
    );

    await this.longTermMemory.appendBatch(batch);

    // Clear from working memory
    await this.persona.workingMemory.clearBatch(candidates.map(c => c.id));

    this.log(`💾 Consolidated ${batch.length} memories`);
  }

  private async activate(contextEmbedding: number[]): Promise<void> {
    this.log(`🔗 Activation triggered`);

    // Find similar in long-term
    const relevant = await this.longTermMemory.findSimilar(contextEmbedding, {
      limit: 5,
      threshold: this.options.activationThreshold
    });

    if (relevant.length === 0) return;

    // Load into persona's working memory (direct access)
    for (const memory of relevant) {
      await this.persona.workingMemory.store({
        domain: memory.domain,
        contextId: memory.contextId,
        thoughtType: memory.thoughtType,
        thoughtContent: memory.thoughtContent,
        importance: memory.importance,
        shareable: true,
        metadata: {
          ...(memory.metadata || {}),
          source: 'long-term-activation',
          activatedAt: Date.now()
        }
      });
    }

    this.log(`🔗 Activated ${relevant.length} memories`);
  }

  private async matchesLongTermPatterns(embeddings: number[][]): Promise<boolean> {
    for (const embedding of embeddings) {
      const matches = await this.longTermMemory.findSimilar(embedding, {
        limit: 1,
        threshold: this.options.activationThreshold
      });

      if (matches.length > 0) return true;
    }

    return false;
  }

  // ==================== Utility Methods ====================

  private computePairwiseSimilarities(embeddings: number[][]): number[][] {
    const n = embeddings.length;
    const similarities: number[][] = Array(n).fill(0).map(() => Array(n).fill(0));

    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        const sim = this.cosineSimilarity(embeddings[i], embeddings[j]);
        similarities[i][j] = sim;
        similarities[j][i] = sim;
      }
      similarities[i][i] = 1.0;
    }

    return similarities;
  }

  private cosineSimilarity(a: number[], b: number[]): number {
    if (a.length !== b.length) return 0;

    const dotProduct = a.reduce((sum, val, i) => sum + val * b[i], 0);
    const magnitudeA = Math.sqrt(a.reduce((sum, val) => sum + val * val, 0));
    const magnitudeB = Math.sqrt(b.reduce((sum, val) => sum + val * val, 0));

    return (magnitudeA === 0 || magnitudeB === 0) ? 0 : dotProduct / (magnitudeA * magnitudeB);
  }

  private detectClusters(
    similarities: number[][],
    options: { minSimilarity: number; minClusterSize: number }
  ): Array<{ indices: number[]; strength: number }> {
    const n = similarities.length;
    const visited = new Set<number>();
    const clusters: Array<{ indices: number[]; strength: number }> = [];

    for (let i = 0; i < n; i++) {
      if (visited.has(i)) continue;

      const cluster = this.expandCluster(i, similarities, visited, options.minSimilarity);

      if (cluster.length >= options.minClusterSize) {
        clusters.push({
          indices: cluster,
          strength: this.computeClusterStrength(cluster, similarities)
        });
      }
    }

    return clusters.sort((a, b) => b.strength - a.strength);
  }

  private expandCluster(
    seed: number,
    similarities: number[][],
    visited: Set<number>,
    threshold: number
  ): number[] {
    const cluster: number[] = [];
    const queue: number[] = [seed];

    while (queue.length > 0) {
      const node = queue.shift()!;
      if (visited.has(node)) continue;

      visited.add(node);
      cluster.push(node);

      for (let i = 0; i < similarities[node].length; i++) {
        if (!visited.has(i) && similarities[node][i] >= threshold) {
          queue.push(i);
        }
      }
    }

    return cluster;
  }

  private computeClusterStrength(cluster: number[], similarities: number[][]): number {
    if (cluster.length <= 1) return 0;

    let sum = 0;
    let count = 0;

    for (let i = 0; i < cluster.length; i++) {
      for (let j = i + 1; j < cluster.length; j++) {
        sum += similarities[cluster[i]][cluster[j]];
        count++;
      }
    }

    return count > 0 ? sum / count : 0;
  }

  private extractText(item: QueueItem): string {
    if (typeof item === 'string') return item;
    if (item && typeof item === 'object' && 'message' in item) {
      return String((item as any).message);
    }
    return JSON.stringify(item);
  }

  private async embed(text: string): Promise<number[]> {
    const client = RustEmbeddingClient.instance;

    // Check if Rust worker is available
    if (!await client.isAvailable()) {
      this.log('WARN: Rust embedding worker not available, returning zero vector');
      return new Array(384).fill(0);
    }

    try {
      return await client.embed(text);
    } catch (error) {
      this.log(`ERROR: Embedding failed: ${error}`);
      return new Array(384).fill(0);
    }
  }
}
