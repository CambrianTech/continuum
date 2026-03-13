/**
 * InferenceGrpcClient - gRPC client for Rust inference worker
 *
 * Replaces the broken Unix socket InferenceWorkerClient with proper gRPC:
 * - Built-in cancellation
 * - Proper timeouts
 * - Streaming responses
 * - No stuck mutexes
 */

import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import * as path from 'path';

// gRPC response types (mirrors inference.proto wire format)
interface GrpcPingResponse { message: string; timestamp: string }
interface GrpcSuccessResponse { success: boolean; error?: string }
interface GrpcLoadResponse extends GrpcSuccessResponse { load_time_ms: string }
interface GrpcGenerateProgress { tokens_generated: number; tokens_total: number }
interface GrpcGenerateComplete { text: string; tokens: number; duration_ms: number }
interface GrpcGenerateResponse { progress?: GrpcGenerateProgress; complete?: GrpcGenerateComplete }
interface GrpcModelEntry { model_id: string; loaded: boolean; memory_bytes: string; dtype: string }
interface GrpcAdapterEntry { adapter_id: string; path: string; scale: number; active: boolean }
interface GrpcAdapterMetadata { base_model: string; rank: number; alpha: number; target_modules: string[]; peft_type: string }
interface GrpcDownloadResponse extends GrpcSuccessResponse {
  download_time_ms: string; adapter_id: string; local_path: string; metadata?: GrpcAdapterMetadata;
}
interface GrpcGenomeResponse extends GrpcSuccessResponse {
  apply_time_ms: string; adapters_applied: number; layers_merged: number;
}
interface GrpcStatusResponse {
  healthy: boolean; current_model: string; memory_used_bytes: string; memory_total_bytes: string;
  requests_pending: number; requests_completed: number; active_adapters: string[];
}

// gRPC client interface (dynamically loaded from proto)
interface InferenceGrpcService {
  ping(req: Record<string, never>, cb: (err: Error | null, res: GrpcPingResponse) => void): void;
  generate(req: Record<string, unknown>, opts: { deadline: Date }): grpc.ClientReadableStream<GrpcGenerateResponse>;
  loadModel(req: Record<string, unknown>, opts: { deadline: Date }, cb: (err: Error | null, res: GrpcLoadResponse) => void): void;
  unloadModel(req: Record<string, never>, cb: (err: Error | null, res: GrpcSuccessResponse) => void): void;
  listModels(req: Record<string, never>, cb: (err: Error | null, res: { models: GrpcModelEntry[] }) => void): void;
  loadAdapter(req: Record<string, unknown>, opts: { deadline: Date }, cb: (err: Error | null, res: GrpcLoadResponse) => void): void;
  unloadAdapter(req: Record<string, unknown>, cb: (err: Error | null, res: GrpcSuccessResponse) => void): void;
  listAdapters(req: Record<string, never>, cb: (err: Error | null, res: { adapters: GrpcAdapterEntry[] }) => void): void;
  downloadAdapter(req: Record<string, unknown>, opts: { deadline: Date }, cb: (err: Error | null, res: GrpcDownloadResponse) => void): void;
  applyGenome(req: Record<string, unknown>, opts: { deadline: Date }, cb: (err: Error | null, res: GrpcGenomeResponse) => void): void;
  status(req: Record<string, never>, cb: (err: Error | null, res: GrpcStatusResponse) => void): void;
  close(): void;
}

// Lazy-loaded proto to avoid module-level side effects
// (Required for CLI bundling - proto path breaks when bundled)
let _inferenceProto: grpc.GrpcObject | null = null;

function getInferenceProto(): grpc.GrpcObject {
  if (!_inferenceProto) {
    const PROTO_PATH = path.join(__dirname, '../../../workers/inference-grpc/proto/inference.proto');
    const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
      keepCase: true,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true,
    });
    _inferenceProto = grpc.loadPackageDefinition(packageDefinition);
  }
  return _inferenceProto;
}

export interface GenerateResult {
  text: string;
  tokens: number;
  durationMs: number;
}

export interface GenerateProgress {
  tokensGenerated: number;
  tokensTotal: number;
}

export interface ModelInfo {
  modelId: string;
  loaded: boolean;
  memoryBytes: number;
  dtype: string;
}

export interface AdapterInfo {
  adapterId: string;
  path: string;
  scale: number;
  active: boolean;
}

export interface AdapterMetadata {
  baseModel: string;
  rank: number;
  alpha: number;
  targetModules: string[];
  peftType: string;
}

export interface DownloadAdapterResult {
  success: boolean;
  error?: string;
  downloadTimeMs: number;
  adapterId: string;
  localPath: string;
  metadata?: AdapterMetadata;
}

export interface ServerStatus {
  healthy: boolean;
  currentModel: string;
  memoryUsedBytes: number;
  memoryTotalBytes: number;
  requestsPending: number;
  requestsCompleted: number;
  activeAdapters: string[];
}

/**
 * gRPC client for inference - NO JavaScript queue
 *
 * The Rust WorkerPool already has proper concurrent queue management:
 * - Multiple workers with own models
 * - Semaphore for backpressure
 * - Bounded channel
 *
 * JavaScript just sends requests to gRPC. Rust handles the queue.
 */
export class InferenceGrpcClient {
  private client: InferenceGrpcService;
  private static instance: InferenceGrpcClient | null = null;

  constructor(address: string = '127.0.0.1:50051') {
    const inferenceProto = getInferenceProto();
    const ServiceConstructor = (inferenceProto.inference as grpc.GrpcObject).Inference as unknown as new (addr: string, creds: grpc.ChannelCredentials) => InferenceGrpcService;
    this.client = new ServiceConstructor(
      address,
      grpc.credentials.createInsecure()
    );
    console.log(`[InferenceGrpcClient] Connected to ${address} (queue handled by Rust WorkerPool)`);
  }

  static sharedInstance(): InferenceGrpcClient {
    if (!InferenceGrpcClient.instance) {
      InferenceGrpcClient.instance = new InferenceGrpcClient();
    }
    return InferenceGrpcClient.instance;
  }

  /**
   * Ping the server
   */
  async ping(): Promise<{ message: string; timestamp: number }> {
    return new Promise((resolve, reject) => {
      this.client.ping({}, (err: Error | null, response: GrpcPingResponse) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            message: response.message,
            timestamp: Number(response.timestamp),
          });
        }
      });
    });
  }

  /**
   * Generate text - sends directly to gRPC
   *
   * Queue management is handled by the Rust WorkerPool:
   * - Semaphore provides backpressure
   * - Multiple workers handle concurrency
   * - Bounded channel prevents queue explosion
   *
   * @param modelId - Model to use
   * @param prompt - The prompt to generate from
   * @param options - Generation options
   */
  async generate(
    modelId: string,
    prompt: string,
    options?: {
      maxTokens?: number;
      temperature?: number;
      timeoutMs?: number;
      onProgress?: (progress: GenerateProgress) => void;
      signal?: AbortSignal;
      personaId?: string;   // For per-persona logging in Rust
      personaName?: string; // Human-readable name for logs
    }
  ): Promise<GenerateResult> {
    return new Promise((resolve, reject) => {
      const personaLabel = options?.personaName || 'unknown';
      console.log(`[InferenceGrpcClient] [${personaLabel}] Sending to Rust WorkerPool (prompt: ${prompt.length} chars)`);
      const deadline = new Date(Date.now() + (options?.timeoutMs ?? 300000)); // 5 min
      const maxTokens = options?.maxTokens ?? 100;
      const temperature = options?.temperature ?? 0.7;

      const call = this.client.generate(
        {
          model_id: modelId,
          prompt,
          max_tokens: maxTokens,
          temperature,
          persona_id: options?.personaId || '',
          persona_name: options?.personaName || '',
        },
        { deadline }
      );

      // Handle abort signal
      if (options?.signal) {
        options.signal.addEventListener('abort', () => {
          call.cancel();
          reject(new Error('Generation cancelled'));
        });
      }

      call.on('data', (response: GrpcGenerateResponse) => {
        if (response.progress) {
          options?.onProgress?.({
            tokensGenerated: response.progress.tokens_generated,
            tokensTotal: response.progress.tokens_total,
          });
        } else if (response.complete) {
          console.log(`[InferenceGrpcClient] Complete (${response.complete.tokens} tokens, ${response.complete.duration_ms}ms)`);
          resolve({
            text: response.complete.text,
            tokens: response.complete.tokens,
            durationMs: response.complete.duration_ms,
          });
        }
      });

      call.on('error', (err: Error) => {
        console.log(`[InferenceGrpcClient] Error: ${err.message}`);

        // Enhance error messages for common issues
        const enhancedError = this.enhanceErrorMessage(err);
        reject(enhancedError);
      });

      call.on('end', () => {
        // If we get here without resolve, something went wrong
      });
    });
  }

  /**
   * Close the client connection
   */
  close(): void {
    this.client.close();
  }

  /**
   * Enhance error messages with troubleshooting context
   * Makes cryptic inference errors more understandable
   */
  private enhanceErrorMessage(err: Error): Error {
    const msg = err.message.toLowerCase();

    // Sampling weight errors (common with bad temperature or logit values)
    if (msg.includes('weight') && (msg.includes('negative') || msg.includes('invalid') || msg.includes('large'))) {
      const enhanced = new Error(
        `Sampling failed: ${err.message}\n\n` +
        `This usually means:\n` +
        `• Temperature is too extreme (try 0.3-0.9)\n` +
        `• Input contains invalid tokens\n` +
        `• Model produced invalid probability distribution\n\n` +
        `Try: Lower temperature, shorter prompt, or retry`
      );
      enhanced.name = err.name;
      return enhanced;
    }

    // OOM errors
    if (msg.includes('out of memory') || msg.includes('oom') || msg.includes('memory allocation')) {
      const enhanced = new Error(
        `Out of memory: ${err.message}\n\n` +
        `Model requires too much GPU/RAM. Try:\n` +
        `• Reduce max_tokens\n` +
        `• Shorter prompt\n` +
        `• Unload other models/adapters`
      );
      enhanced.name = err.name;
      return enhanced;
    }

    // Timeout errors
    if (msg.includes('deadline') || msg.includes('timeout')) {
      const enhanced = new Error(
        `Generation timed out: ${err.message}\n\n` +
        `The model took too long. Try:\n` +
        `• Reduce max_tokens\n` +
        `• Shorter prompt\n` +
        `• Check if server is overloaded`
      );
      enhanced.name = err.name;
      return enhanced;
    }

    // Connection errors
    if (msg.includes('connect') || msg.includes('unavailable') || msg.includes('refused')) {
      const enhanced = new Error(
        `Cannot connect to inference server: ${err.message}\n\n` +
        `The gRPC server may not be running. Check:\n` +
        `• Is the inference-grpc worker running?\n` +
        `• Is port 50051 available?`
      );
      enhanced.name = err.name;
      return enhanced;
    }

    // Return original if no enhancement applies
    return err;
  }

  // ========================================================================
  // Model Management
  // ========================================================================

  /**
   * Load a model by HuggingFace ID
   */
  async loadModel(modelId: string, dtype?: string): Promise<{ success: boolean; error?: string; loadTimeMs: number }> {
    return new Promise((resolve, reject) => {
      const deadline = new Date(Date.now() + 300000); // 5 minutes for model loading
      this.client.loadModel({ model_id: modelId, dtype: dtype || '' }, { deadline }, (err: Error | null, response: GrpcLoadResponse) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            success: response.success,
            error: response.error || undefined,
            loadTimeMs: Number(response.load_time_ms),
          });
        }
      });
    });
  }

  /**
   * Unload the current model
   */
  async unloadModel(): Promise<{ success: boolean; error?: string }> {
    return new Promise((resolve, reject) => {
      this.client.unloadModel({}, (err: Error | null, response: GrpcSuccessResponse) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            success: response.success,
            error: response.error || undefined,
          });
        }
      });
    });
  }

  /**
   * List loaded models
   */
  async listModels(): Promise<ModelInfo[]> {
    return new Promise((resolve, reject) => {
      this.client.listModels({}, (err: Error | null, response: { models: GrpcModelEntry[] }) => {
        if (err) {
          reject(err);
        } else {
          resolve((response.models || []).map((m) => ({
            modelId: m.model_id,
            loaded: m.loaded,
            memoryBytes: Number(m.memory_bytes),
            dtype: m.dtype,
          })));
        }
      });
    });
  }

  // ========================================================================
  // LoRA Adapter Management
  // ========================================================================

  /**
   * Load a LoRA adapter
   *
   * @param adapterId - Unique identifier for this adapter
   * @param adapterPath - Path to the adapter safetensors file
   * @param options - Loading options
   * @param options.scale - LoRA scale factor (default: 1.0)
   * @param options.merge - If true, merge weights into model immediately (slower but permanent)
   */
  async loadAdapter(
    adapterId: string,
    adapterPath: string,
    options?: { scale?: number; merge?: boolean }
  ): Promise<{ success: boolean; error?: string; loadTimeMs: number }> {
    return new Promise((resolve, reject) => {
      // Longer timeout when merging weights (rebuilds model)
      const timeoutMs = options?.merge ? 300000 : 60000;
      const deadline = new Date(Date.now() + timeoutMs);
      this.client.loadAdapter(
        {
          adapter_id: adapterId,
          adapter_path: adapterPath,
          scale: options?.scale ?? 1.0,
          merge: options?.merge ?? false,
        },
        { deadline },
        (err: Error | null, response: GrpcLoadResponse) => {
          if (err) {
            reject(err);
          } else {
            resolve({
              success: response.success,
              error: response.error || undefined,
              loadTimeMs: Number(response.load_time_ms),
            });
          }
        }
      );
    });
  }

  /**
   * Unload a LoRA adapter
   */
  async unloadAdapter(adapterId: string): Promise<{ success: boolean; error?: string }> {
    return new Promise((resolve, reject) => {
      this.client.unloadAdapter({ adapter_id: adapterId }, (err: Error | null, response: GrpcSuccessResponse) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            success: response.success,
            error: response.error || undefined,
          });
        }
      });
    });
  }

  /**
   * List loaded adapters
   */
  async listAdapters(): Promise<AdapterInfo[]> {
    return new Promise((resolve, reject) => {
      this.client.listAdapters({}, (err: Error | null, response: { adapters: GrpcAdapterEntry[] }) => {
        if (err) {
          reject(err);
        } else {
          resolve((response.adapters || []).map((a) => ({
            adapterId: a.adapter_id,
            path: a.path,
            scale: a.scale,
            active: a.active,
          })));
        }
      });
    });
  }

  /**
   * Download a LoRA adapter from HuggingFace Hub
   *
   * Downloads the adapter to the local HF cache (~/.cache/huggingface/hub/),
   * parses adapter_config.json for metadata, and registers it for use.
   *
   * @param repoId - HuggingFace repo ID (e.g., "username/adapter-name")
   * @param options - Download options
   * @param options.adapterId - Local ID to use (defaults to repoId)
   * @param options.revision - Branch/revision to download (default: "main")
   * @param options.scale - Override scale factor (default: from adapter_config.json)
   */
  async downloadAdapter(
    repoId: string,
    options?: { adapterId?: string; revision?: string; scale?: number }
  ): Promise<DownloadAdapterResult> {
    return new Promise((resolve, reject) => {
      const deadline = new Date(Date.now() + 600000); // 10 minutes for download
      this.client.downloadAdapter(
        {
          repo_id: repoId,
          adapter_id: options?.adapterId ?? '',
          revision: options?.revision ?? '',
          scale: options?.scale ?? 0,
        },
        { deadline },
        (err: Error | null, response: GrpcDownloadResponse) => {
          if (err) {
            reject(err);
          } else {
            const result: DownloadAdapterResult = {
              success: response.success,
              error: response.error || undefined,
              downloadTimeMs: Number(response.download_time_ms),
              adapterId: response.adapter_id,
              localPath: response.local_path,
            };
            if (response.metadata) {
              result.metadata = {
                baseModel: response.metadata.base_model,
                rank: response.metadata.rank,
                alpha: response.metadata.alpha,
                targetModules: response.metadata.target_modules || [],
                peftType: response.metadata.peft_type,
              };
            }
            resolve(result);
          }
        }
      );
    });
  }

  // ========================================================================
  // Genome (Multi-Adapter Stacking)
  // ========================================================================

  /**
   * Apply a genome (stack multiple adapters with scales)
   *
   * Formula: W' = W + Σ(scale_i × B_i @ A_i)
   *
   * @param adapters - Array of {adapterId, scale} pairs
   */
  async applyGenome(adapters: Array<{ adapterId: string; scale: number }>): Promise<{
    success: boolean;
    error?: string;
    applyTimeMs: number;
    adaptersApplied: number;
    layersMerged: number;
  }> {
    return new Promise((resolve, reject) => {
      const deadline = new Date(Date.now() + 300000); // 5 minutes for genome rebuild
      const entries = adapters.map(a => ({
        adapter_id: a.adapterId,
        scale: a.scale,
      }));

      this.client.applyGenome({ adapters: entries }, { deadline }, (err: Error | null, response: GrpcGenomeResponse) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            success: response.success,
            error: response.error || undefined,
            applyTimeMs: Number(response.apply_time_ms),
            adaptersApplied: response.adapters_applied,
            layersMerged: response.layers_merged,
          });
        }
      });
    });
  }

  // ========================================================================
  // Server Status
  // ========================================================================

  /**
   * Get server status
   */
  async status(): Promise<ServerStatus> {
    return new Promise((resolve, reject) => {
      this.client.status({}, (err: Error | null, response: GrpcStatusResponse) => {
        if (err) {
          reject(err);
        } else {
          resolve({
            healthy: response.healthy,
            currentModel: response.current_model,
            memoryUsedBytes: Number(response.memory_used_bytes),
            memoryTotalBytes: Number(response.memory_total_bytes),
            requestsPending: response.requests_pending,
            requestsCompleted: response.requests_completed,
            activeAdapters: response.active_adapters || [],
          });
        }
      });
    });
  }
}
