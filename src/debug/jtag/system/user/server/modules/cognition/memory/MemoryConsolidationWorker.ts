/**
 * MemoryConsolidationWorker - Separate thread/process for memory management
 *
 * Runs independently of PersonaUser main thread
 * Observes activity, detects patterns, consolidates/activates intelligently
 * Pattern-driven (cosine similarity), not time-based
 * Non-blocking (peeks at inbox/memory without blocking)
 */

import type { UUID } from '../../../../../core/types/CrossPlatformUUID';
import { WorkingMemoryManager, type RecallQuery } from './WorkingMemoryManager';
import type { WorkingMemoryEntry } from './InMemoryCognitionStorage';
import { LongTermMemoryStore, type LongTermMemoryEntry } from './LongTermMemoryStore';
import { InboxObserver } from './InboxObserver';
import { WorkingMemoryObserver } from './WorkingMemoryObserver';
import type { PersonaInbox, QueueItem } from '../../PersonaInbox';

type LogFn = (message: string) => void;

export interface TriggerState {
  shouldConsolidate: boolean;
  shouldActivate: boolean;
  reason: string | null;
  context: number[] | null;
}

export interface PatternDetection {
  consolidationTriggered: boolean;
  activationTriggered: boolean;
  reason: string | null;
  context: number[] | null;
  patterns: Cluster[];
}

export interface Cluster {
  indices: number[];
  strength: number;
  representative: number;
}

export interface ConsolidationOptions {
  /** Minimum similarity threshold for clustering (0-1) */
  minSimilarity?: number;
  /** Minimum cluster size to trigger consolidation */
  minClusterSize?: number;
  /** Minimum cluster strength to trigger consolidation (0-1) */
  minClusterStrength?: number;
  /** Minimum importance for consolidation candidates (0-1) */
  minImportance?: number;
  /** Similarity threshold for activation (0-1) */
  activationThreshold?: number;
}

/**
 * Memory Consolidation Worker
 *
 * Runs as separate thread/process, non-blocking
 * Pattern-driven consolidation and activation
 */
export class MemoryConsolidationWorker {
  private readonly personaId: UUID;
  private readonly workingMemory: WorkingMemoryManager;
  private readonly longTermMemory: LongTermMemoryStore;
  private running: boolean = false;
  private readonly log: LogFn;

  // Observers (non-blocking peek)
  private inboxObserver: InboxObserver | null = null;
  private memoryObserver: WorkingMemoryObserver;

  // Configuration
  private readonly options: Required<ConsolidationOptions>;

  constructor(
    personaId: UUID,
    inbox: PersonaInbox,
    options: ConsolidationOptions = {},
    logger?: LogFn
  ) {
    this.personaId = personaId;
    this.workingMemory = new WorkingMemoryManager(personaId);
    this.longTermMemory = new LongTermMemoryStore(personaId);
    this.log = logger || (() => {});  // No-op if no logger provided

    // Set up observers (peek at activity, don't block)
    this.inboxObserver = new InboxObserver(inbox);
    this.memoryObserver = new WorkingMemoryObserver(this.workingMemory);

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
   * Start the worker (separate thread/process)
   */
  async start(): Promise<void> {
    if (this.running) {
      this.log(`⚠️ Already running`);
      return;
    }

    this.running = true;
    this.log(`🧠 Started`);

    // Run in background (non-blocking)
    // In a true multi-threaded environment, this would be a worker thread
    // For now, use async event loop
    setImmediate(() => this.serviceLoop());
  }

  /**
   * Main service loop - runs independently
   */
  private async serviceLoop(): Promise<void> {
    while (this.running) {
      try {
        // 1. Check triggers (non-blocking)
        const triggers = await this.checkTriggers();

        // 2. Consolidate if triggered
        if (triggers.shouldConsolidate) {
          await this.consolidate(triggers.reason!);
        }

        // 3. Activate if triggered
        if (triggers.shouldActivate) {
          await this.activate(triggers.context!);
        }

        // 4. Sleep briefly (non-blocking, yields to other operations)
        await this.sleep(100); // 100ms
      } catch (error) {
        this.log(`❌ Error in service loop: ${error}`);
        await this.sleep(1000); // Back off on error
      }
    }

    this.log(`🛑 Stopped`);
  }

  /**
   * Check for consolidation/activation triggers
   * Non-blocking peek at inbox and memory state
   */
  private async checkTriggers(): Promise<TriggerState> {
    // Peek at inbox (non-blocking)
    const inboxItems = this.inboxObserver ? await this.inboxObserver.peek(10) : [];

    // Peek at working memory (non-blocking)
    const recentThoughts = await this.memoryObserver.getRecent(20);

    // No activity, nothing to do
    if (inboxItems.length === 0 && recentThoughts.length === 0) {
      return {
        shouldConsolidate: false,
        shouldActivate: false,
        reason: null,
        context: null
      };
    }

    // Detect patterns via cosine similarity (not hard-coded)
    const patterns = await this.detectPatterns(inboxItems, recentThoughts);

    return {
      shouldConsolidate: patterns.consolidationTriggered,
      shouldActivate: patterns.activationTriggered,
      reason: patterns.reason,
      context: patterns.context
    };
  }

  /**
   * Detect patterns using cosine similarity
   * No hard-coded enums, pure vector similarity
   */
  private async detectPatterns(
    inboxItems: QueueItem[],
    thoughts: WorkingMemoryEntry[]
  ): Promise<PatternDetection> {
    // 1. Extract embeddings from recent activity
    const inboxEmbeddings = await Promise.all(
      inboxItems.map(item => this.embed(this.extractText(item)))
    );

    const thoughtEmbeddings = await Promise.all(
      thoughts.map(t => this.embed(t.thoughtContent))
    );

    // Combine all embeddings
    const allEmbeddings = [...inboxEmbeddings, ...thoughtEmbeddings];

    if (allEmbeddings.length === 0) {
      return {
        consolidationTriggered: false,
        activationTriggered: false,
        reason: null,
        context: null,
        patterns: []
      };
    }

    // 2. Compute pairwise cosine similarities
    const similarities = this.computePairwiseSimilarities(allEmbeddings);

    // 3. Detect clusters (pattern = high similarity cluster)
    const clusters = this.detectClusters(similarities, {
      minSimilarity: this.options.minSimilarity,
      minClusterSize: this.options.minClusterSize
    });

    // 4. Trigger consolidation if strong pattern emerges
    const consolidationTriggered = clusters.some(
      c => c.strength >= this.options.minClusterStrength
    );

    // 5. Trigger activation if inbox matches existing patterns
    const activationTriggered = inboxEmbeddings.length > 0
      ? await this.matchesLongTermPatterns(inboxEmbeddings)
      : false;

    return {
      consolidationTriggered,
      activationTriggered,
      reason: consolidationTriggered
        ? `Pattern cluster detected (strength: ${clusters[0]?.strength.toFixed(2)}, size: ${clusters[0]?.indices.length})`
        : null,
      context: activationTriggered ? inboxEmbeddings[0] : null,
      patterns: clusters
    };
  }

  /**
   * Consolidate to long-term when pattern emerges
   */
  private async consolidate(reason: string): Promise<void> {
    this.log(`💾 Consolidation triggered: ${reason}`);

    try {
      // 1. Get high-importance working memories
      const candidates = await this.workingMemory.recall({
        minImportance: this.options.minImportance,
        limit: 50,
        sortBy: 'important'
      });

      if (candidates.length === 0) {
        this.log(`💾 No candidates for consolidation`);
        return;
      }

      // 2. Encode and store (batch)
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

      // 3. Clear consolidated from working memory
      await this.workingMemory.clearBatch(candidates.map(c => c.id));

      this.log(`💾 Consolidated ${batch.length} memories`);
    } catch (error) {
      this.log(`❌ Consolidation failed: ${error}`);
    }
  }

  /**
   * Activate relevant long-term memories
   */
  private async activate(contextEmbedding: number[]): Promise<void> {
    this.log(`🔗 Activation triggered`);

    try {
      // 1. Find similar in long-term (cosine similarity)
      const relevant = await this.longTermMemory.findSimilar(contextEmbedding, {
        limit: 5,
        threshold: this.options.activationThreshold
      });

      if (relevant.length === 0) {
        this.log(`🔗 No relevant memories to activate`);
        return;
      }

      // 2. Load into working memory (decompression)
      for (const memory of relevant) {
        await this.workingMemory.store({
          domain: memory.domain,
          contextId: memory.contextId,
          thoughtType: memory.thoughtType,
          thoughtContent: memory.thoughtContent,
          importance: memory.importance,
          shareable: true, // Activated from long-term = shareable
          metadata: {
            ...(memory.metadata || {}),
            source: 'long-term-activation',
            activatedAt: Date.now(),
            originalCreatedAt: memory.createdAt
          }
        });
      }

      this.log(`🔗 Activated ${relevant.length} memories`);
    } catch (error) {
      this.log(`❌ Activation failed: ${error}`);
    }
  }

  /**
   * Check if inbox matches existing long-term patterns
   */
  private async matchesLongTermPatterns(
    inboxEmbeddings: number[][]
  ): Promise<boolean> {
    for (const embedding of inboxEmbeddings) {
      const matches = await this.longTermMemory.findSimilar(embedding, {
        limit: 1,
        threshold: this.options.activationThreshold
      });

      if (matches.length > 0) {
        return true; // Inbox matches a long-term pattern
      }
    }

    return false;
  }

  /**
   * Compute pairwise cosine similarities
   */
  private computePairwiseSimilarities(embeddings: number[][]): number[][] {
    const n = embeddings.length;
    const similarities: number[][] = Array(n)
      .fill(0)
      .map(() => Array(n).fill(0));

    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        const sim = this.cosineSimilarity(embeddings[i], embeddings[j]);
        similarities[i][j] = sim;
        similarities[j][i] = sim; // Symmetric
      }
      similarities[i][i] = 1.0; // Self-similarity
    }

    return similarities;
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

  /**
   * Detect clusters in similarity matrix
   */
  private detectClusters(
    similarities: number[][],
    options: { minSimilarity: number; minClusterSize: number }
  ): Cluster[] {
    // Simple clustering: find connected components above threshold
    const n = similarities.length;
    const visited = new Set<number>();
    const clusters: Cluster[] = [];

    for (let i = 0; i < n; i++) {
      if (visited.has(i)) continue;

      const cluster = this.expandCluster(
        i,
        similarities,
        visited,
        options.minSimilarity
      );

      if (cluster.length >= options.minClusterSize) {
        clusters.push({
          indices: cluster,
          strength: this.computeClusterStrength(cluster, similarities),
          representative: cluster[0] // Could be centroid
        });
      }
    }

    return clusters.sort((a, b) => b.strength - a.strength);
  }

  /**
   * Expand cluster from seed node
   */
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

      // Add neighbors above threshold
      for (let i = 0; i < similarities[node].length; i++) {
        if (!visited.has(i) && similarities[node][i] >= threshold) {
          queue.push(i);
        }
      }
    }

    return cluster;
  }

  /**
   * Compute cluster strength (average internal similarity)
   */
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

  /**
   * Extract text from queue item for embedding
   */
  private extractText(item: QueueItem): string {
    // Extract meaningful text from queue item
    // This is a simplified version - may need domain-specific extraction
    if (typeof item === 'string') {
      return item;
    }

    // If it's an object with a message property
    if (item && typeof item === 'object' && 'message' in item) {
      return String((item as any).message);
    }

    // Fallback: stringify
    return JSON.stringify(item);
  }

  /**
   * Generate embedding for text
   * TODO: Replace with actual embedding service (Ollama, OpenAI, etc.)
   */
  private async embed(text: string): Promise<number[]> {
    // Placeholder: Simple hash-based pseudo-embedding
    // In production, use actual embedding model
    const hash = this.simpleHash(text);
    const dim = 128; // Embedding dimension
    const embedding: number[] = [];

    for (let i = 0; i < dim; i++) {
      const seed = hash + i;
      embedding.push(Math.sin(seed) * Math.cos(seed * 2));
    }

    // Normalize
    const magnitude = Math.sqrt(
      embedding.reduce((sum, val) => sum + val * val, 0)
    );
    return embedding.map(v => v / magnitude);
  }

  /**
   * Simple hash function for pseudo-embeddings
   */
  private simpleHash(str: string): number {
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
      const char = str.charCodeAt(i);
      hash = (hash << 5) - hash + char;
      hash = hash & hash; // Convert to 32-bit integer
    }
    return hash;
  }

  /**
   * Sleep for specified milliseconds (non-blocking)
   */
  private async sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Stop the worker
   */
  async stop(): Promise<void> {
    this.running = false;
    this.log(`🛑 Stopping`);
  }

  /**
   * Get worker status
   */
  getStatus(): {
    running: boolean;
    personaId: UUID;
    options: Required<ConsolidationOptions>;
  } {
    return {
      running: this.running,
      personaId: this.personaId,
      options: this.options
    };
  }
}
