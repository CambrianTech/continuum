/**
 * Inference Worker Client - SIMPLIFIED
 *
 * Thin client for the Rust inference-worker. Rust owns all state:
 * - Model handles and status
 * - GPU memory management
 * - Request concurrency
 *
 * TypeScript just sends requests and receives responses.
 * NO queues. NO state tracking. Rust is the source of truth.
 */

import * as net from 'net';
import { Logger } from '../logging/Logger';

const log = Logger.create('InferenceWorkerClient', 'inference');

/** Default socket path for inference worker */
const DEFAULT_SOCKET_PATH = '/tmp/jtag-inference.sock';

// ============================================================================
// Types - Mirror Rust types
// ============================================================================

/** Handle status from Rust */
export type HandleStatus = 'loading' | 'ready' | 'error' | 'unloaded';

/** Handle info returned by Rust */
export interface HandleInfo {
  handleId: string;
  modelId: string;
  status: HandleStatus;
  memoryMb: number;
  lastUsedMs: number;
  ageSecs: number;
  error?: string;
}

/** Model info from list */
export interface ModelInfo {
  modelId: string;
  architecture: string;
  vocabSize: number;
}

/** Adapter info from list */
export interface AdapterInfo {
  adapterId: string;
  targetModel: string;
  sizeMb: number;
  rank: number;
  tensorCount: number;
  loadTimeMs: number;
}

/** Generation result */
export interface GenerateResult {
  text: string;
  promptTokens: number;
  generatedTokens: number;
  modelId: string;
}

/** Worker ping response */
export interface PingResponse {
  worker: string;
  version: string;
  modelsLoaded: number;
  handlesActive: number;
  async: boolean;
  supportedArchitectures: string[];
  api: string[];
}

/** Worker response format */
interface WorkerResponse<T> {
  success: boolean;
  result?: T;
  error?: string;
}

/** Binary response header */
interface BinaryTextHeader {
  type: string;
  length: number;
  dtype: string;
  promptTokens: number;
  generatedTokens: number;
  modelId: string;
}

// ============================================================================
// Client Implementation - THIN, NON-BLOCKING
// ============================================================================

/**
 * Inference Worker Client
 *
 * Thin wrapper around Unix socket to Rust worker.
 * No local state. No queues. Rust handles everything.
 */
export class InferenceWorkerClient {
  private static _instance: InferenceWorkerClient | null = null;

  private socketPath: string;
  private socket: net.Socket | null = null;
  private dataBuffer: Buffer = Buffer.alloc(0);
  private binaryHeader: BinaryTextHeader | null = null;

  /** Pending requests by ID - allows concurrent requests */
  private pendingRequests: Map<string, {
    resolve: (value: unknown) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
  }> = new Map();

  /** Request ID counter */
  private requestIdCounter = 0;

  /** Cached availability */
  private _available: boolean | null = null;
  private _lastAvailabilityCheck: number = 0;
  private static readonly AVAILABILITY_CACHE_MS = 5000;

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

  /** Check if worker is available (cached for 5s) */
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

  /** Ping the worker */
  async ping(): Promise<PingResponse> {
    const result = await this.sendCommand<{
      worker: string;
      version: string;
      models_loaded: number;
      handles_active: number;
      async: boolean;
      supported_architectures: string[];
      api: string[];
    }>({ command: 'ping' });

    return {
      worker: result.worker,
      version: result.version,
      modelsLoaded: result.models_loaded,
      handlesActive: result.handles_active,
      async: result.async,
      supportedArchitectures: result.supported_architectures,
      api: result.api,
    };
  }

  // ============================================================================
  // Handle API - The Right Way (NON-BLOCKING)
  // ============================================================================

  /**
   * Get or create a handle for a model
   *
   * Returns IMMEDIATELY. Model loads async in Rust.
   * Poll status or wait for ready state before generating.
   */
  async getHandle(modelId: string): Promise<HandleInfo> {
    const result = await this.sendCommand<{
      handle_id: string;
      status: HandleStatus;
      model_id: string;
      existing?: boolean;
      load_time_ms?: number;
      error?: string;
    }>({
      command: 'model/handle',
      model_id: modelId,
    }, 300000); // 5 min for model loading

    return {
      handleId: result.handle_id,
      modelId: result.model_id,
      status: result.status,
      memoryMb: 0,
      lastUsedMs: 0,
      ageSecs: 0,
      error: result.error,
    };
  }

  /**
   * Get status of a handle
   *
   * Use to poll until handle is ready.
   */
  async getHandleStatus(handleId: string): Promise<HandleInfo> {
    const result = await this.sendCommand<{
      handle_id: string;
      model_id: string;
      status: HandleStatus;
      memory_mb: number;
      last_used_ms: number;
      age_ms: number;
      error?: string;
    }>({
      command: 'handle/status',
      handle_id: handleId,
    });

    return {
      handleId: result.handle_id,
      modelId: result.model_id,
      status: result.status,
      memoryMb: result.memory_mb,
      lastUsedMs: result.last_used_ms,
      ageSecs: Math.floor(result.age_ms / 1000),
      error: result.error,
    };
  }

  /**
   * List all handles
   */
  async listHandles(): Promise<HandleInfo[]> {
    const result = await this.sendCommand<{
      handles: Array<{
        handle_id: string;
        model_id: string;
        status: HandleStatus;
        memory_mb: number;
        last_used_ms: number;
        age_ms: number;
        error?: string;
      }>;
      count: number;
    }>({ command: 'handle/list' });

    return result.handles.map(h => ({
      handleId: h.handle_id,
      modelId: h.model_id,
      status: h.status,
      memoryMb: h.memory_mb,
      lastUsedMs: h.last_used_ms,
      ageSecs: Math.floor(h.age_ms / 1000),
      error: h.error,
    }));
  }

  /**
   * Release a handle (unloads model)
   */
  async releaseHandle(handleId: string): Promise<{ modelId: string; memoryFreedMb: number }> {
    const result = await this.sendCommand<{
      status: string;
      handle_id: string;
      model_id: string;
      memory_freed_mb: number;
    }>({
      command: 'handle/release',
      handle_id: handleId,
    });

    return {
      modelId: result.model_id,
      memoryFreedMb: result.memory_freed_mb,
    };
  }

  /**
   * Wait for a handle to be ready
   *
   * Polls status until ready or error.
   */
  async waitForHandle(handleId: string, timeoutMs: number = 300000, pollIntervalMs: number = 500): Promise<HandleInfo> {
    const startTime = Date.now();

    while (Date.now() - startTime < timeoutMs) {
      const status = await this.getHandleStatus(handleId);

      if (status.status === 'ready') {
        return status;
      }

      if (status.status === 'error') {
        throw new Error(`Handle ${handleId} failed: ${status.error}`);
      }

      if (status.status === 'unloaded') {
        throw new Error(`Handle ${handleId} was unloaded`);
      }

      // Still loading - wait and poll again
      await new Promise(resolve => setTimeout(resolve, pollIntervalMs));
    }

    throw new Error(`Timeout waiting for handle ${handleId} to be ready`);
  }

  // ============================================================================
  // Legacy Model API (backward compatibility)
  // ============================================================================

  /**
   * Load a model (legacy API - wraps handle API for backward compatibility)
   *
   * Uses model/handle command internally and waits for ready state.
   */
  async loadModel(modelId: string): Promise<{ status: string; modelId: string; loadTimeMs?: number }> {
    const startTime = Date.now();

    // Use handle API - model/handle command
    const handle = await this.getHandle(modelId);

    // getHandle returns when model is ready (Rust blocks until loaded)
    return {
      status: handle.status,
      modelId: handle.modelId,
      loadTimeMs: Date.now() - startTime,
    };
  }

  /**
   * Unload a model (legacy - wraps handle API)
   *
   * Finds handle for model and releases it.
   */
  async unloadModel(modelId: string): Promise<void> {
    // Find handle for this model
    const handles = await this.listHandles();
    const handle = handles.find(h => h.modelId === modelId);

    if (handle) {
      await this.releaseHandle(handle.handleId);
    }
  }

  /**
   * List models (legacy - wraps handle API)
   *
   * Returns loaded models from active handles.
   */
  async listModels(): Promise<{ models: ModelInfo[] }> {
    const handles = await this.listHandles();

    return {
      models: handles
        .filter(h => h.status === 'ready')
        .map(h => ({
          modelId: h.modelId,
          architecture: 'unknown',
          vocabSize: 0,
        })),
    };
  }

  // ============================================================================
  // Text Generation
  // ============================================================================

  /**
   * Generate text from a prompt
   *
   * Model must be loaded (via getHandle or loadModel).
   */
  async generate(
    modelId: string,
    prompt: string,
    options: { maxTokens?: number; temperature?: number } = {}
  ): Promise<GenerateResult> {
    console.log(`🔧 [InferenceWorkerClient] generate() START: modelId=${modelId}, promptLen=${prompt.length}, maxTokens=${options.maxTokens ?? 256}`);
    try {
      const result = await this.sendCommand<{
        text: string;
        prompt_tokens: number;
        generated_tokens: number;
        model_id: string;
      }>({
        command: 'generate',
        model_id: modelId,
        prompt,
        max_tokens: options.maxTokens ?? 256,
        temperature: options.temperature ?? 0.7,
      }, 180000); // 3 min timeout for generation

      console.log(`🔧 [InferenceWorkerClient] generate() GOT RESULT: textLen=${result.text?.length ?? 0}, promptTokens=${result.prompt_tokens}`);
      return {
        text: result.text,
        promptTokens: result.prompt_tokens,
        generatedTokens: result.generated_tokens,
        modelId: result.model_id,
      };
    } catch (err) {
      console.error(`🔧 [InferenceWorkerClient] generate() ERROR: ${err}`);
      throw err;
    }
  }

  /**
   * Generate text using binary protocol
   *
   * Use for prompts with newlines/special chars.
   */
  async generateBinary(
    modelId: string,
    prompt: string,
    maxTokens: number = 256,
    temperature: number = 0.7
  ): Promise<GenerateResult> {
    await this.ensureConnected();

    const promptBytes = Buffer.from(prompt, 'utf8');
    const requestId = `binary-${++this.requestIdCounter}`;

    const header = {
      command: 'generate/binary',
      model_id: modelId,
      prompt_length: promptBytes.length,
      max_tokens: maxTokens,
      temperature,
      request_id: requestId,
    };

    return new Promise<GenerateResult>((resolve, reject) => {
      const timeoutHandle = setTimeout(() => {
        this.pendingRequests.delete(requestId);
        reject(new Error(`Binary generate timeout: ${modelId}`));
      }, 180000);

      this.pendingRequests.set(requestId, {
        resolve: resolve as (value: unknown) => void,
        reject,
        timeout: timeoutHandle,
      });

      // Send header + binary prompt
      this.socket!.write(JSON.stringify(header) + '\n');
      this.socket!.write(promptBytes);
    });
  }

  // ============================================================================
  // Adapter Management (LoRA)
  // ============================================================================

  /** Load a LoRA adapter */
  async loadAdapter(
    adapterId: string,
    adapterPath: string,
    targetModel?: string
  ): Promise<AdapterInfo> {
    const result = await this.sendCommand<{
      status: string;
      adapter_id: string;
      target_model: string;
      load_time_ms: number;
      size_mb: number;
      rank: number;
      tensor_count: number;
    }>({
      command: 'adapter/load',
      adapter_id: adapterId,
      adapter_path: adapterPath,
      target_model: targetModel,
    });

    return {
      adapterId: result.adapter_id,
      targetModel: result.target_model,
      sizeMb: result.size_mb,
      rank: result.rank,
      tensorCount: result.tensor_count,
      loadTimeMs: result.load_time_ms,
    };
  }

  /** Unload an adapter */
  async unloadAdapter(adapterId: string): Promise<void> {
    await this.sendCommand({
      command: 'adapter/unload',
      adapter_id: adapterId,
    });
  }

  /** List adapters */
  async listAdapters(): Promise<AdapterInfo[]> {
    const result = await this.sendCommand<{
      adapters: Array<{
        adapter_id: string;
        target_model: string;
        size_mb: number;
        rank: number;
        tensor_count: number;
        load_time_ms: number;
      }>;
      count: number;
    }>({ command: 'adapter/list' });

    return result.adapters.map(a => ({
      adapterId: a.adapter_id,
      targetModel: a.target_model,
      sizeMb: a.size_mb,
      rank: a.rank,
      tensorCount: a.tensor_count,
      loadTimeMs: a.load_time_ms,
    }));
  }

  // ============================================================================
  // GPU Management
  // ============================================================================

  /** Get GPU status */
  async getGpuStatus(): Promise<{
    totalMb: number;
    allocatedMb: number;
    availableMb: number;
    pressure: number;
    allocationCount: number;
    shouldEvict: boolean;
  }> {
    const result = await this.sendCommand<{
      total_mb: number;
      allocated_mb: number;
      available_mb: number;
      pressure: number;
      allocation_count: number;
      should_evict: boolean;
    }>({ command: 'gpu/status' });

    return {
      totalMb: result.total_mb,
      allocatedMb: result.allocated_mb,
      availableMb: result.available_mb,
      pressure: result.pressure,
      allocationCount: result.allocation_count,
      shouldEvict: result.should_evict,
    };
  }

  // ============================================================================
  // Connection Management
  // ============================================================================

  /** Close connection */
  close(): void {
    if (this.socket) {
      this.socket.destroy();
      this.socket = null;
    }

    // Reject all pending requests
    for (const [id, pending] of this.pendingRequests) {
      clearTimeout(pending.timeout);
      pending.reject(new Error('Connection closed'));
      this.pendingRequests.delete(id);
    }

    this._available = null;
  }

  // ============================================================================
  // Private Methods
  // ============================================================================

  private async ensureConnected(): Promise<void> {
    if (this.socket && !this.socket.destroyed) {
      return;
    }

    return new Promise((resolve, reject) => {
      this.socket = net.createConnection(this.socketPath);
      this.dataBuffer = Buffer.alloc(0);
      this.binaryHeader = null;

      const timeout = setTimeout(() => {
        if (this.socket && this.socket.connecting) {
          this.socket.destroy();
          reject(new Error(`Connection timeout: ${this.socketPath}`));
        }
      }, 5000);

      this.socket.on('connect', () => {
        clearTimeout(timeout);
        log.debug(`Connected to inference worker: ${this.socketPath}`);
        resolve();
      });

      this.socket.on('data', (data) => {
        log.debug(`Received ${data.length} bytes from worker`);
        this.handleData(data);
      });

      this.socket.on('error', (error) => {
        log.debug(`Socket error: ${error.message}`);
        this._available = false;
        this.rejectAllPending(`Socket error: ${error.message}`);
        reject(error);
      });

      this.socket.on('close', () => {
        log.debug('Socket closed');
        this.socket = null;
        this._available = null;
        this.rejectAllPending('Socket closed unexpectedly');
      });
    });
  }

  private rejectAllPending(message: string): void {
    for (const [id, pending] of this.pendingRequests) {
      clearTimeout(pending.timeout);
      pending.reject(new Error(message));
      this.pendingRequests.delete(id);
    }
  }

  private handleData(data: Buffer): void {
    this.dataBuffer = Buffer.concat([this.dataBuffer, data]);

    // Check for binary data
    if (this.binaryHeader) {
      if (this.dataBuffer.length >= this.binaryHeader.length) {
        const textBytes = this.dataBuffer.subarray(0, this.binaryHeader.length);
        this.dataBuffer = this.dataBuffer.subarray(this.binaryHeader.length);

        const text = textBytes.toString('utf8');
        const header = this.binaryHeader;
        this.binaryHeader = null;

        // Find and resolve the binary request
        for (const [id, pending] of this.pendingRequests) {
          if (id.startsWith('binary-')) {
            clearTimeout(pending.timeout);
            this.pendingRequests.delete(id);
            pending.resolve({
              text,
              promptTokens: header.promptTokens,
              generatedTokens: header.generatedTokens,
              modelId: header.modelId,
            } as GenerateResult);
            break;
          }
        }
      }
      return;
    }

    // Look for newline to complete JSON response
    while (true) {
      const newlineIdx = this.dataBuffer.indexOf('\n');
      if (newlineIdx === -1) break;

      const line = this.dataBuffer.subarray(0, newlineIdx).toString();
      this.dataBuffer = this.dataBuffer.subarray(newlineIdx + 1);

      if (!line.trim()) continue;

      try {
        const parsed = JSON.parse(line);

        // Binary response header
        if (parsed.type === 'binary') {
          this.binaryHeader = parsed as BinaryTextHeader;
          if (this.dataBuffer.length >= this.binaryHeader.length) {
            this.handleData(Buffer.alloc(0));
          }
          continue;
        }

        // Regular JSON response - match by request_id for proper correlation
        const response = parsed as WorkerResponse<unknown> & { request_id?: string };
        const requestId = response.request_id;
        console.log(`🔧 [InferenceWorkerClient] handleData: Got response, request_id=${requestId}, success=${response.success}, pending=${this.pendingRequests.size}`);

        if (requestId && this.pendingRequests.has(requestId)) {
          // Proper correlation by request_id
          const pending = this.pendingRequests.get(requestId)!;
          clearTimeout(pending.timeout);
          this.pendingRequests.delete(requestId);

          if (response.success) {
            pending.resolve(response.result);
          } else {
            pending.reject(new Error(response.error || 'Unknown worker error'));
          }
        } else {
          // Fallback: resolve first pending non-binary request (legacy)
          for (const [id, pending] of this.pendingRequests) {
            if (!id.startsWith('binary-')) {
              clearTimeout(pending.timeout);
              this.pendingRequests.delete(id);

              if (response.success) {
                pending.resolve(response.result);
              } else {
                pending.reject(new Error(response.error || 'Unknown worker error'));
              }
              break;
            }
          }
        }
      } catch (error) {
        log.error(`Failed to parse response: ${error}`);
      }
    }
  }

  private async sendCommand<T>(
    command: Record<string, unknown>,
    timeout: number = 60000
  ): Promise<T> {
    await this.ensureConnected();

    const requestId = `req-${++this.requestIdCounter}`;

    return new Promise<T>((resolve, reject) => {
      const timeoutHandle = setTimeout(() => {
        this.pendingRequests.delete(requestId);
        reject(new Error(`Request timeout: ${command.command}`));
      }, timeout);

      this.pendingRequests.set(requestId, {
        resolve: resolve as (value: unknown) => void,
        reject,
        timeout: timeoutHandle,
      });

      // CRITICAL: Include request_id so Rust can echo it back for correlation
      const commandWithId = { ...command, request_id: requestId };
      const data = JSON.stringify(commandWithId) + '\n';

      // Check socket state before writing
      if (!this.socket || this.socket.destroyed || !this.socket.writable) {
        clearTimeout(timeoutHandle);
        this.pendingRequests.delete(requestId);
        reject(new Error(`Socket not writable: destroyed=${this.socket?.destroyed}, writable=${this.socket?.writable}`));
        return;
      }

      // Write with error handling
      const written = this.socket.write(data, (err) => {
        if (err) {
          log.error(`Socket write error: ${err.message}`);
          clearTimeout(timeoutHandle);
          this.pendingRequests.delete(requestId);
          reject(new Error(`Socket write failed: ${err.message}`));
        }
      });

      if (!written) {
        // Buffer is full - wait for drain, but don't fail yet
        log.debug(`Socket buffer full, waiting for drain (requestId=${requestId})`);
      }
    });
  }
}

// Re-export for backward compatibility
export { InferenceWorkerClient as default };
