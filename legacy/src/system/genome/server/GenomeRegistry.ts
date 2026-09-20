/**
 * GenomeRegistry — Capability-based search over the LoRA adapter registry
 *
 * Like Docker Hub for LoRA adapters: search by capability similarity, not keywords.
 * Uses cosine similarity over exam-derived capability embeddings.
 *
 * A biology adapter naturally matches biochem queries because the embedding
 * encodes geometry of competence — the exam questions about cellular processes,
 * molecular interactions, and chemical pathways all cluster in that neighborhood.
 *
 * SERVER-ONLY: Uses Commands infrastructure for embedding generation and data access.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { GenomeLayerEntity } from '../entities/GenomeLayerEntity';
import { DataList } from '../../../commands/data/list/shared/DataListTypes';
import { EmbeddingGenerate } from '../../../commands/ai/embedding/generate/shared/EmbeddingGenerateTypes';

export interface RegistrySearchResult {
  layer: GenomeLayerEntity;
  similarity: number;
}

export interface RegistrySearchOptions {
  /** Scope to a specific persona's layers */
  personaId?: UUID;
  /** Filter by base model compatibility */
  modelFilter?: string;
  /** Minimum cosine similarity threshold (default: 0.5) */
  minSimilarity?: number;
  /** Maximum results to return (default: 5) */
  limit?: number;
}

/**
 * GenomeRegistry — Entity-based search over GenomeLayerEntity collection
 *
 * The registry enables capability-based adapter discovery:
 *   1. Query text → embedding vector (via ai/embedding/generate)
 *   2. All genome_layers with non-empty embeddings → candidates
 *   3. Cosine similarity ranks candidates by capability overlap
 *   4. Top-k above threshold returned
 *
 * This replaces keyword tagging with geometric competence matching.
 */
export class GenomeRegistry {

  /**
   * Search for adapters by capability similarity.
   *
   * Uses cosine similarity over exam-derived capability embeddings.
   * A biology adapter naturally matches biochem queries — geometry, not keywords.
   */
  static async findByCapability(
    queryText: string,
    options?: RegistrySearchOptions
  ): Promise<RegistrySearchResult[]> {
    const minSimilarity = options?.minSimilarity ?? 0.5;
    const limit = options?.limit ?? 5;

    // 1. Generate query embedding
    const embResult = await EmbeddingGenerate.execute({ input: queryText });
    if (!embResult.success || embResult.embeddings.length === 0) {
      return [];
    }
    const queryVector = embResult.embeddings[0];
    const queryDimension = embResult.dimensions;

    // 2. Load all genome layers from database
    const filter: Record<string, unknown> = {};
    if (options?.personaId) {
      filter['creatorId'] = options.personaId;
    }

    const listResult = await DataList.execute<GenomeLayerEntity>({
      collection: GenomeLayerEntity.collection,
      filter: Object.keys(filter).length > 0 ? filter : undefined,
      dbHandle: 'default',
      limit: 500,  // Reasonable upper bound for registry scan
    });

    if (!listResult.success || listResult.items.length === 0) {
      return [];
    }

    // 3. Score each candidate by cosine similarity
    const results: RegistrySearchResult[] = [];

    for (const layer of listResult.items) {
      // Skip layers without embeddings (not yet embedded)
      if (!layer.embedding || layer.embedding.length === 0) continue;

      // Skip dimension mismatches (different embedding model)
      if (layer.embedding.length !== queryDimension) continue;

      // Skip model-incompatible layers if filter specified
      if (options?.modelFilter && layer.modelPath) {
        // Check tags for base model name
        const hasCompatibleModel = layer.tags?.some(
          tag => tag.toLowerCase().includes(options.modelFilter!.toLowerCase())
        );
        if (!hasCompatibleModel) continue;
      }

      const similarity = cosineSimilarity(queryVector, layer.embedding);
      if (similarity >= minSimilarity) {
        results.push({ layer, similarity });
      }
    }

    // 4. Sort by similarity descending, return top-k
    results.sort((a, b) => b.similarity - a.similarity);
    return results.slice(0, limit);
  }
}

/**
 * Cosine similarity between two vectors.
 * Returns 0 if either vector has zero magnitude.
 */
function cosineSimilarity(a: number[], b: number[]): number {
  let dotProduct = 0;
  let normA = 0;
  let normB = 0;

  for (let i = 0; i < a.length; i++) {
    dotProduct += a[i] * b[i];
    normA += a[i] * a[i];
    normB += b[i] * b[i];
  }

  const magnitude = Math.sqrt(normA) * Math.sqrt(normB);
  if (magnitude === 0) return 0;

  return dotProduct / magnitude;
}
