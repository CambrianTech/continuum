/**
 * Rust Vector Search Client
 *
 * Routes vector similarity search to the Rust data-daemon-worker.
 * Vectors stay in Rust (read directly from SQLite) - only query vector sent over IPC.
 *
 * Performance: ~60ms for 3000+ vectors (vs ~500ms when vectors sent to TypeScript)
 *
 * NOTE: Only used for vector search. CRUD operations use TypeScript SqliteStorageAdapter.
 * The Rust worker has well-designed vector search but CRUD had concurrency issues.
 */

import * as net from 'net';
import { Logger } from '../logging/Logger';
import { getDatabasePath } from '../../config/ServerConfig';

const log = Logger.create('RustVectorSearchClient', 'vector');

/** Default socket path for data-daemon worker */
const DEFAULT_SOCKET_PATH = '/tmp/jtag-data-daemon-worker.sock';

/** Response from Rust worker */
interface RustResponse {
  status: 'ok' | 'error' | 'pong';
  data?: any;
  message?: string;
  uptime_seconds?: number;
}

/** Vector search result from Rust */
export interface RustVectorSearchResult {
  id: string;
  score: number;
  data?: Record<string, any>;
}

/** Vector search response from Rust */
export interface RustVectorSearchResponse {
  corpus_size: number;
  count: number;
  results: RustVectorSearchResult[];
}

/**
 * Rust Vector Search Client - Fast native vector search
 *
 * Singleton pattern for connection reuse.
 */
export class RustVectorSearchClient {
  private static _instance: RustVectorSearchClient | null = null;

  private socketPath: string;
  private handle: string | null = null;
  private dbPath: string;

  /** Track availability to avoid repeated connection attempts */
  private _available: boolean | null = null;
  private _lastAvailabilityCheck: number = 0;
  private static readonly AVAILABILITY_CACHE_MS = 5000;

  private constructor(socketPath: string = DEFAULT_SOCKET_PATH) {
    this.socketPath = socketPath;
    this.dbPath = getDatabasePath();
  }

  /** Get shared instance */
  static get instance(): RustVectorSearchClient {
    if (!this._instance) {
      this._instance = new RustVectorSearchClient();
    }
    return this._instance;
  }

  /**
   * Check if Rust vector search worker is available
   */
  async isAvailable(): Promise<boolean> {
    const now = Date.now();
    if (this._available !== null && (now - this._lastAvailabilityCheck) < RustVectorSearchClient.AVAILABILITY_CACHE_MS) {
      return this._available;
    }

    try {
      await this.ping();
      this._available = true;
    } catch {
      this._available = false;
    }
    this._lastAvailabilityCheck = now;
    return this._available;
  }

  /**
   * Ping the worker to check connectivity
   */
  async ping(): Promise<{ uptime_seconds: number }> {
    const response = await this.sendRequest({ command: 'ping' });
    if (response.status === 'pong') {
      return { uptime_seconds: response.uptime_seconds || 0 };
    }
    throw new Error(response.message || 'Ping failed');
  }

  /**
   * Open adapter and get handle (cached)
   */
  private async ensureHandle(): Promise<string> {
    if (this.handle) {
      return this.handle;
    }

    const response = await this.sendRequest({
      command: 'adapter/open',
      config: {
        adapter_type: 'sqlite',
        connection_string: this.dbPath
      }
    });

    if (response.status !== 'ok' || !response.data?.handle) {
      throw new Error(response.message || 'Failed to open adapter');
    }

    const handle = response.data.handle as string;
    this.handle = handle;
    log.info(`Opened Rust adapter: ${handle}`);
    return handle;
  }

  /**
   * Perform vector similarity search
   *
   * @param collection - Collection name (e.g., 'memories')
   * @param queryVector - Query embedding (384 dims for all-minilm)
   * @param k - Number of results (default: 10)
   * @param threshold - Minimum similarity threshold (default: 0.0)
   * @param includeData - Include full record data in results (default: true)
   */
  async search(
    collection: string,
    queryVector: number[],
    k: number = 10,
    threshold: number = 0.0,
    includeData: boolean = true
  ): Promise<RustVectorSearchResponse> {
    const handle = await this.ensureHandle();
    const startTime = Date.now();

    const response = await this.sendRequest({
      command: 'vector/search',
      handle,
      collection,
      query_vector: queryVector,
      k,
      threshold,
      include_data: includeData
    });

    if (response.status !== 'ok') {
      // If handle expired, clear it and retry once
      if (response.message?.includes('Adapter not found')) {
        log.warn('Adapter handle expired, reconnecting...');
        this.handle = null;
        return this.search(collection, queryVector, k, threshold, includeData);
      }
      throw new Error(response.message || 'Vector search failed');
    }

    const duration = Date.now() - startTime;
    log.debug(`Vector search: ${response.data.count}/${response.data.corpus_size} results in ${duration}ms`);

    return response.data;
  }

  /**
   * Close the adapter handle
   */
  async close(): Promise<void> {
    if (!this.handle) return;

    try {
      await this.sendRequest({
        command: 'adapter/close',
        handle: this.handle
      });
    } catch (error) {
      log.debug(`Failed to close adapter: ${error}`);
    }

    this.handle = null;
    this._available = null;
  }

  // ============================================================================
  // Private Methods
  // ============================================================================

  /**
   * Send request to Rust worker via Unix socket
   *
   * Uses one-shot connection per request to avoid concurrency issues.
   * Simple and reliable - each request gets its own socket.
   */
  private async sendRequest(request: Record<string, any>, timeout: number = 30000): Promise<RustResponse> {
    return new Promise((resolve, reject) => {
      const socket = net.createConnection(this.socketPath);
      let buffer = '';
      let responded = false;

      const timeoutHandle = setTimeout(() => {
        if (!responded) {
          responded = true;
          socket.destroy();
          reject(new Error(`Request timeout: ${request.command}`));
        }
      }, timeout);

      socket.on('connect', () => {
        socket.write(JSON.stringify(request) + '\n');
      });

      socket.on('data', (data) => {
        buffer += data.toString();
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (!line.trim()) continue;
          try {
            const response: RustResponse = JSON.parse(line);
            if (!responded) {
              responded = true;
              clearTimeout(timeoutHandle);
              socket.destroy();
              resolve(response);
            }
          } catch (error) {
            log.error(`Failed to parse response: ${error}`);
          }
        }
      });

      socket.on('error', (error) => {
        if (!responded) {
          responded = true;
          clearTimeout(timeoutHandle);
          reject(error);
        }
      });

      socket.on('close', () => {
        if (!responded) {
          responded = true;
          clearTimeout(timeoutHandle);
          reject(new Error('Connection closed'));
        }
      });
    });
  }
}
