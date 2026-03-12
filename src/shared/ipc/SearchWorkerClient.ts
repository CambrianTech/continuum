/**
 * SearchWorkerClient - TypeScript client for SearchModule in continuum-core
 *
 * Uses typed command accessors to call search/execute, search/list, etc.
 * Migrated from standalone search worker to continuum-core.
 */

import { SearchList } from '@commands/search/list/shared/SearchListTypes';
import { SearchExecute } from '@commands/search/execute/shared/SearchExecuteTypes';

// ============================================================================
// Types (matches continuum-core SearchModule)
// ============================================================================

export interface SearchInput {
  query: string;
  corpus: string[];
  params?: Record<string, unknown>;
}

export interface SearchOutput {
  scores: number[];
  ranked_indices: number[];
}

export interface ScoredItem {
  index: number;
  score: number;
  content: string;
}

// ============================================================================
// Client
// ============================================================================

export class SearchWorkerClient {
  /**
   * Check if search is available (always true with continuum-core)
   */
  isAvailable(): boolean {
    return true;  // continuum-core is always available
  }

  /**
   * List available algorithms
   */
  async listAlgorithms(): Promise<string[]> {
    const result = await SearchList.execute({});
    return (result as unknown as { algorithms?: string[] }).algorithms || ['bow', 'bm25'];
  }

  /**
   * Execute search with specified algorithm
   */
  async search(
    algorithm: 'bow' | 'bm25',
    query: string,
    corpus: string[],
    params?: Record<string, unknown>
  ): Promise<ScoredItem[]> {
    const result = await SearchExecute.execute({
      algorithm,
      query,
      corpus,
      params
    });

    const output = result as unknown as SearchOutput;

    // Return scored items sorted by rank
    return output.ranked_indices.map((index) => ({
      index,
      score: output.scores[index],
      content: corpus[index]
    }));
  }

  /**
   * Convenience: BM25 search
   */
  async bm25(query: string, corpus: string[], k1?: number, b?: number): Promise<ScoredItem[]> {
    const params: Record<string, unknown> = {};
    if (k1 !== undefined) params.k1 = k1;
    if (b !== undefined) params.b = b;
    return this.search('bm25', query, corpus, Object.keys(params).length > 0 ? params : undefined);
  }

  /**
   * Convenience: Bag of Words search
   */
  async bow(query: string, corpus: string[]): Promise<ScoredItem[]> {
    return this.search('bow', query, corpus);
  }
}

// ============================================================================
// Singleton Instance
// ============================================================================

let _instance: SearchWorkerClient | null = null;

export function getSearchWorkerClient(): SearchWorkerClient {
  if (!_instance) {
    _instance = new SearchWorkerClient();
  }
  return _instance;
}
