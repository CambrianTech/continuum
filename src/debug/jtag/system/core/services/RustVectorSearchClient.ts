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
 * Singleton pattern with PERSISTENT CONNECTION for maximum throughput.
 * Request queue serializes concurrent requests over single socket.
 */
export class RustVectorSearchClient {
  private static _instance: RustVectorSearchClient | null = null;

  private socketPath: string;
  /** Handle cache: dbPath → handle */
  private handles: Map<string, string> = new Map();

  /** Track availability to avoid repeated connection attempts */
  private _available: boolean | null = null;
  private _lastAvailabilityCheck: number = 0;
  private static readonly AVAILABILITY_CACHE_MS = 5000;

  // Persistent connection
  private socket: net.Socket | null = null;
  private buffer: string = '';
  private isConnecting: boolean = false;
  private connectPromise: Promise<void> | null = null;

  // Serialized request queue (one at a time over persistent connection)
  private requestQueue: Array<{
    request: Record<string, any>;
    timeout: number;
    resolve: (value: RustResponse) => void;
    reject: (error: Error) => void;
  }> = [];
  private isProcessingQueue: boolean = false;
  private currentRequest: {
    resolve: (value: RustResponse) => void;
    reject: (error: Error) => void;
    timeoutHandle: NodeJS.Timeout;
  } | null = null;

  private constructor(socketPath: string = DEFAULT_SOCKET_PATH) {
    this.socketPath = socketPath;
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
   * Open adapter and get handle (cached per database path)
   *
   * @param dbPath - Database path (REQUIRED - no fallbacks)
   */
  private async ensureHandle(dbPath: string): Promise<string> {
    const cached = this.handles.get(dbPath);
    if (cached) {
      return cached;
    }

    const response = await this.sendRequest({
      command: 'adapter/open',
      config: {
        adapter_type: 'sqlite',
        connection_string: dbPath
      }
    });

    if (response.status !== 'ok' || !response.data?.handle) {
      throw new Error(response.message || 'Failed to open adapter');
    }

    const handle = response.data.handle as string;
    this.handles.set(dbPath, handle);
    log.info(`Opened Rust adapter for ${dbPath}: ${handle}`);
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
   * @param dbPath - Database path (REQUIRED - no fallbacks)
   */
  async search(
    collection: string,
    queryVector: number[],
    k: number = 10,
    threshold: number = 0.0,
    includeData: boolean = true,
    dbPath: string
  ): Promise<RustVectorSearchResponse> {
    const handle = await this.ensureHandle(dbPath);
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
        this.handles.delete(dbPath);
        return this.search(collection, queryVector, k, threshold, includeData, dbPath);
      }
      throw new Error(response.message || 'Vector search failed');
    }

    const duration = Date.now() - startTime;
    log.debug(`Vector search: ${response.data.count}/${response.data.corpus_size} results in ${duration}ms`);

    return response.data;
  }

  /**
   * Close all adapter handles
   */
  async close(): Promise<void> {
    if (this.handles.size === 0) return;

    for (const [dbPath, handle] of this.handles) {
      try {
        await this.sendRequest({
          command: 'adapter/close',
          handle
        });
        log.debug(`Closed adapter for ${dbPath}`);
      } catch (error) {
        log.debug(`Failed to close adapter for ${dbPath}: ${error}`);
      }
    }

    this.handles.clear();
    this._available = null;
  }

  // ============================================================================
  // Connection Management (Persistent)
  // ============================================================================

  /**
   * Ensure persistent connection is established
   */
  private async ensureConnected(): Promise<void> {
    if (this.socket && !this.socket.destroyed) {
      return; // Already connected
    }

    if (this.isConnecting && this.connectPromise) {
      return this.connectPromise; // Wait for existing connection attempt
    }

    this.isConnecting = true;
    this.connectPromise = new Promise((resolve, reject) => {
      const socket = net.createConnection(this.socketPath);

      const connectTimeout = setTimeout(() => {
        socket.destroy();
        this.isConnecting = false;
        reject(new Error(`Connection timeout after 10s`));
      }, 10000);

      socket.on('connect', () => {
        clearTimeout(connectTimeout);
        this.socket = socket;
        this.isConnecting = false;
        this.setupSocketHandlers();
        log.info('Persistent connection established to Rust worker');
        resolve();
      });

      socket.on('error', (err) => {
        clearTimeout(connectTimeout);
        this.isConnecting = false;
        reject(err);
      });
    });

    return this.connectPromise;
  }

  /**
   * Set up socket event handlers for persistent connection
   */
  private setupSocketHandlers(): void {
    if (!this.socket) return;

    this.socket.on('data', (data) => {
      this.buffer += data.toString();
      this.processBuffer();
    });

    this.socket.on('error', (err) => {
      log.error(`Socket error: ${err.message}`);
      this.handleDisconnect();
    });

    this.socket.on('close', () => {
      log.warn('Socket closed, will reconnect on next request');
      this.handleDisconnect();
    });
  }

  /**
   * Process buffered data - resolve current request
   */
  private processBuffer(): void {
    const lines = this.buffer.split('\n');
    this.buffer = lines.pop() || '';

    for (const line of lines) {
      if (!line.trim()) continue;
      try {
        const response = JSON.parse(line);
        // Resolve current request (serialized - one at a time)
        if (this.currentRequest) {
          clearTimeout(this.currentRequest.timeoutHandle);
          this.currentRequest.resolve(response);
          this.currentRequest = null;
          // Process next in queue
          this.processQueue();
        }
      } catch (error) {
        log.error(`Failed to parse response: ${error}`);
      }
    }
  }

  /**
   * Handle disconnection - reject all pending requests
   */
  private handleDisconnect(): void {
    this.socket = null;
    // Reject current request
    if (this.currentRequest) {
      clearTimeout(this.currentRequest.timeoutHandle);
      this.currentRequest.reject(new Error('Connection lost'));
      this.currentRequest = null;
    }
    // Reject queued requests
    for (const queued of this.requestQueue) {
      queued.reject(new Error('Connection lost'));
    }
    this.requestQueue = [];
    this.isProcessingQueue = false;
  }

  /**
   * Process next request in queue
   */
  private async processQueue(): Promise<void> {
    if (this.isProcessingQueue || this.requestQueue.length === 0) return;
    if (this.currentRequest) return; // Wait for current to complete

    this.isProcessingQueue = true;

    while (this.requestQueue.length > 0) {
      const next = this.requestQueue.shift()!;

      try {
        await this.ensureConnected();

        const timeoutHandle = setTimeout(() => {
          if (this.currentRequest) {
            this.currentRequest.reject(new Error(`Request timeout: ${next.request.command}`));
            this.currentRequest = null;
            this.processQueue();
          }
        }, next.timeout);

        this.currentRequest = {
          resolve: next.resolve,
          reject: next.reject,
          timeoutHandle
        };

        this.socket!.write(JSON.stringify(next.request) + '\n');
        // Wait for response (processBuffer will call resolve)
        await new Promise<void>((resolve) => {
          const checkDone = setInterval(() => {
            if (!this.currentRequest) {
              clearInterval(checkDone);
              resolve();
            }
          }, 1);
        });
      } catch (error) {
        next.reject(error as Error);
      }
    }

    this.isProcessingQueue = false;
  }

  // ============================================================================
  // Private Methods
  // ============================================================================

  /**
   * Send request to Rust worker via persistent Unix socket
   *
   * Uses serialized request queue over persistent connection.
   * Connection is reused across requests - eliminates connection setup overhead.
   */
  private async sendRequest(request: Record<string, any>, timeout: number = 30000): Promise<RustResponse> {
    return new Promise((resolve, reject) => {
      this.requestQueue.push({ request, timeout, resolve, reject });
      this.processQueue();
    });
  }
}
