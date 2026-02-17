/**
 * Rust Embedding Client
 *
 * Communicates with continuum-core over Unix socket.
 * Uses fastembed (ONNX-based) for native embedding generation without HTTP overhead.
 *
 * Performance: ~5ms per embedding (vs ~80ms via HTTP-based providers)
 * Batch: 100 texts in ~100ms (vs ~8s via HTTP-based providers)
 *
 * PROTOCOL (continuum-core length-prefixed framing):
 * - Requests: JSON (newline-delimited)
 * - Responses:
 *   - JSON: [4 bytes u32 BE length][JSON payload bytes]
 *   - Binary: [4 bytes u32 BE total_length][JSON header bytes][\0][raw binary bytes]
 */

import * as net from 'net';
import * as path from 'path';
import { Logger } from '../logging/Logger';
import { SOCKETS } from '../../../shared/config';

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

/** Default socket path - resolved from shared config */
const DEFAULT_SOCKET_PATH = path.isAbsolute(SOCKETS.CONTINUUM_CORE)
  ? SOCKETS.CONTINUUM_CORE
  : path.resolve(process.cwd(), SOCKETS.CONTINUUM_CORE);

/** Available embedding models in Rust worker */
export type RustEmbeddingModel =
  | 'AllMiniLML6V2'     // 384 dims, fast, default
  | 'AllMiniLML6V2Q'    // 384 dims, quantized, fastest
  | 'BGESmallENV15'     // 384 dims, better quality
  | 'BGEBaseENV15'      // 768 dims, high quality
  | 'NomicEmbedTextV15'; // 768 dims, same as nomic-embed-text

/** Model info returned by worker */
export interface RustModelInfo {
  name: string;
  dimensions: number;
  description: string;
  size_mb: number;
  loaded: boolean;
}

/** Response from Rust worker (continuum-core format) */
interface RustResponse {
  success: boolean;
  result?: any;
  error?: string | null;
  requestId?: number | null;
  // Backwards compat with old format
  status?: 'ok' | 'error' | 'pong';
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
  } | null = null;

  // Frame parsing state
  private expectedFrameLength: number | null = null;

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
   * Ping the worker to check connectivity.
   * Uses health-check (supported by continuum-core) instead of ping.
   */
  async ping(): Promise<{ uptime_seconds: number }> {
    const response = await this.sendCommand('health-check', {});
    // continuum-core format: {success: true, result: {healthy: true, uptime_seconds: N}}
    if (response.success && response.result?.healthy) {
      return { uptime_seconds: response.result.uptime_seconds || 0 };
    }
    // Old format fallback: {status: 'ok', data: {...}}
    if (response.status === 'ok' && response.data?.healthy) {
      return { uptime_seconds: response.data.uptime_seconds || 0 };
    }
    throw new Error(response.error || response.message || 'Health check failed');
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
    // continuum-core format: {success: true, result: {models: [...]}}
    if (response.success && response.result?.models) {
      return response.result.models;
    }
    // Old format fallback
    if (response.status === 'ok' && response.data?.models) {
      return response.data.models;
    }
    throw new Error(response.error || response.message || 'Failed to list models');
  }

  /**
   * Pre-load a model into memory
   *
   * Useful for warming up before first request.
   */
  async loadModel(model: RustEmbeddingModel): Promise<void> {
    const response = await this.sendCommand('embedding/model/load', { model });
    if (!response.success && response.status !== 'ok') {
      throw new Error(response.error || response.message || 'Failed to load model');
    }
    log.info(`Loaded model: ${model}`);
  }

  /**
   * Unload a model from memory
   */
  async unloadModel(model: RustEmbeddingModel): Promise<void> {
    const response = await this.sendCommand('embedding/model/unload', { model });
    if (!response.success && response.status !== 'ok') {
      throw new Error(response.error || response.message || 'Failed to unload model');
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
    this.expectedFrameLength = null;
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
      this.expectedFrameLength = null;

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
   *
   * Protocol: Length-prefixed framing
   * - [4 bytes u32 BE length][payload]
   * - For JSON: payload is JSON bytes
   * - For Binary: payload is [JSON header bytes][\0][raw binary bytes]
   */
  private handleData(data: Buffer): void {
    // Append to buffer
    this.buffer = Buffer.concat([this.buffer, data]);

    // Process complete frames
    this.processFrames();
  }

  /**
   * Process complete length-prefixed frames from buffer
   */
  private processFrames(): void {
    while (true) {
      // Step 1: Read frame length if not yet known
      if (this.expectedFrameLength === null) {
        if (this.buffer.length < 4) {
          return; // Need more data for length prefix
        }
        this.expectedFrameLength = this.buffer.readUInt32BE(0);
        this.buffer = this.buffer.subarray(4);
      }

      // Step 2: Wait for complete frame payload
      if (this.buffer.length < this.expectedFrameLength) {
        return; // Need more data
      }

      // Step 3: Extract frame payload
      const framePayload = this.buffer.subarray(0, this.expectedFrameLength);
      this.buffer = this.buffer.subarray(this.expectedFrameLength);
      this.expectedFrameLength = null;

      // Step 4: Dispatch to appropriate handler
      if (this.pendingBinaryResponse) {
        this.handleBinaryFrame(framePayload);
      } else if (this.pendingJsonResponse) {
        this.handleJsonFrame(framePayload);
      } else {
        log.warn('Received frame with no pending request');
      }
    }
  }

  /**
   * Handle JSON response frame (control messages)
   */
  private handleJsonFrame(payload: Buffer): void {
    const pending = this.pendingJsonResponse;
    if (!pending) return;

    try {
      const jsonStr = payload.toString('utf8');
      const response: RustResponse = JSON.parse(jsonStr);

      clearTimeout(pending.timeout);
      this.pendingJsonResponse = null;
      pending.resolve(response);
    } catch (error) {
      clearTimeout(pending.timeout);
      this.pendingJsonResponse = null;
      pending.reject(new Error(`Failed to parse JSON response: ${error}`));
    }
  }

  /**
   * Handle BINARY response frame (embeddings)
   *
   * Frame format: [JSON header bytes][\0][raw binary bytes]
   */
  private handleBinaryFrame(payload: Buffer): void {
    const pending = this.pendingBinaryResponse;
    if (!pending) return;

    try {
      // Find null separator
      const nullIdx = payload.indexOf(0x00);
      if (nullIdx === -1) {
        // No separator - this is a pure JSON response (error case)
        const jsonStr = payload.toString('utf8');
        const response = JSON.parse(jsonStr);

        if (!response.success) {
          clearTimeout(pending.timeout);
          this.pendingBinaryResponse = null;
          pending.reject(new Error(response.error || 'Request failed'));
          return;
        }

        // Shouldn't happen - binary response without binary data
        clearTimeout(pending.timeout);
        this.pendingBinaryResponse = null;
        pending.reject(new Error('Expected binary data but got pure JSON'));
        return;
      }

      // Parse JSON header before null separator
      const headerStr = payload.subarray(0, nullIdx).toString('utf8');
      const response = JSON.parse(headerStr);

      // Check for error
      if (!response.success) {
        clearTimeout(pending.timeout);
        this.pendingBinaryResponse = null;
        pending.reject(new Error(response.error || 'Embedding generation failed'));
        return;
      }

      // Extract metadata from result (continuum-core wraps in {success, result})
      const metadata = response.result;
      if (!metadata || metadata.type !== 'binary') {
        clearTimeout(pending.timeout);
        this.pendingBinaryResponse = null;
        pending.reject(new Error(`Expected binary metadata, got: ${JSON.stringify(metadata)}`));
        return;
      }

      // Extract binary data after null separator
      const binaryData = payload.subarray(nullIdx + 1);

      // Parse embeddings
      const header: BinaryHeader = {
        type: 'binary',
        length: binaryData.length,
        dtype: metadata.dtype || 'f32',
        shape: metadata.shape || [384],
        batchSize: metadata.batchSize || 1,
        durationMs: metadata.durationMs,
        model: metadata.model,
      };

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
