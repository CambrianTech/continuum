/**
 * Rust Embedding Client
 *
 * Communicates with the Rust embedding-worker over Unix socket.
 * Uses fastembed (ONNX-based) for native embedding generation without HTTP overhead.
 *
 * Performance: ~5ms per embedding (vs ~80ms via Ollama HTTP)
 * Batch: 100 texts in ~100ms (vs ~8s via Ollama HTTP)
 */

import * as net from 'net';
import { Logger } from '../logging/Logger';

const log = Logger.create('RustEmbeddingClient', 'embedding');

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
  private buffer: string = '';
  private pendingResponse: {
    resolve: (value: RustResponse) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
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
    const response = await this.sendCommand('embedding/generate', {
      texts,
      model
    }, timeout);

    if (response.status !== 'ok' || !response.data?.embeddings) {
      throw new Error(response.message || 'Embedding generation failed');
    }

    const duration = Date.now() - startTime;
    log.debug(`Generated ${texts.length} embeddings in ${duration}ms (${(duration / texts.length).toFixed(1)}ms each)`);

    return response.data.embeddings;
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
    if (this.pendingResponse) {
      clearTimeout(this.pendingResponse.timeout);
      this.pendingResponse.reject(new Error('Connection closed'));
      this.pendingResponse = null;
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
      this.buffer = '';

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
   * Handle incoming data from Rust worker (newline-delimited JSON)
   */
  private handleData(data: Buffer): void {
    this.buffer += data.toString();

    // Process complete lines
    const lines = this.buffer.split('\n');
    this.buffer = lines.pop() || '';

    for (const line of lines) {
      if (!line.trim()) continue;

      try {
        const response: RustResponse = JSON.parse(line);

        if (this.pendingResponse) {
          clearTimeout(this.pendingResponse.timeout);
          const pending = this.pendingResponse;
          this.pendingResponse = null;
          pending.resolve(response);
        }
      } catch (error) {
        log.error(`Failed to parse response: ${error}`);
      }
    }
  }

  /**
   * Send command to Rust worker and wait for response
   * Uses queue to serialize concurrent requests - prevents socket contention
   */
  private async sendCommand(
    command: string,
    params: Record<string, any>,
    timeout: number = 30000
  ): Promise<RustResponse> {
    return new Promise((outerResolve, outerReject) => {
      // Create the actual work to be queued
      const doRequest = async (): Promise<void> => {
        try {
          // Connect if needed
          await this.connect();

          const request = {
            command,
            ...params
          };

          const result = await new Promise<RustResponse>((resolve, reject) => {
            const timeoutHandle = setTimeout(() => {
              this.pendingResponse = null;
              reject(new Error(`Request timeout: ${command}`));
            }, timeout);

            this.pendingResponse = { resolve, reject, timeout: timeoutHandle };

            // Send newline-delimited JSON
            this.socket!.write(JSON.stringify(request) + '\n');
          });

          outerResolve(result);
        } catch (error) {
          outerReject(error);
        }
      };

      // Add to queue
      this.requestQueue.push(doRequest);
      this.processQueue();
    });
  }

  /**
   * Process queued requests one at a time
   */
  private async processQueue(): Promise<void> {
    if (this.isProcessingQueue) {
      return; // Already processing
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
