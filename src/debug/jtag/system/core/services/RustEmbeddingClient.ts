/**
 * Rust Embedding Client
 *
 * Communicates with the Rust embedding-worker over Unix socket.
 * Uses fastembed (ONNX-based) for native embedding generation without HTTP overhead.
 *
 * Performance: ~5ms per embedding (vs ~80ms via Ollama HTTP)
 * Batch: 100 texts in ~100ms (vs ~8s via Ollama HTTP)
 *
 * PROTOCOL:
 * - Requests: JSON (newline-delimited)
 * - Responses:
 *   - Control (ping, model/list): JSON
 *   - Data (embedding/generate): BINARY
 *     - JSON header (newline-terminated): {"type":"binary","length":1536,...}
 *     - Raw f32 bytes (no serialization overhead)
 */

import * as net from 'net';
import { Logger } from '../logging/Logger';

const log = Logger.create('RustEmbeddingClient', 'embedding');

/** Binary response header from Rust worker */
interface BinaryHeader {
  type: 'binary';
  length: number;
  dtype: 'f32' | 'f16' | 'u8' | 'i16';
  shape: number[];
  batchSize?: number;
  durationMs?: number;
  model?: string;
}

/** Default socket path for embedding worker */
const DEFAULT_SOCKET_PATH = '/tmp/jtag-embedding.sock';

/** Available embedding models in Rust worker */
export type RustEmbeddingModel =
  | 'AllMiniLML6V2'     // 384 dims, fast, default
  | 'AllMiniLML6V2Q'    // 384 dims, quantized, fastest
  | 'BGESmallENV15'     // 384 dims, better quality
  | 'BGEBaseENV15'      // 768 dims, high quality
  | 'NomicEmbedTextV15'; // 768 dims, same as Ollama nomic-embed-text

/** Model info returned by worker */
export interface RustModelInfo {
  name: string;
  dimensions: number;
  description: string;
  size_mb: number;
  loaded: boolean;
}

/** Response from Rust worker */
interface RustResponse {
  status: 'ok' | 'error' | 'pong';
  data?: any;
  message?: string;
  uptime_seconds?: number;
}

/** Generate embeddings request options */
export interface GenerateOptions {
  /** Model to use (default: AllMiniLML6V2) */
  model?: RustEmbeddingModel;
  /** Connection timeout in ms (default: 30000) */
  timeout?: number;
}

/**
 * Rust Embedding Client - Fast native embedding generation
 *
 * Singleton pattern for connection reuse across requests.
 */
export class RustEmbeddingClient {
  private static _instance: RustEmbeddingClient | null = null;

  private socketPath: string;
  private socket: net.Socket | null = null;
  private buffer: Buffer = Buffer.alloc(0);

  // Pending response for JSON commands
  private pendingJsonResponse: {
    resolve: (value: RustResponse) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
  } | null = null;

  // Pending response for BINARY commands (embeddings)
  private pendingBinaryResponse: {
    resolve: (value: number[][]) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
    header?: BinaryHeader;
  } | null = null;

  /** Track availability to avoid repeated connection attempts */
  private _available: boolean | null = null;
  private _lastAvailabilityCheck: number = 0;
  private static readonly AVAILABILITY_CACHE_MS = 5000; // Cache availability for 5s

  /** Request queue for serializing concurrent requests */
  private requestQueue: (() => Promise<void>)[] = [];
  private isProcessingQueue: boolean = false;

  private constructor(socketPath: string = DEFAULT_SOCKET_PATH) {
    this.socketPath = socketPath;
  }

  /** Get shared instance */
  static get instance(): RustEmbeddingClient {
    if (!this._instance) {
      this._instance = new RustEmbeddingClient();
    }
    return this._instance;
  }

  /**
   * Check if Rust embedding worker is available
   *
   * Uses cached result for 5 seconds to avoid spamming connection attempts.
   */
  async isAvailable(): Promise<boolean> {
    const now = Date.now();
    if (this._available !== null && (now - this._lastAvailabilityCheck) < RustEmbeddingClient.AVAILABILITY_CACHE_MS) {
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
    const response = await this.sendCommand('ping', {});
    if (response.status === 'pong') {
      return { uptime_seconds: response.uptime_seconds || 0 };
    }
    throw new Error(response.message || 'Ping failed');
  }

  /**
   * Generate embeddings for texts
   *
   * Uses BINARY protocol - no JSON serialization of float arrays.
   *
   * @param texts - Array of texts to embed
   * @param options - Generation options (model, timeout)
   * @returns Array of embeddings (same order as input texts)
   */
  async generate(texts: string[], options: GenerateOptions = {}): Promise<number[][]> {
    const model = options.model || 'AllMiniLML6V2';
    const timeout = options.timeout || 30000;

    if (texts.length === 0) {
      return [];
    }

    const startTime = Date.now();

    // Use binary protocol for embeddings
    const embeddings = await this.sendBinaryCommand(
      'embedding/generate',
      { texts, model },
      timeout
    );

    const duration = Date.now() - startTime;
    log.debug(
      `Generated ${texts.length} embeddings in ${duration}ms (${(duration / texts.length).toFixed(1)}ms each) [BINARY]`
    );

    return embeddings;
  }

  /**
   * Generate embedding for a single text
   *
   * Convenience wrapper around generate() for single text.
   */
  async embed(text: string, options: GenerateOptions = {}): Promise<number[]> {
    const [embedding] = await this.generate([text], options);
    return embedding;
  }

  /**
   * List available models with their status
   */
  async listModels(): Promise<RustModelInfo[]> {
    const response = await this.sendCommand('embedding/model/list', {});
    if (response.status !== 'ok' || !response.data?.models) {
      throw new Error(response.message || 'Failed to list models');
    }
    return response.data.models;
  }

  /**
   * Pre-load a model into memory
   *
   * Useful for warming up before first request.
   */
  async loadModel(model: RustEmbeddingModel): Promise<void> {
    const response = await this.sendCommand('embedding/model/load', { model });
    if (response.status !== 'ok') {
      throw new Error(response.message || 'Failed to load model');
    }
    log.info(`Loaded model: ${model}`);
  }

  /**
   * Unload a model from memory
   */
  async unloadModel(model: RustEmbeddingModel): Promise<void> {
    const response = await this.sendCommand('embedding/model/unload', { model });
    if (response.status !== 'ok') {
      throw new Error(response.message || 'Failed to unload model');
    }
    log.info(`Unloaded model: ${model}`);
  }

  /**
   * Close the connection
   */
  close(): void {
    if (this.socket) {
      this.socket.destroy();
      this.socket = null;
    }
    if (this.pendingJsonResponse) {
      clearTimeout(this.pendingJsonResponse.timeout);
      this.pendingJsonResponse.reject(new Error('Connection closed'));
      this.pendingJsonResponse = null;
    }
    if (this.pendingBinaryResponse) {
      clearTimeout(this.pendingBinaryResponse.timeout);
      this.pendingBinaryResponse.reject(new Error('Connection closed'));
      this.pendingBinaryResponse = null;
    }
    this._available = null;
  }

  // ============================================================================
  // Private Methods
  // ============================================================================

  /**
   * Connect to Rust worker Unix socket
   */
  private async connect(): Promise<void> {
    if (this.socket && !this.socket.destroyed) {
      return; // Already connected
    }

    return new Promise((resolve, reject) => {
      this.socket = net.createConnection(this.socketPath);
      this.buffer = Buffer.alloc(0);

      this.socket.on('connect', () => {
        log.debug(`Connected to Rust embedding worker: ${this.socketPath}`);
        resolve();
      });

      this.socket.on('data', (data) => {
        this.handleData(data);
      });

      this.socket.on('error', (error) => {
        log.debug(`Socket error: ${error.message}`);
        this._available = false;
        reject(error);
      });

      this.socket.on('close', () => {
        log.debug('Socket closed');
        this.socket = null;
        this._available = null;
      });

      // Connection timeout
      const timeout = setTimeout(() => {
        if (this.socket && this.socket.connecting) {
          this.socket.destroy();
          reject(new Error(`Connection timeout: ${this.socketPath}`));
        }
      }, 5000);

      this.socket.once('connect', () => clearTimeout(timeout));
    });
  }

  /**
   * Handle incoming data from Rust worker
   * Supports both JSON (control) and BINARY (data) protocols
   */
  private handleData(data: Buffer): void {
    // Append to buffer
    this.buffer = Buffer.concat([this.buffer, data]);

    // Handle BINARY response (embedding/generate)
    if (this.pendingBinaryResponse) {
      this.handleBinaryData();
      return;
    }

    // Handle JSON response (control messages)
    this.handleJsonData();
  }

  /**
   * Handle JSON response data (control messages)
   */
  private handleJsonData(): void {
    // Find newline
    const newlineIdx = this.buffer.indexOf(0x0a); // '\n'
    if (newlineIdx === -1) return;

    const line = this.buffer.subarray(0, newlineIdx).toString();
    this.buffer = this.buffer.subarray(newlineIdx + 1);

    if (!line.trim()) return;

    try {
      const response: RustResponse = JSON.parse(line);

      if (this.pendingJsonResponse) {
        clearTimeout(this.pendingJsonResponse.timeout);
        const pending = this.pendingJsonResponse;
        this.pendingJsonResponse = null;
        pending.resolve(response);
      }
    } catch (error) {
      log.error(`Failed to parse JSON response: ${error}`);
    }
  }

  /**
   * Handle BINARY response data (embeddings)
   *
   * Protocol: JSON header (newline-terminated) + raw f32 bytes
   */
  private handleBinaryData(): void {
    const pending = this.pendingBinaryResponse;
    if (!pending) return;

    // Step 1: Parse header if not yet done
    if (!pending.header) {
      const newlineIdx = this.buffer.indexOf(0x0a);
      if (newlineIdx === -1) return; // Need more data for header

      const headerStr = this.buffer.subarray(0, newlineIdx).toString();
      this.buffer = this.buffer.subarray(newlineIdx + 1);

      try {
        const header = JSON.parse(headerStr);

        // Check for error response (still JSON)
        if (header.status === 'error') {
          clearTimeout(pending.timeout);
          this.pendingBinaryResponse = null;
          pending.reject(new Error(header.message || 'Embedding generation failed'));
          return;
        }

        if (header.type !== 'binary') {
          clearTimeout(pending.timeout);
          this.pendingBinaryResponse = null;
          pending.reject(new Error(`Expected binary header, got: ${header.type}`));
          return;
        }

        pending.header = header as BinaryHeader;
      } catch (error) {
        clearTimeout(pending.timeout);
        this.pendingBinaryResponse = null;
        pending.reject(new Error(`Failed to parse binary header: ${error}`));
        return;
      }
    }

    // Step 2: Wait for complete binary payload
    const header = pending.header;
    if (this.buffer.length < header.length) {
      return; // Need more data
    }

    // Step 3: Extract binary payload and convert to embeddings
    const binaryData = this.buffer.subarray(0, header.length);
    this.buffer = this.buffer.subarray(header.length);

    try {
      const embeddings = this.parseBinaryEmbeddings(binaryData, header);

      clearTimeout(pending.timeout);
      this.pendingBinaryResponse = null;
      pending.resolve(embeddings);
    } catch (error) {
      clearTimeout(pending.timeout);
      this.pendingBinaryResponse = null;
      pending.reject(error instanceof Error ? error : new Error(String(error)));
    }
  }

  /**
   * Parse binary f32 data into embedding arrays
   *
   * ZERO-COPY: Uses Buffer's underlying ArrayBuffer directly
   */
  private parseBinaryEmbeddings(data: Buffer, header: BinaryHeader): number[][] {
    const dims = header.shape[0] || 384;
    const batchSize = header.batchSize || 1;

    if (batchSize === 0 || header.length === 0) {
      return [];
    }

    // Create Float32Array view over the buffer
    // Note: Node Buffer may not be aligned, so we copy to aligned buffer
    const alignedBuffer = new ArrayBuffer(data.length);
    const alignedView = new Uint8Array(alignedBuffer);
    alignedView.set(data);

    const floats = new Float32Array(alignedBuffer);

    // Split into individual embeddings
    const embeddings: number[][] = [];
    for (let i = 0; i < batchSize; i++) {
      const start = i * dims;
      const end = start + dims;
      // Slice creates a copy, which is needed since we're returning number[][]
      embeddings.push(Array.from(floats.subarray(start, end)));
    }

    return embeddings;
  }

  /**
   * Send JSON command to Rust worker and wait for JSON response
   * Uses queue to serialize concurrent requests - prevents socket contention
   */
  private async sendCommand(
    command: string,
    params: Record<string, unknown>,
    timeout: number = 30000
  ): Promise<RustResponse> {
    return new Promise((outerResolve, outerReject) => {
      const doRequest = async (): Promise<void> => {
        try {
          await this.connect();

          const request = { command, ...params };

          const result = await new Promise<RustResponse>((resolve, reject) => {
            const timeoutHandle = setTimeout(() => {
              this.pendingJsonResponse = null;
              reject(new Error(`Request timeout: ${command}`));
            }, timeout);

            this.pendingJsonResponse = { resolve, reject, timeout: timeoutHandle };
            this.socket!.write(JSON.stringify(request) + '\n');
          });

          outerResolve(result);
        } catch (error) {
          outerReject(error);
        }
      };

      this.requestQueue.push(doRequest);
      this.processQueue();
    });
  }

  /**
   * Send command expecting BINARY response (embeddings)
   * Uses queue to serialize concurrent requests
   */
  private async sendBinaryCommand(
    command: string,
    params: Record<string, unknown>,
    timeout: number = 30000
  ): Promise<number[][]> {
    return new Promise((outerResolve, outerReject) => {
      const doRequest = async (): Promise<void> => {
        try {
          await this.connect();

          const request = { command, ...params };

          const result = await new Promise<number[][]>((resolve, reject) => {
            const timeoutHandle = setTimeout(() => {
              this.pendingBinaryResponse = null;
              reject(new Error(`Request timeout: ${command}`));
            }, timeout);

            this.pendingBinaryResponse = { resolve, reject, timeout: timeoutHandle };
            this.socket!.write(JSON.stringify(request) + '\n');
          });

          outerResolve(result);
        } catch (error) {
          outerReject(error);
        }
      };

      this.requestQueue.push(doRequest);
      this.processQueue();
    });
  }

  /**
   * Process queued requests one at a time
   */
  private async processQueue(): Promise<void> {
    if (this.isProcessingQueue) {
      return;
    }

    this.isProcessingQueue = true;

    try {
      while (this.requestQueue.length > 0) {
        const nextRequest = this.requestQueue.shift();
        if (nextRequest) {
          await nextRequest();
        }
      }
    } finally {
      this.isProcessingQueue = false;
    }
  }
}
