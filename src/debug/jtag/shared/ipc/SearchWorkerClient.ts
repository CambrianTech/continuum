/**
 * SearchWorkerClient - TypeScript client for Rust search worker
 *
 * Simple protocol matching the search worker's interface:
 * - Send: { command, algorithm, query, corpus, params }
 * - Receive: { status, data } or { status, message }
 */

import * as net from 'net';
import * as fs from 'fs';

// ============================================================================
// Types
// ============================================================================

export interface SearchRequest {
  command: 'search' | 'ping' | 'list-algorithms' | 'algorithm-params';
  algorithm?: string;
  query?: string;
  corpus?: string[];
  params?: Record<string, unknown>;
}

export interface SearchResult {
  algorithm: string;
  scores: number[];
  ranked_indices: number[];
}

export interface SearchResponse {
  status: 'ok' | 'error' | 'pong';
  data?: SearchResult | { algorithms: string[] };
  message?: string;
  algorithms?: string[];  // For pong response
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
  private socketPath: string;
  private timeout: number;

  constructor(socketPath: string = '/tmp/jtag-search-worker.sock', timeout: number = 5000) {
    this.socketPath = socketPath;
    this.timeout = timeout;
  }

  /**
   * Check if worker is available
   */
  isAvailable(): boolean {
    return fs.existsSync(this.socketPath);
  }

  /**
   * Send request to worker and get response
   */
  private async sendRequest(request: SearchRequest): Promise<SearchResponse> {
    return new Promise((resolve, reject) => {
      if (!this.isAvailable()) {
        reject(new Error(`Search worker not available at ${this.socketPath}`));
        return;
      }

      const socket = net.createConnection(this.socketPath);
      let buffer = '';

      const timeoutId = setTimeout(() => {
        socket.destroy();
        reject(new Error(`Search worker request timeout after ${this.timeout}ms`));
      }, this.timeout);

      socket.on('connect', () => {
        const json = JSON.stringify(request) + '\n';
        socket.write(json);
      });

      socket.on('data', (data) => {
        buffer += data.toString();
        const lines = buffer.split('\n');

        for (const line of lines) {
          if (line.trim()) {
            try {
              const response = JSON.parse(line) as SearchResponse;
              clearTimeout(timeoutId);
              socket.end();
              resolve(response);
              return;
            } catch {
              // Incomplete JSON, wait for more data
            }
          }
        }
      });

      socket.on('error', (err) => {
        clearTimeout(timeoutId);
        reject(err);
      });
    });
  }

  /**
   * Ping the worker
   */
  async ping(): Promise<string[]> {
    const response = await this.sendRequest({ command: 'ping' });
    if (response.status === 'pong' && response.algorithms) {
      return response.algorithms;
    }
    throw new Error('Unexpected ping response');
  }

  /**
   * List available algorithms
   */
  async listAlgorithms(): Promise<string[]> {
    const response = await this.sendRequest({ command: 'list-algorithms' });
    if (response.status === 'ok' && response.data && 'algorithms' in response.data) {
      return response.data.algorithms;
    }
    throw new Error('Failed to list algorithms');
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
    const response = await this.sendRequest({
      command: 'search',
      algorithm,
      query,
      corpus,
      params
    });

    if (response.status === 'error') {
      throw new Error(response.message || 'Search failed');
    }

    if (response.status === 'ok' && response.data && 'scores' in response.data) {
      const result = response.data as SearchResult;

      // Return scored items sorted by rank
      return result.ranked_indices.map((index) => ({
        index,
        score: result.scores[index],
        content: corpus[index]
      }));
    }

    throw new Error('Unexpected search response');
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
