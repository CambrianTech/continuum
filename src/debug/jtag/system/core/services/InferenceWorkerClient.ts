/**
 * Inference Worker Client
 *
 * Communicates with the Rust inference-worker over Unix socket.
 * Uses Candle framework for native LLM inference with multi-adapter LoRA composition.
 *
 * Features:
 * - Model loading from HuggingFace Hub
 * - Multi-adapter LoRA composition (the genome vision)
 * - Metal acceleration on Apple Silicon
 * - JTAG protocol compatible
 */

import * as net from 'net';
import { randomUUID } from 'crypto';
import { Logger } from '../logging/Logger';

const log = Logger.create('InferenceWorkerClient', 'inference');

/** Default socket path for inference worker */
const DEFAULT_SOCKET_PATH = '/tmp/jtag-inference.sock';

// ============================================================================
// Types
// ============================================================================

/** JTAG Request format */
interface JTAGRequest<T> {
  id: string;
  type: string;
  timestamp: string;
  payload: T;
  userId?: string;
  sessionId?: string;
}

/** JTAG Response format */
interface JTAGResponse<T> {
  id: string;
  type: string;
  timestamp: string;
  payload: T;
  requestId: string;
  success: boolean;
  error?: string;
  errorType?: string;
}

/** Model info returned by worker */
export interface ModelInfo {
  modelId: string;
  status: string;
  loadTimeMs?: number;
  device: string;
  loadedAtSecondsAgo?: number;
}

/** Adapter info returned by worker */
export interface AdapterInfo {
  name: string;
  modelId: string;
  path: string;
  status: string;
}

/** Result of applying (merging) adapters into model */
export interface ApplyAdaptersResult {
  modelId: string;
  adaptersApplied: string[];
  layersMerged: number;
  applyTimeMs: number;
}

/** Text generation request */
export interface GenerateRequest {
  modelId: string;
  prompt: string;
  maxTokens?: number;
  temperature?: number;
  topP?: number;
  adapters?: string[];
}

/** Text generation response */
export interface GenerateResponse {
  modelId: string;
  text: string;
  promptTokens: number;
  generatedTokens: number;
  generationTimeMs: number;
  tokensPerSecond: number;
  adaptersUsed: string[];
}

/** Command payload sent to worker */
interface InferenceCommand {
  command: string;
  [key: string]: any;
}

// ============================================================================
// Client Implementation
// ============================================================================

/**
 * Inference Worker Client - Native Rust LLM inference
 *
 * Singleton pattern for connection reuse across requests.
 * Features auto-reconnect with exponential backoff for reliability.
 */
export class InferenceWorkerClient {
  private static _instance: InferenceWorkerClient | null = null;

  private socketPath: string;
  private socket: net.Socket | null = null;
  private buffer: string = '';
  private pendingResponse: {
    resolve: (value: JTAGResponse<any>) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
  } | null = null;

  /** Track availability to avoid repeated connection attempts */
  private _available: boolean | null = null;
  private _lastAvailabilityCheck: number = 0;
  private static readonly AVAILABILITY_CACHE_MS = 5000;

  /** Request queue for serializing concurrent requests */
  private requestQueue: (() => Promise<void>)[] = [];
  private isProcessingQueue: boolean = false;

  /** Retry configuration for reliability */
  private static readonly MAX_RETRIES = 3;
  private static readonly INITIAL_RETRY_DELAY_MS = 100;
  private static readonly MAX_RETRY_DELAY_MS = 2000;

  /** Connection state tracking */
  private connectionAttempts: number = 0;
  private lastConnectionError: string | null = null;

  private constructor(socketPath: string = DEFAULT_SOCKET_PATH) {
    this.socketPath = socketPath;
  }

  /** Get shared instance */
  static get instance(): InferenceWorkerClient {
    if (!this._instance) {
      this._instance = new InferenceWorkerClient();
    }
    return this._instance;
  }

  /** Create instance with custom socket path */
  static create(socketPath: string): InferenceWorkerClient {
    return new InferenceWorkerClient(socketPath);
  }

  // ============================================================================
  // Health Check
  // ============================================================================

  /**
   * Check if Rust inference worker is available
   *
   * Uses cached result for 5 seconds to avoid spamming connection attempts.
   */
  async isAvailable(): Promise<boolean> {
    const now = Date.now();
    if (this._available !== null && (now - this._lastAvailabilityCheck) < InferenceWorkerClient.AVAILABILITY_CACHE_MS) {
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
  async ping(): Promise<{ worker: string; version: string }> {
    const response = await this.sendCommand({ command: 'ping' });
    return response.payload;
  }

  // ============================================================================
  // Model Management
  // ============================================================================

  /**
   * Load a model from HuggingFace Hub
   *
   * @param modelId - HuggingFace model ID (e.g., 'gpt2', 'meta-llama/Llama-3.2-3B-Instruct')
   * @param revision - Optional revision/branch (default: 'main')
   */
  async loadModel(modelId: string, revision?: string): Promise<ModelInfo> {
    const response = await this.sendCommand({
      command: 'model/load',
      model_id: modelId,
      revision
    });
    return response.payload;
  }

  /**
   * Unload a model from memory
   */
  async unloadModel(modelId: string): Promise<void> {
    await this.sendCommand({
      command: 'model/unload',
      model_id: modelId
    });
    log.info(`Unloaded model: ${modelId}`);
  }

  /**
   * List all loaded models
   */
  async listModels(): Promise<{ models: ModelInfo[]; count: number }> {
    const response = await this.sendCommand({ command: 'models/list' });
    return response.payload;
  }

  // ============================================================================
  // Adapter Management (LoRA)
  // ============================================================================

  /**
   * Load a LoRA adapter for a model
   *
   * @param modelId - Model to attach adapter to (must be loaded)
   * @param adapterPath - Path to .safetensors adapter file
   * @param adapterName - Name to identify this adapter
   */
  async loadAdapter(
    modelId: string,
    adapterPath: string,
    adapterName: string
  ): Promise<AdapterInfo> {
    const response = await this.sendCommand({
      command: 'adapter/load',
      model_id: modelId,
      adapter_path: adapterPath,
      adapter_name: adapterName
    });
    return response.payload;
  }

  /**
   * Unload a LoRA adapter from a model
   */
  async unloadAdapter(modelId: string, adapterName: string): Promise<void> {
    await this.sendCommand({
      command: 'adapter/unload',
      model_id: modelId,
      adapter_name: adapterName
    });
    log.info(`Unloaded adapter: ${adapterName} from ${modelId}`);
  }

  /**
   * Apply loaded adapters by merging weights into model
   *
   * This performs weight merging: W' = W + scaling * (B @ A) for each layer.
   * The model is rebuilt with merged weights. After calling this:
   * - Generate requests will use the merged weights
   * - Cannot apply different adapters without reloading the model
   *
   * @param modelId - Model with loaded adapters to merge
   */
  async applyAdapters(modelId: string): Promise<ApplyAdaptersResult> {
    const response = await this.sendCommand({
      command: 'adapter/apply',
      model_id: modelId
    });
    log.info(`Applied adapters to ${modelId}: ${response.payload.layersMerged} layers merged`);
    return response.payload;
  }

  // ============================================================================
  // Text Generation
  // ============================================================================

  /**
   * Generate text from a prompt
   *
   * @param request - Generation request with model, prompt, and options
   */
  async generate(request: GenerateRequest): Promise<GenerateResponse> {
    const startTime = Date.now();

    const response = await this.sendCommand({
      command: 'generate',
      model_id: request.modelId,
      prompt: request.prompt,
      max_tokens: request.maxTokens,
      temperature: request.temperature,
      top_p: request.topP,
      adapters: request.adapters
    });

    const duration = Date.now() - startTime;
    log.debug(`Generated response in ${duration}ms (${response.payload.promptTokens} prompt tokens)`);

    return response.payload;
  }

  /**
   * Generate text with simplified interface
   *
   * @param modelId - Model to use
   * @param prompt - Text prompt
   * @param maxTokens - Maximum tokens to generate (default: 256)
   */
  async generateText(
    modelId: string,
    prompt: string,
    maxTokens: number = 256
  ): Promise<string> {
    const response = await this.generate({
      modelId,
      prompt,
      maxTokens
    });
    return response.text;
  }

  // ============================================================================
  // Connection Management
  // ============================================================================

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

  private async connect(): Promise<void> {
    if (this.socket && !this.socket.destroyed) {
      return; // Already connected
    }

    return new Promise((resolve, reject) => {
      this.socket = net.createConnection(this.socketPath);
      this.buffer = '';

      this.socket.on('connect', () => {
        log.debug(`Connected to inference worker: ${this.socketPath}`);
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

  private handleData(data: Buffer): void {
    this.buffer += data.toString();

    // Process complete lines
    const lines = this.buffer.split('\n');
    this.buffer = lines.pop() || '';

    for (const line of lines) {
      if (!line.trim()) continue;

      try {
        const response: JTAGResponse<any> = JSON.parse(line);

        if (this.pendingResponse) {
          clearTimeout(this.pendingResponse.timeout);
          const pending = this.pendingResponse;
          this.pendingResponse = null;

          if (response.success) {
            pending.resolve(response);
          } else {
            pending.reject(new Error(response.error || 'Unknown worker error'));
          }
        }
      } catch (error) {
        log.error(`Failed to parse response: ${error}`);
      }
    }
  }

  private async sendCommand(
    command: InferenceCommand,
    timeout: number = 60000 // 60s default for model loading
  ): Promise<JTAGResponse<any>> {
    return new Promise((outerResolve, outerReject) => {
      const doRequest = async (): Promise<void> => {
        let lastError: Error | null = null;

        // Retry loop with exponential backoff
        for (let attempt = 0; attempt < InferenceWorkerClient.MAX_RETRIES; attempt++) {
          try {
            // If connection was lost, close and reconnect
            if (this.socket && this.socket.destroyed) {
              this.socket = null;
            }

            await this.connect();
            this.connectionAttempts = 0; // Reset on successful connection

            // Use JTAG protocol format
            const request: JTAGRequest<InferenceCommand> = {
              id: randomUUID(),
              type: command.command,
              timestamp: new Date().toISOString(),
              payload: command
            };

            const result = await new Promise<JTAGResponse<any>>((resolve, reject) => {
              const timeoutHandle = setTimeout(() => {
                this.pendingResponse = null;
                reject(new Error(`Request timeout: ${command.command}`));
              }, timeout);

              this.pendingResponse = { resolve, reject, timeout: timeoutHandle };

              // Send newline-delimited JSON
              this.socket!.write(JSON.stringify(request) + '\n');
            });

            outerResolve(result);
            return; // Success - exit retry loop

          } catch (error) {
            lastError = error instanceof Error ? error : new Error(String(error));
            this.lastConnectionError = lastError.message;

            // Don't retry on timeout (likely model issue, not connection issue)
            if (lastError.message.includes('timeout')) {
              break;
            }

            // Close broken connection for reconnect
            if (this.socket) {
              this.socket.destroy();
              this.socket = null;
            }

            // Calculate delay with exponential backoff
            const delay = Math.min(
              InferenceWorkerClient.INITIAL_RETRY_DELAY_MS * Math.pow(2, attempt),
              InferenceWorkerClient.MAX_RETRY_DELAY_MS
            );

            if (attempt < InferenceWorkerClient.MAX_RETRIES - 1) {
              log.debug(`Retry ${attempt + 1}/${InferenceWorkerClient.MAX_RETRIES} in ${delay}ms: ${lastError.message}`);
              await new Promise(resolve => setTimeout(resolve, delay));
            }
          }
        }

        // All retries exhausted
        outerReject(lastError || new Error('Unknown error'));
      };

      // Add to queue
      this.requestQueue.push(doRequest);
      this.processQueue();
    });
  }

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
