/**
 * Search Worker Client - TypeScript client for Rust search worker
 *
 * Provides high-performance vector search via Unix socket to Rust worker.
 * Primary use case: Semantic memory recall with cosine similarity.
 *
 * Protocol:
 * - Newline-delimited JSON over Unix socket
 * - Commands: ping, vector-search, search, list-algorithms
 */

import * as net from 'net';

// ============================================================================
// Types
// ============================================================================

export interface VectorSearchRequest {
  /** Query embedding vector */
  queryVector: number[];
  /** Corpus embedding vectors to search */
  corpusVectors: number[][];
  /** Optional: normalize vectors before comparison (default: true) */
  normalize?: boolean;
  /** Optional: minimum similarity threshold (default: 0.0) */
  threshold?: number;
}

export interface VectorSearchResponse {
  /** Algorithm used (always 'cosine' for vector search) */
  algorithm: string;
  /** Cosine similarity scores parallel to corpusVectors */
  scores: number[];
  /** Indices sorted by score descending */
  rankedIndices: number[];
}

export interface TextSearchRequest {
  /** Algorithm: 'bow', 'bm25', or 'cosine' (for Jaccard on terms) */
  algorithm: string;
  /** Query text */
  query: string;
  /** Corpus documents */
  corpus: string[];
  /** Optional algorithm parameters */
  params?: Record<string, any>;
}

export interface TextSearchResponse {
  algorithm: string;
  scores: number[];
  rankedIndices: number[];
}

interface SearchWorkerResponse {
  status: 'ok' | 'error' | 'pong';
  data?: any;
  message?: string;
  algorithms?: string[];
}

// ============================================================================
// Client
// ============================================================================

/**
 * Client for Rust search worker
 *
 * Auto-connects on first use, auto-reconnects on connection loss.
 */
export class SearchWorkerClient {
  private static instance: SearchWorkerClient | null = null;

  private socket: net.Socket | null = null;
  private buffer: string = '';
  private pendingResponse: {
    resolve: (value: any) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
  } | null = null;

  private constructor(
    private readonly socketPath: string = '/tmp/jtag-search-worker.sock',
    private readonly timeout: number = 10000
  ) {}

  /**
   * Get singleton instance
   */
  static getInstance(socketPath?: string): SearchWorkerClient {
    if (!SearchWorkerClient.instance) {
      SearchWorkerClient.instance = new SearchWorkerClient(socketPath);
    }
    return SearchWorkerClient.instance;
  }

  /**
   * Vector search using cosine similarity
   *
   * @param request Query and corpus vectors
   * @returns Ranked results with similarity scores
   */
  async vectorSearch(request: VectorSearchRequest): Promise<VectorSearchResponse> {
    const response = await this.sendCommand<any>('vector-search', {
      query_vector: request.queryVector,
      corpus_vectors: request.corpusVectors,
      normalize: request.normalize ?? true,
      threshold: request.threshold ?? 0.0
    });
    return {
      algorithm: response.algorithm,
      scores: response.scores,
      rankedIndices: response.ranked_indices  // Rust uses snake_case
    };
  }

  /**
   * Text search using BM25, BoW, or term-based cosine
   */
  async textSearch(request: TextSearchRequest): Promise<TextSearchResponse> {
    const response = await this.sendCommand<any>('search', {
      algorithm: request.algorithm,
      query: request.query,
      corpus: request.corpus,
      params: request.params
    });
    return {
      algorithm: response.algorithm,
      scores: response.scores,
      rankedIndices: response.ranked_indices  // Rust uses snake_case
    };
  }

  /**
   * List available algorithms
   */
  async listAlgorithms(): Promise<string[]> {
    const response = await this.sendCommand<{ algorithms: string[] }>('list-algorithms', {});
    return response.algorithms;
  }

  /**
   * Ping worker to check if alive
   */
  async ping(): Promise<boolean> {
    try {
      await this.sendCommand('ping', {});
      return true;
    } catch {
      return false;
    }
  }

  // ============================================================================
  // Connection Management
  // ============================================================================

  private async ensureConnected(): Promise<void> {
    if (!this.socket) {
      await this.connect();
    }
  }

  private async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.socket = net.createConnection(this.socketPath);

      this.socket.on('connect', () => {
        console.log(`✅ Connected to search worker: ${this.socketPath}`);
        resolve();
      });

      this.socket.on('data', (data) => {
        this.handleData(data);
      });

      this.socket.on('error', (error) => {
        console.error('❌ Search worker socket error:', error);
        reject(error);
      });

      this.socket.on('close', () => {
        console.warn('⚠️  Search worker connection closed');
        this.socket = null;
      });

      // Connection timeout
      setTimeout(() => {
        if (!this.socket || this.socket.connecting) {
          reject(new Error(`Connection timeout: ${this.socketPath}`));
        }
      }, this.timeout);
    });
  }

  private handleData(data: Buffer): void {
    this.buffer += data.toString();

    const lines = this.buffer.split('\n');
    this.buffer = lines.pop() || '';

    for (const line of lines) {
      if (!line.trim()) continue;

      try {
        const response: SearchWorkerResponse = JSON.parse(line);

        if (this.pendingResponse) {
          clearTimeout(this.pendingResponse.timeout);
          const pending = this.pendingResponse;
          this.pendingResponse = null;

          if (response.status === 'error') {
            pending.reject(new Error(response.message || 'Search worker error'));
          } else {
            pending.resolve(response.data || response);
          }
        }
      } catch (error) {
        console.error('Failed to parse search worker response:', error);
      }
    }
  }

  private async sendCommand<T = any>(command: string, params: Record<string, any>): Promise<T> {
    await this.ensureConnected();

    // Retry once on connection error
    try {
      return await this.doSendCommand<T>(command, params);
    } catch (error: any) {
      if (error.message.includes('Connection') || error.message.includes('socket')) {
        console.log('🔄 Reconnecting to search worker...');
        this.socket = null;
        await this.ensureConnected();
        return await this.doSendCommand<T>(command, params);
      }
      throw error;
    }
  }

  private async doSendCommand<T>(command: string, params: Record<string, any>): Promise<T> {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pendingResponse = null;
        reject(new Error(`Search worker timeout: ${command}`));
      }, this.timeout);

      this.pendingResponse = { resolve, reject, timeout };

      const request = { command, ...params };
      this.socket!.write(JSON.stringify(request) + '\n');
    });
  }

  /**
   * Close connection
   */
  async close(): Promise<void> {
    if (this.socket) {
      this.socket.destroy();
      this.socket = null;
    }

    if (this.pendingResponse) {
      clearTimeout(this.pendingResponse.timeout);
      this.pendingResponse.reject(new Error('Connection closed'));
      this.pendingResponse = null;
    }
  }
}
